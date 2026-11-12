//! Podcasts and follows in the store: followed shows and artists, show pages, Your Episodes,
//! the details episodes carry, and following or unfollowing.

use std::collections::HashMap;
use std::sync::Arc;

use super::super::catalog::{Catalog, SAVED_EPISODES};
use super::{Library, MAX_TRACKS, kind_of};
use crate::model::{ContextKind, ContextSummary, Details, EPISODES_URI, Loadable, Store, Track};

pub(super) fn episodes_summary() -> ContextSummary {
    ContextSummary { uri: EPISODES_URI.into(), name: "Your Episodes".into(), subtitle: String::new(), image_url: None, kind: ContextKind::Episodes }
}

/// The store's list of followed shows or artists, whichever `uri` would be in.
fn follow_list<'a>(store: &'a mut Store, uri: &str) -> Option<&'a mut Loadable<Vec<ContextSummary>>> {
    match kind_of(uri) {
        ContextKind::Show => Some(&mut store.shows),
        ContextKind::Artist => Some(&mut store.artists),
        _ => None,
    }
}

/// Put `summary` at the top of its list, or take its URI out.
fn set_follow(store: &mut Store, uri: &str, summary: Option<&ContextSummary>) {
    let Some(list) = follow_list(store, uri).and_then(Loadable::ready_mut) else { return };
    list.retain(|item| item.uri != uri);
    if let Some(summary) = summary {
        list.insert(0, summary.clone());
    }
}

impl Library {
    /// Episodes among `uris`, described by the catalogue; their details land in the store.
    pub(super) async fn describe_episodes(&self, catalog: &Catalog, uris: &[String]) -> Result<HashMap<String, Track>, String> {
        if !uris.iter().any(|uri| uri.starts_with("spotify:episode:")) {
            return Ok(HashMap::new());
        }
        let episodes = catalog.episodes(uris).await.map_err(|e| e.to_string())?;
        let mut tracks = HashMap::with_capacity(episodes.len());
        let mut details = Vec::with_capacity(episodes.len());
        for (uri, (track, about)) in episodes {
            details.push((uri.clone(), about));
            tracks.insert(uri, track);
        }
        self.shared.update(|store| store.details.extend(details));
        Ok(tracks)
    }

    /// Followed shows and artists, side by side.
    pub(super) async fn load_follows(&self, catalog: &Catalog) {
        self.shared.update(|store| {
            for list in [&mut store.shows, &mut store.artists] {
                if list.ready().is_none() {
                    *list = Loadable::Loading;
                }
            }
        });
        let shows = async {
            let uris = catalog.followed_shows().await?;
            catalog.show_summaries(&uris).await
        };
        let artists = async {
            let uris = catalog.followed_artists().await?;
            catalog.artist_summaries(&uris).await
        };
        let (shows, artists) = tokio::join!(shows, artists);
        self.shared.update(|store| {
            for (list, loaded, what) in [(&mut store.shows, shows, "podcasts"), (&mut store.artists, artists, "artists")] {
                match loaded {
                    Ok(found) => *list = Loadable::Ready(found),
                    Err(error) if list.ready().is_none() => *list = Loadable::Failed(error.to_string()),
                    Err(error) => log::warn!("can't refresh followed {what}: {error}"),
                }
            }
        });
    }

    pub(super) async fn load_show(&self, catalog: &Catalog, uri: &str) -> Result<(), String> {
        let show = catalog.show(uri).await.map_err(|e| e.to_string())?;
        self.shared.update(|store| store.details.insert(uri.to_owned(), Details { description: show.description, released: None }));
        let truncated = show.episode_uris.len() > MAX_TRACKS;
        let uris = &show.episode_uris[..show.episode_uris.len().min(MAX_TRACKS)];
        self.fill_page(catalog, show.summary, uris, truncated).await
    }

    /// Your Episodes, newest saved first.
    pub(super) async fn load_saved_episodes(&self, catalog: &Catalog) -> Result<(), String> {
        let mut uris = catalog.saved(SAVED_EPISODES).await.map_err(|e| e.to_string())?;
        uris.retain(|uri| uri.starts_with("spotify:episode:"));
        let truncated = uris.len() > MAX_TRACKS;
        uris.truncate(MAX_TRACKS);
        self.fill_page(catalog, episodes_summary(), &uris, truncated).await
    }

    /// Follow or unfollow a show or an artist. The lists change at once and change back if
    /// Spotify refuses.
    pub(in crate::engine::spotify) async fn set_followed(self: Arc<Self>, uri: String, followed: bool) {
        let catalog = match self.catalog() {
            Ok(catalog) => catalog,
            Err(offline) => {
                self.shared.update(|store| store.notice = Some(offline));
                return;
            }
        };
        let summary = if followed { self.summary(&catalog, &uri).await } else { None };
        if followed && summary.is_none() {
            self.shared.update(|store| store.notice = Some("Couldn't follow: Spotify doesn't know it".to_owned()));
            return;
        }
        let before = self.shared.update(|store| {
            let before = store.shows.ready().into_iter().chain(store.artists.ready()).flatten().find(|item| item.uri == uri).cloned();
            set_follow(store, &uri, summary.as_ref());
            before
        });
        let _in_order = self.liked_writes.lock().await;
        if let Err(error) = catalog.set_followed(&uri, followed).await {
            self.shared.update(|store| {
                set_follow(store, &uri, before.as_ref());
                store.notice = Some(format!("Couldn't update what you follow: {error}"));
            });
        }
    }

    pub(in crate::engine::spotify) fn episode_left_at(&self, uri: &str, position_ms: u32, duration_ms: u32) {
        self.progress.record(uri, position_ms, duration_ms);
    }

    /// Where to start an episode that Baliset played part of before.
    pub(in crate::engine::spotify) fn resume_at(&self, uri: &str) -> u32 {
        self.progress.resume_at(uri)
    }
}

#[cfg(test)]
mod tests {
    use super::super::known_summary;
    use super::*;

    fn summary(uri: &str) -> ContextSummary {
        ContextSummary { uri: uri.into(), name: uri.into(), kind: kind_of(uri), ..ContextSummary::default() }
    }

    #[test]
    fn following_puts_a_show_or_artist_first_and_unfollowing_takes_it_out() {
        let mut store = Store { shows: Loadable::Ready(vec![summary("spotify:show:a")]), artists: Loadable::Ready(vec![summary("spotify:artist:a")]), ..Store::default() };
        set_follow(&mut store, "spotify:show:b", Some(&summary("spotify:show:b")));
        set_follow(&mut store, "spotify:artist:a", None);
        let uris = |list: &Loadable<Vec<ContextSummary>>| list.ready().unwrap().iter().map(|s| s.uri.clone()).collect::<Vec<_>>();
        assert_eq!(uris(&store.shows), ["spotify:show:b", "spotify:show:a"]);
        assert!(uris(&store.artists).is_empty());
        assert!(store.follows("spotify:show:b") && !store.follows("spotify:artist:a"));
        set_follow(&mut store, "spotify:show:b", Some(&summary("spotify:show:b")));
        assert_eq!(uris(&store.shows).len(), 2, "following twice lists it once");
    }

    #[test]
    fn your_episodes_is_known_by_name() {
        assert_eq!(known_summary(&Store::default(), EPISODES_URI), Some(episodes_summary()));
    }
}
