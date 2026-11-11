use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

/// Encoded image bytes, shared without copying.
pub type ImageBytes = Arc<[u8]>;

const MAX_BYTES: usize = 48 * 1024 * 1024;
const MAX_ENTRIES: usize = 600;
/// A failed fetch is tried again after this long, so covers missed while offline come back.
const RETRY_AFTER: Duration = Duration::from_secs(30);

enum Entry {
    Pending,
    Ready(ImageBytes),
    Failed(Instant),
}

pub(super) enum Lookup {
    Ready(ImageBytes),
    Pending,
    /// Not asked for yet; the caller starts the fetch. Later lookups report `Pending`.
    Missing,
}

#[derive(Default)]
struct Inner {
    entries: HashMap<String, Entry>,
    /// Ready URLs, oldest first.
    order: VecDeque<String>,
    bytes: usize,
}

#[derive(Default)]
pub(super) struct Images {
    inner: Mutex<Inner>,
}

impl Images {
    pub(super) fn get(&self, url: &str) -> Lookup {
        let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        match inner.entries.get(url) {
            Some(Entry::Ready(bytes)) => Lookup::Ready(bytes.clone()),
            Some(Entry::Failed(at)) if at.elapsed() >= RETRY_AFTER => {
                inner.entries.insert(url.to_owned(), Entry::Pending);
                Lookup::Missing
            }
            Some(Entry::Pending | Entry::Failed(_)) => Lookup::Pending,
            None => {
                inner.entries.insert(url.to_owned(), Entry::Pending);
                Lookup::Missing
            }
        }
    }

    pub(super) fn arrived(&self, url: &str, bytes: Option<Vec<u8>>) {
        let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(bytes) = bytes else {
            inner.entries.insert(url.to_owned(), Entry::Failed(Instant::now()));
            return;
        };
        inner.bytes += bytes.len();
        inner.entries.insert(url.to_owned(), Entry::Ready(bytes.into()));
        inner.order.push_back(url.to_owned());
        while inner.bytes > MAX_BYTES || inner.order.len() > MAX_ENTRIES {
            let Some(oldest) = inner.order.pop_front() else { break };
            if let Some(Entry::Ready(evicted)) = inner.entries.remove(&oldest) {
                inner.bytes -= evicted.len();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_url_is_fetched_once_and_served_after_it_arrives() {
        let images = Images::default();
        assert!(matches!(images.get("a"), Lookup::Missing));
        assert!(matches!(images.get("a"), Lookup::Pending));
        images.arrived("a", Some(vec![1, 2, 3]));
        assert!(matches!(images.get("a"), Lookup::Ready(bytes) if *bytes == [1, 2, 3]));
    }

    #[test]
    fn a_failed_url_is_not_fetched_again_right_away() {
        let images = Images::default();
        assert!(matches!(images.get("a"), Lookup::Missing));
        images.arrived("a", None);
        assert!(matches!(images.get("a"), Lookup::Pending));
    }

    #[test]
    fn the_oldest_images_leave_when_the_cache_is_full() {
        let images = Images::default();
        for i in 0..=MAX_ENTRIES {
            let url = i.to_string();
            let _ = images.get(&url);
            images.arrived(&url, Some(vec![0; 8]));
        }
        assert!(matches!(images.get("0"), Lookup::Missing));
        assert!(matches!(images.get("1"), Lookup::Ready(_)));
    }
}
