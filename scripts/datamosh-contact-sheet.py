#!/usr/bin/env python3
"""Datamosh visual-regression contact sheet.

Renders every named destructive datamosh *mode* on a shared input, samples a
handful of evenly spaced frames from each, and tiles them into one labeled
contact-sheet PNG so each mode has pixels to inspect at a glance. It also prints
the mean per-channel RGB cross-delta of each deterministic mode vs the
PASSTHROUGH baseline (the quantitative half of the verify loop) so a regression
shows up as a number, not just by eye.

Two tiers (see docs/DATAMOSH_MILESTONE.md):

  * Deterministic render-graph modes (`render-datamosh-sequence`) run on the
    synthetic bouncing-square / stripe fixture (make-datamosh-fixture.py). They
    are byte-reproducible, so the sheet doubles as a regression baseline.
      - PASSTHROUGH      keyframe-interval 1 (== Source B; the reference row)
      - CODEC BLOOM      full melt, smooth per-pixel bloom (`--preset`)
      - MACROBLOCK SLIDE flow quantized to 16px blocks
      - STRUCTURED MELT  blocks + residual-flow accumulation haze (`--preset`)
      - MACROBLOCK ROT   blocks + per-block keep/drop refresh (trail self-erases, `--preset`)
      - VECTOR SHUFFLE   deterministic block-vector shuffle (`--preset`)

  * Bitstream modes (`datamosh-bitstream`) need ffmpeg + a real input video
    (`--video`). They mangle the compressed stream, so they are NON-deterministic
    (no stable baseline) and are only included when a video is supplied.
      - P-FRAME BLOOM    duplicate a P-frame so its motion vectors re-bloom
      - VOID MOSH        remove the leading keyframe (decode from prediction)

Pure-stdlib: own PNG decode/encode + a built-in 5x7 font, no third-party deps,
so it runs anywhere cargo does.

Usage:
  scripts/datamosh-contact-sheet.py [OUT_PNG] [--video CLIP] [--frames N]
                                    [--workdir DIR] [--bin PATH]
                                    [--p-frame-index I] [--duplicate-count N]
                                    [--fps F]

Examples:
  scripts/datamosh-contact-sheet.py                 # deterministic modes only
  scripts/datamosh-contact-sheet.py sheet.png --video clips/cello.mp4
"""
import argparse
import glob
import os
import struct
import subprocess
import sys
import zlib

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)

# --- contact-sheet layout (pixels) ---
CELL = 96          # each sampled frame is scaled to CELL x CELL (nearest)
GAP = 6
LABEL_W = 156      # left gutter for the mode name
FRAME_BAND = 11    # strip above each cell row for the per-cell "Fn" labels
TITLE_H = 16
PAD = 6
BG = (24, 24, 24)
FG = (235, 235, 235)
DIM = (150, 150, 150)
FAIL = (220, 70, 70)


