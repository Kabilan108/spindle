//! MPRIS (`org.mpris.MediaPlayer2`) on the session bus, so media keys, `playerctl` and a
//! desktop shell's media widget see and drive the player. It runs on its own thread and
//! connection; without a session bus it logs once and the app carries on.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::mpsc::unbounded_channel;
use zbus::object_server::SignalEmitter;
use zbus::{ObjectPath, OwnedVariant, Variant};

use crate::engine::Engine;
use crate::model::{Command, Playback, Repeat, Track};

const BUS_NAME: &str = "org.mpris.MediaPlayer2.baliset";
const OBJECT_PATH: &str = "/org/mpris/MediaPlayer2";
const NO_TRACK: &str = "/org/mpris/MediaPlayer2/TrackList/NoTrack";

type Hook = Arc<dyn Fn() + Send + Sync>;
type Metadata = HashMap<String, OwnedVariant>;

struct Root {
    raise: Hook,
    quit: Hook,
}

#[zbus::interface(name = "org.mpris.MediaPlayer2")]
impl Root {
    fn raise(&self) {
        (self.raise)();
    }
    fn quit(&self) {
        (self.quit)();
    }
    #[zbus(property)]
    fn can_quit(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn can_raise(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn has_track_list(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn identity(&self) -> &str {
        "Baliset"
    }
    #[zbus(property)]
    fn desktop_entry(&self) -> &str {
        "baliset"
    }
    #[zbus(property)]
    fn supported_uri_schemes(&self) -> Vec<String> {
        vec!["spotify".to_owned()]
    }
    #[zbus(property)]
    fn supported_mime_types(&self) -> Vec<String> {
        Vec::new()
    }
}

fn track_path(track: Option<&Track>) -> String {
    let id = track.and_then(|track| track.uri.rsplit(':').next()).filter(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_alphanumeric()));
    id.map_or_else(|| NO_TRACK.to_owned(), |id| format!("/org/baliset/track/{id}"))
}

fn variant<'a>(value: impl Into<Variant<'a>>) -> Option<OwnedVariant> {
    OwnedVariant::try_from(value.into()).ok()
}

fn metadata(track: Option<&Track>) -> Metadata {
    let mut map = Metadata::new();
    let mut put = |key: &str, value: Option<OwnedVariant>| {
        if let Some(value) = value {
            map.insert(key.to_owned(), value);
        }
    };
    let path = track_path(track);
    put("mpris:trackid", ObjectPath::try_from(path.as_str()).ok().and_then(variant));
    let Some(track) = track else { return map };
    put("mpris:length", variant(i64::from(track.duration_ms) * 1000));
    put("xesam:title", variant(track.name.clone()));
    put("xesam:artist", variant(track.artists.iter().map(|artist| artist.name.clone()).collect::<Vec<_>>()));
    if let Some(album) = &track.album {
        put("xesam:album", variant(album.name.clone()));
    }
    if let Some(art) = &track.image_url {
        put("mpris:artUrl", variant(art.clone()));
    }
    if let Some((kind @ ("track" | "episode"), id)) = track.uri.strip_prefix("spotify:").and_then(|rest| rest.split_once(':')) {
        put("xesam:url", variant(format!("https://open.spotify.com/{kind}/{id}")));
    }
    map
}

fn status(playback: &Playback) -> &'static str {
    match (&playback.track, playback.playing) {
        (None, _) => "Stopped",
        (Some(_), true) => "Playing",
        (Some(_), false) => "Paused",
    }
}

fn loop_status(repeat: Repeat) -> &'static str {
    match repeat {
        Repeat::Off => "None",
        Repeat::Context => "Playlist",
        Repeat::Track => "Track",
    }
}

struct PlayerInterface {
    engine: Engine,
}

impl PlayerInterface {
    fn playback(&self) -> Playback {
        self.engine.read(|store| store.playback.clone())
    }

    fn seek_to(&self, position_us: i64) {
        let position_ms = u32::try_from((position_us / 1000).max(0)).unwrap_or(u32::MAX);
        self.engine.send(Command::Seek { position_ms });
    }
}

#[zbus::interface(name = "org.mpris.MediaPlayer2.Player")]
impl PlayerInterface {
    fn next(&self) {
        self.engine.send(Command::Next);
    }
    fn previous(&self) {
        self.engine.send(Command::Previous);
    }
    fn pause(&self) {
        if self.playback().playing {
            self.engine.send(Command::PlayPause);
        }
    }
    fn play_pause(&self) {
        self.engine.send(Command::PlayPause);
    }
    fn stop(&self) {
        self.pause();
    }
    fn play(&self) {
        if !self.playback().playing {
            self.engine.send(Command::PlayPause);
        }
    }
    fn seek(&self, offset: i64) {
        self.seek_to(i64::from(self.playback().position_now()) * 1000 + offset);
    }
    fn set_position(&self, track_id: ObjectPath<'_>, position: i64) {
        // A position for a track that has since changed is stale, as the spec says.
        if track_id.as_str() == track_path(self.playback().track.as_ref()) {
            self.seek_to(position);
        }
    }
    fn open_uri(&self, uri: String) {
        if uri.starts_with("spotify:track:") || uri.starts_with("spotify:episode:") {
            self.engine.send(Command::PlayTracks { uris: vec![uri], index: 0 });
        } else if uri.starts_with("spotify:") {
            self.engine.send(Command::PlayContext { uri, track_uri: None });
        }
    }

