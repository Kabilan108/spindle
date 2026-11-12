//! The account's library and the catalogue's metadata, read through the session the way
//! librespot and the official client do: playlist, collection, listening-history and
//! extended-metadata services on the session's own access point. The public Web API is
//! not an option here: it refuses the desktop client id with a blanket 429.

use std::collections::{HashMap, HashSet};
use std::time::{SystemTime, UNIX_EPOCH};

use http::header::{ACCEPT, CONTENT_TYPE};
use http::{HeaderMap, HeaderValue, Method};
use librespot_core::{Error, Session, SpotifyId};
use librespot_protocol::extended_metadata::{BatchedEntityRequest, EntityRequest, ExtensionQuery};
use librespot_protocol::extension_kind::ExtensionKind;
use librespot_protocol::metadata as proto;
use librespot_protocol::playlist4_external::{ListAttributes, MetaItem, SelectedListContent};
use protobuf::{EnumOrUnknown, Message};

use super::wire::{self, Field};
use crate::model::{ContextKind, ContextSummary, NamedRef, Track};

mod podcasts;

pub(super) use podcasts::SAVED_EPISODES;

/// Entities per extended-metadata request.
pub(super) const BATCH: usize = 100;
const COVER_WIDTH: i32 = 300;
const COLLECTION_TYPE: &str = "application/vnd.collection-v2.spotify.proto";
const COLLECTION_PAGE: u64 = 300;

fn image_url(file_id: &[u8]) -> Option<String> {
    (!file_id.is_empty()).then(|| {
        let hex: String = file_id.iter().map(|byte| format!("{byte:02x}")).collect();
        format!("https://i.scdn.co/image/{hex}")
    })
}

fn cover(group: &proto::ImageGroup) -> Option<String> {
    let nearest = group.image.iter().min_by_key(|image| (image.width() - COVER_WIDTH).abs())?;
    image_url(nearest.file_id())
}

fn uri(kind: &str, gid: &[u8]) -> Option<String> {
    let id = SpotifyId::from_raw(gid).ok()?.to_base62().ok()?;
    Some(format!("spotify:{kind}:{id}"))
}

fn artists(list: &[proto::Artist]) -> Vec<NamedRef> {
    list.iter().map(|artist| NamedRef { uri: uri("artist", artist.gid()).unwrap_or_default(), name: artist.name().to_owned() }).collect()
}

fn track(entity_uri: &str, message: &proto::Track) -> Track {
    Track {
        uri: entity_uri.to_owned(),
        name: message.name().to_owned(),
        artists: artists(&message.artist),
        album: message.album.as_ref().map(|album| NamedRef { uri: uri("album", album.gid()).unwrap_or_default(), name: album.name().to_owned() }),
        duration_ms: u32::try_from(message.duration()).unwrap_or(0),
        image_url: message.album.as_ref().and_then(|album| cover(&album.cover_group)),
        explicit: message.explicit(),
        playable: true,
    }
}

fn album_summary(entity_uri: &str, message: &proto::Album) -> ContextSummary {
    let names: Vec<&str> = message.artist.iter().map(|artist| artist.name()).collect();
    ContextSummary { uri: entity_uri.to_owned(), name: message.name().to_owned(), subtitle: names.join(", "), image_url: cover(&message.cover_group), kind: ContextKind::Album }
}

fn artist_summary(entity_uri: &str, message: &proto::Artist) -> ContextSummary {
    ContextSummary { uri: entity_uri.to_owned(), name: message.name().to_owned(), subtitle: "Artist".into(), image_url: cover(&message.portrait_group), kind: ContextKind::Artist }
}

/// A playlist's cover: its own picture, else the one its generator attached.
fn playlist_image(attributes: &ListAttributes) -> Option<String> {
    let sized = attributes.picture_size.iter().find(|size| size.target_name() == "default").or(attributes.picture_size.first());
    if let Some(sized) = sized.filter(|size| size.url().starts_with("https://")) {
        return Some(sized.url().to_owned());
    }
    image_url(attributes.picture()).or_else(|| {
        let attached = attributes.format_attributes.iter().find(|attribute| attribute.key() == "image_url")?;
        attached.value().starts_with("https://").then(|| attached.value().to_owned())
    })
}