# ----------------------------------------------------------------------------
# 5x7 bitmap font (uppercase + digits + a little punctuation). Each glyph is 7
# rows of 5 columns; '#' = ink. Verify by Reading the rendered sheet.
# ----------------------------------------------------------------------------
_GLYPHS = {
    " ": "..... ..... ..... ..... ..... ..... .....",
    "A": ".###. #...# #...# ##### #...# #...# #...#",
    "B": "####. #...# #...# ####. #...# #...# ####.",
    "C": ".#### #.... #.... #.... #.... #.... .####",
    "D": "####. #...# #...# #...# #...# #...# ####.",
    "E": "##### #.... #.... ####. #.... #.... #####",
    "F": "##### #.... #.... ####. #.... #.... #....",
    "G": ".#### #.... #.... #.### #...# #...# .####",
    "H": "#...# #...# #...# ##### #...# #...# #...#",
    "I": "##### ..#.. ..#.. ..#.. ..#.. ..#.. #####",
    "J": "..### ...#. ...#. ...#. #..#. #..#. .##..",
    "K": "#...# #..#. #.#.. ##... #.#.. #..#. #...#",
    "L": "#.... #.... #.... #.... #.... #.... #####",
    "M": "#...# ##.## #.#.# #.#.# #...# #...# #...#",
    "N": "#...# ##..# #.#.# #..## #...# #...# #...#",
    "O": ".###. #...# #...# #...# #...# #...# .###.",
    "P": "####. #...# #...# ####. #.... #.... #....",
    "Q": ".###. #...# #...# #...# #.#.# #..#. .##.#",
    "R": "####. #...# #...# ####. #.#.. #..#. #...#",
    "S": ".#### #.... #.... .###. ....# ....# ####.",
    "T": "##### ..#.. ..#.. ..#.. ..#.. ..#.. ..#..",
    "U": "#...# #...# #...# #...# #...# #...# .###.",
    "V": "#...# #...# #...# #...# #...# .#.#. ..#..",
    "W": "#...# #...# #...# #.#.# #.#.# ##.## #...#",
    "X": "#...# #...# .#.#. ..#.. .#.#. #...# #...#",
    "Y": "#...# #...# .#.#. ..#.. ..#.. ..#.. ..#..",
    "Z": "##### ....# ...#. ..#.. .#... #.... #####",
    "0": ".###. #...# #..## #.#.# ##..# #...# .###.",
    "1": "..#.. .##.. ..#.. ..#.. ..#.. ..#.. .###.",
    "2": ".###. #...# ....# ...#. ..#.. .#... #####",
    "3": "##### ...#. ..#.. ...#. ....# #...# .###.",
    "4": "...#. ..##. .#.#. #..#. ##### ...#. ...#.",
    "5": "##### #.... ####. ....# ....# #...# .###.",
    "6": "..##. .#... #.... ####. #...# #...# .###.",
    "7": "##### ....# ...#. ..#.. .#... .#... .#...",
    "8": ".###. #...# #...# .###. #...# #...# .###.",
    "9": ".###. #...# #...# .#### ....# ...#. .##..",
    "-": "..... ..... ..... ##### ..... ..... .....",
    ".": "..... ..... ..... ..... ..... .##.. .##..",
    "/": "....# ....# ...#. ..#.. .#... #.... #....",
    ":": "..... .##.. .##.. ..... .##.. .##.. .....",
}
FONT = {ch: [row for row in art.split(" ")] for ch, art in _GLYPHS.items()}
FW, FH = 5, 7


# ----------------------------------------------------------------------------
# PNG decode / encode (pure stdlib)
# ----------------------------------------------------------------------------
def decode_png(path):
    """Return (w, h, rgb_bytes) with any alpha dropped."""
    data = open(path, "rb").read()
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError(f"not a PNG: {path}")
    i, w, h, ct = 8, 0, 0, 0
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
    ch = {0: 1, 2: 3, 4: 2, 6: 4}[ct]
    raw = zlib.decompress(idat)
    stride = w * ch
    rows = []
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
        rows.append(line)
        prev = line
    # to RGB
    rgb = bytearray(w * h * 3)
    for y in range(h):
        line = rows[y]
        for x in range(w):
            s = x * ch
            if ch >= 3:
                r, g, b = line[s], line[s + 1], line[s + 2]
            else:
                r = g = b = line[s]
            d = (y * w + x) * 3
            rgb[d], rgb[d + 1], rgb[d + 2] = r, g, b
    return w, h, bytes(rgb)


def write_png(path, w, h, rgb):
    def chunk(tag, payload):
        body = tag + payload
        return struct.pack(">I", len(payload)) + body + struct.pack(">I", zlib.crc32(body) & 0xFFFFFFFF)

    raw = bytearray()
    for y in range(h):
        raw.append(0)  # no filter
        raw += rgb[y * w * 3 : (y + 1) * w * 3]
    sig = b"\x89PNG\r\n\x1a\n"
    ihdr = struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)
    idat = zlib.compress(bytes(raw), 9)
    with open(path, "wb") as f:
        f.write(sig + chunk(b"IHDR", ihdr) + chunk(b"IDAT", idat) + chunk(b"IEND", b""))


