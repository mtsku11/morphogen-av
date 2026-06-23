#!/usr/bin/env python3
"""Synthesize a datamosh readout fixture.

Source A (modulator): a white square translating rightward on black = strong,
unambiguous horizontal motion the optical flow can lock onto.
Source B (carrier): a static, recognizable diagonal-stripe + dot pattern,
identical every frame so any change in the output is the accumulated mosh.
"""
import os
import struct
import sys
import zlib

W = H = 64
N = 8


def write_png(path, rows):
    def chunk(tag, data):
        c = tag + data
        return struct.pack(">I", len(data)) + c + struct.pack(">I", zlib.crc32(c) & 0xFFFFFFFF)

    raw = bytearray()
    for row in rows:
        raw.append(0)  # no filter
        for (r, g, b) in row:
            raw += bytes((r, g, b))
    sig = b"\x89PNG\r\n\x1a\n"
    ihdr = struct.pack(">IIBBBBB", W, H, 8, 2, 0, 0, 0)  # 8-bit RGB
    idat = zlib.compress(bytes(raw), 9)
    with open(path, "wb") as f:
        f.write(sig + chunk(b"IHDR", ihdr) + chunk(b"IDAT", idat) + chunk(b"IEND", b""))


def modulator_frame(i):
    # White square, side 16, sliding right by 6 px/frame on black.
    x0 = 2 + i * 6
    rows = []
    for y in range(H):
        row = []
        for x in range(W):
            inside = (x0 <= x < x0 + 16) and (24 <= y < 40)
            row.append((255, 255, 255) if inside else (0, 0, 0))
        rows.append(row)
    return rows


def carrier_frame(_i):
    # Static pattern: diagonal stripes + a bright centered dot, fully recognizable.
    rows = []
    for y in range(H):
        row = []
        for x in range(W):
            stripe = 220 if ((x + y) // 6) % 2 == 0 else 40
            r, g, b = stripe, stripe // 2, 255 - stripe
            if (x - 32) ** 2 + (y - 32) ** 2 < 36:
                r, g, b = 255, 255, 0  # yellow dot
            row.append((r, g, b))
        rows.append(row)
    return rows


def main():
    out = sys.argv[1] if len(sys.argv) > 1 else "/tmp/datamosh_fixture"
    a_dir = os.path.join(out, "A")
    b_dir = os.path.join(out, "B")
    os.makedirs(a_dir, exist_ok=True)
    os.makedirs(b_dir, exist_ok=True)
    for i in range(N):
        write_png(os.path.join(a_dir, f"frame_{i:06}.png"), modulator_frame(i))
        write_png(os.path.join(b_dir, f"frame_{i:06}.png"), carrier_frame(i))
    print(f"wrote {N} A + {N} B frames ({W}x{H}) to {out}")


if __name__ == "__main__":
    main()
