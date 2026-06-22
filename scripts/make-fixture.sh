#!/usr/bin/env bash
# Scaffold a synthetic readout fixture for the granular-mosaic pool path. Two
# readout modes; both make each OUTPUT frame reveal *what selection did*, so a
# knob's effect is readable straight off the pixels (with rearrangement=1.0).
#
# --readout frame (default): solid-colour carrier frames reveal which CARRIER
#   FRAME a tile selected. The carrier is an ordered grey ramp (frame 0 darkest ->
#   frame N-1 lightest); the modulator alternates darkest/lightest, so a
#   colour-nearest match wants to jump between the extremes every frame (maximally
#   jumpy with no scheduler) — the baseline a frame-scheduler (pool window,
#   anti-repeat, frame coherence) must visibly tame. See the granular-audio /
#   temporal-coherence findings.
#
# --readout texture: carrier frames alternate FLAT vs busy (vertical 1px stripes)
#   at the SAME mean gray, so mean colour ties across frames and only the texture
#   descriptor (luma variance + gradient magnitude) can decide which frame a tile
#   draws. The modulator alternates flat/busy too, so the demanded texture flips
#   each frame: with --texture-weight ON the output structure tracks that demand
#   (flat<->stripes), with it OFF the colour tie pins selection to the flat frame.
#   This is the readout for the texture dims — frame/origin readouts cannot show
#   them (solid tiles have zero texture; the gradient is spatially uniform).
#
# --readout origin: a STATIC coordinate-gradient carrier (R encodes x, G encodes
#   y) makes each output tile's COLOUR reveal which CARRIER ORIGIN it sampled
#   (blue=left edge, yellow=right edge). The modulator's demanded region flips
#   left<->right every frame, so a tile's source ORIGIN wants to teleport each
#   frame. This is the readout for *spatial-origin coherence* and origin-/
#   selection-space knobs — the solid-colour frame readout cannot show them (all
#   grains in a frame share a colour). See [[spatial-coherence-shares-reach]].
#
# IMPORTANT: render the comparison with --variation 0. The pool render's
# --variation defaults to 0.25, injecting a per-tile *random* alternate grain that
# the schedulers never touch — it scatters the readout and masks the knob.
#
# With --with-chirp it also writes a constant-amplitude linear chirp WAV per
# source (flat RMS, rising spectral centroid) plus the matching RMS + STFT
# caches, so --audio-weight and centroid (k=2) runs have ready inputs that
# isolate the audio dims.
#
# Usage:
#   scripts/make-fixture.sh <output_dir> [--frames N] [--size WxH]
#       [--readout frame|origin] [--with-chirp]
set -euo pipefail

out=""
frames=4
size="32x32"
with_chirp=0
readout="frame"
usage="usage: scripts/make-fixture.sh <output_dir> [--frames N] [--size WxH] [--readout frame|origin] [--with-chirp]"
while [ "$#" -gt 0 ]; do
  case "$1" in
    --frames) frames="$2"; shift 2 ;;
    --size) size="$2"; shift 2 ;;
    --readout) readout="$2"; shift 2 ;;
    --with-chirp) with_chirp=1; shift ;;
    -h|--help) echo "$usage"; exit 0 ;;
    -*) echo "make-fixture: unknown option $1" >&2; exit 2 ;;
    *)
      if [ -z "$out" ]; then out="$1"; shift
      else echo "make-fixture: unexpected argument $1" >&2; exit 2; fi ;;
  esac
done
[ -n "$out" ] || { echo "$usage" >&2; exit 2; }
[ "$frames" -ge 1 ] || { echo "make-fixture: --frames must be >= 1" >&2; exit 2; }
case "$readout" in frame|origin|texture) ;; *) echo "make-fixture: --readout must be frame, origin or texture" >&2; exit 2 ;; esac
command -v ffmpeg >/dev/null 2>&1 || { echo "make-fixture: ffmpeg not found on PATH" >&2; exit 1; }

repo_root="$(cd "$(dirname "$0")/.." && pwd)"

solid() { # $1 outfile  $2 hex6
  ffmpeg -v error -y -f lavfi -i "color=c=0x$2:s=$size" -frames:v 1 "$1"
}

mkdir -p "$out/carrier" "$out/modulator"
if [ "$readout" = "frame" ]; then
  lo=51   # 0x33
  hi=242  # 0xf2
  for ((i = 0; i < frames; i++)); do
    if [ "$frames" -le 1 ]; then val="$lo"; else val=$(( lo + (hi - lo) * i / (frames - 1) )); fi
    h="$(printf '%02x' "$val")"
    solid "$(printf '%s/carrier/frame_%06d.png' "$out" "$i")" "$h$h$h"
    if [ $(( i % 2 )) -eq 0 ]; then e="$(printf '%02x' "$lo")"; else e="$(printf '%02x' "$hi")"; fi
    solid "$(printf '%s/modulator/frame_%06d.png' "$out" "$i")" "$e$e$e"
  done
