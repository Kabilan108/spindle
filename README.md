# Baliset

A native Spotify client for Linux, built with [Rune](https://github.com/anomalyco/rune) and
[librespot](https://github.com/librespot-org/librespot). It needs a Spotify Premium account
and a network connection; nothing is downloaded for offline use.

- Shows up as a Spotify Connect device ("Baliset (hostname)"), so the phone app and other
  clients can play to it and control it.
- Main window: recents, saved albums, playlists, Liked Songs, artist and album pages, search.
- Closing the window keeps the app in the tray. A click on the tray icon shows or hides the
  window; its menu has play, next and previous.
- MPRIS (`org.mpris.MediaPlayer2.baliset`): media keys, `playerctl` and shell media widgets.

![Playlist page](docs/screens/playlist.png)

## Run

```sh
nix run .            # or: nix build . && result/bin/baliset
baliset --demo       # canned data, no account, no sound
```

The first launch shows a sign-in screen. It opens Spotify's sign-in page in the browser and
waits for it to come back to `http://127.0.0.1:5588/login`. The saved sign-in lives in
`~/.local/share/baliset/` (owner-only). The sign-out button sits beside your name at the
bottom of the sidebar. Audio is cached in
`~/.cache/baliset/audio`, up to 2 GB.

Keys: Space play/pause, Ctrl+L or Ctrl+K search, Ctrl+Left/Right previous/next, Ctrl+W close
to tray, Ctrl+Q quit.

## Visualizers

Album and playlist pages sit in slow pools of the cover's colours that drift and swell with
the music. The expand button at the right of the now-playing bar, or Ctrl+E, opens the Stage, a
full-window player: the blurred cover behind, the art turning as a disc inside a spectrum ring,
the track, seek bar and transport beside it. Esc, Ctrl+E or the collapse button closes it.

![The Stage](docs/screens/stage.png)

The music comes from a tap between librespot's player and the sound card, taken before the
volume, so a muted player still drives them. The analysis runs only while one of them is on
screen in a visible window and something plays; paused, hidden or elsewhere, nothing runs. The
demo engine feeds them a simulated track. `baliset-engine` has a `levels SECONDS` command that
prints what the analysis hears.

## Develop

```sh
nix develop -c cargo test
nix develop -c cargo clippy --all-targets
nix develop -c cargo run -- --demo
nix develop -c cargo run -- --snap home.png --scene home   # headless render; --list-scenes
nix develop -c cargo run --bin baliset-engine               # the engine alone, driven from stdin
```

- `src/model.rs`: the store the UI draws and the commands it sends.
- `src/engine/`: the engine handle, a demo backend, and `spotify/`, the real one.
- `src/ui/`, `src/tray.rs`, `src/mpris.rs`: windows, tray icon and MPRIS.
- `vm/`: a headless QEMU VM of the dotfiles desktop for testing on niri (`vm/README.md`).

## How it talks to Spotify

Playback and the Connect device are librespot. The library (playlists, Liked Songs, saved
albums, listening history, metadata, search) is read through the same session, from the
services librespot and the official client use. Spotify's public Web API is not used: it
answers this kind of sign-in with a blanket 429. These services are not a documented API, so
a Spotify change can break the library views until librespot or Baliset catches up.
