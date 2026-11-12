//! `baliset --check`: signs in with the saved credentials, without a Connect device, and makes
//! one cheap read-only call to each Spotify service the app uses, checking that the answer
//! still has the shape the app reads. One line per check, for a bug report.
//!
//! To cover a new endpoint, append a [`Check`] to [`CHECKS`]: what it is for, the call, what
//! in the app breaks without it, the file that makes the call, and an async fn that makes it
//! through the app's own code where there is some. A check returns what it found (counts,
//! never names or ids) or why it failed. Checks run in order and may leave URIs in
//! [`Seeds`] for later ones; each falls back to a well-known catalogue item when an earlier
//! check found nothing to use.

use std::future::Future;
use std::pin::Pin;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use http::Method;
use librespot_core::authentication::Credentials;
use librespot_core::cache::Cache;
use librespot_core::cdn_url::CdnUrl;
use librespot_core::{FileId, Session, SessionConfig, SpotifyId, SpotifyUri};
use librespot_protocol::metadata as proto;
use librespot_protocol::metadata::audio_file::Format;
use protobuf::Message;
use serde::Serialize;

use super::catalog::{self, Catalog};
use super::{Dirs, library};

const TIMEOUT: Duration = Duration::from_secs(20);
const SEARCH_QUERY: &str = "daft punk";
const FALLBACK_TRACK: &str = "spotify:track:4uLU6hMCjMI75M1A2tKUQC";
const FALLBACK_SHOW: &str = "spotify:show:4rOoJ6Egrf8K2IrywzwOMk";
const MANIFEST: &str = include_str!("../../../Cargo.toml");

/// URIs earlier checks found, for later ones to ask about.
#[derive(Default)]
pub(super) struct Seeds {
    playlists: Vec<String>,
    tracks: Vec<String>,
    show: Option<String>,
    episode: Option<String>,
}

pub(super) struct Probe {
    session: Session,
    catalog: Catalog,
    seeds: Mutex<Seeds>,
}

impl Probe {
    fn seeds(&self) -> MutexGuard<'_, Seeds> {
        self.seeds.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn track(&self) -> String {
        self.seeds().tracks.first().cloned().unwrap_or_else(|| FALLBACK_TRACK.to_owned())
    }

    fn user(&self) -> String {
        self.session.username()
    }
}

type Outcome = Result<String, String>;
type Run = for<'a> fn(&'a Probe) -> Pin<Box<dyn Future<Output = Outcome> + 'a>>;

pub(super) struct Check {
    pub feature: &'static str,
    /// GET, POST, or AP for a request over the access point connection.
    pub method: &'static str,
    /// With `{user}` and `{id}` where the call names the account or an item.
    pub endpoint: &'static str,
    pub used_by: &'static str,
    pub source: &'static str,
    pub run: Run,
}

const CATALOG: &str = "src/engine/spotify/catalog.rs";
const CHECK: &str = "src/engine/spotify/check.rs";

