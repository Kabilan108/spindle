//! Changing a playlist, and finding a song's radio. A playlist changes by posting a list of
//! operations against the revision it was read at, as the official client does
//! (`playlist4_external.ListChanges` to `/playlist/v2/playlist/{id}/changes`); a change
//! against a stale revision is merged by Spotify or refused.

use http::Method;
use librespot_core::{Error, SpotifyUri};
use librespot_protocol::playlist4_external::{Add, Delta, Item, ListChanges, Op, Rem, SelectedListContent, op::Kind};
use protobuf::{EnumOrUnknown, Message, MessageField};

use super::catalog::{Catalog, unreadable};

fn item(track_uri: &str) -> Item {
    Item { uri: Some(track_uri.to_owned()), ..Default::default() }
}

/// Append a track.
pub(super) fn add_op(track_uri: &str) -> Op {
    let add = Add { items: vec![item(track_uri)], add_last: Some(true), ..Default::default() };
    Op { kind: Some(EnumOrUnknown::new(Kind::ADD)), add: MessageField::some(add), ..Default::default() }
}

/// Remove the track at `index`, naming it so a mismatch is refused rather than removing
/// something else.
pub(super) fn remove_op(track_uri: &str, index: usize) -> Op {
    let rem = Rem { from_index: i32::try_from(index).ok(), length: Some(1), items: vec![item(track_uri)], ..Default::default() };
    Op { kind: Some(EnumOrUnknown::new(Kind::REM)), rem: MessageField::some(rem), ..Default::default() }
}

pub(super) fn changes(revision: &[u8], op: Op) -> ListChanges {
    ListChanges {
        base_revision: Some(revision.to_vec()),
        deltas: vec![Delta { ops: vec![op], ..Default::default() }],
        want_resulting_revisions: Some(true),
        ..Default::default()
    }
}

fn playlist_id(playlist_uri: &str) -> &str {
    playlist_uri.rsplit(':').next().unwrap_or_default()
}

/// The first radio playlist in an `inspiredby-mix` answer.
fn radio_uri(body: &[u8]) -> Option<String> {
    let answer: serde_json::Value = serde_json::from_slice(body).ok()?;
    let items = answer["mediaItems"].as_array()?;
    items.iter().filter_map(|item| item["uri"].as_str()).find(|uri| uri.starts_with("spotify:playlist:")).map(str::to_owned)
}

impl Catalog {
    /// The playlist's current revision and the URI of its entry at `index`, if it has one.
    async fn head(&self, playlist_uri: &str, index: usize) -> Result<(Vec<u8>, Option<String>), Error> {
        let endpoint = format!("/playlist/v2/playlist/{}?from={index}&length=1&decorate=revision", playlist_id(playlist_uri));
        let body = self.session().spclient().request(&Method::GET, &endpoint, None, None).await?;
        let list = SelectedListContent::parse_from_bytes(&body)?;
        let at = list.contents.items.first().map(|item| item.uri().to_owned());
        Ok((list.revision().to_vec(), at))
    }

    async fn change(&self, playlist_uri: &str, request: &ListChanges) -> Result<(), Error> {
        let endpoint = format!("/playlist/v2/playlist/{}/changes", playlist_id(playlist_uri));
        let body = request.write_to_bytes()?;
        self.session().spclient().request(&Method::POST, &endpoint, None, Some(&body)).await?;
        Ok(())
    }

    pub(super) async fn add_to_playlist(&self, playlist_uri: &str, track_uri: &str) -> Result<(), Error> {
        let (revision, _) = self.head(playlist_uri, 0).await?;
        self.change(playlist_uri, &changes(&revision, add_op(track_uri))).await
    }

    pub(super) async fn remove_from_playlist(&self, playlist_uri: &str, track_uri: &str, index: usize) -> Result<(), Error> {
        let (revision, at) = self.head(playlist_uri, index).await?;
        if at.as_deref() != Some(track_uri) {
            return Err(Error::aborted("The playlist changed since it was opened. Try again."));
        }
        self.change(playlist_uri, &changes(&revision, remove_op(track_uri, index))).await
    }

    /// The URI of the playlist Spotify builds as a song's radio.
    pub(super) async fn radio(&self, track_uri: &str) -> Result<String, Error> {
        let seed = SpotifyUri::from_uri(track_uri)?;
        let body = self.session().spclient().get_radio_for_track(&seed).await?;
        radio_uri(&body).ok_or_else(|| unreadable("radio"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_add_appends_the_track_against_the_revision_it_read() {
        let request = changes(b"rev-7", add_op("spotify:track:a"));
        let decoded = ListChanges::parse_from_bytes(&request.write_to_bytes().unwrap()).unwrap();
        assert_eq!(decoded.base_revision(), b"rev-7");
        assert!(decoded.want_resulting_revisions());
        let [delta] = decoded.deltas.as_slice() else { panic!("one delta") };
        let [op] = delta.ops.as_slice() else { panic!("one op") };
        assert_eq!(op.kind(), Kind::ADD);
        assert!(op.add.add_last());
        assert_eq!(op.add.items.iter().map(Item::uri).collect::<Vec<_>>(), ["spotify:track:a"]);
        assert!(op.rem.is_none());
    }

    #[test]
    fn a_removal_names_one_track_at_its_index() {
        let request = changes(b"rev-8", remove_op("spotify:track:b", 41));
        let decoded = ListChanges::parse_from_bytes(&request.write_to_bytes().unwrap()).unwrap();
        let op = &decoded.deltas[0].ops[0];
        assert_eq!(op.kind(), Kind::REM);
        assert_eq!((op.rem.from_index(), op.rem.length()), (41, 1));
        assert_eq!(op.rem.items.iter().map(Item::uri).collect::<Vec<_>>(), ["spotify:track:b"]);
        assert!(op.add.is_none());
    }

    #[test]
    fn a_radio_answer_names_its_playlist() {
        let body = br#"{"mediaItems":[{"uri":"spotify:playlist:37i9dQZF1E8Mh0qnxjsIzD"}]}"#;
        assert_eq!(radio_uri(body).as_deref(), Some("spotify:playlist:37i9dQZF1E8Mh0qnxjsIzD"));
        assert_eq!(radio_uri(br#"{"mediaItems":[]}"#), None);
        assert_eq!(radio_uri(b"<html>"), None);
        assert_eq!(playlist_id("spotify:user:someone:playlist:abc"), "abc");
    }
}
