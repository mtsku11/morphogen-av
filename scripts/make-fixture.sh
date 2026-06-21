#!/usr/bin/env bash
# Scaffold a synthetic readout fixture for the granular-mosaic pool path.
#
# The trick (see the granular-audio / temporal-coherence findings): solid-colour
# carrier frames + rearrangement=1.0 make each OUTPUT frame's colour reveal
# exactly which CARRIER frame a tile selected, so source-frame jumpiness, pool
# window membership, and scheduler behaviour are readable straight off the
# output. The carrier is an ordered grey ramp (frame 0 darkest -> frame N-1
# lightest); the modulator alternates between the darkest and lightest carrier
# grey, so a colour-nearest match wants to jump between the extremes every frame
# (maximally jumpy with no scheduler) — the baseline a scheduler must visibly tame.
#
# With --with-chirp it also writes a constant-amplitude linear chirp WAV per
# source (flat RMS, rising spectral centroid) plus the matching RMS + STFT
# caches, so --audio-weight and centroid (k=2) runs have ready inputs that
# isolate the audio dims.
#
# Usage:
#   scripts/make-fixture.sh <output_dir> [--frames N] [--size WxH] [--with-chirp]
set -euo pipefail

out=""
frames=4
size="32x32"
with_chirp=0
while [ "$#" -gt 0 ]; do
  case "$1" in
    --frames) frames="$2"; shift 2 ;;
    --size) size="$2"; shift 2 ;;
    --with-chirp) with_chirp=1; shift ;;
    -h|--help)
      echo "usage: scripts/make-fixture.sh <output_dir> [--frames N] [--size WxH] [--with-chirp]"
      exit 0 ;;
    -*) echo "make-fixture: unknown option $1" >&2; exit 2 ;;
    *)
      if [ -z "$out" ]; then out="$1"; shift
      else echo "make-fixture: unexpected argument $1" >&2; exit 2; fi ;;
  esac
done
[ -n "$out" ] || { echo "usage: scripts/make-fixture.sh <output_dir> [--frames N] [--size WxH] [--with-chirp]" >&2; exit 2; }
[ "$frames" -ge 1 ] || { echo "make-fixture: --frames must be >= 1" >&2; exit 2; }
command -v ffmpeg >/dev/null 2>&1 || { echo "make-fixture: ffmpeg not found on PATH" >&2; exit 1; }

repo_root="$(cd "$(dirname "$0")/.." && pwd)"

solid() { # $1 outfile  $2 hex6
  ffmpeg -v error -y -f lavfi -i "color=c=0x$2:s=$size" -frames:v 1 "$1"
}

lo=51   # 0x33
hi=242  # 0xf2
mkdir -p "$out/carrier" "$out/modulator"
for ((i = 0; i < frames; i++)); do
  if [ "$frames" -le 1 ]; then val="$lo"; else val=$(( lo + (hi - lo) * i / (frames - 1) )); fi
  h="$(printf '%02x' "$val")"
  solid "$(printf '%s/carrier/frame_%06d.png' "$out" "$i")" "$h$h$h"
  if [ $(( i % 2 )) -eq 0 ]; then e="$(printf '%02x' "$lo")"; else e="$(printf '%02x' "$hi")"; fi
  solid "$(printf '%s/modulator/frame_%06d.png' "$out" "$i")" "$e$e$e"
done

echo "make-fixture: wrote $frames carrier + $frames modulator frame(s) ($size) to $out"

if [ "$with_chirp" -eq 1 ]; then
  dur="$(awk "BEGIN { printf \"%.4f\", $frames / 12.0 }")"
  for role in carrier modulator; do
    wav="$out/$role.wav"
    # Constant amplitude (flat RMS), instantaneous frequency rising 200->1000 Hz
    # over the clip (rising spectral centroid) — isolates the centroid dim.
    ffmpeg -v error -y -f lavfi \
      -i "aevalsrc=0.5*sin(2*PI*(200+800*t)*t):d=$dur:s=48000" -ac 1 "$wav"
    ( cd "$repo_root" && cargo run -q -p morphogen-cli -- \
        cache-rms "$wav" "$out/$role-rms.json" >/dev/null )
    ( cd "$repo_root" && cargo run -q -p morphogen-cli -- \
        cache-stft "$wav" "$out/$role-stft.json" >/dev/null )
  done
  echo "make-fixture: + chirp WAVs and RMS/STFT caches ($out/{carrier,modulator}-{rms,stft}.json)"
fi

echo "make-fixture: try — scripts/parity-check.sh $out/modulator $out/carrier -- --rearrangement 1.0 --pool-window 2"