pub(super) const CHECKS: &[Check] = &[
    Check {
        feature: "profile name",
        method: "GET",
        endpoint: "/user-profile-view/v3/profile/{user}",
        used_by: "the account name in the sidebar and on your playlists",
        source: CATALOG,
        run: |p| Box::pin(profile(p)),
    },
    Check {
        feature: "playlist list (rootlist)",
        method: "GET",
        endpoint: "/playlist/v2/user/{user}/rootlist",
        used_by: "the sidebar's playlists",
        source: CATALOG,
        run: |p| Box::pin(rootlist(p)),
    },
    Check {
        feature: "playlist contents",
        method: "GET",
        endpoint: "/playlist/v2/playlist/{id}",
        used_by: "playlist pages",
        source: CATALOG,
        run: |p| Box::pin(playlist(p)),
    },
    Check {
        feature: "track metadata",
        method: "POST",
        endpoint: "/extended-metadata/v0/extended-metadata (TRACK_V4)",
        used_by: "every track row: playlists, albums, Liked Songs, search",
        source: CATALOG,
        run: |p| Box::pin(tracks(p)),
    },
    Check {
        feature: "album metadata",
        method: "POST",
        endpoint: "/extended-metadata/v0/extended-metadata (ALBUM_V4)",
        used_by: "album pages and saved albums on Home",
        source: CATALOG,
        run: |p| Box::pin(albums(p)),
    },
    Check {
        feature: "artist metadata",
        method: "POST",
        endpoint: "/extended-metadata/v0/extended-metadata (ARTIST_V4)",
        used_by: "artist pages",
        source: CATALOG,
        run: |p| Box::pin(artist(p)),
    },
    Check {
        feature: "Liked Songs and saved albums",
        method: "POST",
        endpoint: "/collection/v2/paging (set collection)",
        used_by: "Liked Songs, the hearts on track rows, saved albums on Home",
        source: CATALOG,
        run: |p| Box::pin(collection(p)),
    },
    Check {
        feature: "recently played",
        method: "GET",
        endpoint: "/recently-played/v3/user/{user}/recently-played",
        used_by: "Recents on Home",
        source: CATALOG,
        run: |p| Box::pin(recents(p)),
    },
    Check {
        feature: "search",
        method: "GET",
        endpoint: "/context-resolve/v1/spotify:search:{query}",
        used_by: "search results",
        source: CATALOG,
        run: |p| Box::pin(search(p)),
    },
    Check {
        feature: "song radio",
        method: "GET",
        endpoint: "/inspiredby-mix/v2/seed_to_playlist/{id}",
        used_by: "radio from a track",
        source: CHECK,
        run: |p| Box::pin(song_radio(p)),
    },
    Check {
        feature: "station context",
        method: "GET",
        endpoint: "/context-resolve/v1/spotify:station:track:{id}",
        used_by: "playing a radio station",
        source: CHECK,
        run: |p| Box::pin(station(p)),
    },
    Check {
        feature: "followed shows",
        method: "POST",
        endpoint: "/collection/v2/paging (set show)",
        used_by: "the podcasts in your library",
        source: CHECK,
        run: |p| Box::pin(collection_set(p, "show")),
    },
    Check {
        feature: "saved episodes",
        method: "POST",
        endpoint: "/collection/v2/paging (set listenlater)",
        used_by: "Your Episodes",
        source: CHECK,
        run: |p| Box::pin(collection_set(p, "listenlater")),
    },
    Check {
        feature: "followed artists",
        method: "POST",
        endpoint: "/collection/v2/paging (set artist)",
        used_by: "the artists in your library",
        source: CHECK,
        run: |p| Box::pin(collection_set(p, "artist")),
    },
    Check {
        feature: "show metadata",
        method: "POST",
        endpoint: "/extended-metadata/v0/extended-metadata (SHOW_V4)",
        used_by: "show pages",
        source: CHECK,
        run: |p| Box::pin(show_metadata(p)),
    },
    Check {
        feature: "show episodes",
        method: "GET",
        endpoint: "/context-resolve/v1/spotify:show:{id}",
        used_by: "a show's episode list, playing a show",
        source: CHECK,
        run: |p| Box::pin(show_context(p)),
    },
    Check {
        feature: "episode metadata",
        method: "POST",
        endpoint: "/extended-metadata/v0/extended-metadata (EPISODE_V4)",
        used_by: "episode rows and playing episodes",
        source: CHECK,
        run: |p| Box::pin(episode_metadata(p)),
    },
    Check {
        feature: "audio file location",
        method: "GET",
        endpoint: "/storage-resolve/files/audio/interactive/{id}",
        used_by: "playback (librespot fetches the file from here)",
        source: CHECK,
        run: |p| Box::pin(audio_file(p)),
    },
    Check {
        feature: "audio key",
        method: "AP",
        endpoint: "audio key request (track, file)",
        used_by: "playback (librespot decrypts the file with it)",
        source: CHECK,
        run: |p| Box::pin(audio_key(p)),
    },
];