elif [ "$readout" = "origin" ]; then
  # Static coordinate-gradient carrier: tile colour == its origin (R=x, G=y).
  ffmpeg -v error -y -f lavfi -i "nullsrc=s=$size" \
    -vf "geq=r='X/W*255':g='Y/H*255':b=128,format=rgb24" -frames:v 1 "$out/_car.png"
  # Modulator: a horizontal grey gradient whose direction flips every frame, so
  # the demanded source ORIGIN teleports left<->right (the spatial analogue of the
  # frame-readout's alternating extremes).
  ffmpeg -v error -y -f lavfi -i "nullsrc=s=$size" \
    -vf "geq=r='X/W*255':g='X/W*255':b='X/W*255',format=rgb24" -frames:v 1 "$out/_gradL.png"
  ffmpeg -v error -y -i "$out/_gradL.png" -vf "hflip" "$out/_gradR.png"
  for ((i = 0; i < frames; i++)); do
    cp "$out/_car.png" "$(printf '%s/carrier/frame_%06d.png' "$out" "$i")"
    if [ $(( i % 2 )) -eq 0 ]; then src="$out/_gradL.png"; else src="$out/_gradR.png"; fi
    cp "$src" "$(printf '%s/modulator/frame_%06d.png' "$out" "$i")"
  done
else
  # Texture readout: a FLAT mid-gray and a busy vertical-stripe frame, both with
  # mean 0x80 so mean colour ties — only the texture descriptor can tell them
  # apart. Carrier and modulator both alternate flat (even) / busy (odd).
  ffmpeg -v error -y -f lavfi -i "color=c=0x808080:s=$size" -frames:v 1 "$out/_flat.png"
  ffmpeg -v error -y -f lavfi -i "nullsrc=s=$size" \
    -vf "geq=r='if(mod(floor(X)\,2)\,192\,64)':g='if(mod(floor(X)\,2)\,192\,64)':b='if(mod(floor(X)\,2)\,192\,64)',format=rgb24" \
    -frames:v 1 "$out/_busy.png"
  for ((i = 0; i < frames; i++)); do
    if [ $(( i % 2 )) -eq 0 ]; then src="$out/_flat.png"; else src="$out/_busy.png"; fi
    cp "$src" "$(printf '%s/carrier/frame_%06d.png' "$out" "$i")"
    cp "$src" "$(printf '%s/modulator/frame_%06d.png' "$out" "$i")"
  done
fi

echo "make-fixture: wrote $frames carrier + $frames modulator frame(s) ($size, readout=$readout) to $out"

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

if [ "$readout" = "origin" ]; then
  echo "make-fixture: try — render OFF vs ON, then compare (note --variation 0):"
  echo "  cargo run -q -p morphogen-cli -- render-granular-mosaic-pool-sequence \\"
  echo "    $out/modulator $out/carrier $out/off --grain-size 8 --rearrangement 1.0 --variation 0 --coherence-reach 10 --spatial-coherence-weight 0"
  echo "  cargo run -q -p morphogen-cli -- render-granular-mosaic-pool-sequence \\"
  echo "    $out/modulator $out/carrier $out/on  --grain-size 8 --rearrangement 1.0 --variation 0 --coherence-reach 10 --spatial-coherence-weight 6"
  echo "  scripts/frame-delta.py $out/off $out/on   # OFF strobes, ON holds"
elif [ "$readout" = "texture" ]; then
  echo "make-fixture: try — render OFF vs ON, then compare (note --variation 0):"
  echo "  cargo run -q -p morphogen-cli -- render-granular-mosaic-pool-sequence \\"
  echo "    $out/modulator $out/carrier $out/off --grain-size 8 --rearrangement 1.0 --variation 0 --texture-weight 0"
  echo "  cargo run -q -p morphogen-cli -- render-granular-mosaic-pool-sequence \\"
  echo "    $out/modulator $out/carrier $out/on  --grain-size 8 --rearrangement 1.0 --variation 0 --texture-weight 8"
  echo "  scripts/frame-delta.py $out/off $out/on   # OFF holds flat, ON tracks the flat<->stripes demand"
else
  echo "make-fixture: try — scripts/parity-check.sh $out/modulator $out/carrier -- --rearrangement 1.0 --pool-window 2"
fi