fn playlist_summary(playlist_uri: &str, attributes: &ListAttributes, owner: &str) -> ContextSummary {
    ContextSummary { uri: playlist_uri.to_owned(), name: attributes.name().to_owned(), subtitle: owner.to_owned(), image_url: playlist_image(attributes), kind: ContextKind::Playlist }
}

/// The account's playlists, and which of them it may change.
#[derive(Default)]
pub(super) struct Rootlist {
    pub playlists: Vec<ContextSummary>,
    pub editable: HashSet<String>,
}

pub(super) struct Playlist {
    pub summary: ContextSummary,
    pub track_uris: Vec<String>,
}

pub(super) struct Album {
    pub summary: ContextSummary,
    pub track_uris: Vec<String>,
}

pub(super) struct Artist {
    pub summary: ContextSummary,
    pub top_track_uris: Vec<String>,
    pub album_uris: Vec<String>,
}

/// What the collection holds: liked tracks and saved albums, newest first.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Collection {
    pub tracks: Vec<String>,
    pub albums: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct CollectionPage {
    /// `(uri, added_at)` of items that are present.
    pub items: Vec<(String, u64)>,
    pub next: Option<String>,
}

pub(super) fn collection_page(body: &[u8]) -> Option<CollectionPage> {
    let mut page = CollectionPage { items: Vec::new(), next: None };
    for field in wire::fields(body)? {
        match field {
            Field::Bytes(1, item) => {
                let (mut uri, mut added_at, mut removed) = (None, 0, false);
                for field in wire::fields(item)? {
                    match field {
                        Field::Bytes(1, bytes) => uri = std::str::from_utf8(bytes).ok().map(str::to_owned),
                        Field::Varint(2, at) => added_at = at,
                        Field::Varint(3, flag) => removed = flag != 0,
                        _ => {}
                    }
                }
                if let Some(uri) = uri.filter(|_| !removed) {
                    page.items.push((uri, added_at));
                }
            }
            Field::Bytes(2, token) if !token.is_empty() => page.next = std::str::from_utf8(token).ok().map(str::to_owned),
            _ => {}
        }
    }
    Some(page)
}

/// The collection's sets: Liked Songs and saved albums share this one.
const COLLECTION_SET: &str = "collection";

fn page_request(username: &str, set: &str, token: Option<&str>) -> Vec<u8> {
    let mut body = Vec::new();
    wire::put_bytes(&mut body, 1, username.as_bytes());
    wire::put_bytes(&mut body, 2, set.as_bytes());
    if let Some(token) = token {
        wire::put_bytes(&mut body, 3, token.as_bytes());
    }
    wire::put_varint(&mut body, 4, COLLECTION_PAGE);
    body
}

fn write_request(username: &str, set: &str, item_uri: &str, present: bool, now: u64) -> Vec<u8> {
    let mut item = Vec::new();
    wire::put_bytes(&mut item, 1, item_uri.as_bytes());
    wire::put_varint(&mut item, 2, now);
    wire::put_varint(&mut item, 3, u64::from(!present));
    let mut body = Vec::new();
    wire::put_bytes(&mut body, 1, username.as_bytes());
    wire::put_bytes(&mut body, 2, set.as_bytes());
    wire::put_bytes(&mut body, 3, &item);
    wire::put_bytes(&mut body, 4, format!("{now:x}").as_bytes());
    body
}

/// Whether the account may add and remove a listed playlist's tracks. The list says so in its
/// capabilities; without them, owning the playlist or its being collaborative is enough.
fn may_edit(meta: &MetaItem, username: &str) -> bool {
    match meta.capabilities.as_ref() {
        Some(capabilities) if capabilities.has_can_edit_items() => capabilities.can_edit_items(),
        _ => meta.owner_username() == username || meta.attributes.collaborative(),
    }
}

pub(super) fn unreadable(what: &str) -> Error {
    Error::failed_precondition(format!("Spotify sent an unreadable {what}"))
}

