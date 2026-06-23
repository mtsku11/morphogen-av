#!/usr/bin/env python3
"""Cross-sequence per-frame RGB delta between two PNG dirs (handles RGB vs RGBA).

Usage: dm-cross-delta.py <dirA> <dirB>
Prints the mean per-channel abs RGB difference for each matching frame index,
ignoring any alpha channel (so a re-encode RGB->RGBA reads as 0, not noise).
"""
import glob
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from importlib import import_module

fd = import_module("frame-delta")


def channels(path):
    data = open(path, "rb").read()
    import struct

    i = 8
    ct = 0
    while i < len(data):
        ln = struct.unpack(">I", data[i : i + 4])[0]
        if data[i + 4 : i + 8] == b"IHDR":
            ct = data[i + 8 + 9]
            break
        i += 12 + ln
    return {0: 1, 2: 3, 4: 2, 6: 4}[ct]


def to_rgb(path):
    raw = fd.load_png(path)
    ch = channels(path)
    if ch >= 3:
        return bytes(raw[i] for i in range(len(raw)) if i % ch < 3)
    return raw  # grayscale; compare as-is


def main(a, b):
    pa = sorted(glob.glob(os.path.join(a, "frame_*.png")))
    pb = sorted(glob.glob(os.path.join(b, "frame_*.png")))
    for ia, ib in zip(pa, pb):
        ra, rb = to_rgb(ia), to_rgb(ib)
        n = min(len(ra), len(rb))
        d = sum(abs(ra[i] - rb[i]) for i in range(n)) / n
        print(f"{os.path.basename(ia)}: mean RGB delta = {d:.3f} /255")


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
