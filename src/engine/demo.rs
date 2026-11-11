//! Canned data and a pretend player. Nothing here touches the network or the sound card.

use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use super::{Backend, Shared};
use crate::model::{
    Command, ContextKind, ContextPage, ContextSummary, LIKED_URI, Loadable, NamedRef, Search, SearchResults,
    SessionState, Store, Track,
};

const ALBUMS: &[(&str, &str, &[&str])] = &[
    ("Low Tide Radio", "Marlowe & The Estuary", &["Harbour Lights", "Saltwater Static", "Slack Water", "Tin Roof Rain", "Nightferry", "Driftwood Choir", "Last Buoy Out"]),
    ("Paper Lanterns", "Ines Calder", &["Paper Lanterns", "Borrowed Coat", "Seventeen Stairs", "Good Glass", "Small Hours, Big Sky", "Hollow Reed"]),
    ("Concrete Garden", "Okonkwo Sound System", &["Rebar Bloom", "Third Mainland", "Danfo Dub", "Generator Hum", "Harmattan", "Lagoon Front", "Up NEPA", "Sunday Jollof"]),
    ("Static Bloom", "Velvet Antenna", &["Carrier Wave", "Static Bloom", "Ghost Channel", "Dial Tone Lullaby", "Shortwave Heart"]),
    ("A Field Guide to Getting Lost", "The Cartographers", &["Trailhead", "Switchback", "False Summit", "Cairn", "Treeline", "Whiteout", "Descent"]),
    ("Slow Machines", "Hara Mio", &["Warm Up", "Flywheel", "Idle Speed", "Slow Machines", "Cooling Fan", "Power Down"]),
];

const PLAYLISTS: &[(&str, &str, &[usize])] = &[
    ("Deep Focus Without The Piano", "kabilan", &[0, 3, 5]),
    ("Sunday Cooking", "kabilan", &[2, 1]),
    ("Late Night Drive", "kabilan", &[3, 0, 4]),
    ("Gym, Reluctantly", "kabilan", &[2, 5]),
    ("Songs My Sister Sent", "adaeze", &[1, 4, 0]),
    ("Discover Weekly", "Spotify", &[4, 5, 1, 2]),
    ("Rainy Afternoon Jazz-ish", "kabilan", &[5, 1]),
];

fn album_uri(index: usize) -> String {
    format!("spotify:album:demo{index}")
}

fn image_url(seed: usize) -> String {
    format!("demo://cover/{seed}")
}

fn album_summary(index: usize) -> ContextSummary {
    let (name, artist, _) = ALBUMS[index];
    ContextSummary {
        uri: album_uri(index),
        name: name.into(),
        subtitle: artist.into(),
        image_url: Some(image_url(index)),
        kind: ContextKind::Album,
    }
}

fn album_tracks(index: usize) -> Vec<Track> {
    let (name, artist, titles) = ALBUMS[index];
    titles
        .iter()
        .enumerate()
        .map(|(n, title)| Track {
            uri: format!("spotify:track:demo{index}x{n}"),
            name: (*title).into(),
            artists: vec![NamedRef { uri: format!("spotify:artist:demo{index}"), name: artist.into() }],
            album: Some(NamedRef { uri: album_uri(index), name: name.into() }),
            duration_ms: 150_000 + ((index * 37 + n * 53) % 140) as u32 * 1000,
            image_url: Some(image_url(index)),
            explicit: (index + n).is_multiple_of(7),
            playable: true,
        })
        .collect()
}

fn playlist_summary(index: usize) -> ContextSummary {
    let (name, owner, albums) = PLAYLISTS[index];
    ContextSummary {
        uri: format!("spotify:playlist:demo{index}"),
        name: name.into(),
        subtitle: owner.into(),
        image_url: Some(image_url(albums[0] + 10)),
        kind: ContextKind::Playlist,
    }
}

fn playlist_tracks(index: usize) -> Vec<Track> {
    let albums = PLAYLISTS[index].2;
    let mut tracks: Vec<Track> = Vec::new();
    for round in 0..4 {
        for &album in albums {
            tracks.extend(album_tracks(album).into_iter().skip(round).step_by(4));
        }
    }
    tracks
}

fn liked_summary() -> ContextSummary {
    ContextSummary { uri: LIKED_URI.into(), name: "Liked Songs".into(), subtitle: "kabilan".into(), image_url: None, kind: ContextKind::Liked }
}

