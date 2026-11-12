---
name: spotify-endpoints
description: Add, change or repair a call Baliset makes to Spotify. Use when a feature needs data the engine doesn't read yet, when `baliset --check` reports a FAIL, or when a library view, the queue or a playlist edit stops working.
---

# Spotify endpoints

Baliset reads Spotify through the librespot session, from services that have no public
documentation. The official client's behaviour and librespot's source are the only references,
so every call is found, probed and then pinned by a check. `AGENTS.md` has the account rules
(what may be written, the lock, volume 0); they apply to every step here.

## Repairing a broken call

1. Run `nix develop -c cargo run -- --check`, or take the pasted output. Each FAIL line names
   the call, what in the app depends on it and the source file.
2. A failed `session connect` means the saved sign-in or the network, and every later line
   fails with it. Fix that first and run again.
3. For any other FAIL, probe the call (below) and compare the answer with what the parser in
   the named file expects. A changed format is the usual cause, a moved or removed endpoint
   the rare one.
4. Fix the parser, add a unit test built from the new answer's layout, and run `--check`
   until it prints `all N checks passed`.

## Adding a call

1. **Find the request.** Look in librespot first:
   `~/.cargo/registry/src/*/librespot-core-0.8.0/src/spclient.rs` has a method for many
   services, and `librespot-protocol-0.8.0/proto/` has the message definitions. Prefer a
   librespot method over a hand-built request. If librespot has nothing, search for how other
   open clients call the service, and treat what you find as a guess until the probe confirms
   it. The public Web API (`api.spotify.com`) answers this sign-in with 429 and is not an
   option.
2. **Probe it.** Copy [`probe.rs`](probe.rs) to `examples/probe.rs`, put the call in `main`
   and run `flock /tmp/baliset-live.lock nix develop -c cargo run --example probe`. Print
   the outline of the answer: JSON keys, byte counts, how many URIs. Probe with GETs and
   reads; a call that changes the account follows the write rules in `AGENTS.md`. Done when
   you can say which fields the feature needs and where they sit in the answer.
3. **Write the call** where its kind lives:

   | Kind | File |
   |---|---|
   | Metadata, playlists, search, contexts, collection reads | `src/engine/spotify/catalog.rs`, `catalog/podcasts.rs` |
   | What the account has saved or followed, and loading it into the store | `library.rs`, `library/` |
   | Playlist edits | `edits.rs` |
   | Queue and other Connect commands | `queue.rs` |

   Parse into the app's own types in `src/model.rs`. For a protobuf message librespot doesn't
   compile in, `wire.rs` reads raw fields by number. Anything that awaits the network and
   then writes to the store or the account checks `signed_out_since(account)` first.
4. **Test the parser** with a unit test beside it, fed bytes or JSON in the layout the probe
   showed, with invented names and ids.
5. **Pin it.** Append a `Check` to `CHECKS` in `check.rs`. It calls through the app's own
   code, asserts the answer still reads the way the app reads it, and returns counts only.
   The header comment in `check.rs` has the rules.
6. **Give the demo engine the same data** (`src/engine/demo.rs`), or the scenes and UI tests
   can't show the feature.
7. **Run it live** through `baliset-engine` as the `verify` skill describes.

Done when `--check` passes with the new line in it, the unit test passes, and the feature
works in a live engine run. Then delete `examples/probe.rs`.

## What the services do that you wouldn't guess

- Collection sets are named strings: `collection` (liked tracks and albums), `show`,
  `listenlater` (Your Episodes), `artist`. Reads page through `/collection/v2/paging`, writes
  go to `/collection/v2/write`, both as protobuf with the `vnd.collection-v2.spotify.proto`
  content type.
- Radio is a playlist. `get_radio_for_track` answers with a playlist URI to open; station
  contexts (`spotify:station:track:<id>`, `spotify:station:artist:<id>`) resolve through
  `get_context`.
- A playlist edit posts playlist4 ops to `/playlist/v2/playlist/{id}/changes` against the
  revision you last read. A stale revision is refused, so read, then edit.
- The queue is the device's Connect state. Commands go to
  `/connect-state/v1/player/command/from/{device}/to/{device}`, and the truth is read back
  from `GET /connect-state/v1/cluster` after a short settle. Send them through the worker in
  `queue.rs`, one at a time.
- These services rate-limit by account. Keep a probe to a handful of calls and page sizes the
  app already uses.
- Tokens stay inside the session. Print outlines and counts, and keep account names and ids
  out of anything committed or pasted.
