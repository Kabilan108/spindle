//! The real engine: a librespot session that shows up as a Spotify Connect device and plays
//! through the local sound card, plus the Web API for the library.
//!
//! One thread runs the session's lifetime as a loop: sign in if nothing is saved, connect,
//! serve commands and player events until the connection drops, then connect again.

mod auth;
mod catalog;
pub(super) mod check;
mod edits;
mod http;
mod library;
mod queue;
mod wire;

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use librespot_connect::{ConnectConfig, LoadContextOptions, LoadRequest, LoadRequestOptions, Options, PlayingTrack, Spirc};
use librespot_core::authentication::Credentials;
use librespot_core::cache::Cache;
use librespot_core::config::DeviceType;
use librespot_core::dealer::manager::BoxedStreamResult;
use librespot_core::dealer::protocol::Message;
use librespot_core::error::ErrorKind;
use librespot_core::{Session, SessionConfig};
use librespot_metadata::audio::{AudioItem, UniqueFields};
use librespot_playback::config::{AudioFormat, Bitrate, PlayerConfig};
use librespot_playback::mixer::{self, Mixer, MixerConfig};
use librespot_playback::player::{Player, PlayerEvent, PlayerEventChannel};
use librespot_playback::audio_backend;
use librespot_protocol::connect::ClusterUpdate;
use tokio::sync::Semaphore;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use self::catalog::Catalog;
use self::auth::Login;
use self::http::Http;
use self::library::Library;
use super::analysis::tap::{self, TapSink, TapSource};
use super::{Backend, Shared};
use crate::model::{Command, EPISODES_URI, LIKED_URI, Loadable, NamedRef, Playback, Repeat, Search, SessionState, Track};
use crate::settings::AudioQuality;

const AUDIO_CACHE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_IMAGE_BYTES: usize = 8 * 1024 * 1024;
const CONCURRENT_IMAGES: usize = 6;
const COVER_WIDTH: i32 = 300;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
const CLUSTER_UPDATES: &str = "hm://connect-state/v1/cluster";
/// How long librespot takes to publish a change to its queue, near enough.
const QUEUE_SETTLE: Duration = Duration::from_millis(700);

struct Dirs {
    data: PathBuf,
    cache: PathBuf,
}

impl Dirs {
    fn resolve() -> Self {
        let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
        let base = |var: &str, fallback: &str| std::env::var_os(var).map(PathBuf::from).filter(|p| p.is_absolute()).unwrap_or_else(|| home.join(fallback));
        let dirs = Self { data: base("XDG_DATA_HOME", ".local/share").join("baliset"), cache: base("XDG_CACHE_HOME", ".cache").join("baliset") };
        dirs.adopt("spindle");
        dirs
    }

    /// Carries the sign-in and cache over from the app's earlier name, once.
    fn adopt(&self, earlier: &str) {
        for dir in [&self.data, &self.cache] {
            let old = dir.with_file_name(earlier);
            if !dir.exists()
                && old.is_dir()
                && let Err(error) = std::fs::rename(&old, dir)
            {
                log::warn!("could not move {} to {}: {error}", old.display(), dir.display());
            }
        }
    }

    fn credentials(&self) -> PathBuf {
        self.data.join("credentials.json")
    }
}

struct Spotify {
    commands: UnboundedSender<Command>,
    runtime: tokio::runtime::Runtime,
    http: Arc<Http>,
    shared: Arc<Shared>,
    image_slots: Arc<Semaphore>,
}

impl Backend for Spotify {
    fn command(&self, command: Command) {
        // The session thread only ends with the process.
        let _sent = self.commands.send(command);
    }

    fn fetch_image(&self, url: String) {
        let (http, shared, slots) = (self.http.clone(), self.shared.clone(), self.image_slots.clone());
        self.runtime.spawn(async move {
            let _slot = slots.acquire().await;
            let bytes = match http.get_bytes(&url).await {
                Ok(bytes) if bytes.len() <= MAX_IMAGE_BYTES => Some(bytes.to_vec()),
                Ok(_) => None,
                Err(error) => {
                    log::debug!("no image at {url}: {error}");
                    None
                }
            };
            shared.image_arrived(&url, bytes);
        });
    }
}