fn liked_tracks() -> Vec<Track> {
    (0..ALBUMS.len()).flat_map(|album| album_tracks(album).into_iter().step_by(2)).collect()
}

fn artist_summary(index: usize) -> ContextSummary {
    ContextSummary {
        uri: format!("spotify:artist:demo{index}"),
        name: ALBUMS[index].1.into(),
        subtitle: "Artist".into(),
        image_url: Some(image_url(index + 20)),
        kind: ContextKind::Artist,
    }
}

fn page(uri: &str) -> Option<ContextPage> {
    let index = |prefix: &str, len: usize| uri.strip_prefix(prefix)?.parse::<usize>().ok().filter(|i| *i < len);
    if uri == LIKED_URI {
        return Some(ContextPage { summary: liked_summary(), tracks: liked_tracks(), albums: Vec::new(), truncated: false });
    }
    if let Some(i) = index("spotify:album:demo", ALBUMS.len()) {
        return Some(ContextPage { summary: album_summary(i), tracks: album_tracks(i), albums: Vec::new(), truncated: false });
    }
    if let Some(i) = index("spotify:playlist:demo", PLAYLISTS.len()) {
        return Some(ContextPage { summary: playlist_summary(i), tracks: playlist_tracks(i), albums: Vec::new(), truncated: false });
    }
    let i = index("spotify:artist:demo", ALBUMS.len())?;
    Some(ContextPage { summary: artist_summary(i), tracks: album_tracks(i).into_iter().take(5).collect(), albums: vec![album_summary(i)], truncated: false })
}

fn search(query: &str) -> SearchResults {
    let needle = query.to_lowercase();
    let hit = |text: &str| text.to_lowercase().contains(&needle);
    let all = 0..ALBUMS.len();
    SearchResults {
        tracks: all.clone().flat_map(album_tracks).filter(|t| hit(&t.name) || hit(&t.artist_line())).take(20).collect(),
        albums: all.clone().map(album_summary).filter(|a| hit(&a.name) || hit(&a.subtitle)).collect(),
        artists: all.map(artist_summary).filter(|a| hit(&a.name)).collect(),
        playlists: (0..PLAYLISTS.len()).map(playlist_summary).filter(|p| hit(&p.name)).collect(),
    }
}

/// A two-colour diagonal PNG, different for every seed.
fn cover(seed: usize) -> Vec<u8> {
    const SIZE: u32 = 96;
    let channel = |salt: usize| ((seed * 67 + salt * 101) % 200 + 40) as f32;
    let (a, b) = ([channel(1), channel(2), channel(3)], [channel(4), channel(5), channel(6)]);
    let mut pixels = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let t = (x + y) as f32 / (2 * SIZE - 2) as f32;
            for c in 0..3 {
                pixels.push((a[c] + (b[c] - a[c]) * t) as u8);
            }
            pixels.push(255);
        }
    }
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, SIZE, SIZE);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let written = encoder.write_header().and_then(|mut writer| writer.write_image_data(&pixels));
    if written.is_err() {
        return Vec::new();
    }
    out
}

/// The pretend player's queue: the tracks of the playing context and where it is in them.
#[derive(Default)]
struct Queue {
    tracks: Vec<Track>,
    index: usize,
}

struct Demo {
    commands: Mutex<mpsc::Sender<Command>>,
    shared: Arc<Shared>,
}

impl Backend for Demo {
    fn command(&self, command: Command) {
        let sender = self.commands.lock().unwrap_or_else(PoisonError::into_inner);
        // The player thread only ends with the process.
        let _sent = sender.send(command);
    }

    fn fetch_image(&self, url: String) {
        let seed = url.rsplit('/').next().and_then(|s| s.parse().ok()).unwrap_or(0);
        self.shared.image_arrived(&url, Some(cover(seed)));
    }
}

