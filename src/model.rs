//! Plain data shared by the engine and the UI. Nothing here knows about Rune or librespot.

use std::collections::{HashMap, HashSet};
use std::time::Instant;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NamedRef {
    pub uri: String,
    pub name: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Track {
    pub uri: String,
    pub name: String,
    pub artists: Vec<NamedRef>,
    pub album: Option<NamedRef>,
    pub duration_ms: u32,
    pub image_url: Option<String>,
    pub explicit: bool,
    pub playable: bool,
}

impl Track {
    pub fn artist_line(&self) -> String {
        self.artists.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", ")
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ContextKind {
    Playlist,
    Album,
    Artist,
    /// The user's Liked Songs.
    Liked,
    Show,
    #[default]
    Other,
}

/// Something that can be opened as a page and played as a whole: a playlist, an album, an
/// artist or Liked Songs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ContextSummary {
    pub uri: String,
    pub name: String,
    /// The owner of a playlist, the artists of an album, "Artist" for an artist.
    pub subtitle: String,
    pub image_url: Option<String>,
    pub kind: ContextKind,
}

/// A context's tracks, loaded on demand with [`Command::LoadContext`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ContextPage {
    pub summary: ContextSummary,
    pub tracks: Vec<Track>,
    /// For an artist page: their albums. Empty otherwise.
    pub albums: Vec<ContextSummary>,
    /// More tracks exist than were loaded.
    pub truncated: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Loadable<T> {
    #[default]
    Idle,
    Loading,
    Ready(T),
    Failed(String),
}

impl<T> Loadable<T> {
    pub fn ready(&self) -> Option<&T> {
        match self {
            Self::Ready(value) => Some(value),
            _ => None,
        }
    }

    pub fn ready_mut(&mut self) -> Option<&mut T> {
        match self {
            Self::Ready(value) => Some(value),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Repeat {
    #[default]
    Off,
    Context,
    Track,
}

#[derive(Clone, Debug)]
pub struct Playback {
    pub track: Option<Track>,
    pub playing: bool,
    /// A track is buffering; `playing` says what happens once it has.
    pub loading: bool,
    /// The position at `position_at`. While `playing`, the live position is this plus the
    /// time since; use [`Playback::position_now`].
    pub position_ms: u32,
    pub position_at: Instant,
    pub shuffle: bool,
    pub repeat: Repeat,
    /// 0.0 to 1.0.
    pub volume: f32,
    /// The playlist, album or artist being played, when known.
    pub context_uri: Option<String>,
    /// This device is the account's active Spotify Connect device.
    pub active: bool,
}

impl Default for Playback {
    fn default() -> Self {
        Self {
            track: None,
            playing: false,
            loading: false,
            position_ms: 0,
            position_at: Instant::now(),
            shuffle: false,
            repeat: Repeat::Off,
            volume: 1.0,
            context_uri: None,
            active: false,
        }
    }
}

impl Playback {
    /// Stop the clock where it is: not playing, with the position it had reached.
    pub fn halt(&mut self) {
        self.position_ms = self.position_now();
        self.position_at = Instant::now();
        self.playing = false;
        self.loading = false;
    }

    pub fn position_now(&self) -> u32 {
        let duration = self.track.as_ref().map_or(u32::MAX, |t| t.duration_ms);
        if !self.playing || self.loading {
            return self.position_ms.min(duration);
        }
        let elapsed = self.position_at.elapsed().as_millis().min(u128::from(u32::MAX)) as u32;
        self.position_ms.saturating_add(elapsed).min(duration)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum SessionState {
    /// Reading saved credentials.
    #[default]
    Starting,
    /// No usable credentials. After [`Command::Login`], `url` is the sign-in page the browser
    /// was sent to, for the UI to show and copy in case it didn't open.
    NeedsLogin { url: Option<String>, error: Option<String> },
    Connecting,
    Online { user: String },
    /// Credentials exist but the connection failed; the engine retries on its own.
    Offline { error: String },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchResults {
    pub tracks: Vec<Track>,
    pub albums: Vec<ContextSummary>,
    pub artists: Vec<ContextSummary>,
    pub playlists: Vec<ContextSummary>,
}

#[derive(Clone, Debug, Default)]
pub struct Search {
    /// The query `results` answer.
    pub query: String,
    pub results: Loadable<SearchResults>,
}

/// Everything the UI draws. The engine owns it; the UI reads it under [`Engine::read`].
///
/// [`Engine::read`]: crate::engine::Engine::read
#[derive(Clone, Debug, Default)]
pub struct Store {
    pub session: SessionState,
    pub playback: Playback,
    /// Recently played playlists, albums and artists, newest first, without duplicates.
    pub recents: Vec<ContextSummary>,
    pub playlists: Loadable<Vec<ContextSummary>>,
    pub albums: Loadable<Vec<ContextSummary>>,
    /// Pages by context URI. Liked Songs is [`LIKED_URI`].
    pub pages: HashMap<String, Loadable<ContextPage>>,
    pub search: Search,
    /// URIs of tracks known to be in Liked Songs.
    pub liked: HashSet<String>,
    /// The last thing that went wrong, for a toast. Cleared by [`Command::DismissNotice`].
    pub notice: Option<String>,
}

pub const LIKED_URI: &str = "spotify:collection";

/// What the UI asks of the engine. Every command returns at once; results arrive in the
/// [`Store`].
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    /// Start the browser sign-in.
    Login,
    Logout,
    PlayPause,
    Next,
    Previous,
    Seek { position_ms: u32 },
    /// 0.0 to 1.0.
    SetVolume(f32),
    SetShuffle(bool),
    SetRepeat(Repeat),
    /// Play a playlist, album, artist or Liked Songs from its start, or from `track_uri`.
    PlayContext { uri: String, track_uri: Option<String> },
    /// Play loose tracks (search results), starting at `index`.
    PlayTracks { uris: Vec<String>, index: usize },
    /// Fill `Store::pages[uri]`.
    LoadContext { uri: String },
    /// Refill playlists, albums and recents.
    #[allow(dead_code)]
    RefreshLibrary,
    /// Fill `Store::search`. An empty query clears it.
    Search { query: String },
    SetLiked { uri: String, liked: bool },
    DismissNotice,
}