/// A connected session's view of the catalogue. Cheap to clone.
#[derive(Clone)]
pub(super) struct Catalog {
    session: Session,
}

impl Catalog {
    pub(super) fn new(session: Session) -> Self {
        Self { session }
    }

    pub(super) fn username(&self) -> String {
        self.session.username()
    }

    pub(super) fn session(&self) -> &Session {
        &self.session
    }

    /// The account's display name.
    pub(super) async fn display_name(&self) -> Result<String, Error> {
        let body = self.session.spclient().get_user_profile(&self.username(), None, None).await?;
        let profile: serde_json::Value = serde_json::from_slice(&body)?;
        profile["name"].as_str().map(str::to_owned).ok_or_else(|| unreadable("profile"))
    }

    /// Every playlist in the account's list, in its order. Folders are flattened away.
    pub(super) async fn playlists(&self, limit: usize) -> Result<Rootlist, Error> {
        let username = self.username();
        let mut rootlist = Rootlist::default();
        let mut from = 0;
        loop {
            let body = self.session.spclient().get_rootlist(from, Some(BATCH)).await?;
            let list = SelectedListContent::parse_from_bytes(&body)?;
            let contents = &list.contents;
            for (item, meta) in contents.items.iter().zip(&contents.meta_items) {
                if item.uri().starts_with("spotify:playlist:") && meta.attributes.is_some() {
                    rootlist.playlists.push(playlist_summary(item.uri(), &meta.attributes, meta.owner_username()));
                    if may_edit(meta, &username) {
                        rootlist.editable.insert(item.uri().to_owned());
                    }
                }
            }
            from += contents.items.len();
            let total = usize::try_from(list.length()).unwrap_or(0);
            if contents.items.is_empty() || from >= total || rootlist.playlists.len() >= limit {
                return Ok(rootlist);
            }
        }
    }

    /// A playlist's name, owner and cover without its tracks.
    pub(super) async fn playlist_summary(&self, playlist_uri: &str) -> Result<ContextSummary, Error> {
        Ok(self.playlist_slice(playlist_uri, 0, 1).await?.0.summary)
    }

    async fn playlist_slice(&self, playlist_uri: &str, from: usize, length: usize) -> Result<(Playlist, usize), Error> {
        let id = playlist_uri.rsplit(':').next().unwrap_or_default();
        let endpoint = format!("/playlist/v2/playlist/{id}?from={from}&length={length}&decorate=revision,attributes,length,owner");
        let body = self.session.spclient().request(&Method::GET, &endpoint, None, None).await?;
        let list = SelectedListContent::parse_from_bytes(&body)?;
        let summary = playlist_summary(playlist_uri, &list.attributes, list.owner_username());
        let track_uris = list.contents.items.iter().map(|item| item.uri().to_owned()).collect();
        Ok((Playlist { summary, track_uris }, usize::try_from(list.length()).unwrap_or(0)))
    }

    /// A playlist with the URIs of up to `limit` of its entries, in order.
    pub(super) async fn playlist(&self, playlist_uri: &str, limit: usize) -> Result<(Playlist, bool), Error> {
        const SLICE: usize = 500;
        let (mut playlist, total) = self.playlist_slice(playlist_uri, 0, SLICE.min(limit)).await?;
        while playlist.track_uris.len() < total.min(limit) {
            let (more, _) = self.playlist_slice(playlist_uri, playlist.track_uris.len(), SLICE).await?;
            if more.track_uris.is_empty() {
                break;
            }
            playlist.track_uris.extend(more.track_uris);
        }
        playlist.track_uris.truncate(limit);
        Ok((playlist, total > limit))
    }

