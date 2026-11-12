//! Where episodes played in Baliset were left, kept in a small file so the next play picks
//! up there. Spotify keeps its own record, but not anywhere this client can read.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use super::lock;
use crate::engine::Shared;
use crate::model::EpisodeProgress;

/// Less than this in is a peek, not a listen.
const MIN_LISTEN_MS: u32 = 10_000;
/// Stopping this close to the end counts as having heard it.
const NEAR_END_MS: u32 = 30_000;
const MAX_REMEMBERED: usize = 500;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Entry {
    progress: EpisodeProgress,
    /// Seconds since the epoch, to forget the oldest first.
    at: u64,
}

/// What stopping at `position_ms` says, or `None` when it says nothing worth keeping.
fn progress(position_ms: u32, duration_ms: u32) -> Option<EpisodeProgress> {
    if duration_ms > 0 && position_ms.saturating_add(NEAR_END_MS) >= duration_ms {
        return Some(EpisodeProgress { position_ms: duration_ms, played: true });
    }
    (position_ms >= MIN_LISTEN_MS).then_some(EpisodeProgress { position_ms, played: false })
}

fn to_json(entries: &HashMap<String, Entry>) -> Value {
    let list: Vec<Value> = entries.iter().map(|(uri, e)| json!({"uri": uri, "position_ms": e.progress.position_ms, "played": e.progress.played, "at": e.at})).collect();
    Value::Array(list)
}

fn from_json(value: &Value) -> HashMap<String, Entry> {
    let list = value.as_array().map_or(&[][..], Vec::as_slice);
    list.iter()
        .filter_map(|entry| {
            let uri = entry["uri"].as_str().filter(|uri| uri.starts_with("spotify:episode:"))?;
            let position_ms = u32::try_from(entry["position_ms"].as_u64()?).ok()?;
            let progress = EpisodeProgress { position_ms, played: entry["played"].as_bool().unwrap_or(false) };
            Some((uri.to_owned(), Entry { progress, at: entry["at"].as_u64().unwrap_or(0) }))
        })
        .collect()
}

pub(super) struct Progress {
    shared: Arc<Shared>,
    path: PathBuf,
    entries: Mutex<HashMap<String, Entry>>,
}

impl Progress {
    /// Reads what an earlier run left and puts it in the store.
    pub(super) fn open(shared: Arc<Shared>, path: PathBuf) -> Self {
        let entries = std::fs::read(&path).ok().and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok()).map(|value| from_json(&value)).unwrap_or_default();
        if !entries.is_empty() {
            shared.update(|store| store.episode_progress = entries.iter().map(|(uri, e)| (uri.clone(), e.progress)).collect());
        }
        Self { shared, path, entries: Mutex::new(entries) }
    }

    pub(super) fn record(&self, uri: &str, position_ms: u32, duration_ms: u32) {
        if !uri.starts_with("spotify:episode:") {
            return;
        }
        let Some(progress) = progress(position_ms, duration_ms) else { return };
        let at = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.as_secs());
        let saved = {
            let mut entries = lock(&self.entries);
            if entries.get(uri).is_some_and(|entry| entry.progress == progress) {
                return;
            }
            entries.insert(uri.to_owned(), Entry { progress, at });
            while entries.len() > MAX_REMEMBERED {
                let Some(oldest) = entries.iter().min_by_key(|(_, entry)| entry.at).map(|(uri, _)| uri.clone()) else { break };
                entries.remove(&oldest);
            }
            to_json(&entries).to_string()
        };
        self.shared.update(|store| {
            store.episode_progress.insert(uri.to_owned(), progress);
        });
        if let Err(error) = std::fs::write(&self.path, saved) {
            log::warn!("can't save episode positions: {error}");
        }
    }

    pub(super) fn resume_at(&self, uri: &str) -> u32 {
        lock(&self.entries).get(uri).filter(|entry| !entry.progress.played).map_or(0, |entry| entry.progress.position_ms)
    }

    pub(super) fn forget(&self) {
        lock(&self.entries).clear();
        if let Err(error) = std::fs::remove_file(&self.path) {
            log::debug!("no saved episode positions to remove: {error}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_peek_is_not_kept_and_the_last_half_minute_counts_as_heard() {
        assert_eq!(progress(4_000, 3_600_000), None);
        assert_eq!(progress(1_800_000, 3_600_000), Some(EpisodeProgress { position_ms: 1_800_000, played: false }));
        assert_eq!(progress(3_580_000, 3_600_000), Some(EpisodeProgress { position_ms: 3_600_000, played: true }));
        assert_eq!(progress(20_000, 0), Some(EpisodeProgress { position_ms: 20_000, played: false }), "an unknown length is never finished");
    }

    #[test]
    fn positions_survive_their_file_format_and_strangers_are_dropped() {
        let mut entries = HashMap::new();
        entries.insert("spotify:episode:a".to_owned(), Entry { progress: EpisodeProgress { position_ms: 61_000, played: false }, at: 5 });
        entries.insert("spotify:episode:b".to_owned(), Entry { progress: EpisodeProgress { position_ms: 9_000_000, played: true }, at: 6 });
        assert_eq!(from_json(&to_json(&entries)), entries);
        let odd = json!([{"uri": "spotify:track:x", "position_ms": 1}, {"uri": "spotify:episode:c"}, 7]);
        assert!(from_json(&odd).is_empty());
    }

    #[test]
    fn a_heard_episode_starts_over_and_a_half_heard_one_resumes() {
        let dir = std::env::temp_dir().join(format!("baliset-progress-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("episodes.json");
        let shared = Shared::new();
        let positions = Progress::open(shared.clone(), path.clone());
        positions.record("spotify:episode:a", 1_234_000, 3_600_000);
        positions.record("spotify:episode:b", 3_590_000, 3_600_000);
        positions.record("spotify:track:c", 100_000, 200_000);
        assert_eq!((positions.resume_at("spotify:episode:a"), positions.resume_at("spotify:episode:b")), (1_234_000, 0));
        assert_eq!(shared.read(|store| store.episode_progress.len()), 2);

        let again = Progress::open(Shared::new(), path.clone());
        assert_eq!(again.resume_at("spotify:episode:a"), 1_234_000, "a later run reads the file");
        again.forget();
        assert!(!path.exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
