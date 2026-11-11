# Baliset visualization lab

Animated mockups of music visualizations for Baliset. It's a standalone crate, not part of the app or its workspace. Each concept is a Rune scene driven by a simulated audio signal and rendered headlessly with Rune's own `--snap` and `--video` tooling.

## Render

From the repo root:

```sh
nix develop -c cargo build --manifest-path viz/Cargo.toml          # dev build
nix develop -c viz/render.sh OUT_DIR                              # every concept: .mp4 + two .png
nix develop -c viz/render.sh OUT_DIR stage aura                   # only some scenes
```

`render.sh` needs `ffmpeg` on `PATH` for the videos. It prints each scene's frame CPU time and a `viz:` line with how many view commits and live-value publishes the scene made, and when it last changed anything after the pause.

One scene by hand (run inside `viz/`):

```sh
B=target/release/baliset-viz
$B --scene groundline --snap g.png --at 7 --scale 2                       # mini scenes are 380×208
$B --scene stage --size 1100x720 --scale 2 --fps 60 --duration 11 --video stage.mp4
```

Scenes: `groundline`, `trace`, `sleeve`, `aura` (mini player, 380×208), `edge-light`, `header`, `stage` (main window, `--size 1100x720`), and `signal`, a plain readout of the simulated analysis.

Every scene follows the same script. The track plays from 0:50: one bar of verse, a four-second build, then the drop at 6 s. Playback pauses at 8 s and the scene settles until 11 s.

## Layout

- `src/signal.rs`: the stand-in for audio analysis. `BandSource` is the seam a real implementation replaces: band powers per FFT hop from librespot's decoded PCM. `SimTrack` synthesizes those powers for a 120 BPM track. `Analyzer` does smoothing, normalization, bass/mid/treble, loudness and onset detection, and is the code the app would keep.
- `src/art.rs`: generated covers, and `ArtPalette`, which runs k-means in OKLab over a cover thumbnail and clamps the colours so they stay readable on Baliset's dark surfaces.
- `src/chrome.rs`: the mini player and main window, rebuilt from `src/ui/*` closely enough to judge the concepts in place.
- `src/gfx.rs`: the compositor that turns `El::effect` slots into rune-fx layers.
- `src/scenes/*.rs`: one module per concept.
- `src/theme.rs`, `src/icons.rs`: copies of the app's own palette and icons.