pub(super) fn start(shared: Arc<Shared>) -> Arc<dyn Backend> {
    let mut queue = Queue { tracks: playlist_tracks(0), index: 2 };
    shared.update(|store| {
        *store = Store {
            session: SessionState::Online { user: "kabilan".into() },
            recents: vec![playlist_summary(0), album_summary(2), playlist_summary(2), album_summary(0), artist_summary(3), playlist_summary(5)],
            playlists: Loadable::Ready((0..PLAYLISTS.len()).map(playlist_summary).collect()),
            albums: Loadable::Ready((0..ALBUMS.len()).map(album_summary).collect()),
            liked: liked_tracks().into_iter().map(|t| t.uri).collect(),
            ..Store::default()
        };
        store.playback.track = queue.tracks.get(queue.index).cloned();
        store.playback.context_uri = Some(playlist_summary(0).uri);
        store.playback.position_ms = 47_000;
        store.playback.volume = 0.7;
        store.playback.active = true;
    });

    let (sender, receiver) = mpsc::channel();
    let player = shared.clone();
    std::thread::spawn(move || {
        loop {
            match receiver.recv_timeout(Duration::from_millis(500)) {
                Ok(command) => player.update(|store| apply(store, &mut queue, command)),
                Err(RecvTimeoutError::Timeout) => {
                    let ended = player.read(|store| {
                        let playback = &store.playback;
                        playback.playing && playback.track.as_ref().is_some_and(|t| playback.position_now() >= t.duration_ms)
                    });
                    if ended {
                        player.update(|store| skip(store, &mut queue, 1));
                    }
                }
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
    });
    Arc::new(Demo { commands: Mutex::new(sender), shared })
}

fn seek(store: &mut Store, position_ms: u32) {
    store.playback.position_ms = position_ms;
    store.playback.position_at = Instant::now();
}

fn skip(store: &mut Store, queue: &mut Queue, by: isize) {
    if queue.tracks.is_empty() {
        return;
    }
    queue.index = (queue.index as isize + by).rem_euclid(queue.tracks.len() as isize) as usize;
    store.playback.track = queue.tracks.get(queue.index).cloned();
    seek(store, 0);
}

fn start_queue(store: &mut Store, queue: &mut Queue, tracks: Vec<Track>, index: usize, context: Option<String>) {
    *queue = Queue { index: index.min(tracks.len().saturating_sub(1)), tracks };
    store.playback.track = queue.tracks.get(queue.index).cloned();
    store.playback.context_uri = context;
    store.playback.playing = true;
    seek(store, 0);
}

fn apply(store: &mut Store, queue: &mut Queue, command: Command) {
    match command {
        Command::Login => store.session = SessionState::Online { user: "kabilan".into() },
        Command::Logout => store.session = SessionState::NeedsLogin { url: None, error: None },
        Command::PlayPause => {
            let position = store.playback.position_now();
            seek(store, position);
            store.playback.playing = !store.playback.playing;
        }
        Command::Next => skip(store, queue, 1),
        Command::Previous => {
            if store.playback.position_now() > 3000 {
                seek(store, 0);
            } else {
                skip(store, queue, -1);
            }
        }
        Command::Seek { position_ms } => seek(store, position_ms),
        Command::SetVolume(volume) => store.playback.volume = volume.clamp(0.0, 1.0),
        Command::SetShuffle(shuffle) => store.playback.shuffle = shuffle,
        Command::SetRepeat(repeat) => store.playback.repeat = repeat,
        Command::PlayContext { uri, track_uri } => {
            let Some(page) = page(&uri) else {
                store.notice = Some(format!("Can't play {uri}"));
                return;
            };
            let index = track_uri.and_then(|t| page.tracks.iter().position(|track| track.uri == t)).unwrap_or(0);
            store.recents.retain(|recent| recent.uri != uri);
            store.recents.insert(0, page.summary);
            start_queue(store, queue, page.tracks, index, Some(uri));
        }
        Command::PlayTracks { uris, index } => {
            let all: Vec<Track> = (0..ALBUMS.len()).flat_map(album_tracks).collect();
            let tracks = uris.iter().filter_map(|uri| all.iter().find(|t| &t.uri == uri).cloned()).collect();
            start_queue(store, queue, tracks, index, None);
        }
        Command::LoadContext { uri } => {
            let loaded = page(&uri).map_or_else(|| Loadable::Failed("Not found".into()), Loadable::Ready);
            store.pages.insert(uri, loaded);
        }
        Command::RefreshLibrary => {}
        Command::Search { query } => {
            let results = if query.trim().is_empty() { Loadable::Idle } else { Loadable::Ready(search(query.trim())) };
            store.search = Search { query, results };
        }
        Command::SetLiked { uri, liked } => {
            if liked {
                store.liked.insert(uri);
            } else {
                store.liked.remove(&uri);
            }
        }
        Command::DismissNotice => store.notice = None,
    }
}