# ----------------------------------------------------------------------------
# Canvas helpers
# ----------------------------------------------------------------------------
class Canvas:
    def __init__(self, w, h, bg=BG):
        self.w, self.h = w, h
        self.buf = bytearray(bytes(bg) * (w * h))

    def px(self, x, y, c):
        if 0 <= x < self.w and 0 <= y < self.h:
            d = (y * self.w + x) * 3
            self.buf[d], self.buf[d + 1], self.buf[d + 2] = c

    def blit_scaled(self, x0, y0, sw, sh, src_rgb, dw, dh):
        """Nearest-neighbor scale src (sw x sh RGB) into a dw x dh box at (x0,y0)."""
        for dy in range(dh):
            sy = dy * sh // dh
            for dx in range(dw):
                sx = dx * sw // dw
                s = (sy * sw + sx) * 3
                self.px(x0 + dx, y0 + dy, (src_rgb[s], src_rgb[s + 1], src_rgb[s + 2]))

    def text(self, x, y, s, color=FG, scale=1):
        cx = x
        for chh in s.upper():
            glyph = FONT.get(chh, FONT[" "])
            for ry in range(FH):
                row = glyph[ry]
                for rx in range(FW):
                    if row[rx] == "#":
                        for sy in range(scale):
                            for sx in range(scale):
                                self.px(cx + rx * scale + sx, y + ry * scale + sy, color)
            cx += (FW + 1) * scale


# ----------------------------------------------------------------------------
# Rendering the datamosh modes
# ----------------------------------------------------------------------------
def find_cli(explicit):
    if explicit:
        return [explicit]
    # Build (fast no-op when current) so the sheet always matches the source —
    # a stale prebuilt binary would silently lack newer datamosh flags.
    subprocess.run(["cargo", "build", "-p", "morphogen-cli"], cwd=ROOT, check=True)
    return [os.path.join(ROOT, "target/debug/morphogen")]


def sample_indices(n, count):
    if n <= 0:
        return []
    if n <= count:
        return list(range(n))
    return [round(i * (n - 1) / (count - 1)) for i in range(count)]


def seq_frames(d):
    return sorted(glob.glob(os.path.join(d, "frame_*.png")))


def cross_delta(seq_dir, base_dir):
    """Mean per-channel abs RGB delta of seq vs base, averaged over frames."""
    sa, sb = seq_frames(seq_dir), seq_frames(base_dir)
    if not sa or not sb:
        return None
    tot, k = 0.0, 0
    for pa, pb in zip(sa, sb):
        _, _, ra = decode_png(pa)
        _, _, rb = decode_png(pb)
        n = min(len(ra), len(rb))
        tot += sum(abs(ra[i] - rb[i]) for i in range(n)) / n
        k += 1
    return tot / k if k else None


DET_MODES = [
    ("PASSTHROUGH", ["--keyframe-interval", "1"]),
    ("CODEC BLOOM", ["--preset", "codec-bloom"]),
    ("MACROBLOCK SLIDE", ["--keyframe-interval", "0", "--block-size", "16"]),
    ("STRUCTURED MELT", ["--preset", "structured-melt"]),
    ("MACROBLOCK ROT", ["--preset", "macroblock-rot"]),
    ("VECTOR SHUFFLE", ["--preset", "vector-shuffle", "--remix-seed", "42"]),
]


