//! Album art: encoded bytes from the engine, decoded once on Rune's image workers, kept in a
//! small LRU bounded by entries and by decoded bytes. Views only look handles up.

use std::cell::{Cell, OnceCell, RefCell};
use std::collections::HashMap;
use std::sync::Arc;

use rune::{Image, ImageCache, Pixels, Waker};

use super::link::Link;

const MAX_ENTRIES: usize = 240;
const MAX_DECODED_BYTES: usize = 96 * 1024 * 1024;
/// What an image not yet decoded is counted as: a 300 px cover with its mips.
const PENDING_BYTES: usize = 300 * 300 * 4 * 4 / 3;

struct Entry {
    image: Image,
    used: u64,
}

impl Entry {
    fn bytes(&self) -> usize {
        self.image.size().map_or(PENDING_BYTES, |[w, h]| w as usize * h as usize * 4 * 4 / 3)
    }
}

#[derive(Default)]
pub struct Art {
    waker: RefCell<Option<Waker>>,
    cache: OnceCell<ImageCache>,
    entries: RefCell<HashMap<String, Entry>>,
    view: Cell<u64>,
}

impl Art {
    /// The waker from `AppSpec::setup`, so decoded art redraws the window.
    pub fn set_waker(&self, waker: Waker) {
        *self.waker.borrow_mut() = Some(waker);
    }

    /// Call once at the top of each view: entries used since are kept over older ones.
    pub fn begin_view(&self) {
        self.view.set(self.view.get() + 1);
    }

    /// The art at `url` once the engine has its bytes. The first call starts the fetch; the
    /// engine's notification re-runs the view when they land.
    pub fn pixels(&self, link: &Link, url: &str) -> Option<Pixels> {
        let now = self.view.get();
        if let Some(entry) = self.entries.borrow_mut().get_mut(url) {
            entry.used = now;
            return Self::ready(&entry.image);
        }
        let bytes = link.image(url)?;
        let image = self.cache().load(bytes);
        if rune::tooling::headless_run().is_some() {
            // Snapshots are deterministic: the frame that first shows art draws it.
            let _decoded = image.wait();
        }
        let shown = Self::ready(&image);
        self.entries.borrow_mut().insert(url.to_owned(), Entry { image, used: now });
        self.evict();
        shown
    }

    fn ready(image: &Image) -> Option<Pixels> {
        match image.error() {
            Some(_) => None,
            None => Some(image.pixels().clone()),
        }
    }

    fn cache(&self) -> &ImageCache {
        self.cache.get_or_init(|| {
            let waker = self.waker.borrow().clone().unwrap_or_else(|| Arc::new(|| {}));
            ImageCache::new(waker)
        })
    }

    fn evict(&self) {
        let now = self.view.get();
        let mut entries = self.entries.borrow_mut();
        let mut bytes: usize = entries.values().map(Entry::bytes).sum();
        while entries.len() > MAX_ENTRIES || bytes > MAX_DECODED_BYTES {
            let oldest = entries
                .iter()
                .filter(|(_, e)| e.used < now)
                .min_by_key(|(_, e)| e.used)
                .map(|(url, _)| url.clone());
            let Some(url) = oldest else { break };
            if let Some(entry) = entries.remove(&url) {
                bytes = bytes.saturating_sub(entry.bytes());
            }
        }
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.entries.borrow().len()
    }
}
