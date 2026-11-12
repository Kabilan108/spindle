//! Shows, episodes and follows. Followed shows, saved episodes and followed artists are sets
//! of the same collection service that holds Liked Songs.

use std::collections::HashMap;

use librespot_core::Error;
use librespot_protocol::extension_kind::ExtensionKind;
use librespot_protocol::metadata as proto;
use protobuf::Message;

use super::{BATCH, Catalog, artist_summary, cover, uri};
use crate::model::{ContextKind, ContextSummary, Day, Details, NamedRef, Track};

pub(in crate::engine::spotify) const SAVED_EPISODES: &str = "listenlater";
const FOLLOWED_SHOWS: &str = "show";
const FOLLOWED_ARTISTS: &str = "artist";

pub(in crate::engine::spotify) struct Show {
    pub summary: ContextSummary,
    pub description: String,
    /// In Spotify's order: newest first for most shows, first chapter first for a book.
    pub episode_uris: Vec<String>,
}

/// Collapses the line breaks and runs of spaces of a description.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Undated episodes come with a zeroed date.
fn day(date: &proto::Date) -> Option<Day> {
    let (month, day) = (u32::try_from(date.month()).ok()?, u32::try_from(date.day()).ok()?);
    (date.year() > 0 && (1..=12).contains(&month) && (1..=31).contains(&day)).then_some(Day { year: date.year(), month, day })
}

fn show_summary(show_uri: &str, message: &proto::Show) -> ContextSummary {
    ContextSummary { uri: show_uri.to_owned(), name: message.name().to_owned(), subtitle: message.publisher().to_owned(), image_url: cover(&message.cover_image), kind: ContextKind::Show }
}

/// An episode as a row: its show stands where a song's artist and album would.
fn episode(episode_uri: &str, message: &proto::Episode) -> (Track, Details) {
    let show = NamedRef { uri: uri("show", message.show.gid()).unwrap_or_default(), name: message.show.name().to_owned() };
    let track = Track {
        uri: episode_uri.to_owned(),
        name: message.name().trim().to_owned(),
        artists: vec![show.clone()],
        album: Some(show),
        duration_ms: u32::try_from(message.duration()).unwrap_or(0),
        image_url: cover(&message.cover_image).or_else(|| cover(&message.show.cover_image)),
        explicit: message.explicit(),
        playable: true,
    };
    let details = Details { description: one_line(message.description()), released: message.publish_time.as_ref().and_then(day) };
    (track, details)
}

impl Catalog {
    pub(in crate::engine::spotify) async fn show(&self, show_uri: &str) -> Result<Show, Error> {
        let mut answer = self.metadata(&[show_uri.to_owned()], ExtensionKind::SHOW_V4).await?;
        let (_, bytes) = answer.pop().ok_or_else(|| Error::not_found("Spotify doesn't know this show"))?;
        let message = proto::Show::parse_from_bytes(&bytes)?;
        let episode_uris = message.episode.iter().filter_map(|episode| uri("episode", episode.gid())).collect();
        Ok(Show { summary: show_summary(show_uri, &message), description: one_line(message.description()), episode_uris })
    }

    /// Show summaries in the order of `uris`, skipping any Spotify doesn't answer for.
    pub(in crate::engine::spotify) async fn show_summaries(&self, uris: &[String]) -> Result<Vec<ContextSummary>, Error> {
        let mut found = HashMap::with_capacity(uris.len());
        for chunk in uris.chunks(BATCH) {
            for (entity_uri, bytes) in self.metadata(chunk, ExtensionKind::SHOW_V4).await? {
                if let Ok(message) = proto::Show::parse_from_bytes(&bytes) {
                    found.insert(entity_uri.clone(), show_summary(&entity_uri, &message));
                }
            }
        }
        Ok(uris.iter().filter_map(|uri| found.remove(uri)).collect())
    }

    pub(in crate::engine::spotify) async fn artist_summaries(&self, uris: &[String]) -> Result<Vec<ContextSummary>, Error> {
        let mut found = HashMap::with_capacity(uris.len());
        for chunk in uris.chunks(BATCH) {
            for (entity_uri, bytes) in self.metadata(chunk, ExtensionKind::ARTIST_V4).await? {
                if let Ok(message) = proto::Artist::parse_from_bytes(&bytes) {
                    found.insert(entity_uri.clone(), artist_summary(&entity_uri, &message));
                }
            }
        }
        Ok(uris.iter().filter_map(|uri| found.remove(uri)).collect())
    }

    /// Episodes by URI. `uris` may be any length; other URIs are ignored.
    pub(in crate::engine::spotify) async fn episodes(&self, uris: &[String]) -> Result<HashMap<String, (Track, Details)>, Error> {
        let wanted: Vec<String> = uris.iter().filter(|uri| uri.starts_with("spotify:episode:")).cloned().collect();
        let mut episodes = HashMap::with_capacity(wanted.len());
        for chunk in wanted.chunks(BATCH) {
            for (entity_uri, bytes) in self.metadata(chunk, ExtensionKind::EPISODE_V4).await? {
                if let Ok(message) = proto::Episode::parse_from_bytes(&bytes) {
                    episodes.insert(entity_uri.clone(), episode(&entity_uri, &message));
                }
            }
        }
        Ok(episodes)
    }

