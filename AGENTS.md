# Baliset

A native Spotify client for Linux: Rune for the UI, librespot for playback and the Connect
device. `README.md` covers what the app does, the commands and the source layout. This file
holds what the code and README don't say.

## Verify a change

Run everything through the dev shell (`nix develop -c …`, or direnv). The `verify` skill has
the commands for each step below.

1. `cargo clippy --all-targets` and `cargo test`. Both must be clean.
2. For UI changes, render the affected scene and look at the picture:
   `cargo run -- --snap OUT.png --scene NAME` (`--list-scenes` names them). A new screen or
   state gets a scene in `src/ui/scenes.rs`; scenes run on the demo engine.
3. For changes under `src/engine/spotify/`, run `cargo run -- --check` (20 checks, all must
   pass) and exercise the path live with `baliset-engine` (below).
4. For changes to `Cargo.toml`, `Cargo.lock`, `nix/` or `vendor/`, run `nix build .`.

A first build takes several minutes: Rune and librespot are large and dependencies build at
`opt-level = 2`.

The code is formatted by hand with long lines. Leave `cargo fmt` alone: it rewrites nearly
every file.

## Live testing against the account

The saved sign-in in `~/.local/share/baliset/` is Tony's real Spotify account, and a running
engine is a real Connect device that makes sound on this machine.

- Drive the engine alone with `cargo run --bin baliset-engine`, which reads commands from
  stdin (`state`, `ctx URI`, `queue URI`, `upnext`, `levels SECONDS`, …; the match in
  `src/bin/baliset-engine.rs` is the list).
- Send `vol 0` before anything that plays.
- Hold the shared lock for the whole run, so two agents never fight over the one device:
  `flock /tmp/baliset-live.lock <command>`.
- Reads are free. Writes are limited to the playlist named "old head": add, check, then
  remove what you added. Follow or like something only to test it, and put it back in the
  same run. Ask before any other change to the account.
- Tokens, credentials and account ids stay out of output, logs and commits. `--check`
  output is safe to share as is.

`--demo` needs no account and makes no sound; prefer it whenever the engine isn't the thing
under test.

## How Spotify is reached

Everything goes through the librespot session to the services the official client uses
(spclient, Connect state). Spotify's public Web API answers this kind of sign-in with a
blanket 429, so adding a Web API call is a dead end. These services are undocumented:

- Adding or repairing a call follows the `spotify-endpoints` skill: find the request in
  librespot's source (`~/.cargo/registry/src/*/librespot-*-0.8.0/`), probe it, then pin it.
- Every endpoint the app reads has an entry in `CHECKS` in `src/engine/spotify/check.rs`.
  Add one with each new endpoint; it checks that the answer still parses the way the app
  reads it.
- When a library view breaks, start from `baliset --check`: the failing line names the call,
  what depends on it and the source file.

Things that cost time to learn:

- A cluster update may name no context; keep the one in the store (`cluster_update`).
- Queue changes go through the single ordered worker in `queue.rs`: read the device's queue,
  build the command from that snapshot, send, wait, read back. Separate tasks racing for it
  land out of order.
- A removal from the queue names its track as well as its index, since the queue moves under
  an open menu.
- Anything that waits on the network and then writes checks `signed_out_since(account)`
  first, so a pending edit can't land on another account after a sign-out.
- `PlayerEvent::Loading` for the next track arrives before `TrackChanged`; the outgoing
  episode's position is saved at `Loading`.
- Volume is applied in `TapSink`, after the audio tap, with librespot's mixer set to
  `NoOpVolume`. The visualizers therefore hear a muted player at full level.
- Bitrate and loudness normalisation are fixed when the player starts; a change applies on
  the next launch.

## Rune

Rune is pinned to one git revision in `Cargo.toml` and has little documentation; its source
is the reference (`~/.cargo/git/checkouts/rune-*/`). Read it before assuming an API exists.

- State changed outside an event handler shows up only after an update is requested; a test
  that mutates state and asserts without `rt.update` is flaky.
- `cx.window_layout` returns coordinates inside the zoomed stage. Overlays (the context menu)
  sit outside the zoomed root, so a position taken from layout is multiplied by
  `zoom::factor`.
- The Stage and other full-window layers close on navigation (`open`, `go_search`), or the
  page changes unseen behind them.
- Analysis for a visualizer runs only while it is on screen, in a visible window, with
  something playing. Keep new visuals on that rule.
- Artist names are links everywhere through `widgets::artist_links`.

## Dependencies and packaging

- `vendor/librespot-connect` is the published 0.8.0 crate with one fix, marked `Baliset:`
  (its README explains). Drop it when upstream has the fix. Other librespot crates come from
  crates.io.
- `nix/package.nix` builds from a file set. A new top-level directory the build needs must
  be added there, or `nix build` fails while `cargo build` passes.
- The package wraps the binary with `LD_LIBRARY_PATH`; a child process that isn't ours (the
  browser at sign-in) is started with it removed.
- `viz/` is a separate crate of visualizer mockups, outside the app's build.
- `docs/screens/` are scene renders shown in the README. Re-render the ones a UI change
  affects.

## Desktop testing

`vm/` boots a headless VM of Tony's niri desktop for tray, MPRIS and window behaviour
(`vm/README.md`). It evaluates `~/dotfiles`, which is read-only from here. On the real
desktop, use the `niri-computer-use` skill.

## Writing

User-facing text is short and plain: "Sign in", "Up next", "Add to queue". Comments explain
why, never what.