pub(super) fn start(shared: Arc<Shared>) -> std::io::Result<Arc<dyn Backend>> {
    shared.analysis.set_source(Box::new(TapSource::new(tap::tap())));
    let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(2).thread_name("baliset-engine").enable_all().build()?;
    let (commands, receiver) = unbounded_channel();
    let http = Arc::new(Http::new());
    let (handle, session_shared, session_http) = (runtime.handle().clone(), shared.clone(), http.clone());
    // The Connect device's task isn't `Send`, so the session gets a thread of its own.
    std::thread::Builder::new().name("baliset-session".into()).spawn(move || {
        handle.block_on(run(session_shared, receiver, session_http));
    })?;
    Ok(Arc::new(Spotify { commands, runtime, http, shared, image_slots: Arc::new(Semaphore::new(CONCURRENT_IMAGES)) }))
}

/// The player and its volume control. They outlive a dropped connection, so a reconnect
/// doesn't reopen the sound card. The bitrate and loudness normalisation are the settings
/// at launch: librespot's player keeps its config for its lifetime.
struct Audio {
    player: Arc<Player>,
    mixer: Arc<dyn Mixer>,
}

impl Audio {
    fn open(session: &Session, cache: &Cache) -> Result<Self, String> {
        let open_mixer = mixer::find(None).ok_or("No volume control is built in")?;
        let mixer = open_mixer(MixerConfig::default()).map_err(|e| format!("Can't open the volume control: {e}"))?;
        mixer.set_volume(cache.volume().unwrap_or(u16::MAX / 10 * 7));
        let open_sink = audio_backend::find(None).ok_or("No audio output is built in")?;
        let chosen = crate::settings::at_launch();
        let bitrate = match chosen.audio_quality {
            AudioQuality::Low => Bitrate::Bitrate96,
            AudioQuality::Normal => Bitrate::Bitrate160,
            AudioQuality::VeryHigh => Bitrate::Bitrate320,
        };
        let config = PlayerConfig { bitrate, normalisation: chosen.normalize_volume, ..PlayerConfig::default() };
        let tap_mixer = mixer.clone();
        let player = Player::new(config, session.clone(), Box::new(mixer::NoOpVolume), move || {
            Box::new(TapSink::new(open_sink(None, AudioFormat::default()), tap::tap(), tap_mixer.get_soft_volume()))
        });
        Ok(Self { player, mixer })
    }
}

struct Connected {
    spirc: Spirc,
    device: Pin<Box<dyn Future<Output = ()>>>,
    events: PlayerEventChannel,
    cluster: BoxedStreamResult<ClusterUpdate>,
}

fn device_name() -> String {
    let host = std::fs::read_to_string("/etc/hostname").ok().map(|name| name.trim().to_owned()).filter(|name| !name.is_empty());
    host.map_or_else(|| "Baliset".to_owned(), |host| format!("Baliset ({host})"))
}

/// Hands `url` to the desktop's browser. The library path this process may carry is its own
/// business: a browser started with it can fail to load.
fn open_browser(url: &str) {
    let mut commands = open::commands(url);
    std::thread::spawn(move || {
        for command in &mut commands {
            match command.env_remove("LD_LIBRARY_PATH").stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status() {
                Ok(status) if status.success() => return,
                Ok(status) => log::warn!("{:?} could not open the sign-in page: {status}", command.get_program()),
                Err(error) => log::warn!("{:?} could not open the sign-in page: {error}", command.get_program()),
            }
        }
        log::error!("no way to open a browser; copy the sign-in link instead");
    });
}

