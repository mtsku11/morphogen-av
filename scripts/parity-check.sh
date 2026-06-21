#!/usr/bin/env bash
# Cross-path determinism check for the granular-mosaic temporal-pool render.
#
# The project's core invariant is that identical inputs + settings are
# bit-reproducible regardless of which path produced them. This renders the SAME
# pool job two ways — the direct CLI render and the persisted queue add->run
# path — with one shared set of selection-knob flags, then byte-compares every
# output frame. It is the inner-loop complement to the per-feature determinism
# assertions in crates/morphogen-cli/tests/smoke.rs: reach for it while exploring
# a new knob, before that durable assertion exists.
#
# Pass --backend metal in the flags to exercise the Metal path on both renders
# (queue-run gates Metal vs CPU per frame internally); CPU is the default.
#
# Usage:
#   scripts/parity-check.sh <modulator_dir> <carrier_dir> [-- <pool flags...>]
# Example:
#   scripts/parity-check.sh /tmp/fix/modulator /tmp/fix/carrier -- \
#     --rearrangement 1.0 --pool-window 3 --anti-repeat-weight 0.4 \
#     --coherence-weight 0.5 --coherence-reach 4
#
# Env: KEEP=1 keeps the temp workdir (printed) instead of deleting it on exit.
set -euo pipefail

if [ "$#" -lt 2 ]; then
  echo "usage: scripts/parity-check.sh <modulator_dir> <carrier_dir> [-- <pool flags...>]" >&2
  exit 2
fi
mod_dir="$1"
car_dir="$2"
shift 2
[ "${1:-}" = "--" ] && shift
flags=("$@")

for d in "$mod_dir" "$car_dir"; do
  [ -d "$d" ] || { echo "parity: FAIL — not a directory: $d" >&2; exit 2; }
done

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"

work="$(mktemp -d "${TMPDIR:-/tmp}/morphogen-parity.XXXXXX")"
trap '[ -n "${KEEP:-}" ] && echo "parity: kept workdir $work" || rm -rf "$work"' EXIT

direct="$work/direct"
queue="$work/queue.json"
qroot="$work/queue"
qframes="$qroot/job-0001/frames"
manifest="$qroot/job-0001/manifest.json"

# Path 1 — direct render (writes frames straight into the output dir).
cargo run -q -p morphogen-cli -- render-granular-mosaic-pool-sequence \
  "$mod_dir" "$car_dir" "$direct" "${flags[@]}" >/dev/null

# Path 2 — persisted queue add -> run (writes frames under job-0001/frames).
# --no-grain-cache keeps the run self-contained; it does not affect pixels.
cargo run -q -p morphogen-cli -- queue-add-granular-mosaic-pool-sequence \
  "$queue" "$mod_dir" "$car_dir" "$qroot" --no-grain-cache "${flags[@]}" >/dev/null
cargo run -q -p morphogen-cli -- queue-run-granular-mosaic-pool-sequence \
  "$queue" >/dev/null

shopt -s nullglob
direct_frames=("$direct"/frame_*.png)
n="${#direct_frames[@]}"
if [ "$n" -eq 0 ]; then
  echo "parity: FAIL — direct render produced no frames"
  exit 1
fi

rc=0
first_diff=""
for f in "${direct_frames[@]}"; do
  base="$(basename "$f")"
  q="$qframes/$base"
  if [ ! -f "$q" ]; then
    rc=1
    [ -z "$first_diff" ] && first_diff="$base (missing in queue)"
    continue
  fi
  if ! cmp -s "$f" "$q"; then
    rc=1
    [ -z "$first_diff" ] && first_diff="$base"
  fi
done

qn=0
for q in "$qframes"/frame_*.png; do qn=$((qn + 1)); done
[ "$n" -ne "$qn" ] && rc=1

if [ "$rc" -eq 0 ]; then
  echo "parity: OK   $n/$n frames byte-identical (direct == queue)"
else
  echo "parity: FAIL direct=$n queue=$qn frame(s); first divergent: ${first_diff:-<count mismatch>}"
fi

# Provenance: show the knobs the queue persisted into the bundle manifest.
if [ -f "$manifest" ]; then
  echo "parity: queue manifest granular_mosaic_pool knobs:"
  grep -E '"(pool_window|anti_repeat_weight|anti_repeat_cooldown|coherence_weight|coherence_reach|spatial_coherence_weight|audio_weight|modulator_centroid_cache|carrier_centroid_cache|backend)"' \
    "$manifest" | sed 's/^[[:space:]]*/    /'
fi

exit "$rc"