fn err(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn non_empty<T>(list: Vec<T>, what: &str) -> Result<Vec<T>, String> {
    if list.is_empty() { Err(format!("parsed, but no {what}: the answer's format may have changed")) } else { Ok(list) }
}

async fn profile(p: &Probe) -> Outcome {
    let name = p.catalog.display_name().await.map_err(err)?;
    if name.trim().is_empty() { Err("the profile has an empty name".into()) } else { Ok("has a name".into()) }
}

async fn rootlist(p: &Probe) -> Outcome {
    let playlists = non_empty(p.catalog.playlists(1000).await.map_err(err)?.playlists, "playlists")?;
    let named = playlists.iter().filter(|playlist| !playlist.name.is_empty()).count();
    if named == 0 {
        return Err(format!("{} playlists, none with a name", playlists.len()));
    }
    let count = playlists.len();
    p.seeds().playlists = playlists.into_iter().map(|playlist| playlist.uri).collect();
    Ok(format!("{count} playlists, {named} named"))
}

async fn playlist(p: &Probe) -> Outcome {
    let candidates: Vec<String> = p.seeds().playlists.iter().take(5).cloned().collect();
    if candidates.is_empty() {
        return Err("no playlist to open (the playlist list check failed)".into());
    }
    for uri in candidates {
        let (playlist, _) = p.catalog.playlist(&uri, 100).await.map_err(err)?;
        if playlist.summary.name.is_empty() {
            return Err("a playlist came back without a name".into());
        }
        let tracks: Vec<String> = playlist.track_uris.into_iter().filter(|uri| uri.starts_with("spotify:track:")).collect();
        if !tracks.is_empty() {
            let count = tracks.len();
            p.seeds().tracks = tracks;
            return Ok(format!("{count} tracks in the first playlist that has any"));
        }
    }
    Err("the first playlists read back with no tracks".into())
}

async fn tracks(p: &Probe) -> Outcome {
    let mut uris: Vec<String> = p.seeds().tracks.iter().take(catalog::BATCH).cloned().collect();
    if uris.is_empty() {
        uris.push(FALLBACK_TRACK.to_owned());
    }
    let tracks = p.catalog.tracks(&uris).await.map_err(err)?;
    let complete = tracks.values().filter(|track| !track.name.is_empty() && !track.artists.is_empty() && track.album.is_some() && track.duration_ms > 0).count();
    if complete == 0 {
        return Err(format!("asked for {}, {} answered, none with a name, artist, album and length", uris.len(), tracks.len()));
    }
    Ok(format!("{} of {} described in full", complete, uris.len()))
}

async fn albums(p: &Probe) -> Outcome {
    let tracks = p.catalog.tracks(&[p.track()]).await.map_err(err)?;
    let album = tracks.values().find_map(|track| track.album.clone()).filter(|album| !album.uri.is_empty()).ok_or("no album to ask about (the track has none)")?;
    let page = p.catalog.album(&album.uri).await.map_err(err)?;
    if page.summary.name.is_empty() {
        return Err("the album came back without a name".into());
    }
    let tracks = non_empty(page.track_uris, "tracks on the album")?;
    Ok(format!("{} tracks", tracks.len()))
}

async fn artist(p: &Probe) -> Outcome {
    let tracks = p.catalog.tracks(&[p.track()]).await.map_err(err)?;
    let artist = tracks.values().find_map(|track| track.artists.first().cloned()).filter(|artist| !artist.uri.is_empty()).ok_or("no artist to ask about")?;
    let page = p.catalog.artist(&artist.uri, 30).await.map_err(err)?;
    if page.summary.name.is_empty() {
        return Err("the artist came back without a name".into());
    }
    Ok(format!("{} top tracks, {} albums", page.top_track_uris.len(), page.album_uris.len()))
}

async fn collection(p: &Probe) -> Outcome {
    let collection = p.catalog.collection().await.map_err(err)?;
    if collection.tracks.is_empty() && collection.albums.is_empty() {
        return Err("parsed, but holds nothing: the answer's format may have changed".into());
    }
    if p.seeds().tracks.is_empty() {
        p.seeds().tracks = collection.tracks.iter().take(catalog::BATCH).cloned().collect();
    }
    Ok(format!("{} liked songs, {} saved albums", collection.tracks.len(), collection.albums.len()))
}

async fn recents(p: &Probe) -> Outcome {
    let contexts = non_empty(p.catalog.recent_contexts(12).await.map_err(err)?, "recent contexts")?;
    Ok(format!("{} recent contexts", contexts.len()))
}

async fn search(p: &Probe) -> Outcome {
    let tracks = non_empty(p.catalog.search_tracks(&library::search_context(SEARCH_QUERY)).await.map_err(err)?, "tracks")?;
    Ok(format!("{} tracks for a fixed query", tracks.len()))
}

fn track_uri(uri: &str) -> Result<SpotifyUri, String> {
    SpotifyUri::from_uri(uri).map_err(err)
}

async fn song_radio(p: &Probe) -> Outcome {
    let body = p.session.spclient().get_radio_for_track(&track_uri(&p.track())?).await.map_err(err)?;
    let answer: serde_json::Value = serde_json::from_slice(&body).map_err(|e| format!("not JSON: {e}"))?;
    let items = answer["mediaItems"].as_array().ok_or("no mediaItems in the answer")?;
    let playlists = items.iter().filter_map(|item| item["uri"].as_str()).filter(|uri| uri.starts_with("spotify:playlist:")).count();
    if playlists == 0 {
        return Err(format!("{} media items, none a playlist", items.len()));
    }
    Ok(format!("{playlists} radio playlist"))
}

async fn station(p: &Probe) -> Outcome {
    let id = track_uri(&p.track())?.to_id().map_err(err)?;
    let context = p.session.spclient().get_context(&format!("spotify:station:track:{id}")).await.map_err(err)?;
    let tracks = context.pages.iter().flat_map(|page| &page.tracks).filter(|track| track.uri.as_deref().is_some_and(|uri| uri.starts_with("spotify:track:"))).count();
    if tracks == 0 {
        return Err(format!("{} pages, no tracks", context.pages.len()));
    }
    Ok(format!("{tracks} tracks"))
}

fn set_request(user: &str, set: &str) -> Vec<u8> {
    let mut body = Vec::new();
    super::wire::put_bytes(&mut body, 1, user.as_bytes());
    super::wire::put_bytes(&mut body, 2, set.as_bytes());
    super::wire::put_varint(&mut body, 4, 300);
    body
}

async fn collection_set(p: &Probe, set: &str) -> Outcome {
    let request = set_request(&p.user(), set);
    let body = p.session.spclient().request(&Method::POST, "/collection/v2/paging", Some(Catalog::collection_headers()), Some(&request)).await.map_err(err)?;
    let page = catalog::collection_page(&body).ok_or("the answer isn't a collection page")?;
    let prefix = match set {
        "show" => "spotify:show:",
        "listenlater" => "spotify:episode:",
        _ => "spotify:artist:",
    };
    let wanted: Vec<String> = page.items.iter().map(|item| item.0.clone()).filter(|uri| uri.starts_with(prefix)).collect();
    let others = page.items.len() - wanted.len();
    if others > 0 && wanted.is_empty() {
        return Err(format!("{others} items, none of the expected kind"));
    }
    let mut seeds = p.seeds();
    match set {
        "show" => seeds.show = seeds.show.take().or_else(|| wanted.first().cloned()),
        "listenlater" => seeds.episode = seeds.episode.take().or_else(|| wanted.first().cloned()),
        _ => {}
    }
    let more = if page.next.is_some() { " on the first page" } else { "" };
    Ok(format!("{} items{more}", wanted.len()))
}

fn show(p: &Probe) -> String {
    p.seeds().show.clone().unwrap_or_else(|| FALLBACK_SHOW.to_owned())
}

async fn show_metadata(p: &Probe) -> Outcome {
    let body = p.session.spclient().get_show_metadata(&track_uri(&show(p))?).await.map_err(err)?;
    let message = proto::Show::parse_from_bytes(&body).map_err(|e| format!("not a Show message: {e}"))?;
    if message.name().is_empty() {
        return Err("the show came back without a name".into());
    }
    Ok(format!("named, {} episodes listed", message.episode.len()))
}

async fn show_context(p: &Probe) -> Outcome {
    let context = p.session.spclient().get_context(&show(p)).await.map_err(err)?;
    let episodes: Vec<String> = context.pages.iter().flat_map(|page| &page.tracks).filter_map(|track| track.uri.clone()).filter(|uri| uri.starts_with("spotify:episode:")).collect();
    let first = non_empty(episodes, "episodes")?;
    let count = first.len();
    let mut seeds = p.seeds();
    seeds.episode = seeds.episode.take().or_else(|| first.into_iter().next());
    Ok(format!("{count} episodes on the first pages"))
}

async fn episode_metadata(p: &Probe) -> Outcome {
    let episode = p.seeds().episode.clone().ok_or("no episode to ask about (the show checks found none)")?;
    let body = p.session.spclient().get_episode_metadata(&track_uri(&episode)?).await.map_err(err)?;
    let message = proto::Episode::parse_from_bytes(&body).map_err(|e| format!("not an Episode message: {e}"))?;
    if message.name().is_empty() {
        return Err("the episode came back without a name".into());
    }
    Ok(format!("named, {} audio files", message.audio.len()))
}

/// The track to check playback with, and the file the player would pick at the chosen bitrate.
async fn playable_file(p: &Probe) -> Result<(SpotifyId, FileId), String> {
    let uri = track_uri(&p.track())?;
    let body = p.session.spclient().get_track_metadata(&uri).await.map_err(err)?;
    let message = proto::Track::parse_from_bytes(&body).map_err(|e| format!("not a Track message: {e}"))?;
    let files = message.file.iter().chain(message.alternative.iter().flat_map(|alt| &alt.file));
    let preferred = match crate::settings::at_launch().audio_quality {
        crate::settings::AudioQuality::Low => Format::OGG_VORBIS_96,
        crate::settings::AudioQuality::Normal => Format::OGG_VORBIS_160,
        crate::settings::AudioQuality::VeryHigh => Format::OGG_VORBIS_320,
    };
    let files: Vec<&proto::AudioFile> = files.collect();
    let file = files.iter().find(|file| file.format() == preferred).or_else(|| files.first()).ok_or("the track lists no audio files")?;
    let id = SpotifyId::from_raw(message.gid()).map_err(err)?;
    Ok((id, FileId::from(*file)))
}

async fn audio_file(p: &Probe) -> Outcome {
    let (_, file) = playable_file(p).await?;
    let resolved = CdnUrl::new(file).resolve_audio(&p.session).await.map_err(err)?;
    let urls = resolved.try_get_urls().map_err(err)?;
    if urls.is_empty() {
        return Err("no file locations".into());
    }
    Ok(format!("{} locations", urls.len()))
}

async fn audio_key(p: &Probe) -> Outcome {
    let (track, file) = playable_file(p).await?;
    p.session.audio_key().request(track, file).await.map_err(err)?;
    Ok("granted".into())
}

#[derive(Serialize)]
struct Line {
    feature: &'static str,
    method: &'static str,
    endpoint: &'static str,
    passed: bool,
    detail: String,
    ms: u128,
    #[serde(skip_serializing_if = "Option::is_none")]
    used_by: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<&'static str>,
}

#[derive(Serialize)]
struct Versions {
    baliset: &'static str,
    librespot: &'static str,
    rune: String,
}

#[derive(Serialize)]
struct Report {
    versions: Versions,
    checks: Vec<Line>,
    passed: usize,
    failed: usize,
}

fn rune_rev() -> String {
    let line = MANIFEST.lines().find(|line| line.starts_with("rune ="));
    let rev = line.and_then(|line| line.split("rev = \"").nth(1)).and_then(|rest| rest.split('"').next());
    rev.map_or_else(|| "unknown".to_owned(), |rev| rev.chars().take(12).collect())
}

/// Error text with the account's name taken out.
fn scrub(text: &str, user: &str) -> String {
    if user.is_empty() { text.to_owned() } else { text.replace(user, "{user}") }
}

fn line(check: &Check, outcome: Outcome, started: Instant, user: &str) -> Line {
    let ms = started.elapsed().as_millis();
    let passed = outcome.is_ok();
    let detail = scrub(&outcome.unwrap_or_else(|e| e), user);
    let (used_by, source) = if passed { (None, None) } else { (Some(check.used_by), Some(check.source)) };
    Line { feature: check.feature, method: check.method, endpoint: check.endpoint, passed, detail, ms, used_by, source }
}

const CONNECT: Check = Check {
    feature: "session connect",
    method: "AP",
    endpoint: "login with the saved credentials",
    used_by: "everything: sign-in, the library, playback",
    source: "src/engine/spotify/mod.rs",
    run: |_| Box::pin(async { Ok(String::new()) }),
};

fn saved_credentials(dirs: &Dirs) -> Result<(Credentials, String), String> {
    let cache = Cache::new(Some(dirs.data.clone()), None, None, None).map_err(err)?;
    let credentials = cache.credentials().ok_or("no saved sign-in: open Baliset and sign in first")?;
    let device_id = std::fs::read_to_string(dirs.data.join("device_id")).ok().map(|id| id.trim().to_owned()).filter(|id| id.len() >= 32);
    Ok((credentials, device_id.unwrap_or_else(|| SessionConfig::default().device_id)))
}

async fn connect(dirs: &Dirs) -> Result<Session, String> {
    let (credentials, device_id) = saved_credentials(dirs)?;
    let session = Session::new(SessionConfig { device_id, ..SessionConfig::default() }, None);
    match tokio::time::timeout(TIMEOUT, session.connect(credentials, false)).await {
        Ok(Ok(())) => Ok(session),
        Ok(Err(error)) => Err(err(error)),
        Err(_) => Err(format!("no answer in {} s", TIMEOUT.as_secs())),
    }
}

async fn report() -> Report {
    let dirs = Dirs::resolve();
    let versions = Versions { baliset: env!("CARGO_PKG_VERSION"), librespot: librespot_core::version::SEMVER, rune: rune_rev() };
    let started = Instant::now();
    let mut checks = Vec::new();
    match connect(&dirs).await {
        Ok(session) => {
            let user = session.username();
            checks.push(line(&CONNECT, Ok("signed in".into()), started, &user));
            let probe = Probe { catalog: Catalog::new(session.clone()), session, seeds: Mutex::new(Seeds::default()) };
            for check in CHECKS {
                let started = Instant::now();
                let outcome = tokio::time::timeout(TIMEOUT, (check.run)(&probe)).await.unwrap_or_else(|_| Err(format!("no answer in {} s", TIMEOUT.as_secs())));
                checks.push(line(check, outcome, started, &user));
            }
            probe.session.shutdown();
        }
        Err(error) => {
            checks.push(line(&CONNECT, Err(error), started, ""));
            for check in CHECKS {
                checks.push(line(check, Err("not run: no session".into()), Instant::now(), ""));
            }
        }
    }
    let passed = checks.iter().filter(|line| line.passed).count();
    let failed = checks.len() - passed;
    Report { versions, checks, passed, failed }
}

fn print_text(report: &Report) {
    let v = &report.versions;
    println!("baliset {} · librespot {} · rune {}", v.baliset, v.librespot, v.rune);
    let feature_w = report.checks.iter().map(|line| line.feature.len()).max().unwrap_or(0);
    let endpoint_w = report.checks.iter().map(|line| line.endpoint.len()).max().unwrap_or(0);
    for line in &report.checks {
        let verdict = if line.passed { "PASS" } else { "FAIL" };
        let mut out = format!("{verdict}  {:feature_w$}  {:4} {:endpoint_w$}  {:>5} ms  {}", line.feature, line.method, line.endpoint, line.ms, line.detail);
        if let (Some(used_by), Some(source)) = (line.used_by, line.source) {
            out.push_str(&format!(" | breaks: {used_by} | call: {source}"));
        }
        println!("{out}");
    }
    let total = report.passed + report.failed;
    if report.failed == 0 {
        println!("all {total} checks passed");
    } else {
        println!("{} of {total} checks passed, {} failed", report.passed, report.failed);
    }
}

/// Runs every check and prints the report; the process's exit code: 0 when all passed.
pub(crate) fn run(json: bool) -> i32 {
    let runtime = match tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build() {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("can't start: {error}");
            return 2;
        }
    };
    let report = runtime.block_on(report());
    if json {
        match serde_json::to_string_pretty(&report) {
            Ok(text) => println!("{text}"),
            Err(error) => eprintln!("can't write the report: {error}"),
        }
    } else {
        print_text(&report);
    }
    i32::from(report.failed > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rune_rev_comes_from_the_manifest() {
        let rev = rune_rev();
        assert_eq!(rev.len(), 12, "{rev}");
        assert!(rev.chars().all(|c| c.is_ascii_hexdigit()), "{rev}");
    }

    #[test]
    fn the_account_name_never_reaches_the_report() {
        assert_eq!(scrub("GET /recently-played/v3/user/someone/recently-played: 404", "someone"), "GET /recently-played/v3/user/{user}/recently-played: 404");
        assert_eq!(scrub("fine", ""), "fine");
    }

    #[test]
    fn every_check_names_its_endpoint_and_what_depends_on_it() {
        for check in CHECKS {
            assert!(!check.feature.is_empty() && !check.endpoint.is_empty() && !check.used_by.is_empty(), "{}", check.feature);
            assert!(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(check.source).is_file(), "{} names {}", check.feature, check.source);
        }
    }
}