fn device_id(path: &Path) -> String {
    if let Ok(saved) = std::fs::read_to_string(path) {
        let saved = saved.trim();
        if saved.len() >= 32 {
            return saved.to_owned();
        }
    }
    let fresh = SessionConfig::default().device_id;
    if let Err(error) = std::fs::write(path, &fresh) {
        log::warn!("can't save the device id: {error}");
    }
    fresh
}

/// Create a directory only its owner can enter, tightening it if it already exists.
fn private_dir(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    std::fs::create_dir_all(path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
}

/// Make a file readable by its owner alone. librespot writes credentials with the umask's
/// permissions; the directory already keeps others out, and this covers a copied file.
fn restrict(path: &Path) {
    use std::os::unix::fs::PermissionsExt;

    if path.exists()
        && let Err(error) = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
    {
        log::warn!("can't restrict {}: {error}", path.display());
    }
}

async fn connect(session: &Session, credentials: Credentials, audio: &mut Option<Audio>, cache: &Cache) -> Result<Connected, librespot_core::Error> {
    // Listeners have to be registered before the session connects.
    let cluster = session.dealer().listen_for(CLUSTER_UPDATES, Message::from_raw::<ClusterUpdate>)?;
    let audio = match audio {
        Some(audio) => {
            audio.player.set_session(session.clone());
            audio
        }
        None => audio.insert(Audio::open(session, cache).map_err(librespot_core::Error::unavailable)?),
    };
    let events = audio.player.get_player_event_channel();
    let config = ConnectConfig { name: device_name(), device_type: DeviceType::Computer, initial_volume: audio.mixer.volume(), ..ConnectConfig::default() };
    let (spirc, device) = Spirc::new(config, session.clone(), credentials, audio.player.clone(), audio.mixer.clone()).await?;
    Ok(Connected { spirc, device: Box::pin(device), events, cluster })
}

/// The saved sign-in itself was refused, as opposed to the network being down.
fn refused(error: &librespot_core::Error) -> bool {
    matches!(error.kind, ErrorKind::Unauthenticated | ErrorKind::PermissionDenied)
}

enum Ended {
    /// The UI is gone.
    Closed,
    Logout,
    Dropped,
}

async fn run(shared: Arc<Shared>, mut commands: UnboundedReceiver<Command>, http: Arc<Http>) {
    let dirs = Dirs::resolve();
    if let Err(error) = private_dir(&dirs.data) {
        log::error!("can't prepare {}: {error}", dirs.data.display());
    }
    restrict(&dirs.credentials());
    let library = Arc::new(Library::new(shared.clone(), dirs.data.join("recents.json")));
    library.restore_recents();
    let cache = match Cache::new(Some(dirs.data.clone()), Some(dirs.data.clone()), Some(dirs.cache.join("audio")), Some(AUDIO_CACHE_BYTES)) {
        Ok(cache) => cache,
        Err(error) => {
            shared.update(|store| store.session = SessionState::Offline { error: format!("Can't use {}: {error}", dirs.data.display()) });
            return;
        }
    };
    let device_id = device_id(&dirs.data.join("device_id"));
    let mut audio = None;
    let mut sign_in_error = None;
    let mut retry_in = Duration::from_secs(2);

    loop {
        let credentials = match cache.credentials() {
            Some(saved) => saved,
            None => match sign_in(&shared, &mut commands, &http, sign_in_error.take()).await {
                Some(access_token) => Credentials::with_access_token(access_token),
                None => return,
            },
        };
        shared.update(|store| store.session = SessionState::Connecting);
        let session = Session::new(SessionConfig { device_id: device_id.clone(), ..SessionConfig::default() }, Some(cache.clone()));
        let attempt = tokio::time::timeout(CONNECT_TIMEOUT, connect(&session, credentials, &mut audio, &cache)).await;
        let connected = match attempt.unwrap_or_else(|_| Err(librespot_core::Error::deadline_exceeded("Spotify didn't answer in time"))) {
            Ok(connected) => connected,
            Err(error) if refused(&error) => {
                log::warn!("sign-in refused: {error}");
                forget_sign_in(&dirs);
                sign_in_error = Some("Spotify didn't accept the saved sign-in. Sign in again.".to_owned());
                continue;
            }
            Err(error) => {
                log::warn!("can't connect: {error}");
                shared.update(|store| store.session = SessionState::Offline { error: error.to_string() });
                match idle(&shared, &mut commands, retry_in).await {
                    Idle::Elapsed => retry_in = (retry_in * 2).min(Duration::from_secs(60)),
                    Idle::Logout => sign_out(&dirs, &shared, &library),
                    Idle::Closed => return,
                }
                continue;
            }
        };
        retry_in = Duration::from_secs(2);
        restrict(&dirs.credentials());
        library.set_catalog(Some(Catalog::new(session.clone())));
        let volume = audio.as_ref().map(|audio| f32::from(audio.mixer.volume()) / f32::from(u16::MAX));
        shared.update(|store| {
            store.session = SessionState::Online { user: session.username() };
            if let Some(volume) = volume {
                store.playback.volume = volume;
            }
        });
        tokio::spawn(library.clone().refresh());

        let online = Online {
            shared: &shared,
            library: &library,
            session: &session,
            mixer: audio.as_ref().map(|audio| audio.mixer.clone()),
            player: audio.as_ref().map(|audio| audio.player.clone()),
            queue: queue::Watch::new(library.clone(), session.clone(), shared.clone()),
        };
        let ended = online.serve(&mut commands, connected).await;
        library.clear_up_next();
        library.set_catalog(None);
        session.shutdown();
        match ended {
            Ended::Closed => return,
            Ended::Logout => sign_out(&dirs, &shared, &library),
            Ended::Dropped => {
                shared.update(|store| {
                    store.session = SessionState::Offline { error: "Connection lost. Reconnecting.".to_owned() };
                    store.playback.active = false;
                    store.playback.halt();
                });
                match idle(&shared, &mut commands, Duration::from_secs(1)).await {
                    Idle::Elapsed => {}
                    Idle::Logout => sign_out(&dirs, &shared, &library),
                    Idle::Closed => return,
                }
            }
        }
    }
}

/// Sign out: drop the saved sign-in and everything the account had loaded.
fn sign_out(dirs: &Dirs, shared: &Shared, library: &Library) {
    forget_sign_in(dirs);
    library.forget_account();
    shared.update(|store| {
        let volume = store.playback.volume;
        *store = crate::model::Store { playback: Playback { volume, ..Playback::default() }, session: SessionState::NeedsLogin { url: None, error: None }, ..Default::default() };
    });
}

fn forget_sign_in(dirs: &Dirs) {
    if let Err(error) = std::fs::remove_file(dirs.credentials()) {
        log::debug!("no saved credentials to remove: {error}");
    }
}

/// Answer a command without a connection: what can't be done says so in the store, so no
/// page or search is left waiting for an answer that will never come.
fn offline_command(shared: &Shared, command: Command) {
    const OFFLINE: &str = "Not connected to Spotify";
    shared.update(|store| match command {
        Command::DismissNotice => store.notice = None,
        Command::LoadContext { uri } => {
            if store.pages.get(&uri).and_then(Loadable::ready).is_none() {
                store.pages.insert(uri, Loadable::Failed(OFFLINE.to_owned()));
            }
        }
        Command::Search { query } if !query.trim().is_empty() => store.search = Search { query, results: Loadable::Failed(OFFLINE.to_owned()) },
        Command::Search { .. } => store.search = Search::default(),
        _ => {}
    });
}

enum Idle {
    Elapsed,
    Logout,
    /// The UI is gone.
    Closed,
}

/// Wait out `pause`, answering what can be answered offline.
async fn idle(shared: &Shared, commands: &mut UnboundedReceiver<Command>, pause: Duration) -> Idle {
    let wake = tokio::time::sleep(pause);
    tokio::pin!(wake);
    loop {
        tokio::select! {
            () = &mut wake => return Idle::Elapsed,
            command = commands.recv() => match command {
                Some(Command::Logout) => return Idle::Logout,
                Some(command) => offline_command(shared, command),
                None => return Idle::Closed,
            },
        }
    }
}

/// Show the sign-in screen and see one sign-in through. `None` when the UI is gone.
async fn sign_in(shared: &Shared, commands: &mut UnboundedReceiver<Command>, http: &Http, mut error: Option<String>) -> Option<String> {
    let mut asked = false;
    loop {
        if !asked {
            shared.update(|store| store.session = SessionState::NeedsLogin { url: None, error: error.take() });
            loop {
                match commands.recv().await? {
                    Command::Login => break,
                    other => offline_command(shared, other),
                }
            }
        }
        asked = false;
        let login = match Login::begin().await {
            Ok(login) => login,
            Err(failure) => {
                error = Some(failure);
                continue;
            }
        };
        log::info!("sign in at {}", login.url);
        shared.update(|store| store.session = SessionState::NeedsLogin { url: Some(login.url.clone()), error: None });
        open_browser(&login.url);
        let finish = login.finish(http);
        tokio::pin!(finish);
        loop {
            tokio::select! {
                outcome = &mut finish => match outcome {
                    Ok(access_token) => return Some(access_token),
                    Err(failure) => {
                        error = Some(failure);
                        break;
                    }
                },
                command = commands.recv() => match command? {
                    // Pressed again: start over with a fresh page.
                    Command::Login => {
                        asked = true;
                        break;
                    }
                    other => offline_command(shared, other),
                },
            }
        }
    }
}

fn track_of(item: &AudioItem) -> Track {
    let (artists, album) = match &item.unique_fields {
        UniqueFields::Track { artists, album, .. } => {
            let artists = artists.iter().map(|artist| NamedRef { uri: artist.id.to_uri().unwrap_or_default(), name: artist.name.clone() }).collect();
            (artists, Some(NamedRef { uri: String::new(), name: album.clone() }))
        }
        UniqueFields::Local { artists, album, .. } => {
            let artists = artists.iter().map(|name| NamedRef { uri: String::new(), name: name.clone() }).collect();
            (artists, album.clone().map(|name| NamedRef { uri: String::new(), name }))
        }
        UniqueFields::Episode { show_name, .. } => (vec![NamedRef { uri: String::new(), name: show_name.clone() }], None),
    };
    let cover = item.covers.iter().min_by_key(|cover| (cover.width - COVER_WIDTH).abs());
    Track {
        uri: item.uri.clone(),
        name: item.name.clone(),
        artists,
        album,
        duration_ms: item.duration_ms,
        image_url: cover.map(|cover| cover.url.clone()),
        explicit: item.is_explicit,
        playable: true,
    }
}

struct Online<'a> {
    shared: &'a Arc<Shared>,
    library: &'a Arc<Library>,
    session: &'a Session,
    mixer: Option<Arc<dyn Mixer>>,
    player: Option<Arc<Player>>,
    queue: Arc<queue::Watch>,
}

