//! The engine without a window: prints what the store holds as it changes and takes
//! commands on stdin. For checking sign-in, the Connect device and the library by hand.
//!
//! ```sh
//! cargo run --features spotify --bin baliset-engine
//! login | play | next | prev | seek MS | vol 0.5 | shuffle on|off
//! ctx URI [TRACK_URI] | load URI | search WORDS | like URI | unlike URI | refresh | state | logout | quit
//! queue URI | skipto URI | unqueue INDEX | upnext | radio TRACK_URI | shuffled URI | find NAME | tracks URI
//! addto PLAYLIST_URI TRACK_URI | removefrom PLAYLIST_URI TRACK_URI INDEX
//! ```
#![allow(dead_code)]

#[path = "../engine/mod.rs"]
mod engine;
#[path = "../model.rs"]
mod model;

use std::io::BufRead;

use model::{Command, Loadable, Store};

fn summary(store: &Store) -> String {
    let playback = &store.playback;
    let track = playback.track.as_ref().map_or_else(|| "nothing".to_owned(), |t| format!("{} by {} [{}]", t.name, t.artist_line(), t.uri));
    let load = |state: &Loadable<Vec<model::ContextSummary>>| match state {
        Loadable::Idle => "idle".to_owned(),
        Loadable::Loading => "loading".to_owned(),
        Loadable::Ready(list) => list.len().to_string(),
        Loadable::Failed(error) => format!("failed: {error}"),
    };
    format!(
        "session {:?} | {} {} at {} ms, active {}, loading {}, vol {:.2}, shuffle {}, repeat {:?}, context {:?} | playlists {}, albums {}, recents {}, liked {}, pages {} | notice {:?}",
        store.session,
        if playback.playing { "playing" } else { "paused" },
        track,
        playback.position_now(),
        playback.active,
        playback.loading,
        playback.volume,
        playback.shuffle,
        playback.repeat,
        playback.context_uri,
        load(&store.playlists),
        load(&store.albums),
        store.recents.len(),
        store.liked.len(),
        store.pages.len(),
        store.notice,
    )
}

fn detail(store: &Store) {
    for recent in &store.recents {
        println!("  recent: {} ({}) {}", recent.name, recent.subtitle, recent.uri);
    }
    for (uri, page) in &store.pages {
        match page {
            Loadable::Ready(page) => {
                println!("  page {uri}: {} with {} tracks, {} albums, truncated {}", page.summary.name, page.tracks.len(), page.albums.len(), page.truncated);
                for track in page.tracks.iter().take(5) {
                    println!("    {} by {} [{}] liked {}", track.name, track.artist_line(), track.uri, store.liked.contains(&track.uri));
                }
            }
            other => println!("  page {uri}: {other:?}"),
        }
    }
    up_next(store);
    for (seed, radio) in &store.radios {
        println!("  radio for {seed}: {radio:?}");
    }
    println!("  editable playlists: {}", store.editable.len());
    if let Loadable::Ready(results) = &store.search.results {
        println!("  search {:?}: {} tracks, {} albums, {} artists, {} playlists", store.search.query, results.tracks.len(), results.albums.len(), results.artists.len(), results.playlists.len());
        for track in results.tracks.iter().take(5) {
            println!("    {} by {} [{}]", track.name, track.artist_line(), track.uri);
        }
    } else {
        println!("  search: {:?}", store.search.results);
    }
}

fn up_next(store: &Store) {
    let line = |track: &model::Track| format!("{} by {} [{}]", track.name, track.artist_line(), track.uri);
    println!("  up next: {} queued, {} from the context", store.up_next.queued.len(), store.up_next.upcoming.len());
    for track in &store.up_next.queued {
        println!("    queued: {}", line(track));
    }
    for track in store.up_next.upcoming.iter().take(3) {
        println!("    then: {}", line(track));
    }
}

fn main() -> std::io::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let engine = engine::Engine::spotify()?;
    let watcher = engine.clone();
    let last = std::sync::Mutex::new(String::new());
    engine.subscribe(move || {
        let line = watcher.read(summary);
        let mut last = last.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if *last != line {
            println!("{line}");
            *last = line;
        }
    });
    for line in std::io::stdin().lock().lines() {
        let line = line?;
        let (word, rest) = line.trim().split_once(' ').unwrap_or((line.trim(), ""));
        let rest = rest.trim().to_owned();
        let command = match word {
            "login" => Command::Login,
            "logout" => Command::Logout,
            "play" => Command::PlayPause,
            "next" => Command::Next,
            "prev" => Command::Previous,
            "seek" => Command::Seek { position_ms: rest.parse().unwrap_or(0) },
            "vol" => Command::SetVolume(rest.parse().unwrap_or(0.5)),
            "shuffle" => Command::SetShuffle(rest == "on"),
            "ctx" => {
                let (uri, track) = rest.split_once(' ').map_or((rest.clone(), None), |(uri, track)| (uri.to_owned(), Some(track.to_owned())));
                Command::PlayContext { uri, track_uri: track }
            }
            "load" => Command::LoadContext { uri: rest },
            "search" => Command::Search { query: rest },
            "like" => Command::SetLiked { uri: rest, liked: true },
            "unlike" => Command::SetLiked { uri: rest, liked: false },
            "refresh" => Command::RefreshLibrary,
            "queue" => Command::AddToQueue { uri: rest },
            "skipto" => Command::SkipTo { uri: rest },
            "unqueue" => Command::RemoveFromQueue { index: rest.parse().unwrap_or(0) },
            "radio" => Command::LoadRadio { track_uri: rest },
            "shuffled" => Command::PlayShuffled { uri: rest },
            "addto" => {
                let (playlist_uri, track_uri) = rest.split_once(' ').unwrap_or((&rest, ""));
                Command::AddToPlaylist { playlist_uri: playlist_uri.to_owned(), track_uri: track_uri.trim().to_owned() }
            }
            "removefrom" => {
                let parts: Vec<&str> = rest.split_whitespace().collect();
                let [playlist_uri, track_uri, index] = parts[..] else {
                    println!("removefrom PLAYLIST_URI TRACK_URI INDEX");
                    continue;
                };
                Command::RemoveFromPlaylist { playlist_uri: playlist_uri.to_owned(), track_uri: track_uri.to_owned(), index: index.parse().unwrap_or(0) }
            }
            "upnext" => {
                engine.read(up_next);
                continue;
            }
            "tracks" => {
                engine.read(|store| match store.pages.get(&rest).and_then(Loadable::ready) {
                    Some(page) => println!("  tracks {} {}", page.tracks.len(), page.tracks.iter().map(|t| t.uri.as_str()).collect::<Vec<_>>().join(",")),
                    None => println!("  tracks: {rest} isn't loaded"),
                });
                continue;
            }
            "find" => {
                engine.read(|store| {
                    let playlists = store.playlists.ready().map_or(&[][..], Vec::as_slice);
                    for playlist in playlists.iter().filter(|p| p.name.eq_ignore_ascii_case(&rest)) {
                        println!("  found {} {} editable {}", playlist.name, playlist.uri, store.editable.contains(&playlist.uri));
                    }
                });
                continue;
            }
            "state" => {
                engine.read(|store| {
                    println!("{}", summary(store));
                    detail(store);
                });
                continue;
            }
            "quit" => break,
            "" => continue,
            other => {
                println!("unknown command {other:?}");
                continue;
            }
        };
        engine.send(command);
    }
    Ok(())
}
