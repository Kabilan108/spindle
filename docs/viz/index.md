# Baliset visualization concepts

Seven concepts: four for the mini player (380×208) and three for the main window (1100×720). Each video is 11 s at 60 fps and 2× scale. The track plays a bar of verse, a four-second build and the drop at 6 s, pauses at 8 s, and the video runs on until 11 s to show the scene settling. `NN-name.png` is a still from the drop and `NN-name-paused.png` is the same scene at rest.

All of it is driven by a simulated signal that a real implementation could supply. librespot's sink hands decoded PCM to a 2048-point FFT every 1024 samples (about 43 hops a second), the FFT bins are summed into 48 log-spaced bands (40 Hz to 16 kHz), and an analyzer turns those into smoothed band levels, loudness, bass/mid/treble energy and kick onsets. The analyzer in `viz/src/signal.rs` is real code; only the band powers are synthesized. Album colours come from k-means over the cover thumbnail, run once per track.

Cost figures below come from the headless runs: whole-frame CPU on this machine, plus counts of view commits and live-value publishes. "Settled" means the time after the pause at which the scene stopped changing anything; from then on it asks for no frames. In every scene a hidden window costs nothing: `Live` values don't wake hidden windows, and the analysis thread should stop when no visible window subscribes.

## In the app

06 and 07 shipped (`src/ui/viz/`, `src/engine/analysis/`). Differences from the lab:

- **Real audio.** A sink wraps librespot's rodio output, copies each packet before the volume
  into a ring buffer, and keeps an estimate of the play head behind rodio's half-second queue
  and PipeWire's buffers (about 55 ms). Each step FFTs the 2048 samples centred on what is heard
  now. When the output drains, the estimate has been within 5 to 21 ms of it in live runs, plus
  the uncertainty in the output buffer, so the visuals stay within one or two frames of the
  sound.
- **No commits for the music.** The Stage's 88 strokes are fixed canvas shapes whose drawn length
  is a `live_trim`, so the view never re-runs while playing (the lab rebuilt the ring about 34
  times a second). The disc still turns on the long committed tween.
- **No compositor.** The glow is a radial gradient with live opacity and scale, the ripple a
  ring with `Live::pulse`d scale and opacity. There is no grain.

Measured with `top` (one-second samples, 10 per state, release build, demo engine, 1711×1400
window on niri, % of one core):

| State | CPU |
|---|---|
| Home, paused | 0.2% |
| Home, playing (no visual) | 0.4% |
| Album header, playing | 7.9% |
| Album header, paused | 0.1% |
| Stage, playing | 9.3% |
| Stage, paused | 0.1% |
| Stage open and playing, window hidden to the tray | 0.2% |

## 01 Groundline (mini player, the quietest)

`01-groundline.mp4`, `01-groundline.png`

A 64-bar spectrum, at most 11 pt tall, stands on the mini player's bottom edge in the gap under the recents tiles. Its colours run from the cover's main colour to its second, and a faint glow over the bars follows loudness. It stays out of the way of the controls and is gone at rest.

- **Rune primitives:** `live_rect` on each bar with `Live::smooth` springs; `live_opacity` with a `radial_gradient` for the glow.
- **Per frame:** no view commits. 65 live values are published per analysis hop (43 Hz), and the renderer's springs fill in the display frames. Values are quantized to quarter points, so a settled spectrum repeats and stops waking the window.
- **Paused:** bars fall to zero height. Settled 1.15 s after the pause.
- **Data:** band levels, loudness, cover palette.

## 02 Trace (mini player)

`02-trace.mp4`, `02-trace.png`

The progress bar becomes the track's loudness trace, drawn as it plays. Each played column is a mirrored line in the cover's colour, the unplayed part is a dotted rule, and a bright "pen" at the playhead moves with the current loudness. You can read progress and the shape of the song at a glance. The tiles shrink from 52 to 48 pt to make room.

- **Rune primitives:** a `canvas` `Shape` (round-capped strokes) inside `memo`; one `live_rect` for the pen.
- **Per frame:** one live rect per hop. The view commits only when a column completes: 118 columns over 4:21 is one commit every 2.2 s. That was 6 commits in 11 s, including the first frame and the pause.
- **Paused:** the pen settles to the column's running average. One publish, then nothing; settled at 8.02 s.
- **Data:** loudness, track position and duration. The real app would record the trace while playing and could cache it per track (118 bytes). After a seek or a restart, unplayed stretches stay dotted.

## 03 Sleeve (mini player, no audio analysis)

`03-sleeve.mp4`, `03-sleeve.png`

The cover is a record sleeve. On play, a record slides out from behind it and turns at 33⅓ rpm, with the cover as its label. A highlight stays fixed while the record turns under it, which is what makes the spin read. On pause the record eases to a stop over 1.6 s and slides back into the sleeve. It ties straight to the Baliset name and needs nothing the app doesn't already have.

- **Rune primitives:** `rotate` with `rotate_motion` (see "Hard in Rune" below), layout `motion` springs for the slide, `radial_gradient` grooves, `pixels_fit` label, `linear_gradient` sheen.
- **Per frame:** no commits while playing. One commit starts a 240 s linear rotation and the renderer turns it; the app re-commits every 4 minutes. A turned element draws into its own isolated layer, so each frame composites one 60 pt layer.
- **Paused:** one commit swaps in an ease-out whose starting slope matches the spin's speed, so it slows down with no jolt. Still within 1.6 s.
- **Data:** play/pause state only.