impl Online<'_> {
    async fn serve(&self, commands: &mut UnboundedReceiver<Command>, connected: Connected) -> Ended {
        let Connected { spirc, mut device, mut events, mut cluster } = connected;
        loop {
            tokio::select! {
                command = commands.recv() => match command {
                    None => {
                        self.stop(&spirc, device).await;
                        return Ended::Closed;
                    }
                    Some(Command::Logout) => {
                        self.stop(&spirc, device).await;
                        return Ended::Logout;
                    }
                    Some(command) => self.command(&spirc, command),
                },
                event = events.recv() => match event {
                    Some(event) => self.player_event(event),
                    None => return Ended::Dropped,
                },
                Some(update) = cluster.next() => match update {
                    Ok(update) => self.cluster_update(&update),
                    Err(error) => log::debug!("unreadable device update: {error}"),
                },
                () = &mut device => return Ended::Dropped,
            }
        }
    }

    /// Silence the player and give the device a moment to tell Spotify it's leaving.
    async fn stop(&self, spirc: &Spirc, device: Pin<Box<dyn Future<Output = ()>>>) {
        self.note_episode_position();
        if let Some(player) = &self.player {
            player.stop();
        }
        match spirc.shutdown() {
            Ok(()) => {
                if tokio::time::timeout(Duration::from_secs(2), device).await.is_err() {
                    log::debug!("the device didn't finish shutting down in time");
                }
            }
            Err(error) => log::debug!("the device had already stopped: {error}"),
        }
    }

    fn position(&self, position_ms: u32) {
        self.shared.update(|store| {
            store.playback.position_ms = position_ms;
            store.playback.position_at = Instant::now();
        });
    }

    /// An episode playing here keeps the place it reached, for the next time it's played.
    fn note_episode_position(&self) {
        let reached = self.shared.read(|store| store.playback.track.as_ref().map(|t| (t.uri.clone(), store.playback.position_now(), t.duration_ms)));
        if let Some((uri, position_ms, duration_ms)) = reached {
            self.library.episode_left_at(&uri, position_ms, duration_ms);
        }
    }

    fn player_event(&self, event: PlayerEvent) {
        match event {
            PlayerEvent::TrackChanged { audio_item } => {
                self.note_episode_position();
                let track = track_of(&audio_item);
                let uri = track.uri.clone();
                self.shared.update(|store| {
                    store.playback.track = Some(track);
                    store.playback.position_ms = 0;
                    store.playback.position_at = Instant::now();
                });
                tokio::spawn(self.library.clone().describe_playing(uri));
                self.queue.reread(QUEUE_SETTLE);
            }
            PlayerEvent::Loading { position_ms, .. } => self.shared.update(|store| {
                store.playback.loading = true;
                store.playback.position_ms = position_ms;
                store.playback.position_at = Instant::now();
            }),
            PlayerEvent::Playing { position_ms, .. } => self.shared.update(|store| {
                store.playback.playing = true;
                store.playback.loading = false;
                store.playback.active = true;
                store.playback.position_ms = position_ms;
                store.playback.position_at = Instant::now();
            }),
            PlayerEvent::Paused { position_ms, .. } => {
                self.shared.update(|store| {
                    store.playback.playing = false;
                    store.playback.loading = false;
                    store.playback.position_ms = position_ms;
                    store.playback.position_at = Instant::now();
                });
                self.note_episode_position();
            }
            PlayerEvent::EndOfTrack { track_id, .. } => {
                let uri = track_id.to_uri().unwrap_or_default();
                let duration = self.shared.read(|store| store.playback.track.as_ref().filter(|t| t.uri == uri).map(|t| t.duration_ms));
                if let Some(duration_ms) = duration {
                    self.library.episode_left_at(&uri, duration_ms, duration_ms);
                }
            }
            PlayerEvent::Seeked { position_ms, .. } | PlayerEvent::PositionCorrection { position_ms, .. } => self.position(position_ms),
            PlayerEvent::Stopped { .. } => {
                self.note_episode_position();
                self.shared.update(|store| store.playback.halt());
            }
            PlayerEvent::Unavailable { .. } => self.shared.update(|store| {
                store.playback.loading = false;
                store.notice = Some("That track can't be played here.".to_owned());
            }),
            PlayerEvent::VolumeChanged { volume } => self.shared.update(|store| store.playback.volume = f32::from(volume) / f32::from(u16::MAX)),
            PlayerEvent::ShuffleChanged { shuffle } => {
                self.shared.update(|store| store.playback.shuffle = shuffle);
                self.queue.reread(QUEUE_SETTLE);
            }
            PlayerEvent::RepeatChanged { context, track } => self.shared.update(|store| {
                store.playback.repeat = match (track, context) {
                    (true, _) => Repeat::Track,
                    (false, true) => Repeat::Context,
                    (false, false) => Repeat::Off,
                };
            }),
            PlayerEvent::SessionConnected { .. } => self.shared.update(|store| store.playback.active = true),
            PlayerEvent::SessionDisconnected { .. } => {
                self.note_episode_position();
                self.shared.update(|store| {
                    store.playback.active = false;
                    store.playback.halt();
                });
            }
            _ => {}
        }
    }

    /// The account's devices changed: note whether this one is playing, and what.
    fn cluster_update(&self, update: &ClusterUpdate) {
        let cluster = &update.cluster;
        if cluster.active_device_id != self.session.device_id() {
            let was_active = self.shared.read(|store| store.playback.active);
            if was_active {
                self.note_episode_position();
                self.shared.update(|store| {
                    store.playback.active = false;
                    store.playback.halt();
                });
            }
            self.queue.clear();
            return;
        }
        self.queue.report(&cluster.player_state);
        let context = library::normalize_context(&cluster.player_state.context_uri);
        // A cluster sent while a load is still settling names no context; what was asked for
        // stands until one does.
        let context = Some(context).filter(|uri| !uri.is_empty()).or_else(|| self.shared.read(|store| store.playback.context_uri.clone()));
        let changed = self.shared.read(|store| store.playback.context_uri != context || !store.playback.active);
        if !changed {
            return;
        }
        self.shared.update(|store| {
            store.playback.active = true;
            store.playback.context_uri.clone_from(&context);
        });
        if let Some(uri) = context {
            tokio::spawn(self.library.clone().note_played(uri));
        }
    }

    /// Hand a player command to this device through Connect, in the background, then read
    /// the queue back.
    fn send_to_self(&self, body: serde_json::Value, failure: &'static str) {
        let (session, shared, watch) = (self.session.clone(), self.shared.clone(), self.queue.clone());
        tokio::spawn(async move {
            match queue::send(&session, &body).await {
                Ok(()) => watch.reread(QUEUE_SETTLE),
                Err(error) => {
                    log::warn!("{failure}: {error}");
                    shared.update(|store| store.notice = Some(format!("{failure}: {error}")));
                }
            }
        });
    }

    fn play_context(&self, spirc: &Spirc, uri: String, track_uri: Option<String>, options: Options, active: bool) -> Result<(), librespot_core::Error> {
        let context = match uri.as_str() {
            LIKED_URI => format!("spotify:user:{}:collection", self.session.username()),
            EPISODES_URI => format!("spotify:user:{}:collection:your-episodes", self.session.username()),
            _ => uri.clone(),
        };
        // librespot reports no shuffle change for a load that sets it; the queue's read-back
        // confirms it.
        let shuffle = options.shuffle;
        let options = LoadRequestOptions {
            start_playing: true,
            seek_to: track_uri.as_deref().map_or(0, |track| self.library.resume_at(track)),
            context_options: Some(LoadContextOptions::Options(options)),
            playing_track: track_uri.map(PlayingTrack::Uri),
        };
        self.shared.update(|store| {
            store.playback.context_uri = Some(uri.clone());
            store.playback.loading = true;
            store.playback.shuffle = shuffle;
        });
        tokio::spawn(self.library.clone().note_played(uri));
        self.activate(spirc, active).and_then(|()| spirc.load(LoadRequest::from_context_uri(context, options)))
    }

    fn command(&self, spirc: &Spirc, command: Command) {
        let (active, shuffle, repeat) = self.shared.read(|store| (store.playback.active, store.playback.shuffle, store.playback.repeat));
        let sent = match command {
            Command::Login | Command::Logout => Ok(()),
            // Not the active device yet: pull over whatever the account was playing.
            Command::PlayPause if !active => spirc.transfer(None),
            Command::PlayPause => spirc.play_pause(),
            Command::Next => spirc.next(),
            Command::Previous => spirc.prev(),
            Command::Seek { position_ms } => spirc.set_position_ms(position_ms),
            Command::SetVolume(volume) => {
                let volume = volume.clamp(0.0, 1.0);
                let steps = (volume * f32::from(u16::MAX)) as u16;
                if active {
                    spirc.set_volume(steps)
                } else {
                    if let Some(mixer) = &self.mixer {
                        mixer.set_volume(steps);
                    }
                    self.shared.update(|store| store.playback.volume = volume);
                    Ok(())
                }
            }
            Command::SetShuffle(shuffle) => spirc.shuffle(shuffle),
            Command::SetRepeat(repeat) => spirc.repeat(repeat != Repeat::Off).and_then(|()| spirc.repeat_track(repeat == Repeat::Track)),
            Command::PlayContext { uri, track_uri } => {
                let options = Options { shuffle, repeat: repeat != Repeat::Off, repeat_track: repeat == Repeat::Track };
                self.play_context(spirc, uri, track_uri, options, active)
            }
            Command::PlayTracks { uris, index } => {
                let options = LoadRequestOptions {
                    start_playing: true,
                    seek_to: uris.get(index).map_or(0, |track| self.library.resume_at(track)),
                    context_options: None,
                    playing_track: Some(PlayingTrack::Index(u32::try_from(index).unwrap_or(0))),
                };
                self.shared.update(|store| {
                    store.playback.context_uri = None;
                    store.playback.loading = true;
                });
                self.activate(spirc, active).and_then(|()| spirc.load(LoadRequest::from_tracks(uris, options)))
            }
            Command::LoadContext { uri } => {
                tokio::spawn(self.library.clone().load_context(uri));
                Ok(())
            }
            Command::RefreshLibrary => {
                tokio::spawn(self.library.clone().refresh());
                Ok(())
            }
            Command::Search { query } => {
                tokio::spawn(self.library.clone().search(query));
                Ok(())
            }
            Command::SetLiked { uri, liked } => {
                tokio::spawn(self.library.clone().set_liked(uri, liked));
                Ok(())
            }
            Command::DismissNotice => {
                self.shared.update(|store| store.notice = None);
                Ok(())
            }
            Command::SetFollowed { uri, followed } => {
                tokio::spawn(self.library.clone().set_followed(uri, followed));
                Ok(())
            }
            Command::AddToQueue { uri } => {
                if active {
                    self.send_to_self(queue::add_to_queue(&uri), "Couldn't add to the queue");
                } else {
                    self.shared.update(|store| store.notice = Some("Play something here first, then add to the queue.".to_owned()));
                }
                Ok(())
            }
            Command::SkipTo { uri } => {
                // librespot skips until it reaches the track, so it has to be there.
                if self.queue.snapshot().plays_next(&uri) {
                    self.send_to_self(queue::skip_to(&uri), "Couldn't skip to that song");
                }
                Ok(())
            }
            Command::RemoveFromQueue { index } => {
                let body = {
                    let snapshot = self.queue.snapshot();
                    snapshot.without_queued(index).map(|next| queue::set_queue(&next, &snapshot))
                };
                if let Some(body) = body {
                    self.send_to_self(body, "Couldn't change the queue");
                }
                Ok(())
            }
            Command::LoadRadio { track_uri } => {
                tokio::spawn(self.library.clone().load_radio(track_uri));
                Ok(())
            }
            Command::AddToPlaylist { playlist_uri, track_uri } => {
                tokio::spawn(self.library.clone().add_to_playlist(playlist_uri, track_uri));
                Ok(())
            }
            Command::RemoveFromPlaylist { playlist_uri, track_uri, index } => {
                tokio::spawn(self.library.clone().remove_from_playlist(playlist_uri, track_uri, index));
                Ok(())
            }
            Command::PlayShuffled { uri } => {
                let options = Options { shuffle: true, repeat: repeat != Repeat::Off, repeat_track: repeat == Repeat::Track };
                self.play_context(spirc, uri, None, options, active)
            }
        };
        if let Err(error) = sent {
            log::warn!("the device didn't take a command: {error}");
        }
    }

    fn activate(&self, spirc: &Spirc, active: bool) -> Result<(), librespot_core::Error> {
        if active { Ok(()) } else { spirc.activate() }
    }
}