    /// One extended-metadata request for up to [`BATCH`] entities of one kind, as
    /// `(entity URI, encoded message)`. Entities Spotify doesn't answer for are absent.
    async fn metadata(&self, uris: &[String], kind: ExtensionKind) -> Result<Vec<(String, Vec<u8>)>, Error> {
        let request = BatchedEntityRequest {
            entity_request: uris
                .iter()
                .map(|uri| EntityRequest { entity_uri: uri.clone(), query: vec![ExtensionQuery { extension_kind: EnumOrUnknown::new(kind), ..Default::default() }], ..Default::default() })
                .collect(),
            ..Default::default()
        };
        let answer = self.session.spclient().get_extended_metadata(request).await?;
        let entities = answer.extended_metadata.into_iter().flat_map(|array| array.extension_data);
        Ok(entities.filter_map(|mut data| Some((std::mem::take(&mut data.entity_uri), data.extension_data.take()?.value))).collect())
    }

    /// Tracks by URI. `uris` may be any length; non-track URIs are ignored.
    pub(super) async fn tracks(&self, uris: &[String]) -> Result<HashMap<String, Track>, Error> {
        let wanted: Vec<String> = uris.iter().filter(|uri| uri.starts_with("spotify:track:")).cloned().collect();
        let mut tracks = HashMap::with_capacity(wanted.len());
        for chunk in wanted.chunks(BATCH) {
            for (entity_uri, bytes) in self.metadata(chunk, ExtensionKind::TRACK_V4).await? {
                if let Ok(message) = proto::Track::parse_from_bytes(&bytes) {
                    tracks.insert(entity_uri.clone(), track(&entity_uri, &message));
                }
            }
        }
        Ok(tracks)
    }

    async fn album_messages(&self, uris: &[String]) -> Result<HashMap<String, proto::Album>, Error> {
        let mut albums = HashMap::with_capacity(uris.len());
        for chunk in uris.chunks(BATCH) {
            for (entity_uri, bytes) in self.metadata(chunk, ExtensionKind::ALBUM_V4).await? {
                if let Ok(message) = proto::Album::parse_from_bytes(&bytes) {
                    albums.insert(entity_uri, message);
                }
            }
        }
        Ok(albums)
    }

    /// Album summaries in the order of `uris`, skipping any Spotify doesn't answer for.
    pub(super) async fn albums(&self, uris: &[String]) -> Result<Vec<ContextSummary>, Error> {
        let messages = self.album_messages(uris).await?;
        Ok(uris.iter().filter_map(|uri| Some(album_summary(uri, messages.get(uri)?))).collect())
    }

    pub(super) async fn album(&self, album_uri: &str) -> Result<Album, Error> {
        let mut messages = self.album_messages(&[album_uri.to_owned()]).await?;
        let message = messages.remove(album_uri).ok_or_else(|| Error::not_found("Spotify doesn't know this album"))?;
        let track_uris = message.disc.iter().flat_map(|disc| &disc.track).filter_map(|track| uri("track", track.gid())).collect();
        Ok(Album { summary: album_summary(album_uri, &message), track_uris })
    }

    pub(super) async fn artist(&self, artist_uri: &str, albums: usize) -> Result<Artist, Error> {
        let mut answer = self.metadata(&[artist_uri.to_owned()], ExtensionKind::ARTIST_V4).await?;
        let (_, bytes) = answer.pop().ok_or_else(|| Error::not_found("Spotify doesn't know this artist"))?;
        let message = proto::Artist::parse_from_bytes(&bytes)?;
        let country = self.session.country();
        let top = message.top_track.iter().find(|top| top.country() == country).or(message.top_track.first());
        let top_track_uris = top.into_iter().flat_map(|top| &top.track).filter_map(|track| uri("track", track.gid())).collect();
        let groups = message.album_group.iter().chain(&message.single_group);
        let album_uris = groups.filter_map(|group| uri("album", group.album.first()?.gid())).take(albums).collect();
        Ok(Artist { summary: artist_summary(artist_uri, &message), top_track_uris, album_uris })
    }

    /// The catalogue's tracks for a search, best first. `context` is a `spotify:search:`
    /// URI, which is also what plays the results as a queue.
    pub(super) async fn search_tracks(&self, context: &str) -> Result<Vec<String>, Error> {
        let resolved = self.session.spclient().get_context(context).await?;
        let tracks = resolved.pages.iter().flat_map(|page| &page.tracks);
        Ok(tracks.filter_map(|track| track.uri.clone()).filter(|uri| uri.starts_with("spotify:track:")).collect())
    }

