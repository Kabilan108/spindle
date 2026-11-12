---
name: verify
description: Prove a Baliset change works by rendering it, driving it or running it live. Use after changing the UI, the engine, the tray or MPRIS, before calling the work done, and when asked for a screenshot or recording of the app.
---

# Verifying a Baliset change

Tests and clippy say the code holds together. This skill is for seeing the change work. Pick
the cheapest rung that can show it, and say in your report which rung you reached.

| The change touches | Rung |
|---|---|
| How a screen or state looks | Render a scene |
| Hover, click, drag, typing, shortcuts | Script a scene |
| Animation, the visualizers | Record a scene |
| Anything under `src/engine/spotify/` | Live engine run |
| Tray, MPRIS, window behaviour on niri | The VM |
| Sound, sign-in, phone as remote | The real desktop, with Tony |

Run every command through the dev shell (`nix develop -c …`). Build once with
`cargo build`, then call `target/debug/baliset` directly; a render takes about a second.

## Render a scene

```sh
B=target/debug/baliset
$B --list-scenes
$B --scene playlist --snap out.png
```

Scenes run on the demo engine with no account, window or sound. Open the PNG and look at it;
a render that exits 0 proves nothing about what it drew.

Flags that earn their keep (all Rune's; the full list is in its `tooling/args.rs`):

- `--scale 2` for a picture sharp enough to judge text and edges.
- `--size 1400x900` for another window size. The default is 1100x720.
- `--theme light` for the light theme.
- `--crop KEY` to cut the picture to one element, by its `.key("…")` in the source.
- `--tree out.json` to dump the laid-out tree with every element's key and bounds, when you
  need a key or want to assert a position.

A new screen or state gets a scene. Add a function to `src/ui/scenes.rs` and a line to
`MAIN_SCENES`. Set up state through commands and wait for the engine's answer with
`s.link.send_and_wait(command, |store| …)` or `s.link.wait_for(…)`, or the snapshot races
the data. If the demo engine lacks the data, add it in `src/engine/demo.rs`.

## Script a scene

A script is JSON events on a timeline, aimed at element keys:

```json
{"events": [
  {"t": 0.30, "type": "double-click", "key": "trk4"},
  {"t": 0.90, "type": "mark", "name": "played"},
  {"t": 1.80, "type": "drag", "key": "seek", "dx": 160, "dur": 0.4},
  {"t": 2.80, "type": "move", "key": "trk7"},
  {"t": 3.30, "type": "mark", "name": "end"}
]}
```

```sh
$B --scene playlist --script docs/scripts/interaction.json --snap played.png --at mark:played
```

`--at` takes seconds or `mark:NAME`. Event types include `tap`, `double-click`, `move`
(hover), `drag`, `scroll`, `type`, `key` and `shortcut`; `tooling/script.rs` in Rune's source
is the reference. `docs/scripts/` holds the scripts behind the README's pictures. One
snapshot per mark shows a whole interaction step by step.

## Record a scene

```sh
$B --scene stage --fps 30 --duration 4 --video stage.mp4
```

It needs `ffmpeg`. Watch the video. The last lines of output give frame CPU time and how many
frames animated; a scene at rest reports zero animating frames, so a paused visualizer that
still animates is a bug.

## Tests for behaviour

A picture can't show that a click sent the right command. `src/ui/tests.rs`,
`src/ui/zoom_tests.rs` and `src/ui/viz/tests.rs` drive the UI headlessly and assert on state and layout;
copy the nearest test. After changing state from outside an event handler, call
`rt.update` before asserting.

## Live engine run

`baliset-engine` is the engine with no window. It signs in as Tony's account and is a real
Connect device, so the rules in `AGENTS.md` apply: hold the lock, mute first, write only
where allowed and put it back.

It reads one command per line and prints a summary line whenever the store changes. It
doesn't wait between commands, so pace them with sleeps:

```sh
(echo "vol 0"; sleep 6
 echo "ctx spotify:album:ID"; sleep 8
 echo state; echo upnext; sleep 1
 echo quit) | flock /tmp/baliset-live.lock target/debug/baliset-engine > run.log 2>&1
```

- `vol 0` goes first and gets a few seconds, since nothing applies until the session is
  `Online`.
- `state` prints playback, context and library counts. `upnext` prints the queue.
  `tracks URI` lists a loaded page. `find NAME` finds a playlist by name.
- `levels SECONDS` prints what the visualizers hear, which shows audio is flowing while
  muted.
- The match in `src/bin/baliset-engine.rs` lists every command. Add one there when a new
  engine command needs driving.
- The log holds account data. Read it, quote counts and states in your report, and leave
  names and ids out.

After engine changes, also run `target/debug/baliset --check`; it must end with
`all N checks passed`.

## The VM and the real desktop

`vm/README.md` covers the VM: `vm/up`, `vm/spawn` the app, `vm/click` and `vm/key` to drive
it, `vm/shot` for a screenshot, `vm/down`. Use it for the tray icon and its menu,
`playerctl`, closing to tray and restoring the window. Run `--demo` there unless the change
needs the account.

Some things only Tony can confirm: how it sounds, whether the visuals keep time with the
music, sign-in in a real browser, and the phone as a remote. Say so plainly and tell him
what to try.

## Refresh the README's pictures

`docs/screens/NAME.png` is `--scene NAME --scale 2`, or a script mark for the
`interaction-*` and `hover-card` ones. Re-render the pictures a UI change affects and commit
them with it.

## Report

State what you ran and what you saw: which scenes you looked at, which live commands ran and
their result, and what you could not verify. "Tests pass" alone is not a verification of a
visible change.