## 04 Aura (mini player, the showy one)

`04-aura.mp4`, `04-aura.png`

The mini player's background is a rune-fx mesh gradient made from deep versions of the cover's colours. It drifts faster when the music is louder. The art glows with the bass, and strong kicks send a ripple out from the art across the window. A scrim keeps the text readable. At rest the mesh freezes where it is, so the window keeps the cover's colours without moving.

- **Rune primitives:** compositor hook with `El::effect` slots; rune-fx `MeshGradientParams` fill, `GlowParams` (Soft) with `heat`, `PulseParams::Ripple` one-shots.
- **Per frame:** the view never re-runs (1 commit, at the pause, for the play icon). The poll advances three numbers and the ripple list, and the compositor builds layers from them. GPU cost is a mesh fill over 316k device pixels plus a glow and up to a few ripples; rune-fx's timings table puts a five-colour mesh at 0.37 ms for 544k pixels on an M4 Pro, so roughly 0.2 ms here. I didn't measure GPU time on this machine. While playing, `Compositor::animating` keeps frames coming.
- **Paused:** drift speed eases to zero, the glow cools (rune-fx cools a glow instead of cutting it) and ripples finish. `animating()` turns false 2.3 s after the pause.
- **Data:** cover palette, loudness, bass, onsets.

## 05 Edge light (main window, now-playing bar)

`05-edge-light.mp4`, `05-edge-light.png`

The now-playing bar's top border becomes a thin line of light in the cover's colours that brightens with loudness, with a faint bloom rising into the page. Next to the track title, the Baliset mark's three threads stretch with bass, mids and treble. At rest they settle back to the mark's own proportions, so the paused glyph is exactly the logo. This is something to leave on all day.

- **Rune primitives:** `live_opacity` on a `linear_gradient` line and a `radial_gradient` bloom; three `live_rect` threads with smoothing.
- **Per frame:** no commits. Five live values are published per hop.
- **Paused:** the line and bloom fade out and the threads return to the mark. Settled 1.5 s after the pause.
- **Data:** loudness, bass/mid/treble, cover palette.

## 06 Ambient header (main window, album page)

`06-header.mp4`, `06-header.png`

The album page's header sits in four large pools of the cover's colours that drift slowly with the music and swell with the bass. They fade into the page background before the track list. Paused, they stop where they are and the page stays tinted. Text sits on deep tones and reads cleanly.

- **Rune primitives:** `live_offset`, `live_scale` and `live_opacity` on `radial_gradient` elements, smoothed with springs; `clip_rounded` for the page; a static rune-fx grain slot against banding in the large gradients.
- **Per frame:** no commits. 12 live values per hop. The drift clock only advances while playing, at a rate set by loudness. The grain is static and never animates.
- **Paused:** settled 1.07 s after the pause.
- **Data:** loudness, bass/mid/treble, cover palette.

## 07 Stage (main window, full now-playing view)

`07-stage.mp4`, `07-stage.png`

A full-window now-playing view. The cover, blurred once per track on the CPU, fills the window under a dark scrim and vignette. The cover turns slowly as a disc inside a 88-bar radial spectrum, with lows at the bottom. The disc glows with the bass, and kicks in the drop send a ripple across the window. Paused, the spectrum collapses to a calm ring of dots and the disc coasts to a stop.

- **Rune primitives:** `canvas` `Shape` for the ring; `rotate` with the long-tween trick for the disc; `pixels_fit` on a 96 px pre-blurred thumbnail (bilinear upscaling does the rest, so there's no GPU blur pass); `memo` for the backdrop and the info column; compositor glow, ripples and grain.
- **Per frame:** this is the one concept that commits the view continuously, once per analysis hop when the levels change: about 34 commits a second while playing (375 in the run). It rebuilds a single canvas of 88 strokes, and everything else is memoized. Mean frame CPU in the run was 1.3 ms. The main-window scenes that commit nothing came in at 1.1 to 1.3 ms, though they draw about 500 instances to Stage's 180.
- **Paused:** commits stop once the bands reach zero (about 0.9 s), the disc stops at 2.4 s and the glow has cooled by 2.3 s.
- **Data:** band levels, bass, onsets, cover palette, play state.

## Hard in Rune

- **No live rotation.** `LoopSpec::rotation` only turns icons, and `rotate` is a committed property. Spinning discs use one commit that starts a long linear tween and one that swaps in an ease-out on pause. It works, but a `live_rotate` binding would make it ordinary.
- **No live geometry.** Live bindings cover rects, offsets, scale, opacity, colours and trim, but not a shape's points. Axis-aligned bars can be live rects, so 01, 05 and 06 commit nothing. A radial or curved spectrum has to rebuild its `Shape` each hop (07), or move to a custom compositor shader.
- **rune-light isn't wired into Rune yet.** Its README marks Runtime integration as "next step, not done". Using it means splitting the frame into a base target and a text-only target and adding resolve passes by hand, as `examples/lab` does (hundreds of lines of wgpu). A beat-pulsing light over the main window would cost about 1 ms of GPU a frame at 2×. I left it out of this round.
- **Radial gradients take 16 stops**, which caps the record's groove texture at about five grooves.
