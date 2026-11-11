//! Everything the catalogue feeds into the store: playlists, saved albums, listening
//! history, pages, search and Liked Songs. Each method runs to completion on its own task
//! and writes its result, or its failure, into the store.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use serde_json::{Value, json};

use super::catalog::{BATCH, Catalog};
use crate::engine::Shared;
use crate::model::{ContextKind, ContextPage, ContextSummary, LIKED_URI, Loadable, Search, SearchResults, SessionState, Store, Track};

/// Pages stop here; a longer playlist is marked truncated.
const MAX_TRACKS: usize = 5000;
const MAX_PLAYLISTS: usize = 1000;
const MAX_ALBUMS: usize = 300;
const MAX_RECENTS: usize = 12;
const ARTIST_ALBUMS: usize = 30;
/// Tracks remembered for reopening pages and searching the library.
const MAX_REMEMBERED_TRACKS: usize = 40_000;
const SEARCH_TRACKS: usize = 30;
const SEARCH_OTHERS: usize = 8;

pub(super) fn kind_of(uri: &str) -> ContextKind {
    match uri.split(':').collect::<Vec<_>>().as_slice() {
        ["spotify", "playlist", _] | ["spotify", "user", _, "playlist", _] => ContextKind::Playlist,
        ["spotify", "album", _] => ContextKind::Album,
        ["spotify", "artist", _] => ContextKind::Artist,
        ["spotify", "show", _] => ContextKind::Show,
        ["spotify", "collection"] | ["spotify", "user", _, "collection"] => ContextKind::Liked,
        _ => ContextKind::Other,
    }
}

/// Liked Songs plays as `spotify:user:<name>:collection`; the store calls it [`LIKED_URI`].
pub(super) fn normalize_context(uri: &str) -> String {
    if kind_of(uri) == ContextKind::Liked { LIKED_URI.to_owned() } else { uri.to_owned() }
}

/// The context a search plays as, which is also how its tracks are found.
pub(super) fn search_context(query: &str) -> String {
    let encoded: String = form_urlencoded::byte_serialize(query.trim().as_bytes()).collect();
    format!("spotify:search:{encoded}")
}

pub(super) fn liked_summary() -> ContextSummary {
    ContextSummary { uri: LIKED_URI.into(), name: "Liked Songs".into(), subtitle: String::new(), image_url: None, kind: ContextKind::Liked }
}

fn summaries_to_json(summaries: &[ContextSummary]) -> Value {
    let entries: Vec<Value> = summaries.iter().map(|s| json!({"uri": s.uri, "name": s.name, "subtitle": s.subtitle, "image_url": s.image_url})).collect();
    Value::Array(entries)
}

fn summaries_from_json(value: &Value) -> Vec<ContextSummary> {
    let entries = value.as_array().map_or(&[][..], Vec::as_slice);
    entries
        .iter()
        .filter_map(|entry| {
            let uri = entry["uri"].as_str()?;
            Some(ContextSummary {
                uri: uri.to_owned(),
                name: entry["name"].as_str()?.to_owned(),
                subtitle: entry["subtitle"].as_str().unwrap_or_default().to_owned(),
                image_url: entry["image_url"].as_str().map(str::to_owned),
                kind: kind_of(uri),
            })
        })
        .collect()
}

/// A context's summary if anything already in the store names it.
fn known_summary(store: &Store, uri: &str) -> Option<ContextSummary> {
    if uri == LIKED_URI {
        return Some(liked_summary());
    }
    let in_list = |list: &[ContextSummary]| list.iter().find(|s| s.uri == uri).cloned();
    in_list(&store.recents)
        .or_else(|| store.playlists.ready().and_then(|list| in_list(list)))
        .or_else(|| store.albums.ready().and_then(|list| in_list(list)))
        .or_else(|| store.pages.get(uri).and_then(Loadable::ready).map(|page| page.summary.clone()))
        .or_else(|| store.pages.values().filter_map(Loadable::ready).find_map(|page| in_list(&page.albums)))
        .or_else(|| {
            let results = store.search.results.ready()?;
            in_list(&results.albums).or_else(|| in_list(&results.playlists)).or_else(|| in_list(&results.artists))
        })
}