def main():
    ap = argparse.ArgumentParser(description="Datamosh visual-regression contact sheet.")
    ap.add_argument("out", nargs="?", default="/tmp/datamosh-contact-sheet/sheet.png",
                    help="output contact-sheet PNG")
    ap.add_argument("--video", help="real input clip; enables the bitstream modes (needs ffmpeg)")
    ap.add_argument("--frames", type=int, default=5, help="sampled frames per mode (columns)")
    ap.add_argument("--workdir", default="/tmp/datamosh-contact-sheet", help="scratch dir for renders")
    ap.add_argument("--bin", help="path to the morphogen CLI binary (default: auto-detect/build)")
    ap.add_argument("--p-frame-index", type=int, default=5, help="bitstream: P-frame to bloom")
    ap.add_argument("--duplicate-count", type=int, default=30, help="bitstream: P-frame duplications")
    ap.add_argument("--fps", type=float, default=24.0, help="bitstream: encode/decode fps")
    args = ap.parse_args()

    cli = find_cli(args.bin)
    os.makedirs(args.workdir, exist_ok=True)
    cols = max(1, args.frames)

    # --- fixture for the deterministic modes ---
    fixture = os.path.join(args.workdir, "fixture")
    subprocess.run([sys.executable, os.path.join(HERE, "make-datamosh-fixture.py"), fixture], check=True)
    a_dir, b_dir = os.path.join(fixture, "A"), os.path.join(fixture, "B")

    rows = []  # (label, frames_dir or None, info_string)
    base_dir = None

    for name, flags in DET_MODES:
        out_dir = os.path.join(args.workdir, "det_" + name.lower().replace(" ", "_"))
        os.makedirs(out_dir, exist_ok=True)
        cmd = cli + ["render-datamosh-sequence", a_dir, b_dir, out_dir] + flags
        r = subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True)
        if r.returncode != 0:
            print(f"[FAIL] {name}: {r.stderr.strip().splitlines()[-1:] or r.stderr.strip()}", file=sys.stderr)
            rows.append((name, None, "render failed"))
            continue
        if name == "PASSTHROUGH":
            base_dir = out_dir
            rows.append((name, out_dir, "== Source B (baseline)"))
        else:
            d = cross_delta(out_dir, base_dir) if base_dir else None
            info = f"delta vs B {d:.1f}/255" if d is not None else ""
            rows.append((name, out_dir, info))
            if d is not None:
                print(f"{name:18s} cross-delta vs PASSTHROUGH = {d:.3f} /255")

    # --- bitstream modes (optional, non-deterministic) ---
    if args.video:
        bit_modes = [
            ("P-FRAME BLOOM", ["--operation", "pframe-duplicate",
                               "--p-frame-index", str(args.p_frame_index),
                               "--duplicate-count", str(args.duplicate_count)]),
            ("VOID MOSH", ["--operation", "remove-keyframe"]),
        ]
        for name, flags in bit_modes:
            out_dir = os.path.join(args.workdir, "bit_" + name.lower().replace(" ", "_").replace("-", "_"))
            os.makedirs(out_dir, exist_ok=True)
            cmd = cli + ["datamosh-bitstream", args.video, out_dir, "--fps", str(args.fps)] + flags
            r = subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True)
            if r.returncode != 0:
                last = r.stderr.strip().splitlines()[-1] if r.stderr.strip() else "error"
                print(f"[FAIL] {name}: {last}", file=sys.stderr)
                rows.append((name, None, "needs ffmpeg / failed"))
                continue
            rows.append((name, out_dir, "non-deterministic"))
    else:
        print("(no --video: skipping bitstream modes P-FRAME BLOOM / VOID MOSH)")

    # --- compose the contact sheet ---
    W = LABEL_W + cols * (CELL + GAP) + PAD
    row_h = FRAME_BAND + CELL + GAP
    H = TITLE_H + len(rows) * row_h + PAD
    cv = Canvas(W, H)
    cv.text(PAD, 4, "DATAMOSH CONTACT SHEET", FG, 1)

    for ri, (name, fdir, info) in enumerate(rows):
        rtop = TITLE_H + ri * row_h
        ctop = rtop + FRAME_BAND
        # mode name + info in the left gutter
        cv.text(PAD, ctop + CELL // 2 - FH, name, FG, 1)
        if info:
            cv.text(PAD, ctop + CELL // 2 + 3, info, DIM, 1)
        if fdir is None:
            cv.text(LABEL_W, ctop + CELL // 2 - FH, "FAILED", FAIL, 1)
            continue
        paths = seq_frames(fdir)
        idxs = sample_indices(len(paths), cols)
        for ci in range(cols):
            cx = LABEL_W + ci * (CELL + GAP)
            if ci >= len(idxs):
                continue
            fi = idxs[ci]
            cv.text(cx, rtop, f"F{fi}", DIM, 1)
            w, h, rgb = decode_png(paths[fi])
            cv.blit_scaled(cx, ctop, w, h, rgb, CELL, CELL)

    os.makedirs(os.path.dirname(os.path.abspath(args.out)), exist_ok=True)
    write_png(args.out, cv.w, cv.h, cv.buf)
    print(f"wrote contact sheet ({W}x{H}, {len(rows)} modes) to {args.out}")


if __name__ == "__main__":
    main()
