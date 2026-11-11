#!/usr/bin/env bash
# Render every concept: an 11 s video (playing, a pause at 8 s, at rest) and two stills.
# Usage: viz/render.sh OUT_DIR [scene...]   (run from the repo root or viz/)
set -euo pipefail
cd "$(dirname "$0")"
out=${1:?output directory}
shift || true
mkdir -p "$out"
cargo build --release --quiet
bin=target/release/spindle-viz
concepts=(
  "01 groundline mini"
  "02 trace mini"
  "03 sleeve mini"
  "04 aura mini"
  "05 edge-light main"
  "06 header main"
  "07 stage main"
)
for c in "${concepts[@]}"; do
  read -r num scene kind <<<"$c"
  if [ $# -gt 0 ] && [[ ! " $* " =~ " $scene " ]]; then continue; fi
  size=380x208
  [ "$kind" = main ] && size=1100x720
  base="$out/$num-$scene"
  "$bin" --scene "$scene" --size "$size" --scale 2 --fps 60 --duration 11 --video "$base.mp4" 2>&1 | grep -E '^viz:|frame cpu' | sed "s/^/$num $scene: /"
  "$bin" --scene "$scene" --size "$size" --scale 2 --snap "$base.png" --at 7.05 >/dev/null 2>&1
  "$bin" --scene "$scene" --size "$size" --scale 2 --snap "$base-paused.png" --at 10.8 >/dev/null 2>&1
done