/// Whether every word of the query appears somewhere in `text`, ignoring case.
fn matches(words: &[String], text: &str) -> bool {
    let text = text.to_lowercase();
    words.iter().all(|word| text.contains(word.as_str()))
}

fn query_words(query: &str) -> Vec<String> {
    query.split_whitespace().map(str::to_lowercase).collect()
}

/// The albums a list of tracks names, and the artists among them that the query mentions, in
/// order of first appearance. Artists come without pictures: track metadata has none.
fn albums_and_artists(tracks: &[Track], words: &[String]) -> (Vec<ContextSummary>, Vec<ContextSummary>) {
    let (mut albums, mut artists): (Vec<ContextSummary>, Vec<ContextSummary>) = (Vec::new(), Vec::new());
    for track in tracks {
        if let Some(album) = track.album.as_ref().filter(|album| !album.uri.is_empty())
            && albums.len() < SEARCH_OTHERS
            && !albums.iter().any(|seen| seen.uri == album.uri)
        {
            albums.push(ContextSummary { uri: album.uri.clone(), name: album.name.clone(), subtitle: track.artist_line(), image_url: track.image_url.clone(), kind: ContextKind::Album });
        }
        let mentioned = |name: &str| {
            let name = name.to_lowercase();
            words.iter().any(|word| name.contains(word.as_str()))
        };
        for artist in track.artists.iter().filter(|artist| !artist.uri.is_empty() && mentioned(&artist.name)) {
            if artists.len() < SEARCH_OTHERS && !artists.iter().any(|seen| seen.uri == artist.uri) {
                artists.push(ContextSummary { uri: artist.uri.clone(), name: artist.name.clone(), subtitle: "Artist".into(), image_url: None, kind: ContextKind::Artist });
            }
        }
    }
    (albums, artists)
}

/// What the library itself holds for a query: instant, and right even offline.
fn search_library(store: &Store, query: &str) -> SearchResults {
    let words = query_words(query);
    let named = |list: &Loadable<Vec<ContextSummary>>| -> Vec<ContextSummary> {
        let list = list.ready().map_or(&[][..], Vec::as_slice);
        list.iter().filter(|summary| matches(&words, &format!("{} {}", summary.name, summary.subtitle))).take(SEARCH_OTHERS).cloned().collect()
    };
    SearchResults { tracks: Vec::new(), albums: named(&store.albums), artists: Vec::new(), playlists: named(&store.playlists) }
}

fn merge(into: &mut Vec<ContextSummary>, more: Vec<ContextSummary>) {
    for summary in more {
        if into.len() < SEARCH_OTHERS && !into.iter().any(|seen| seen.uri == summary.uri) {
            into.push(summary);
        }
    }
}

#[derive(Default)]
struct Liked {
    /// Newest first.
    tracks: Vec<String>,
    loaded: bool,
}

