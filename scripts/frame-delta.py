#!/usr/bin/env python3
# Temporal frame-to-frame change of a rendered PNG sequence — the quantitative
# half of the visual verification loop. For a selection/scheduling knob you render
# the same job with the knob OFF and ON and compare the numbers: a temporal
# smoother (e.g. spatial-origin coherence) lowers this, a diversifier raises it.
# Pair it with actually Reading the frames; the number alone never proves a look.
#
# Reports the mean per-channel absolute difference between consecutive frames,
# in 0..255 units, for each sequence directory given.
#
# Usage:
#   scripts/frame-delta.py <frames_dir> [<frames_dir> ...]
# Example (OFF vs ON, e.g. for an /fixture --readout origin comparison):
#   scripts/frame-delta.py /tmp/fix/off /tmp/fix/on
#
# No third-party deps (pure-stdlib PNG decode) so it runs anywhere cargo does.
import sys
import struct
import zlib
import glob
import os


def load_png(path):
    data = open(path, "rb").read()
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError(f"not a PNG: {path}")
    i = 8
    w = h = ct = 0
    idat = b""
    while i < len(data):
        ln = struct.unpack(">I", data[i : i + 4])[0]
        typ = data[i + 4 : i + 8]
        chunk = data[i + 8 : i + 8 + ln]
        if typ == b"IHDR":
            w, h, _, ct = struct.unpack(">IIBB", chunk[:10])
        elif typ == b"IDAT":
            idat += chunk
        elif typ == b"IEND":
            break
        i += 12 + ln
    raw = zlib.decompress(idat)
    ch = {0: 1, 2: 3, 4: 2, 6: 4}[ct]
    stride = w * ch
    out = bytearray()
    prev = bytearray(stride)
    pos = 0
    for _ in range(h):
        f = raw[pos]
        pos += 1
        line = bytearray(raw[pos : pos + stride])
        pos += stride
        for x in range(stride):
            a = line[x - ch] if x >= ch else 0
            b = prev[x]
            c = prev[x - ch] if x >= ch else 0
            if f == 1:
                line[x] = (line[x] + a) & 255
            elif f == 2:
                line[x] = (line[x] + b) & 255
            elif f == 3:
                line[x] = (line[x] + ((a + b) >> 1)) & 255
            elif f == 4:
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                pr = a if (pa <= pb and pa <= pc) else (b if pb <= pc else c)
                line[x] = (line[x] + pr) & 255
        out += line
        prev = line
    return bytes(out)


def mean_delta(seq):
    paths = sorted(glob.glob(os.path.join(seq, "frame_*.png")))
    if len(paths) < 2:
        return None, len(paths)
    frames = [load_png(p) for p in paths]
    tot = 0.0
    for a, b in zip(frames, frames[1:]):
        tot += sum(abs(a[i] - b[i]) for i in range(len(a))) / len(a)
    return tot / (len(frames) - 1), len(paths)


def main(argv):
    if not argv:
        print("usage: scripts/frame-delta.py <frames_dir> [<frames_dir> ...]", file=sys.stderr)
        return 2
    for seq in argv:
        d, n = mean_delta(seq)
        if d is None:
            print(f"{seq}: need >=2 frames (found {n})")
        else:
            print(f"{seq}: mean frame-to-frame abs delta = {d:.3f} /255  ({n} frames)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