    pub(in crate::engine::spotify) async fn followed_shows(&self) -> Result<Vec<String>, Error> {
        self.saved(FOLLOWED_SHOWS).await
    }

    pub(in crate::engine::spotify) async fn followed_artists(&self) -> Result<Vec<String>, Error> {
        self.saved(FOLLOWED_ARTISTS).await
    }

    /// Follow a show or an artist, or stop.
    pub(in crate::engine::spotify) async fn set_followed(&self, item_uri: &str, followed: bool) -> Result<(), Error> {
        let set = follow_set(item_uri).ok_or_else(|| Error::invalid_argument("Only shows and artists can be followed"))?;
        self.set_saved_in(set, item_uri, followed).await
    }
}

fn follow_set(item_uri: &str) -> Option<&'static str> {
    if item_uri.starts_with("spotify:show:") {
        Some(FOLLOWED_SHOWS)
    } else if item_uri.starts_with("spotify:artist:") {
        Some(FOLLOWED_ARTISTS)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use librespot_core::SpotifyId;

    use super::*;

    fn gid(base62: &str) -> Vec<u8> {
        SpotifyId::from_base62(base62).unwrap().to_raw().to_vec()
    }

    fn images(id: u8) -> proto::ImageGroup {
        let mut image = proto::Image::new();
        image.set_width(300);
        image.set_file_id(vec![id]);
        proto::ImageGroup { image: vec![image], ..Default::default() }
    }

    fn date(year: i32, month: i32, day: i32) -> proto::Date {
        let mut date = proto::Date::new();
        date.set_year(year);
        date.set_month(month);
        date.set_day(day);
        date
    }

    #[test]
    fn descriptions_come_out_on_one_line() {
        assert_eq!(one_line("  Part one.\n\nPart  two.\t"), "Part one. Part two.");
        assert_eq!(one_line(""), "");
    }

    #[test]
    fn a_zeroed_or_impossible_date_is_no_date() {
        assert_eq!(day(&date(2026, 11, 6)), Some(Day { year: 2026, month: 11, day: 6 }));
        assert_eq!(day(&date(0, 0, 0)), None);
        assert_eq!(day(&date(2026, 13, 1)), None);
        assert_eq!(day(&proto::Date::new()), None);
    }

    #[test]
    fn an_episode_names_its_show_as_artist_and_album() {
        let mut show = proto::Show::new();
        show.set_gid(gid("01A062kejnXFkJE01bjN5J"));
        show.set_name("The Standup".into());
        show.cover_image = Some(images(9)).into();
        let mut message = proto::Episode::new();
        message.set_name("The Death of Stack Overflow ".into());
        message.set_duration(3_991_579);
        message.set_description("[Sponsor] Linear.\nThen the episode.".into());
        message.publish_time = Some(date(2026, 11, 6)).into();
        message.show = Some(show).into();
        let (track, details) = episode("spotify:episode:0KhbTiRLZGEr1SlEVRcbKO", &message);
        let show_ref = NamedRef { uri: "spotify:show:01A062kejnXFkJE01bjN5J".into(), name: "The Standup".into() };
        assert_eq!(track.name, "The Death of Stack Overflow");
        assert_eq!((track.artists.as_slice(), track.album.as_ref()), (std::slice::from_ref(&show_ref), Some(&show_ref)));
        assert_eq!(track.duration_ms, 3_991_579);
        assert_eq!(track.image_url.as_deref(), Some("https://i.scdn.co/image/09"), "without art of its own it takes the show's");
        assert_eq!(details, Details { description: "[Sponsor] Linear. Then the episode.".into(), released: Some(Day { year: 2026, month: 11, day: 6 }) });

        message.cover_image = Some(images(4)).into();
        assert_eq!(episode("spotify:episode:x", &message).0.image_url.as_deref(), Some("https://i.scdn.co/image/04"));
    }

    #[test]
    fn a_show_is_summarised_by_its_publisher() {
        let mut message = proto::Show::new();
        message.set_name("Heretics of Dune".into());
        message.set_publisher("Frank Herbert".into());
        message.cover_image = Some(images(1)).into();
        let summary = show_summary("spotify:show:0g6GRLM7bmcjj5svbkYsln", &message);
        assert_eq!((summary.name.as_str(), summary.subtitle.as_str(), summary.kind), ("Heretics of Dune", "Frank Herbert", ContextKind::Show));
        assert_eq!(summary.image_url.as_deref(), Some("https://i.scdn.co/image/01"));
    }

    #[test]
    fn only_shows_and_artists_can_be_followed() {
        assert_eq!(follow_set("spotify:show:a"), Some("show"));
        assert_eq!(follow_set("spotify:artist:a"), Some("artist"));
        assert_eq!(follow_set("spotify:playlist:a"), None);
    }
}