pub(super) struct Library {
    shared: Arc<Shared>,
    recents_path: PathBuf,
    catalog: Mutex<Option<Catalog>>,
    /// The newest search; an answer for an older one is dropped.
    search: AtomicU64,
    /// Contexts started from this device since launch, newest first. The listening history
    /// lags by a track or so, so these lead it.
    played_here: Mutex<Vec<String>>,
    /// Every track described so far, so a page opens at once the second time.
    tracks: Mutex<HashMap<String, Track>>,
    liked: Mutex<Liked>,
    /// Likes and unlikes reach Spotify one at a time, in the order they were asked for.
    liked_writes: tokio::sync::Mutex<()>,
    /// The newest like or unlike asked for each track; only it may undo itself on failure.
    liked_asks: Mutex<HashMap<String, u64>>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Library {
    pub(super) fn new(shared: Arc<Shared>, recents_path: PathBuf) -> Self {
        Self {
            shared,
            recents_path,
            catalog: Mutex::new(None),
            search: AtomicU64::new(0),
            played_here: Mutex::new(Vec::new()),
            tracks: Mutex::new(HashMap::new()),
            liked: Mutex::new(Liked::default()),
            liked_writes: tokio::sync::Mutex::new(()),
            liked_asks: Mutex::new(HashMap::new()),
        }
    }

    pub(super) fn set_catalog(&self, catalog: Option<Catalog>) {
        *lock(&self.catalog) = catalog;
    }

    /// Forget everything one account loaded, so the next account starts clean.
    pub(super) fn forget_account(&self) {
        self.search.fetch_add(1, Ordering::SeqCst);
        lock(&self.played_here).clear();
        lock(&self.tracks).clear();
        *lock(&self.liked) = Liked::default();
        lock(&self.liked_asks).clear();
        if let Err(error) = std::fs::remove_file(&self.recents_path) {
            log::debug!("no saved recents to remove: {error}");
        }
    }

    fn catalog(&self) -> Result<Catalog, String> {
        lock(&self.catalog).clone().ok_or_else(|| "Not connected to Spotify".to_owned())
    }

    /// Last run's recents, so Home has something to offer before the network does.
    pub(super) fn restore_recents(&self) {
        let Ok(saved) = std::fs::read(&self.recents_path) else { return };
        let Ok(value) = serde_json::from_slice::<Value>(&saved) else { return };
        let recents = summaries_from_json(&value);
        if !recents.is_empty() {
            self.shared.update(|store| store.recents = recents);
        }
    }

    fn publish_recents(&self, recents: Vec<ContextSummary>) {
        let saved = summaries_to_json(&recents).to_string();
        self.shared.update(|store| store.recents = recents);
        if let Err(error) = std::fs::write(&self.recents_path, saved) {
            log::warn!("can't save recents: {error}");
        }
    }

    /// Tracks for `uris`, in their order, described from memory where possible. URIs that
    /// aren't tracks, or that Spotify doesn't answer for, are left out.
    async fn describe(&self, catalog: &Catalog, uris: &[String]) -> Result<Vec<Track>, String> {
        let missing: Vec<String> = {
            let known = lock(&self.tracks);
            uris.iter().filter(|uri| !known.contains_key(*uri)).cloned().collect()
        };
        let fetched = if missing.is_empty() { HashMap::new() } else { catalog.tracks(&missing).await.map_err(|e| e.to_string())? };
        let mut known = lock(&self.tracks);
        if known.len() + fetched.len() > MAX_REMEMBERED_TRACKS {
            known.clear();
        }
        known.extend(fetched);
        Ok(uris.iter().filter_map(|uri| known.get(uri).cloned()).collect())
    }

    pub(super) async fn refresh(self: Arc<Self>) {
        let Ok(catalog) = self.catalog() else { return };
        // First, so the user's own playlists are labelled with their name.
        let name = self.load_profile(&catalog).await;
        tokio::join!(self.load_playlists(&catalog, name.as_deref()), self.load_collection(&catalog), self.load_recents(&catalog));
    }

    async fn load_profile(&self, catalog: &Catalog) -> Option<String> {
        match catalog.display_name().await {
            Ok(name) => {
                self.shared.update(|store| {
                    if matches!(store.session, SessionState::Online { .. }) {
                        store.session = SessionState::Online { user: name.clone() };
                    }
                });
                Some(name)
            }
            Err(error) => {
                log::warn!("can't read the profile: {error}");
                None
            }
        }
    }

    async fn load_playlists(&self, catalog: &Catalog, display_name: Option<&str>) {
        self.shared.update(|store| {
            if store.playlists.ready().is_none() {
                store.playlists = Loadable::Loading;
            }
        });
        let username = catalog.username();
        let loaded = catalog.playlists(MAX_PLAYLISTS).await.map(|mut playlists| {
            for playlist in &mut playlists {
                if playlist.subtitle == username {
                    playlist.subtitle = display_name.unwrap_or(&username).to_owned();
                } else if playlist.subtitle == "spotify" {
                    playlist.subtitle = "Spotify".to_owned();
                }
            }
            playlists
        });
        self.shared.update(|store| match loaded {
            Ok(mut playlists) => {
                // A cover found earlier from a playlist's first track outlives the refresh.
                if let Some(previous) = store.playlists.ready() {
                    for playlist in playlists.iter_mut().filter(|playlist| playlist.image_url.is_none()) {
                        playlist.image_url = previous.iter().find(|old| old.uri == playlist.uri).and_then(|old| old.image_url.clone());
                    }
                }
                store.playlists = Loadable::Ready(playlists);
            }
            Err(error) if store.playlists.ready().is_none() => store.playlists = Loadable::Failed(error.to_string()),
            Err(error) => store.notice = Some(format!("Couldn't refresh playlists: {error}")),
        });
    }

    /// Liked Songs and saved albums come from one listing.
    async fn load_collection(&self, catalog: &Catalog) {
        self.shared.update(|store| {
            if store.albums.ready().is_none() {
                store.albums = Loadable::Loading;
            }
        });
        let collection = match catalog.collection().await {
            Ok(collection) => collection,
            Err(error) => {
                log::warn!("can't read the collection: {error}");
                self.shared.update(|store| {
                    if store.albums.ready().is_none() {
                        store.albums = Loadable::Failed(error.to_string());
                    }
                });
                return;
            }
        };
        let liked: HashSet<String> = collection.tracks.iter().cloned().collect();
        let changed = {
            let mut known = lock(&self.liked);
            let changed = known.tracks != collection.tracks;
            *known = Liked { tracks: collection.tracks, loaded: true };
            changed
        };
        let liked_page_open = self.shared.update(|store| {
            store.liked = liked;
            store.pages.get(LIKED_URI).is_some_and(|page| page.ready().is_some())
        });
        let wanted: Vec<String> = collection.albums.into_iter().take(MAX_ALBUMS).collect();
        let albums = catalog.albums(&wanted).await;
        self.shared.update(|store| match albums {
            Ok(albums) => store.albums = Loadable::Ready(albums),
            Err(error) if store.albums.ready().is_none() => store.albums = Loadable::Failed(error.to_string()),
            Err(error) => log::warn!("can't refresh saved albums: {error}"),
        });
        if changed
            && liked_page_open
            && let Err(error) = self.load_liked(catalog).await
        {
            log::info!("can't reload Liked Songs: {error}");
        }
    }

    async fn fetch_summary(&self, catalog: &Catalog, uri: &str) -> Option<ContextSummary> {
        let fetched = match kind_of(uri) {
            ContextKind::Playlist => catalog.playlist_summary(uri).await.map(Some),
            ContextKind::Album => catalog.albums(&[uri.to_owned()]).await.map(|mut albums| albums.pop()),
            ContextKind::Artist => catalog.artist(uri, 0).await.map(|artist| Some(artist.summary)),
            ContextKind::Liked | ContextKind::Show | ContextKind::Other => return None,
        };
        match fetched {
            Ok(summary) => summary.filter(|summary| !summary.name.is_empty()),
            Err(error) => {
                log::debug!("no summary for {uri}: {error}");
                None
            }
        }
    }

    async fn summary(&self, catalog: &Catalog, uri: &str) -> Option<ContextSummary> {
        match self.shared.read(|store| known_summary(store, uri)) {
            Some(known) => Some(known),
            None => self.fetch_summary(catalog, uri).await,
        }
    }

    async fn load_recents(&self, catalog: &Catalog) {
        let history = match catalog.recent_contexts(MAX_RECENTS * 3).await {
            Ok(history) => history,
            Err(error) => {
                log::warn!("can't read the listening history: {error}");
                return;
            }
        };
        let mut wanted = lock(&self.played_here).clone();
        for uri in history.iter().map(|uri| normalize_context(uri)) {
            if !wanted.contains(&uri) {
                wanted.push(uri);
            }
        }
        let mut recents = Vec::new();
        for uri in wanted {
            if recents.len() == MAX_RECENTS {
                break;
            }
            if let Some(summary) = self.summary(catalog, &uri).await {
                recents.push(summary);
            }
        }
        if !recents.is_empty() {
            self.publish_recents(recents);
        }
    }

    /// A context started playing on this device: it leads the recents.
    pub(super) async fn note_played(self: Arc<Self>, uri: String) {
        if !matches!(kind_of(&uri), ContextKind::Playlist | ContextKind::Album | ContextKind::Artist | ContextKind::Liked) {
            return;
        }
        {
            let mut played = lock(&self.played_here);
            played.retain(|seen| *seen != uri);
            played.insert(0, uri.clone());
            played.truncate(MAX_RECENTS);
        }
        let Ok(catalog) = self.catalog() else { return };
        let Some(summary) = self.summary(&catalog, &uri).await else { return };
        let mut recents = self.shared.read(|store| store.recents.clone());
        recents.retain(|recent| recent.uri != uri);
        recents.insert(0, summary);
        recents.truncate(MAX_RECENTS);
        self.publish_recents(recents);
    }

    fn publish_page(&self, uri: &str, page: &ContextPage) {
        let mut page = page.clone();
        self.shared.update(|store| {
            if uri == LIKED_URI {
                // A song unliked while the page was still filling stays gone.
                page.tracks.retain(|track| store.liked.contains(&track.uri));
            }
            store.pages.insert(uri.to_owned(), Loadable::Ready(page));
        });
    }

    /// A playlist without a cover of its own takes its first track's, wherever it's listed.
    fn adopt_cover(&self, summary: &mut ContextSummary, tracks: &[Track]) {
        if summary.image_url.is_some() || summary.kind != ContextKind::Playlist {
            return;
        }
        let Some(cover) = tracks.iter().find_map(|track| track.image_url.clone()) else { return };
        summary.image_url = Some(cover.clone());
        let uri = summary.uri.clone();
        self.shared.update(|store| {
            let lists = [store.playlists.ready_mut(), Some(&mut store.recents)];
            for entry in lists.into_iter().flatten().flatten().filter(|entry| entry.uri == uri && entry.image_url.is_none()) {
                entry.image_url = Some(cover.clone());
            }
        });
    }

    /// Describe a page's tracks a batch at a time, publishing after each, so a long playlist
    /// shows its top at once.
    async fn fill_page(&self, catalog: &Catalog, mut summary: ContextSummary, uris: &[String], truncated: bool) -> Result<(), String> {
        let uri = summary.uri.clone();
        let mut tracks = Vec::with_capacity(uris.len());
        let mut batches = uris.chunks(BATCH).peekable();
        if batches.peek().is_none() {
            self.publish_page(&uri, &ContextPage { summary, tracks, albums: Vec::new(), truncated });
            return Ok(());
        }
        let mut batch_number = 0;
        while let Some(batch) = batches.next() {
            tracks.extend(self.describe(catalog, batch).await?);
            self.adopt_cover(&mut summary, &tracks);
            let more = batches.peek().is_some();
            batch_number += 1;
            // The top of the list shows at once; after that, every few batches is enough.
            if !more || batch_number <= 3 || batch_number % 8 == 0 {
                self.publish_page(&uri, &ContextPage { summary: summary.clone(), tracks: tracks.clone(), albums: Vec::new(), truncated: truncated || more });
            }
        }
        Ok(())
    }

    pub(super) async fn load_context(self: Arc<Self>, uri: String) {
        enum Before {
            Loading,
            Shown,
            Blank,
        }
        let before = self.shared.update(|store| match store.pages.get(&uri) {
            Some(Loadable::Loading) => Before::Loading,
            Some(Loadable::Ready(_)) => Before::Shown,
            _ => {
                store.pages.insert(uri.clone(), Loadable::Loading);
                Before::Blank
            }
        });
        if matches!(before, Before::Loading) {
            return;
        }
        let loaded = match self.catalog() {
            Ok(catalog) => match kind_of(&uri) {
                ContextKind::Playlist => self.load_playlist(&catalog, &uri).await,
                ContextKind::Album => self.load_album(&catalog, &uri).await,
                ContextKind::Artist => self.load_artist(&catalog, &uri).await,
                ContextKind::Liked => self.load_liked(&catalog).await,
                ContextKind::Show | ContextKind::Other => Err("Baliset can't open this yet".to_owned()),
            },
            Err(offline) => Err(offline),
        };
        if let Err(error) = loaded {
            self.shared.update(|store| {
                // A page that was whole before this refresh stays; one that failed part-way
                // through its first load is not kept as if it were complete.
                if matches!(before, Before::Shown) {
                    store.notice = Some(error);
                } else {
                    store.pages.insert(uri, Loadable::Failed(error));
                }
            });
        }
    }

    async fn load_playlist(&self, catalog: &Catalog, uri: &str) -> Result<(), String> {
        let (playlist, truncated) = catalog.playlist(uri, MAX_TRACKS).await.map_err(|e| e.to_string())?;
        let mut summary = playlist.summary;
        // The list of playlists knows the owner's display name; the playlist itself doesn't.
        if let Some(listed) = self.shared.read(|store| store.playlists.ready().and_then(|list| list.iter().find(|entry| entry.uri == uri).cloned())) {
            summary.subtitle = listed.subtitle;
            summary.image_url = summary.image_url.or(listed.image_url);
        }
        self.fill_page(catalog, summary, &playlist.track_uris, truncated).await
    }

    async fn load_album(&self, catalog: &Catalog, uri: &str) -> Result<(), String> {
        let album = catalog.album(uri).await.map_err(|e| e.to_string())?;
        self.fill_page(catalog, album.summary, &album.track_uris, false).await
    }

    async fn load_artist(&self, catalog: &Catalog, uri: &str) -> Result<(), String> {
        let artist = catalog.artist(uri, ARTIST_ALBUMS).await.map_err(|e| e.to_string())?;
        let (tracks, albums) = tokio::join!(self.describe(catalog, &artist.top_track_uris), catalog.albums(&artist.album_uris));
        let albums = albums.unwrap_or_else(|error| {
            log::info!("no albums for {uri}: {error}");
            Vec::new()
        });
        self.publish_page(uri, &ContextPage { summary: artist.summary, tracks: tracks?, albums, truncated: false });
        Ok(())
    }

    async fn load_liked(&self, catalog: &Catalog) -> Result<(), String> {
        if !lock(&self.liked).loaded {
            let collection = catalog.collection().await.map_err(|e| e.to_string())?;
            *lock(&self.liked) = Liked { tracks: collection.tracks, loaded: true };
        }
        let mut uris = lock(&self.liked).tracks.clone();
        let truncated = uris.len() > MAX_TRACKS;
        uris.truncate(MAX_TRACKS);
        self.fill_page(catalog, liked_summary(), &uris, truncated).await
    }

    /// The playing track as the catalogue describes it: the player's own metadata has no
    /// album or artist URIs.
    pub(super) async fn describe_playing(self: Arc<Self>, uri: String) {
        let Ok(catalog) = self.catalog() else { return };
        let Ok(mut described) = self.describe(&catalog, std::slice::from_ref(&uri)).await else { return };
        let Some(described) = described.pop() else { return };
        self.shared.update(|store| {
            if let Some(playing) = store.playback.track.as_mut().filter(|playing| playing.uri == uri) {
                *playing = described;
            }
        });
    }

    pub(super) async fn search(self: Arc<Self>, query: String) {
        let attempt = self.search.fetch_add(1, Ordering::SeqCst) + 1;
        if query.trim().is_empty() {
            self.shared.update(|store| store.search = Search::default());
            return;
        }
        let words = query_words(&query);
        let mut results = self.shared.read(|store| search_library(store, &query));
        self.shared.update(|store| {
            if self.search.load(Ordering::SeqCst) == attempt {
                store.search = Search { query: query.clone(), results: Loadable::Loading };
            }
        });
        let found = match self.catalog() {
            Ok(catalog) => match catalog.search_tracks(&search_context(&query)).await {
                Ok(uris) => {
                    let uris: Vec<String> = uris.into_iter().take(SEARCH_TRACKS).collect();
                    self.describe(&catalog, &uris).await
                }
                Err(error) => Err(error.to_string()),
            },
            Err(offline) => Err(offline),
        };
        if self.search.load(Ordering::SeqCst) != attempt {
            return;
        }
        let outcome = match found {
            Ok(tracks) => {
                let (albums, artists) = albums_and_artists(&tracks, &words);
                merge(&mut results.albums, albums);
                merge(&mut results.artists, artists);
                results.tracks = tracks;
                Loadable::Ready(results)
            }
            // The library's own matches are still worth showing.
            Err(error) if results.albums.is_empty() && results.playlists.is_empty() => Loadable::Failed(error),
            Err(error) => {
                log::info!("catalogue search failed: {error}");
                Loadable::Ready(results)
            }
        };
        self.shared.update(|store| {
            if self.search.load(Ordering::SeqCst) == attempt {
                store.search = Search { query, results: outcome };
            }
        });
    }

    pub(super) async fn set_liked(self: Arc<Self>, uri: String, liked: bool) {
        let apply = |store: &mut Store, liked: bool| {
            if liked {
                store.liked.insert(uri.clone());
            } else {
                store.liked.remove(&uri);
            }
        };
        let ask = {
            let mut asks = lock(&self.liked_asks);
            let ask = asks.get(&uri).copied().unwrap_or(0) + 1;
            asks.insert(uri.clone(), ask);
            ask
        };
        let was_liked = self.shared.update(|store| {
            let was = store.liked.contains(&uri);
            apply(store, liked);
            was
        });
        let _in_order = self.liked_writes.lock().await;
        let saved = match self.catalog() {
            Ok(catalog) => catalog.set_saved(&uri, liked).await.map_err(|e| e.to_string()),
            Err(offline) => Err(offline),
        };
        let newest = lock(&self.liked_asks).get(&uri).copied() == Some(ask);
        if let Err(error) = saved {
            self.shared.update(|store| {
                // A newer ask for this track will settle its state itself.
                if newest {
                    apply(store, was_liked);
                }
                store.notice = Some(format!("Couldn't update Liked Songs: {error}"));
            });
            return;
        }
        {
            let mut known = lock(&self.liked);
            known.tracks.retain(|track| *track != uri);
            if liked {
                known.tracks.insert(0, uri.clone());
            }
        }
        let track = lock(&self.tracks).get(&uri).cloned();
        self.shared.update(|store| {
            let Some(Loadable::Ready(page)) = store.pages.get_mut(LIKED_URI) else { return };
            page.tracks.retain(|listed| listed.uri != uri);
            if let Some(track) = track.filter(|_| liked) {
                page.tracks.insert(0, track);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::NamedRef;

    fn summary(uri: &str, name: &str, subtitle: &str) -> ContextSummary {
        ContextSummary { uri: uri.into(), name: name.into(), subtitle: subtitle.into(), image_url: None, kind: kind_of(uri) }
    }

    fn track(n: u32, name: &str, artist: &str, album: &str) -> Track {
        Track {
            uri: format!("spotify:track:{n}"),
            name: name.into(),
            artists: vec![NamedRef { uri: format!("spotify:artist:{artist}"), name: artist.into() }],
            album: Some(NamedRef { uri: format!("spotify:album:{album}"), name: album.into() }),
            image_url: Some(format!("cover-{album}")),
            ..Track::default()
        }
    }

    #[test]
    fn uris_map_to_kinds() {
        let cases = [
            ("spotify:playlist:37i9dQZF1DXcBWIGoYBM5M", ContextKind::Playlist),
            ("spotify:user:someone:playlist:37i9dQZF1DXcBWIGoYBM5M", ContextKind::Playlist),
            ("spotify:album:4aawyAB9vmqN3uQ7FjRGTy", ContextKind::Album),
            ("spotify:artist:0TnOYISbd1XYRBk9myaseg", ContextKind::Artist),
            ("spotify:user:someone:collection", ContextKind::Liked),
            ("spotify:collection", ContextKind::Liked),
            ("spotify:search:query", ContextKind::Other),
        ];
        for (uri, kind) in cases {
            assert_eq!(kind_of(uri), kind, "{uri}");
        }
        assert_eq!(normalize_context("spotify:user:someone:collection"), LIKED_URI);
        assert_eq!(normalize_context("spotify:album:a"), "spotify:album:a");
    }

    #[test]
    fn a_search_context_keeps_the_query_url_safe() {
        assert_eq!(search_context("  kendrick lamar  "), "spotify:search:kendrick+lamar");
        assert_eq!(search_context("AC/DC? #1"), "spotify:search:AC%2FDC%3F+%231");
    }

    #[test]
    fn recents_survive_a_round_trip_through_their_file_format() {
        let recents = vec![ContextSummary { image_url: Some("u".into()), ..summary("spotify:album:4aawyAB9vmqN3uQ7FjRGTy", "Record", "One") }, liked_summary()];
        assert_eq!(summaries_from_json(&summaries_to_json(&recents)), recents);
    }

    #[test]
    fn a_summary_is_found_wherever_the_store_already_names_it() {
        let album = summary("spotify:album:a", "A", "");
        let mut store = Store::default();
        assert_eq!(known_summary(&store, "spotify:album:a"), None);
        store.albums = Loadable::Ready(vec![album.clone()]);
        assert_eq!(known_summary(&store, "spotify:album:a"), Some(album));
        assert_eq!(known_summary(&store, LIKED_URI), Some(liked_summary()));
    }

    #[test]
    fn the_library_matches_every_word_in_any_order_and_case() {
        let store = Store {
            playlists: Loadable::Ready(vec![summary("spotify:playlist:1", "Late Night Drive", "Tony"), summary("spotify:playlist:2", "Sunday Cooking", "Tony")]),
            albums: Loadable::Ready(vec![summary("spotify:album:1", "The Lost Boy", "Cordae")]),
            ..Store::default()
        };
        let found = search_library(&store, "drive LATE");
        assert_eq!(found.playlists.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["Late Night Drive"]);
        assert_eq!(search_library(&store, "cordae").albums.len(), 1);
        assert!(search_library(&store, "late cooking").playlists.is_empty());
    }

    #[test]
    fn found_tracks_name_their_albums_once_and_only_matching_artists() {
        let tracks = [track(1, "Creep", "Radiohead", "Pablo Honey"), track(2, "Stop Whispering", "Radiohead", "Pablo Honey"), track(3, "Creep (cover)", "Someone", "Covers")];
        let (albums, artists) = albums_and_artists(&tracks, &query_words("radiohead creep"));
        assert_eq!(albums.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(), ["Pablo Honey", "Covers"]);
        assert_eq!(albums[0].image_url.as_deref(), Some("cover-Pablo Honey"));
        assert_eq!(artists.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(), ["Radiohead"]);
    }

    #[test]
    fn merging_skips_what_is_already_listed() {
        let mut albums = vec![summary("spotify:album:1", "One", "")];
        merge(&mut albums, vec![summary("spotify:album:1", "One", ""), summary("spotify:album:2", "Two", "")]);
        assert_eq!(albums.len(), 2);
    }
}