    pub(super) fn collection_headers() -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static(COLLECTION_TYPE));
        headers.insert(ACCEPT, HeaderValue::from_static(COLLECTION_TYPE));
        headers
    }

    /// Liked Songs and saved albums, newest first.
    pub(super) async fn collection(&self) -> Result<Collection, Error> {
        let mut collection = Collection::default();
        for item_uri in self.saved(COLLECTION_SET).await? {
            if item_uri.starts_with("spotify:track:") {
                collection.tracks.push(item_uri);
            } else if item_uri.starts_with("spotify:album:") {
                collection.albums.push(item_uri);
            }
        }
        Ok(collection)
    }

    /// Every item in one of the collection's sets, newest first.
    pub(super) async fn saved(&self, set: &str) -> Result<Vec<String>, Error> {
        let username = self.username();
        let mut items = Vec::new();
        let mut token = None;
        loop {
            let request = page_request(&username, set, token.as_deref());
            let body = self.session.spclient().request(&Method::POST, "/collection/v2/paging", Some(Self::collection_headers()), Some(&request)).await?;
            let page = collection_page(&body).ok_or_else(|| unreadable("collection"))?;
            items.extend(page.items);
            match page.next {
                Some(next) if token.as_deref() != Some(next.as_str()) => token = Some(next),
                _ => break,
            }
        }
        items.sort_by_key(|item| std::cmp::Reverse(item.1));
        Ok(items.into_iter().map(|(item_uri, _)| item_uri).collect())
    }

    /// Add a track to Liked Songs, or take it out.
    pub(super) async fn set_saved(&self, item_uri: &str, saved: bool) -> Result<(), Error> {
        self.set_saved_in(COLLECTION_SET, item_uri, saved).await
    }

    /// Put an item in one of the collection's sets, or take it out.
    async fn set_saved_in(&self, set: &str, item_uri: &str, saved: bool) -> Result<(), Error> {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.as_secs());
        let request = write_request(&self.username(), set, item_uri, saved, now);
        self.session.spclient().request(&Method::POST, "/collection/v2/write", Some(Self::collection_headers()), Some(&request)).await?;
        Ok(())
    }

    /// The playlists, albums and artists played lately, newest first.
    pub(super) async fn recent_contexts(&self, limit: usize) -> Result<Vec<String>, Error> {
        let endpoint = format!("/recently-played/v3/user/{}/recently-played?format=json&offset=0&limit={limit}&filter=default,collection-new-episodes", self.username());
        let body = self.session.spclient().request_as_json(&Method::GET, &endpoint, None, None).await?;
        let history: serde_json::Value = serde_json::from_slice(&body)?;
        let contexts = history["playContexts"].as_array().ok_or_else(|| unreadable("listening history"))?;
        Ok(contexts.iter().filter_map(|context| context["uri"].as_str().map(str::to_owned)).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_and_images_take_spotifys_public_forms() {
        let gid = SpotifyId::from_base62("4uLU6hMCjMI75M1A2tKUQC").unwrap().to_raw();
        assert_eq!(uri("track", &gid).as_deref(), Some("spotify:track:4uLU6hMCjMI75M1A2tKUQC"));
        assert_eq!(uri("track", &[1, 2, 3]), None);
        assert_eq!(image_url(&[0xab, 0x67, 0x01]).as_deref(), Some("https://i.scdn.co/image/ab6701"));
        assert_eq!(image_url(&[]), None);
    }

    #[test]
    fn a_track_message_becomes_a_track_with_its_mid_sized_cover() {
        let image = |width: i32, id: u8| {
            let mut image = proto::Image::new();
            image.set_width(width);
            image.set_file_id(vec![id]);
            image
        };
        let mut artist = proto::Artist::new();
        artist.set_name("Russ".into());
        artist.set_gid(SpotifyId::from_base62("1z7b1Pr1rSlvWRzsW3HOrS").unwrap().to_raw().to_vec());
        let mut album = proto::Album::new();
        album.set_name("W!LD".into());
        album.cover_group = Some(proto::ImageGroup { image: vec![image(640, 1), image(300, 2), image(64, 3)], ..Default::default() }).into();
        let mut message = proto::Track::new();
        message.set_name("Pent Up in a Penthouse".into());
        message.set_duration(195_624);
        message.set_explicit(true);
        message.artist.push(artist);
        message.album = Some(album).into();
        let track = track("spotify:track:0badHcCAFCKnMlwdvqPN8u", &message);
        assert_eq!((track.name.as_str(), track.duration_ms, track.explicit), ("Pent Up in a Penthouse", 195_624, true));
        assert_eq!(track.artists[0], NamedRef { uri: "spotify:artist:1z7b1Pr1rSlvWRzsW3HOrS".into(), name: "Russ".into() });
        assert_eq!(track.image_url.as_deref(), Some("https://i.scdn.co/image/02"));
    }

    #[test]
    fn a_playlists_cover_prefers_its_own_picture() {
        let mut attributes = ListAttributes::new();
        assert_eq!(playlist_image(&attributes), None);
        let mut attached = librespot_protocol::playlist4_external::FormatListAttribute::new();
        attached.set_key("image_url".into());
        attached.set_value("https://i.scdn.co/image/generated".into());
        attributes.format_attributes.push(attached);
        assert_eq!(playlist_image(&attributes).as_deref(), Some("https://i.scdn.co/image/generated"));
        attributes.set_picture(vec![0xab, 0xcd]);
        assert_eq!(playlist_image(&attributes).as_deref(), Some("https://i.scdn.co/image/abcd"));
    }

    #[test]
    fn a_playlist_is_editable_when_its_capabilities_say_so_or_else_when_owned_or_shared() {
        let mut meta = MetaItem::new();
        meta.set_owner_username("me".into());
        assert!(may_edit(&meta, "me"));
        assert!(!may_edit(&meta, "someone"));
        meta.attributes.mut_or_insert_default().set_collaborative(true);
        assert!(may_edit(&meta, "someone"));
        meta.capabilities.mut_or_insert_default().set_can_edit_items(false);
        assert!(!may_edit(&meta, "me"), "the capabilities have the last word");
        meta.capabilities.mut_or_insert_default().set_can_edit_items(true);
        assert!(may_edit(&meta, "someone"));
    }

    #[test]
    fn a_collection_page_lists_present_items_and_its_next_token() {
        let item = |uri: &str, at: u64, removed: bool| {
            let mut item = Vec::new();
            wire::put_bytes(&mut item, 1, uri.as_bytes());
            wire::put_varint(&mut item, 2, at);
            wire::put_varint(&mut item, 3, u64::from(removed));
            item
        };
        let mut body = Vec::new();
        wire::put_bytes(&mut body, 1, &item("spotify:track:a", 10, false));
        wire::put_bytes(&mut body, 1, &item("spotify:track:gone", 20, true));
        wire::put_bytes(&mut body, 1, &item("spotify:album:b", 30, false));
        wire::put_bytes(&mut body, 2, b"token-2");
        wire::put_bytes(&mut body, 3, b"sync");
        let page = collection_page(&body).unwrap();
        assert_eq!(page, CollectionPage { items: vec![("spotify:track:a".into(), 10), ("spotify:album:b".into(), 30)], next: Some("token-2".into()) });
        assert_eq!(collection_page(&[0x0a, 0x09]), None);
    }

    #[test]
    fn a_save_writes_the_item_as_present_and_a_removal_as_removed() {
        for (saved, flag) in [(true, 0), (false, 1)] {
            let body = write_request("someone", "collection", "spotify:track:a", saved, 1_794_349_469);
            let fields = wire::fields(&body).unwrap();
            assert_eq!(fields[0], Field::Bytes(1, b"someone"));
            assert_eq!(fields[1], Field::Bytes(2, b"collection"));
            let Field::Bytes(3, item) = fields[2] else { panic!("the item is field 3") };
            assert_eq!(wire::fields(item).unwrap(), [Field::Bytes(1, b"spotify:track:a"), Field::Varint(2, 1_794_349_469), Field::Varint(3, flag)]);
        }
    }
}