    #[zbus(signal)]
    async fn seeked(emitter: &SignalEmitter<'_>, position: i64) -> zbus::Result<()>;

    #[zbus(property)]
    fn playback_status(&self) -> &str {
        status(&self.playback())
    }
    #[zbus(property)]
    fn loop_status(&self) -> &str {
        loop_status(self.playback().repeat)
    }
    #[zbus(property)]
    fn set_loop_status(&self, value: &str) {
        let repeat = match value {
            "Playlist" => Repeat::Context,
            "Track" => Repeat::Track,
            _ => Repeat::Off,
        };
        self.engine.send(Command::SetRepeat(repeat));
    }
    #[zbus(property)]
    fn rate(&self) -> f64 {
        1.0
    }
    #[zbus(property)]
    fn set_rate(&self, _value: f64) {}
    #[zbus(property)]
    fn minimum_rate(&self) -> f64 {
        1.0
    }
    #[zbus(property)]
    fn maximum_rate(&self) -> f64 {
        1.0
    }
    #[zbus(property)]
    fn shuffle(&self) -> bool {
        self.playback().shuffle
    }
    #[zbus(property)]
    fn set_shuffle(&self, value: bool) {
        self.engine.send(Command::SetShuffle(value));
    }
    #[zbus(property)]
    fn metadata(&self) -> Metadata {
        metadata(self.playback().track.as_ref())
    }
    #[zbus(property)]
    fn volume(&self) -> f64 {
        f64::from(self.playback().volume)
    }
    #[zbus(property)]
    fn set_volume(&self, value: f64) {
        self.engine.send(Command::SetVolume(value.clamp(0.0, 1.0) as f32));
    }
    #[zbus(property(emits_changed_signal = "false"))]
    fn position(&self) -> i64 {
        i64::from(self.playback().position_now()) * 1000
    }
    #[zbus(property)]
    fn can_go_next(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn can_go_previous(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn can_play(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn can_pause(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn can_seek(&self) -> bool {
        self.playback().track.is_some()
    }
    #[zbus(property(emits_changed_signal = "const"))]
    fn can_control(&self) -> bool {
        true
    }
}

/// What clients were last told, to announce only what changed.
#[derive(PartialEq)]
struct Announced {
    status: &'static str,
    track: Option<Track>,
    shuffle: bool,
    repeat: Repeat,
    volume_percent: u32,
}

impl Announced {
    fn of(playback: &Playback) -> Self {
        Self {
            status: status(playback),
            track: playback.track.clone(),
            shuffle: playback.shuffle,
            repeat: playback.repeat,
            volume_percent: (playback.volume * 100.0).round() as u32,
        }
    }
}

/// Where the track would be now had it played on untouched from `playback`'s last report.
/// A seek, a pause or a new track moves it; ordinary playing doesn't.
fn timeline_origin_ms(playback: &Playback, started: std::time::Instant) -> i64 {
    let reported_at = playback.position_at.saturating_duration_since(started).as_millis() as i64;
    if playback.playing && !playback.loading { i64::from(playback.position_ms) - reported_at } else { i64::from(playback.position_ms) }
}

async fn serve(engine: Engine, raise: Hook, quit: Hook) -> zbus::Result<()> {
    let (changed, mut changes) = unbounded_channel();
    engine.subscribe(move || {
        // The receiver lives as long as this thread.
        let _sent = changed.send(());
    });
    let connection = zbus::connection::Builder::session()
        .name(BUS_NAME)
        .serve_at(OBJECT_PATH, Root { raise, quit })
        .serve_at(OBJECT_PATH, PlayerInterface { engine: engine.clone() })
        .build()
        .await?;
    let player = connection.object_server().interface::<_, PlayerInterface>(OBJECT_PATH).await?;
    let started = std::time::Instant::now();
    let mut playback = engine.read(|store| store.playback.clone());
    let mut announced = Announced::of(&playback);
    let mut origin = timeline_origin_ms(&playback, started);
    while changes.recv().await.is_some() {
        while changes.try_recv().is_ok() {}
        playback = engine.read(|store| store.playback.clone());
        let now = Announced::of(&playback);
        let emitter = player.signal_emitter();
        let interface = player.get().await;
        if now.status != announced.status {
            interface.playback_status_changed(emitter).await?;
        }
        let track_changed = now.track != announced.track;
        if track_changed {
            interface.metadata_changed(emitter).await?;
            interface.can_seek_changed(emitter).await?;
        }
        if now.shuffle != announced.shuffle {
            interface.shuffle_changed(emitter).await?;
        }
        if now.repeat != announced.repeat {
            interface.loop_status_changed(emitter).await?;
        }
        if now.volume_percent != announced.volume_percent {
            interface.volume_changed(emitter).await?;
        }
        let now_origin = timeline_origin_ms(&playback, started);
        let jumped = (now_origin - origin).abs() > 1500 && now.status == announced.status;
        if jumped && !track_changed {
            PlayerInterface::seeked(emitter, i64::from(playback.position_now()) * 1000).await?;
        }
        origin = now_origin;
        announced = now;
    }
    Ok(())
}

/// Publish the player on the session bus. `raise` shows the main window; `quit` ends the app.
pub fn start(engine: Engine, raise: impl Fn() + Send + Sync + 'static, quit: impl Fn() + Send + Sync + 'static) {
    let (raise, quit): (Hook, Hook) = (Arc::new(raise), Arc::new(quit));
    let spawned = std::thread::Builder::new().name("baliset-mpris".into()).spawn(move || {
        let runtime = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
            Ok(runtime) => runtime,
            Err(error) => {
                log::warn!("MPRIS is off: {error}");
                return;
            }
        };
        if let Err(error) = runtime.block_on(serve(engine, raise, quit)) {
            log::warn!("MPRIS is off: {error}");
        }
    });
    if let Err(error) = spawned {
        log::warn!("MPRIS is off: {error}");
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;
    use crate::model::NamedRef;

    fn track() -> Track {
        Track {
            uri: "spotify:track:4uLU6hMCjMI75M1A2tKUQC".into(),
            name: "Song".into(),
            artists: vec![NamedRef { uri: String::new(), name: "One".into() }, NamedRef { uri: String::new(), name: "Two".into() }],
            album: Some(NamedRef { uri: String::new(), name: "Record".into() }),
            duration_ms: 201_000,
            image_url: Some("https://i.scdn.co/image/x".into()),
            explicit: false,
            playable: true,
        }
    }

    #[test]
    fn metadata_carries_what_a_media_widget_shows() {
        let map = metadata(Some(&track()));
        let text = |key: &str| String::try_from(map[key].try_clone().unwrap()).unwrap();
        assert_eq!(text("xesam:title"), "Song");
        assert_eq!(text("xesam:album"), "Record");
        assert_eq!(text("mpris:artUrl"), "https://i.scdn.co/image/x");
        assert_eq!(i64::try_from(map["mpris:length"].try_clone().unwrap()).unwrap(), 201_000_000);
        assert_eq!(Vec::<String>::try_from(map["xesam:artist"].try_clone().unwrap()).unwrap(), ["One", "Two"]);
    }

    #[test]
    fn an_episode_shows_its_show_as_artist_and_album_and_its_full_length() {
        let show = NamedRef { uri: "spotify:show:01A062kejnXFkJE01bjN5J".into(), name: "The Standup".into() };
        let episode = Track {
            uri: "spotify:episode:0KhbTiRLZGEr1SlEVRcbKO".into(),
            name: "The Death of Stack Overflow".into(),
            artists: vec![show.clone()],
            album: Some(show),
            duration_ms: 10_947_603,
            ..track()
        };
        let map = metadata(Some(&episode));
        let text = |key: &str| String::try_from(map[key].try_clone().unwrap()).unwrap();
        assert_eq!(text("xesam:album"), "The Standup");
        assert_eq!(Vec::<String>::try_from(map["xesam:artist"].try_clone().unwrap()).unwrap(), ["The Standup"]);
        assert_eq!(text("xesam:url"), "https://open.spotify.com/episode/0KhbTiRLZGEr1SlEVRcbKO");
        assert_eq!(i64::try_from(map["mpris:length"].try_clone().unwrap()).unwrap(), 10_947_603_000);
        assert_eq!(track_path(Some(&episode)), "/org/baliset/track/0KhbTiRLZGEr1SlEVRcbKO");
    }

    #[test]
    fn nothing_playing_is_the_no_track_path() {
        assert_eq!(track_path(None), NO_TRACK);
        assert_eq!(track_path(Some(&track())), "/org/baliset/track/4uLU6hMCjMI75M1A2tKUQC");
        assert_eq!(metadata(None).len(), 1);
    }

    #[test]
    fn steady_playing_keeps_its_origin_and_a_seek_moves_it() {
        let started = Instant::now();
        let at = |position_ms: u32, after_ms: u64| Playback { playing: true, position_ms, position_at: started + Duration::from_millis(after_ms), ..Playback::default() };
        assert_eq!(timeline_origin_ms(&at(10_000, 0), started), timeline_origin_ms(&at(15_000, 5_000), started));
        assert_eq!(timeline_origin_ms(&at(60_000, 5_000), started) - timeline_origin_ms(&at(10_000, 0), started), 45_000);
    }
}
