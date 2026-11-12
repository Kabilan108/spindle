//! The play queue. librespot's Connect device keeps it but offers no call to change it, so
//! changes go the way another client's would: a player command posted to Connect, addressed
//! from this device to itself, which Spotify hands back to the device to apply
//! (`add_to_queue`, `skip_next` with a track, `set_queue`). What plays next is the device's
//! own `next_tracks` in the account's Connect cluster. Spotify pushes the cluster when another
//! device changes it, not reliably when this one does, so after a change here it is read back
//! (`GET /connect-state/v1/cluster`).

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use http::header::{ACCEPT, CONTENT_TYPE};
use http::{HeaderMap, HeaderValue, Method};
use librespot_core::{Error, Session};
use librespot_protocol::connect::Cluster;
use librespot_protocol::player::{PlayerState, ProvidedTrack};
use protobuf::Message;
use serde_json::{Map, Value, json};

use super::library::Library;
use crate::engine::Shared;

const QUEUED: &str = "is_queued";

fn is_queued(track: &ProvidedTrack) -> bool {
    track.provider == "queue" || track.metadata.get(QUEUED).is_some_and(|flag| flag == "true")
}

fn is_track(track: &ProvidedTrack) -> bool {
    track.uri.starts_with("spotify:track:") || track.uri.starts_with("spotify:episode:")
}

/// The device's next tracks as the cluster last reported them.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Snapshot {
    pub next: Vec<ProvidedTrack>,
    pub prev: Vec<ProvidedTrack>,
    pub revision: String,
}

impl Snapshot {
    pub(super) fn of(state: &PlayerState) -> Self {
        Self { next: state.next_tracks.clone(), prev: state.prev_tracks.clone(), revision: state.queue_revision.clone() }
    }

    /// URIs of the queued tracks, then of the context's, in the order they play. Delimiters
    /// and anything that isn't a track or an episode are left out.
    pub(super) fn uris(&self) -> (Vec<String>, Vec<String>) {
        let tracks = self.next.iter().filter(|track| is_track(track));
        let (queued, upcoming): (Vec<&ProvidedTrack>, Vec<&ProvidedTrack>) = tracks.partition(|track| is_queued(track));
        let uris = |list: Vec<&ProvidedTrack>| list.into_iter().map(|track| track.uri.clone()).collect();
        (uris(queued), uris(upcoming))
    }

    pub(super) fn plays_next(&self, uri: &str) -> bool {
        self.next.iter().any(|track| track.uri == uri)
    }

    /// The queue without its `index`th queued track, or `None` when there is no such track.
    pub(super) fn without_queued(&self, index: usize) -> Option<Vec<ProvidedTrack>> {
        let position = self.next.iter().enumerate().filter(|(_, track)| is_track(track) && is_queued(track)).nth(index)?.0;
        let mut next = self.next.clone();
        next.remove(position);
        Some(next)
    }
}

fn track_json(track: &ProvidedTrack) -> Value {
    let metadata: Map<String, Value> = track.metadata.iter().map(|(key, value)| (key.clone(), Value::String(value.clone()))).collect();
    let mut out = json!({ "uri": track.uri, "uid": track.uid, "provider": track.provider, "metadata": metadata });
    for (key, value) in [("album_uri", &track.album_uri), ("artist_uri", &track.artist_uri)] {
        if !value.is_empty() {
            out[key] = Value::String(value.clone());
        }
    }
    out
}

fn command(endpoint: &str, mut fields: Value) -> Value {
    fields["endpoint"] = Value::String(endpoint.to_owned());
    fields["logging_params"] = json!({});
    json!({ "command": fields })
}

pub(super) fn add_to_queue(track_uri: &str) -> Value {
    command("add_to_queue", json!({ "track": { "uri": track_uri, "metadata": { QUEUED: "true" }, "provider": "queue" } }))
}

pub(super) fn skip_to(track_uri: &str) -> Value {
    command("skip_next", json!({ "track": { "uri": track_uri } }))
}

pub(super) fn set_queue(next: &[ProvidedTrack], snapshot: &Snapshot) -> Value {
    let list = |tracks: &[ProvidedTrack]| Value::Array(tracks.iter().map(track_json).collect());
    command("set_queue", json!({ "next_tracks": list(next), "prev_tracks": list(&snapshot.prev), "queue_revision": snapshot.revision }))
}

/// The account's Connect cluster as Spotify holds it now.
async fn cluster(session: &Session) -> Result<Cluster, Error> {
    let mut headers = HeaderMap::new();
    headers.insert(ACCEPT, HeaderValue::from_static("application/x-protobuf"));
    let body = session.spclient().request(&Method::GET, "/connect-state/v1/cluster", Some(headers), None).await?;
    Ok(Cluster::parse_from_bytes(&body)?)
}

/// This device's next tracks, kept current from pushed cluster updates and from reading the
/// cluster back after changes made here. Cheap to share.
pub(super) struct Watch {
    snapshot: Mutex<Snapshot>,
    library: Arc<Library>,
    session: Session,
    shared: Arc<Shared>,
}

impl Watch {
    pub(super) fn new(library: Arc<Library>, session: Session, shared: Arc<Shared>) -> Arc<Self> {
        Arc::new(Self { snapshot: Mutex::new(Snapshot::default()), library, session, shared })
    }

    pub(super) fn snapshot(&self) -> MutexGuard<'_, Snapshot> {
        self.snapshot.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The device reported its player state: describe what's next again if it changed. Its
    /// shuffle is the truth too: librespot reports none for a load that turns it on.
    pub(super) fn report(&self, state: &PlayerState) {
        let shuffle = state.options.shuffling_context;
        if self.shared.read(|store| store.playback.shuffle != shuffle) {
            self.shared.update(|store| store.playback.shuffle = shuffle);
        }
        let fresh = Snapshot::of(state);
        let (queued, upcoming) = fresh.uris();
        let before = std::mem::replace(&mut *self.snapshot(), fresh).uris();
        if before != (queued.clone(), upcoming.clone()) {
            tokio::spawn(self.library.clone().describe_up_next(queued, upcoming));
        }
    }

    /// Another device plays, or none: nothing is next here.
    pub(super) fn clear(&self) {
        *self.snapshot() = Snapshot::default();
        self.library.clear_up_next();
    }

    /// Read the cluster back after `delay`, once librespot has had time to publish a change.
    pub(super) fn reread(self: &Arc<Self>, delay: Duration) {
        let watch = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep(delay).await;
            match cluster(&watch.session).await {
                Ok(cluster) if cluster.active_device_id == watch.session.device_id() => watch.report(&cluster.player_state),
                Ok(_) => watch.clear(),
                Err(error) => log::info!("can't read the queue back: {error}"),
            }
        });
    }
}

/// Post a player command to this device through Connect.
pub(super) async fn send(session: &Session, body: &Value) -> Result<(), Error> {
    let device = session.device_id();
    let endpoint = format!("/connect-state/v1/player/command/from/{device}/to/{device}");
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    let body = serde_json::to_vec(body)?;
    session.spclient().request(&Method::POST, &endpoint, Some(headers), Some(&body)).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use librespot_core::dealer::protocol::Command;

    use super::*;

    fn provided(uri: &str, provider: &str) -> ProvidedTrack {
        ProvidedTrack { uri: uri.into(), uid: format!("uid-{uri}"), provider: provider.into(), ..Default::default() }
    }

    /// librespot's own reading of a command, as its device gets it back from Connect.
    fn read_back(body: &Value) -> Command {
        serde_json::from_value(body["command"].clone()).expect("librespot reads the command")
    }

    #[test]
    fn add_to_queue_is_a_command_librespots_device_reads_as_one_queued_track() {
        let body = add_to_queue("spotify:track:a");
        let Command::AddToQueue(add) = read_back(&body) else { panic!("{body}") };
        assert_eq!(add.track.uri, "spotify:track:a");
        assert_eq!(add.track.provider, "queue");
        assert_eq!(add.track.metadata.get(QUEUED).map(String::as_str), Some("true"));
    }

    #[test]
    fn skip_to_names_the_track_to_skip_to() {
        let Command::SkipNext(skip) = read_back(&skip_to("spotify:track:b")) else { panic!("skip_next") };
        assert_eq!(skip.track.map(|track| track.uri).as_deref(), Some("spotify:track:b"));
    }

    #[test]
    fn set_queue_carries_the_tracks_and_the_revision_it_replaces() {
        let snapshot = Snapshot { next: vec![provided("spotify:track:q", "queue"), provided("spotify:track:c", "context")], prev: vec![provided("spotify:track:p", "context")], revision: "42".into() };
        let next = snapshot.without_queued(0).unwrap();
        let Command::SetQueue(set) = read_back(&set_queue(&next, &snapshot)) else { panic!("set_queue") };
        assert_eq!(set.next_tracks.iter().map(|t| t.uri.as_str()).collect::<Vec<_>>(), ["spotify:track:c"]);
        assert_eq!(set.next_tracks[0].uid, "uid-spotify:track:c");
        assert_eq!(set.prev_tracks.len(), 1);
        assert_eq!(set.queue_revision, "42");
        assert_eq!(snapshot.without_queued(1), None);
    }

    #[test]
    fn up_next_splits_queued_tracks_from_the_context_and_drops_delimiters() {
        let mut flagged = provided("spotify:track:f", "context");
        flagged.metadata.insert(QUEUED.into(), "true".into());
        let snapshot = Snapshot {
            next: vec![provided("spotify:track:q", "queue"), flagged, provided("spotify:track:c1", "context"), provided("spotify:delimiter", "context"), provided("spotify:track:a1", "autoplay")],
            ..Snapshot::default()
        };
        let (queued, upcoming) = snapshot.uris();
        assert_eq!(queued, ["spotify:track:q", "spotify:track:f"]);
        assert_eq!(upcoming, ["spotify:track:c1", "spotify:track:a1"]);
        assert!(snapshot.plays_next("spotify:track:c1"));
        assert!(!snapshot.plays_next("spotify:track:zz"));
    }
}
