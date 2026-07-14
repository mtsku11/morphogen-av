//! Pure-Rust MPEG-4 Part 2 P-VOP motion-vector surgery — the ffglitch-style
//! "vector remix" tier of the experimental datamosh path.
//!
//! Scope is exactly the substrate the datamosh-bitstream tier standardizes on:
//! FFmpeg's LGPL `mpeg4` encoder output (`-c:v mpeg4 -bf 0 -g 999999
//! -sc_threshold 0 -an`, AVI) — MPEG-4 Simple Profile, rectangular VOL,
//! progressive, half-pel, single video packet per VOP. Anything outside that
//! (quarter-pel, GMC/sprite, interlace, data partitioning, resync markers,
//! scalability, complexity estimation) is rejected with a clear error.
//!
//! The parser walks every P-VOP's macroblock layer at the bit level, but only
//! motion vectors are decoded to *values*; every other syntax element (headers,
//! MCBPC/CBPY, quantizer updates, intra DC, all DCT coefficient VLCs) is recorded
//! as a bit *span* and copied verbatim on re-emit. Editing a motion vector
//! re-encodes its differential against the freshly recomputed median predictor
//! (the same prediction the decoder will run), so the emitted stream is exactly
//! what a compliant decoder expects. With no edits, re-emission is bit-identical
//! by construction — [`verify_roundtrip`] proves that per chunk and is the
//! ground-truth gate for the VLC tables below.
//!
//! VLC table values are interoperability constants from ISO/IEC 14496-2
//! (Tables B-6 to B-16), cross-checked against FFmpeg's decoder so that the
//! canonical encoding choices (e.g. the sign of a wrapped ±32·2^(fcode-1) MV
//! differential) match the encoder that produced the substrate.
//!
//! **Invariant carve-out.** Same as [`crate::avi`]: the surgery itself is
//! deterministic and unit-tested; the surrounding encode/decode is external
//! ffmpeg, so end-to-end output is not bit-reproducible.

use crate::{avi, MediaError};

// ---------------------------------------------------------------------------
// Bit I/O
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
struct BitSpan {
    start: usize,
    len: usize,
}

struct BitReader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> BitReader<'a> {
    fn at(data: &'a [u8], bit_pos: usize) -> Self {
        Self { data, pos: bit_pos }
    }

    fn bits_left(&self) -> usize {
        self.data.len() * 8 - self.pos.min(self.data.len() * 8)
    }

    fn read_bit(&mut self) -> Result<u32, MediaError> {
        if self.pos >= self.data.len() * 8 {
            return Err(MediaError::MalformedMpeg4(format!(
                "bitstream truncated at bit {}",
                self.pos
            )));
        }
        let byte = self.data[self.pos / 8];
        let bit = (byte >> (7 - (self.pos % 8))) & 1;
        self.pos += 1;
        Ok(bit as u32)
    }

    fn read_bits(&mut self, count: u32) -> Result<u32, MediaError> {
        debug_assert!(count <= 24);
        let mut value = 0u32;
        for _ in 0..count {
            value = (value << 1) | self.read_bit()?;
        }
        Ok(value)
    }

    /// Peek up to `count` bits without consuming; missing bits past the end of
    /// the buffer are zero-padded on the right (standard VLC-LUT convention).
    fn peek_bits_padded(&self, count: u32) -> u32 {
        let mut value = 0u32;
        let total = self.data.len() * 8;
        for offset in 0..count as usize {
            let pos = self.pos + offset;
            let bit = if pos < total {
                (self.data[pos / 8] >> (7 - (pos % 8))) & 1
            } else {
                0
            };
            value = (value << 1) | bit as u32;
        }
        value
    }

    fn skip(&mut self, count: usize) -> Result<(), MediaError> {
        if self.pos + count > self.data.len() * 8 {
            return Err(MediaError::MalformedMpeg4(format!(
                "bitstream truncated skipping {count} bits at bit {}",
                self.pos
            )));
        }
        self.pos += count;
        Ok(())
    }
}

#[derive(Default)]
struct BitWriter {
    bytes: Vec<u8>,
    bit_len: usize,
}

impl BitWriter {
    fn put_bit(&mut self, bit: u32) {
        if self.bit_len % 8 == 0 {
            self.bytes.push(0);
        }
        if bit & 1 != 0 {
            let idx = self.bit_len / 8;
            self.bytes[idx] |= 1 << (7 - (self.bit_len % 8));
        }
        self.bit_len += 1;
    }

    fn put_bits(&mut self, count: u32, value: u32) {
        for shift in (0..count).rev() {
            self.put_bit((value >> shift) & 1);
        }
    }

    fn copy_span(&mut self, src: &[u8], span: BitSpan) -> Result<(), MediaError> {
        let mut reader = BitReader::at(src, span.start);
        let mut remaining = span.len;
        while remaining > 0 {
            let take = remaining.min(24) as u32;
            let value = reader.read_bits(take)?;
            self.put_bits(take, value);
            remaining -= take as usize;
        }
        Ok(())
    }

    /// MPEG-4 end-of-VOP stuffing: `0` followed by ones up to the next byte
    /// boundary; a full `01111111` byte when already aligned (matches the
    /// reference encoder's `ff_mpeg4_stuffing`).
    fn put_vop_stuffing(&mut self) {
        let length = 8 - (self.bit_len % 8) as u32;
        self.put_bits(length, (1 << (length - 1)) - 1);
    }

    fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

// ---------------------------------------------------------------------------
// VLC tables (ISO/IEC 14496-2 interoperability constants)
// ---------------------------------------------------------------------------

/// A canonical prefix-code table entry: `(code, bits, symbol)`.
type VlcEntry = (u16, u8, u16);

struct Vlc {
    /// Indexed by the next `max_bits` bits (left-aligned); `(symbol, length)`.
    /// `length == 0` marks an invalid prefix.
    lut: Vec<(u16, u8)>,
    max_bits: u8,
}

impl Vlc {
    fn build(entries: &[VlcEntry]) -> Self {
        let max_bits = entries.iter().map(|entry| entry.1).max().unwrap_or(1);
        let mut lut = vec![(u16::MAX, 0u8); 1usize << max_bits];
        for &(code, bits, symbol) in entries {
            let shift = max_bits - bits;
            let base = (code as usize) << shift;
            for slot in &mut lut[base..base + (1usize << shift)] {
                *slot = (symbol, bits);
            }
        }
        Self { lut, max_bits }
    }

    fn decode(&self, reader: &mut BitReader<'_>, what: &str) -> Result<u16, MediaError> {
        let index = reader.peek_bits_padded(self.max_bits as u32) as usize;
        let (symbol, length) = self.lut[index];
        if length == 0 {
            return Err(MediaError::MalformedMpeg4(format!(
                "invalid {what} VLC at bit {}",
                reader.pos
            )));
        }
        if reader.bits_left() < length as usize {
            return Err(MediaError::MalformedMpeg4(format!(
                "bitstream truncated inside {what} VLC at bit {}",
                reader.pos
            )));
        }
        reader.skip(length as usize)?;
        Ok(symbol)
    }
}

/// P-VOP MCBPC (Table B-7/B-8). Symbol = raw index: `sym & 3` = CBPC,
/// `sym & 4` = intra, `sym & 8` = +Q, `sym & 16` = four-vector; 20 = stuffing.
const INTER_MCBPC_ENTRIES: &[VlcEntry] = &[
    (1, 1, 0),
    (3, 4, 1),
    (2, 4, 2),
    (5, 6, 3),
    (3, 5, 4),
    (4, 8, 5),
    (3, 8, 6),
    (3, 7, 7),
    (3, 3, 8),
    (7, 7, 9),
    (6, 7, 10),
    (5, 9, 11),
    (4, 6, 12),
    (4, 9, 13),
    (3, 9, 14),
    (2, 9, 15),
    (2, 3, 16),
    (5, 7, 17),
    (4, 7, 18),
    (5, 8, 19),
    (1, 9, 20),
    (2, 11, 24),
    (12, 13, 25),
    (14, 13, 26),
    (15, 13, 27),
];
const MCBPC_STUFFING: u16 = 20;

/// CBPY (Table B-9). Symbol = the 4-bit intra pattern; inter uses `sym ^ 0xF`.
const CBPY_ENTRIES: &[VlcEntry] = &[
    (3, 4, 0),
    (5, 5, 1),
    (4, 5, 2),
    (9, 4, 3),
    (3, 5, 4),
    (7, 4, 5),
    (2, 6, 6),
    (11, 4, 7),
    (2, 5, 8),
    (3, 6, 9),
    (5, 4, 10),
    (10, 4, 11),
    (4, 4, 12),
    (8, 4, 13),
    (6, 4, 14),
    (3, 2, 15),
];

/// Motion-vector magnitude (Table B-12). Symbol 0 = zero differential.
const MV_ENTRIES: &[VlcEntry] = &[
    (1, 1, 0),
    (1, 2, 1),
    (1, 3, 2),
    (1, 4, 3),
    (3, 6, 4),
    (5, 7, 5),
    (4, 7, 6),
    (3, 7, 7),
    (11, 9, 8),
    (10, 9, 9),
    (9, 9, 10),
    (17, 10, 11),
    (16, 10, 12),
    (15, 10, 13),
    (14, 10, 14),
    (13, 10, 15),
    (12, 10, 16),
    (11, 10, 17),
    (10, 10, 18),
    (9, 10, 19),
    (8, 10, 20),
    (7, 10, 21),
    (6, 10, 22),
    (5, 10, 23),
    (4, 10, 24),
    (7, 11, 25),
    (6, 11, 26),
    (5, 11, 27),
    (4, 11, 28),
    (3, 11, 29),
    (2, 11, 30),
    (3, 12, 31),
    (2, 12, 32),
];

/// Intra DC size, luma (Table B-13).
const DC_LUM_ENTRIES: &[VlcEntry] = &[
    (3, 3, 0),
    (3, 2, 1),
    (2, 2, 2),
    (2, 3, 3),
    (1, 3, 4),
    (1, 4, 5),
    (1, 5, 6),
    (1, 6, 7),
    (1, 7, 8),
    (1, 8, 9),
    (1, 9, 10),
    (1, 10, 11),
    (1, 11, 12),
];

/// Intra DC size, chroma (Table B-14).
const DC_CHROM_ENTRIES: &[VlcEntry] = &[
    (3, 2, 0),
    (2, 2, 1),
    (1, 2, 2),
    (1, 3, 3),
    (1, 4, 4),
    (1, 5, 5),
    (1, 6, 6),
    (1, 7, 7),
    (1, 8, 8),
    (1, 9, 9),
    (1, 10, 10),
    (1, 11, 11),
    (1, 12, 12),
];

/// TCOEF `(code, bits)` pairs in RL-table order. Only the *last* flag matters
/// for parsing (run/level values never influence syntax), so the symbol is the
/// RL index: inter indices < 58 have last=0, intra indices < 67 have last=0;
/// index 102 is the escape prefix.
const TCOEF_ESCAPE: u16 = 102;
const INTER_TCOEF_LAST_SPLIT: u16 = 58;
const INTRA_TCOEF_LAST_SPLIT: u16 = 67;

const INTER_TCOEF_CODES: [(u16, u8); 103] = [
    (0x2, 2),
    (0xf, 4),
    (0x15, 6),
    (0x17, 7),
    (0x1f, 8),
    (0x25, 9),
    (0x24, 9),
    (0x21, 10),
    (0x20, 10),
    (0x7, 11),
    (0x6, 11),
    (0x20, 11),
    (0x6, 3),
    (0x14, 6),
    (0x1e, 8),
    (0xf, 10),
    (0x21, 11),
    (0x50, 12),
    (0xe, 4),
    (0x1d, 8),
    (0xe, 10),
    (0x51, 12),
    (0xd, 5),
    (0x23, 9),
    (0xd, 10),
    (0xc, 5),
    (0x22, 9),
    (0x52, 12),
    (0xb, 5),
    (0xc, 10),
    (0x53, 12),
    (0x13, 6),
    (0xb, 10),
    (0x54, 12),
    (0x12, 6),
    (0xa, 10),
    (0x11, 6),
    (0x9, 10),
    (0x10, 6),
    (0x8, 10),
    (0x16, 7),
    (0x55, 12),
    (0x15, 7),
    (0x14, 7),
    (0x1c, 8),
    (0x1b, 8),
    (0x21, 9),
    (0x20, 9),
    (0x1f, 9),
    (0x1e, 9),
    (0x1d, 9),
    (0x1c, 9),
    (0x1b, 9),
    (0x1a, 9),
    (0x22, 11),
    (0x23, 11),
    (0x56, 12),
    (0x57, 12),
    (0x7, 4),
    (0x19, 9),
    (0x5, 11),
    (0xf, 6),
    (0x4, 11),
    (0xe, 6),
    (0xd, 6),
    (0xc, 6),
    (0x13, 7),
    (0x12, 7),
    (0x11, 7),
    (0x10, 7),
    (0x1a, 8),
    (0x19, 8),
    (0x18, 8),
    (0x17, 8),
    (0x16, 8),
    (0x15, 8),
    (0x14, 8),
    (0x13, 8),
    (0x18, 9),
    (0x17, 9),
    (0x16, 9),
    (0x15, 9),
    (0x14, 9),
    (0x13, 9),
    (0x12, 9),
    (0x11, 9),
    (0x7, 10),
    (0x6, 10),
    (0x5, 10),
    (0x4, 10),
    (0x24, 11),
    (0x25, 11),
    (0x26, 11),
    (0x27, 11),
    (0x58, 12),
    (0x59, 12),
    (0x5a, 12),
    (0x5b, 12),
    (0x5c, 12),
    (0x5d, 12),
    (0x5e, 12),
    (0x5f, 12),
    (0x3, 7),
];

const INTRA_TCOEF_CODES: [(u16, u8); 103] = [
    (0x2, 2),
    (0x6, 3),
    (0xf, 4),
    (0xd, 5),
    (0xc, 5),
    (0x15, 6),
    (0x13, 6),
    (0x12, 6),
    (0x17, 7),
    (0x1f, 8),
    (0x1e, 8),
    (0x1d, 8),
    (0x25, 9),
    (0x24, 9),
    (0x23, 9),
    (0x21, 9),
    (0x21, 10),
    (0x20, 10),
    (0xf, 10),
    (0xe, 10),
    (0x7, 11),
    (0x6, 11),
    (0x20, 11),
    (0x21, 11),
    (0x50, 12),
    (0x51, 12),
    (0x52, 12),
    (0xe, 4),
    (0x14, 6),
    (0x16, 7),
    (0x1c, 8),
    (0x20, 9),
    (0x1f, 9),
    (0xd, 10),
    (0x22, 11),
    (0x53, 12),
    (0x55, 12),
    (0xb, 5),
    (0x15, 7),
    (0x1e, 9),
    (0xc, 10),
    (0x56, 12),
    (0x11, 6),
    (0x1b, 8),
    (0x1d, 9),
    (0xb, 10),
    (0x10, 6),
    (0x22, 9),
    (0xa, 10),
    (0xd, 6),
    (0x1c, 9),
    (0x8, 10),
    (0x12, 7),
    (0x1b, 9),
    (0x54, 12),
    (0x14, 7),
    (0x1a, 9),
    (0x57, 12),
    (0x19, 8),
    (0x9, 10),
    (0x18, 8),
    (0x23, 11),
    (0x17, 8),
    (0x19, 9),
    (0x18, 9),
    (0x7, 10),
    (0x58, 12),
    (0x7, 4),
    (0xc, 6),
    (0x16, 8),
    (0x17, 9),
    (0x6, 10),
    (0x5, 11),
    (0x4, 11),
    (0x59, 12),
    (0xf, 6),
    (0x16, 9),
    (0x5, 10),
    (0xe, 6),
    (0x4, 10),
    (0x11, 7),
    (0x24, 11),
    (0x10, 7),
    (0x25, 11),
    (0x13, 7),
    (0x5a, 12),
    (0x15, 8),
    (0x5b, 12),
    (0x14, 8),
    (0x13, 8),
    (0x1a, 8),
    (0x15, 9),
    (0x14, 9),
    (0x13, 9),
    (0x12, 9),
    (0x11, 9),
    (0x26, 11),
    (0x27, 11),
    (0x5c, 12),
    (0x5d, 12),
    (0x5e, 12),
    (0x5f, 12),
    (0x3, 7),
];

/// Run values per inter RL index (ISO/IEC 14496-2 Table B-14 order, matching
/// [`INTER_TCOEF_CODES`]).
const INTER_TCOEF_RUN: [u8; 102] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 4, 4, 4, 5, 5, 5,
    6, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24,
    25, 26, 0, 0, 0, 1, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20,
    21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40,
];

/// Level magnitudes per inter RL index (same order).
const INTER_TCOEF_LEVEL: [u8; 102] = [
    1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 1, 2, 3, 4, 5, 6, 1, 2, 3, 4, 1, 2, 3, 1, 2, 3, 1, 2,
    3, 1, 2, 3, 1, 2, 1, 2, 1, 2, 1, 2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 2, 3,
    1, 2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
];

/// One decoded inter TCOEF event. `run` zeros precede a coefficient of signed
/// quantized `level`; the block's final event is implicitly `last = 1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TcoefEvent {
    run: u8,
    level: i16,
}

/// Canonical inter-TCOEF encoder state, mirroring the reference encoder's
/// `init_uni_mpeg4_rl_tab`: for every `(last, run, level ∈ [-64, 63])` the
/// shortest of {direct VLC, escape 1, escape 2, escape 3} with the reference
/// tie-breaking, so re-encoding unedited events is byte-identical. Levels
/// outside the table range always use the 30-bit escape 3. `lmax`/`rmax` feed
/// escape decoding.
struct InterRlCodec {
    uni_bits: Vec<u32>,
    uni_len: Vec<u8>,
    lmax: [[u8; 64]; 2],
    rmax: [[u8; 64]; 2],
}

const ESC3_LEN: u8 = 30;

impl InterRlCodec {
    fn uni_index(last: usize, run: usize, level: i32) -> usize {
        (last * 64 + run) * 128 + (level + 64) as usize
    }

    fn build() -> Self {
        let mut uni_bits = vec![0u32; 2 * 64 * 128];
        let mut uni_len = vec![0u8; 2 * 64 * 128];

        // Default: every representable (run, level) slot holds its escape-3
        // encoding (the reference table's memset(30) + type-3 prefill).
        for run in 0..64usize {
            for level in -64i32..64 {
                if level == 0 {
                    continue;
                }
                let code = (3u32 << 23)
                    | (3 << 21)
                    | ((run as u32) << 14)
                    | (1 << 13)
                    | (((level & 0xfff) as u32) << 1)
                    | 1;
                for last in 0..2usize {
                    let idx = Self::uni_index(last, run, level);
                    uni_bits[idx] = code | ((last as u32) << 20);
                    uni_len[idx] = ESC3_LEN;
                }
            }
        }

        let mut lmax = [[0u8; 64]; 2];
        let mut rmax = [[0u8; 64]; 2];
        for i in 0..102usize {
            let last = usize::from(i >= INTER_TCOEF_LAST_SPLIT as usize);
            let run = INTER_TCOEF_RUN[i] as usize;
            let level = INTER_TCOEF_LEVEL[i] as usize;
            lmax[last][run] = lmax[last][run].max(level as u8);
            rmax[last][level] = rmax[last][level].max(run as u8);
        }

        // Downward traversal exactly like the reference: direct VLC, then the
        // escape-2 slot if shorter, then the escape-1 slot unconditionally.
        let mut max_run = [[0u8; 64]; 2];
        let mut max_level = 0i32;
        let mut cur_run = usize::MAX;
        for i in (0..102usize).rev() {
            let last = usize::from(i >= INTER_TCOEF_LAST_SPLIT as usize);
            let run = INTER_TCOEF_RUN[i] as usize;
            let level = INTER_TCOEF_LEVEL[i] as i32;
            let (vlc_code, vlc_bits) = INTER_TCOEF_CODES[i];
            let code = (vlc_code as u32) << 1;
            let len = vlc_bits + 1;

            for (value, sign) in [(level, 0u32), (-level, 1u32)] {
                let idx = Self::uni_index(last, run, value);
                uni_bits[idx] = code | sign;
                uni_len[idx] = len;
            }

            if max_run[last][level as usize] == 0 {
                max_run[last][level as usize] = run as u8 + 1;
            }
            let run3 = run + max_run[last][level as usize] as usize;
            let len3 = len + 7 + 2;
            if run3 < 64 && len3 < uni_len[Self::uni_index(last, run3, level)] {
                let code3 = code | (0b1110u32 << len);
                for (value, sign) in [(level, 0u32), (-level, 1u32)] {
                    let idx = Self::uni_index(last, run3, value);
                    uni_bits[idx] = code3 | sign;
                    uni_len[idx] = len3;
                }
            }

            if run != cur_run {
                max_level = level;
                cur_run = run;
            }
            let esc1_code = code | (0x3u32 << (len + 1));
            let esc1_len = len + 7 + 1;
            let esc1_level = level + max_level;
            for (value, sign) in [(esc1_level, 0u32), (-esc1_level, 1u32)] {
                let idx = Self::uni_index(last, run, value);
                uni_bits[idx] = esc1_code | sign;
                uni_len[idx] = esc1_len;
            }
        }

        Self {
            uni_bits,
            uni_len,
            lmax,
            rmax,
        }
    }

    fn encode_event(&self, writer: &mut BitWriter, last: bool, run: u8, level: i16) {
        debug_assert!(level != 0 && run < 64);
        let level = level as i32;
        if (-64..64).contains(&level) {
            let idx = Self::uni_index(usize::from(last), run as usize, level);
            writer.put_bits(self.uni_len[idx] as u32, self.uni_bits[idx]);
        } else {
            let code = (3u32 << 23)
                | (3 << 21)
                | (u32::from(last) << 20)
                | ((run as u32) << 14)
                | (1 << 13)
                | (((level & 0xfff) as u32) << 1)
                | 1;
            writer.put_bits(ESC3_LEN as u32, code);
        }
    }
}

struct Tables {
    inter_mcbpc: Vlc,
    cbpy: Vlc,
    mv: Vlc,
    dc_lum: Vlc,
    dc_chrom: Vlc,
    tcoef_inter: Vlc,
    tcoef_intra: Vlc,
    inter_rl: InterRlCodec,
}

fn tables() -> &'static Tables {
    use std::sync::OnceLock;
    static TABLES: OnceLock<Tables> = OnceLock::new();
    TABLES.get_or_init(|| {
        let rl = |codes: &[(u16, u8); 103]| {
            let entries: Vec<VlcEntry> = codes
                .iter()
                .enumerate()
                .map(|(index, &(code, bits))| (code, bits, index as u16))
                .collect();
            Vlc::build(&entries)
        };
        Tables {
            inter_mcbpc: Vlc::build(INTER_MCBPC_ENTRIES),
            cbpy: Vlc::build(CBPY_ENTRIES),
            mv: Vlc::build(MV_ENTRIES),
            dc_lum: Vlc::build(DC_LUM_ENTRIES),
            dc_chrom: Vlc::build(DC_CHROM_ENTRIES),
            tcoef_inter: rl(&INTER_TCOEF_CODES),
            tcoef_intra: rl(&INTRA_TCOEF_CODES),
            inter_rl: InterRlCodec::build(),
        }
    })
}

/// `intra_dc_vlc_thr` → the quantizer below which intra DC uses the DC VLC
/// (ISO/IEC 14496-2 Table 6-21; index 0 = "always DC VLC").
const DC_VLC_THRESHOLD: [u8; 8] = [99, 13, 15, 17, 19, 21, 23, 0];

// ---------------------------------------------------------------------------
// Motion-vector arithmetic
// ---------------------------------------------------------------------------

fn sign_extend(value: i32, bits: u32) -> i32 {
    let shift = 32 - bits;
    (value << shift) >> shift
}

fn median3(a: i32, b: i32, c: i32) -> i32 {
    a.max(b).min(b.max(c)).min(a.max(c))
}

/// Half-pel window representable at a given fcode: `[-32<<(fcode-1), (32<<(fcode-1))-1]`.
fn mv_window(fcode: u8) -> (i32, i32) {
    let half = 32i32 << (fcode - 1);
    (-half, half - 1)
}

fn decode_motion(
    reader: &mut BitReader<'_>,
    pred: i32,
    fcode: u8,
) -> Result<i32, MediaError> {
    let symbol = tables().mv.decode(reader, "motion")? as i32;
    if symbol == 0 {
        return Ok(pred);
    }
    let sign = reader.read_bit()?;
    let shift = (fcode - 1) as u32;
    let mut value = symbol;
    if shift > 0 {
        value = ((value - 1) << shift) | reader.read_bits(shift)? as i32;
        value += 1;
    }
    if sign == 1 {
        value = -value;
    }
    Ok(sign_extend(value + pred, 5 + fcode as u32))
}

fn encode_motion(writer: &mut BitWriter, diff: i32, fcode: u8) {
    if diff == 0 {
        let (code, bits, _) = MV_ENTRIES[0];
        writer.put_bits(bits as u32, code as u32);
        return;
    }
    let bit_size = (fcode - 1) as u32;
    let value = sign_extend(diff, 6 + bit_size);
    let sign = if value < 0 { 1u32 } else { 0u32 };
    let mut magnitude = value.unsigned_abs();
    magnitude -= 1;
    let code = (magnitude >> bit_size) + 1;
    let residual = magnitude & ((1u32 << bit_size).wrapping_sub(1));
    let (vlc_code, vlc_bits, _) = MV_ENTRIES[code as usize];
    writer.put_bits(vlc_bits as u32, vlc_code as u32);
    writer.put_bit(sign);
    if bit_size > 0 {
        writer.put_bits(bit_size, residual);
    }
}

/// Per-video-packet prediction availability state, mirroring the reference
/// decoder's `resync_mb_x` / `resync_mb_y` / `first_slice_line` trio.
#[derive(Debug, Clone, Copy)]
struct SliceState {
    resync_mb_x: usize,
    resync_mb_y: usize,
    first_slice_line: bool,
}

impl SliceState {
    fn frame_start() -> Self {
        Self {
            resync_mb_x: 0,
            resync_mb_y: 0,
            first_slice_line: true,
        }
    }

    /// Decoder rule: the "first slice line" ends at the MB directly below the
    /// packet's starting MB.
    fn at_mb_start(&mut self, mb_x: usize, mb_y: usize) {
        if mb_x == self.resync_mb_x && mb_y == self.resync_mb_y + 1 {
            self.first_slice_line = false;
        }
    }

    fn on_packet(&mut self, mb_x: usize, mb_y: usize) {
        self.resync_mb_x = mb_x;
        self.resync_mb_y = mb_y;
        self.first_slice_line = true;
    }
}

/// The decoder-side motion-vector grid at 8×8-block granularity, with the same
/// border trick as the reference decoder: stride `2*mb_w + 1` so `index - 1` at
/// the left edge lands on the never-written spare column (zeros), plus a zeroed
/// top border row.
struct MvGrid {
    wrap: usize,
    vals: Vec<[i16; 2]>,
}

/// Per-block offset of the C (top-right / top-left) predictor candidate,
/// applied as `xy + OFF[block] - wrap` (matches the reference decoder).
const PRED_C_OFF: [isize; 4] = [2, 1, 1, -1];

impl MvGrid {
    fn new(mb_w: usize, mb_h: usize) -> Self {
        let wrap = 2 * mb_w + 1;
        Self {
            wrap,
            vals: vec![[0, 0]; wrap * (2 * mb_h + 1)],
        }
    }

    fn index(&self, mb_x: usize, mb_y: usize, block: usize) -> usize {
        (2 * mb_y + (block >> 1) + 1) * self.wrap + 2 * mb_x + (block & 1)
    }

    fn at(&self, xy: isize) -> [i16; 2] {
        self.vals[xy as usize]
    }

    fn pred(&self, mb_x: usize, mb_y: usize, block: usize, st: &SliceState) -> (i32, i32) {
        let xy = self.index(mb_x, mb_y, block) as isize;
        let wrap = self.wrap as isize;
        let a = self.at(xy - 1);
        let median = |a: [i16; 2], b: [i16; 2], c: [i16; 2]| {
            (
                median3(a[0] as i32, b[0] as i32, c[0] as i32),
                median3(a[1] as i32, b[1] as i32, c[1] as i32),
            )
        };
        if st.first_slice_line && block < 3 {
            match block {
                0 => {
                    if mb_x == st.resync_mb_x {
                        (0, 0)
                    } else if mb_x + 1 == st.resync_mb_x {
                        let c = self.at(xy + PRED_C_OFF[0] - wrap);
                        if mb_x == 0 {
                            (c[0] as i32, c[1] as i32)
                        } else {
                            median(a, [0, 0], c)
                        }
                    } else {
                        (a[0] as i32, a[1] as i32)
                    }
                }
                1 => {
                    if mb_x + 1 == st.resync_mb_x {
                        let c = self.at(xy + PRED_C_OFF[1] - wrap);
                        median(a, [0, 0], c)
                    } else {
                        (a[0] as i32, a[1] as i32)
                    }
                }
                _ => {
                    let b = self.at(xy - wrap);
                    let c = self.at(xy + PRED_C_OFF[2] - wrap);
                    let a = if mb_x == st.resync_mb_x { [0, 0] } else { a };
                    median(a, b, c)
                }
            }
        } else {
            let b = self.at(xy - wrap);
            let c = self.at(xy + PRED_C_OFF[block] - wrap);
            median(a, b, c)
        }
    }

    fn set_block(&mut self, mb_x: usize, mb_y: usize, block: usize, mv: (i32, i32)) {
        let xy = self.index(mb_x, mb_y, block);
        self.vals[xy] = [mv.0 as i16, mv.1 as i16];
    }

    fn set_mb(&mut self, mb_x: usize, mb_y: usize, mv: (i32, i32)) {
        for block in 0..4 {
            self.set_block(mb_x, mb_y, block, mv);
        }
    }
}

// ---------------------------------------------------------------------------
// VOL configuration
// ---------------------------------------------------------------------------

/// The VOL-header facts the P-VOP parser needs (plus the checks that gate the
/// supported syntax subset).
#[derive(Debug, Clone)]
pub struct VolConfig {
    pub width: u32,
    pub height: u32,
    time_increment_bits: u32,
    quant_precision: u32,
}

impl VolConfig {
    fn mb_width(&self) -> usize {
        self.width.div_ceil(16) as usize
    }

    fn mb_height(&self) -> usize {
        self.height.div_ceil(16) as usize
    }
}

fn find_start_code(data: &[u8], from: usize, matches: impl Fn(u8) -> bool) -> Option<usize> {
    let mut i = from;
    while i + 4 <= data.len() {
        if data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 1 && matches(data[i + 3]) {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn unsupported(feature: &str) -> MediaError {
    MediaError::MalformedMpeg4(format!(
        "{feature} is not supported by the pure-Rust MV editor; re-encode the substrate with \
         plain `ffmpeg -c:v mpeg4 -bf 0 -g 999999 -sc_threshold 0 -an`"
    ))
}

/// Parse the VOL header out of a buffer that contains one (FFmpeg puts the
/// VOS/VO/VOL headers in front of the first keyframe chunk).
pub fn parse_vol_config(data: &[u8]) -> Result<VolConfig, MediaError> {
    let vol_pos = find_start_code(data, 0, |code| (0x20..=0x2f).contains(&code))
        .ok_or_else(|| MediaError::MalformedMpeg4("no VOL start code found".to_string()))?;
    let mut r = BitReader::at(data, (vol_pos + 4) * 8);

    r.read_bit()?; // random_accessible_vol
    let vo_type = r.read_bits(8)?;
    if vo_type == 14 || vo_type == 15 {
        return Err(unsupported("studio profile"));
    }
    let vo_ver_id = if r.read_bit()? == 1 {
        // is_object_layer_identifier
        let ver = r.read_bits(4)?;
        r.read_bits(3)?; // vo_priority
        ver
    } else {
        1
    };
    let aspect = r.read_bits(4)?;
    if aspect == 0xf {
        r.read_bits(16)?; // par_width + par_height
    }
    if r.read_bit()? == 1 {
        // vol_control_parameters
        r.read_bits(2)?; // chroma_format
        r.read_bit()?; // low_delay
        if r.read_bit()? == 1 {
            // vbv_parameters
            r.read_bits(15)?;
            r.read_bit()?;
            r.read_bits(15)?;
            r.read_bit()?;
            r.read_bits(15)?;
            r.read_bit()?;
            r.read_bits(3)?;
            r.read_bits(11)?;
            r.read_bit()?;
            r.read_bits(15)?;
            r.read_bit()?;
        }
    }
    let shape = r.read_bits(2)?;
    if shape != 0 {
        return Err(unsupported("non-rectangular VOL shape"));
    }
    r.read_bit()?; // marker
    let time_increment_resolution = r.read_bits(16)?;
    if time_increment_resolution == 0 {
        return Err(MediaError::MalformedMpeg4(
            "VOL time increment resolution is zero".to_string(),
        ));
    }
    let time_increment_bits =
        (32 - (time_increment_resolution.saturating_sub(1)).leading_zeros()).max(1);
    r.read_bit()?; // marker
    if r.read_bit()? == 1 {
        // fixed_vop_rate
        r.read_bits(time_increment_bits)?;
    }
    r.read_bit()?; // marker
    let width = r.read_bits(13)?;
    r.read_bit()?; // marker
    let height = r.read_bits(13)?;
    r.read_bit()?; // marker
    if r.read_bit()? == 1 {
        return Err(unsupported("interlaced VOL"));
    }
    r.read_bit()?; // obmc_disable
    let sprite = if vo_ver_id == 1 {
        r.read_bits(1)?
    } else {
        r.read_bits(2)?
    };
    if sprite != 0 {
        return Err(unsupported("sprite/GMC coding"));
    }
    let quant_precision = if r.read_bit()? == 1 {
        // not_8_bit
        let precision = r.read_bits(4)?;
        r.read_bits(4)?; // bits_per_pixel
        if !(3..=9).contains(&precision) {
            return Err(MediaError::MalformedMpeg4(format!(
                "invalid quant precision {precision}"
            )));
        }
        precision
    } else {
        5
    };
    if r.read_bit()? == 1 {
        // quant_type == 1 (MPEG quantization): skip optional custom matrices
        for _ in 0..2 {
            if r.read_bit()? == 1 {
                for _ in 0..64 {
                    if r.read_bits(8)? == 0 {
                        break;
                    }
                }
            }
        }
    }
    if vo_ver_id != 1 && r.read_bit()? == 1 {
        return Err(unsupported("quarter-pel motion (`+qpel`)"));
    }
    if r.read_bit()? == 0 {
        return Err(unsupported("complexity estimation header"));
    }
    // resync_marker_disable: FFmpeg writes 0 (markers allowed) even though its
    // single-slice output never emits one. Actual in-stream video packets are
    // detected per frame instead (a legal VLC layer cannot emulate the marker's
    // byte-aligned >= 16-zero-bit run).
    r.read_bit()?;
    if r.read_bit()? == 1 {
        return Err(unsupported("data partitioning"));
    }
    if vo_ver_id != 1 {
        if r.read_bit()? == 1 {
            return Err(unsupported("NEWPRED"));
        }
        if r.read_bit()? == 1 {
            return Err(unsupported("reduced-resolution VOPs"));
        }
    }
    if r.read_bit()? == 1 {
        return Err(unsupported("scalability"));
    }

    Ok(VolConfig {
        width,
        height,
        time_increment_bits,
        quant_precision,
    })
}

// ---------------------------------------------------------------------------
// P-VOP parsing
// ---------------------------------------------------------------------------

/// One parsed macroblock. Everything except inter motion vectors is kept as a
/// verbatim bit span.
struct MbRecord {
    /// A video-packet header immediately preceding this MB: the span covers the
    /// byte-aligned resync marker through the end of the packet header (the
    /// stuffing before it is regenerated on emit so alignment survives edits).
    packet: Option<BitSpan>,
    kind: MbKind,
}

/// One of an inter MB's six 8×8 blocks in DCT-edit mode: the original bit span
/// plus decoded coefficient events (`None` = not coded per CBP). `dirty` marks
/// blocks whose events changed and must be re-encoded instead of span-copied.
struct InterBlock {
    span: BitSpan,
    events: Option<Vec<TcoefEvent>>,
    dirty: bool,
}

enum MbKind {
    /// Skipped (`not_coded == 1`) or intra macroblock — copied bit-for-bit; its
    /// four predictor-grid entries stay zero, exactly like the decoder.
    Copied(BitSpan),
    Inter {
        head: BitSpan,
        body: BitSpan,
        four_mv: bool,
        /// Decoded absolute motion vectors in half-pel units. Index 0 is the
        /// 16×16 vector for one-MV macroblocks; all four are populated for 4MV.
        mvs: [[i32; 2]; 4],
        /// Per-block detail, populated only in DCT-edit mode (empty otherwise —
        /// the plain MV path copies `body` wholesale).
        blocks: Vec<InterBlock>,
    },
}

struct PvopParse {
    /// Everything from the start of the chunk through the last VOP-header bit.
    header: BitSpan,
    /// Everything after the final macroblock (stuffing + any trailing bytes).
    tail: BitSpan,
    fcode: u8,
    mbs: Vec<MbRecord>,
}

struct PvopParser<'a> {
    reader: BitReader<'a>,
    fcode: u8,
    qscale: i32,
    dc_threshold: u8,
    quant_max: i32,
    quant_precision: u32,
    time_increment_bits: u32,
    mb_total: usize,
    state: SliceState,
    /// Decode inter-block TCOEF events into [`InterBlock`] records (DCT-edit mode).
    collect_dct: bool,
}

impl<'a> PvopParser<'a> {
    fn mb_num_bits(&self) -> u32 {
        if self.mb_total > 1 {
            32 - (self.mb_total as u32 - 1).leading_zeros()
        } else {
            1
        }
    }

    /// Resync-marker prefix length for a P-VOP: `15 + fcode` zero bits.
    fn marker_zeros(&self) -> u32 {
        15 + self.fcode as u32
    }

    /// Strict video-packet detection at an MB boundary: canonical stuffing
    /// (`0` + ones to the byte boundary, a full `0x7F` byte when aligned)
    /// followed by the resync marker's zero run and terminating `1`.
    fn peek_packet(&self) -> bool {
        let stuff_len = 8 - (self.reader.pos % 8) as u32;
        let total = stuff_len + self.marker_zeros() + 1;
        if self.reader.bits_left() < total as usize {
            return false;
        }
        let bits = self.reader.peek_bits_padded(total);
        let stuffing = bits >> (self.marker_zeros() + 1);
        stuffing == (1 << (stuff_len - 1)) - 1 && bits & ((1 << (self.marker_zeros() + 1)) - 1) == 1
    }

    /// Consume stuffing + video packet header; returns the span from the
    /// byte-aligned marker through the end of the header. `mb_index` is the MB
    /// the packet must start at (sequential decode admits nothing else).
    fn parse_packet(&mut self, mb_index: usize) -> Result<BitSpan, MediaError> {
        let stuff_len = 8 - (self.reader.pos % 8) as u32;
        let stuffing = self.reader.read_bits(stuff_len)?;
        if stuffing != (1 << (stuff_len - 1)) - 1 {
            return Err(MediaError::MalformedMpeg4(format!(
                "non-canonical stuffing before video packet at bit {}",
                self.reader.pos
            )));
        }
        let start = self.reader.pos;
        if self.reader.read_bits(self.marker_zeros())? != 0 || self.reader.read_bit()? != 1 {
            return Err(MediaError::MalformedMpeg4(format!(
                "malformed resync marker at bit {start}"
            )));
        }
        let mb_num = self.reader.read_bits(self.mb_num_bits())? as usize;
        if mb_num != mb_index {
            return Err(MediaError::MalformedMpeg4(format!(
                "video packet claims MB {mb_num} but decode is at MB {mb_index}"
            )));
        }
        let quant = self.reader.read_bits(self.quant_precision)? as i32;
        if quant != 0 {
            self.qscale = quant;
        }
        if self.reader.read_bit()? == 1 {
            // header_extension_code: repeated timing + type + fcode fields,
            // ignored by the reference decoder (and so by us) but consumed.
            let mut guard = 0;
            while self.reader.read_bit()? == 1 {
                guard += 1;
                if guard > 32 {
                    return Err(MediaError::MalformedMpeg4(
                        "runaway modulo_time_base in video packet header".to_string(),
                    ));
                }
            }
            if self.reader.read_bit()? != 1 {
                return Err(MediaError::MalformedMpeg4(
                    "missing marker in video packet header".to_string(),
                ));
            }
            self.reader.read_bits(self.time_increment_bits)?;
            if self.reader.read_bit()? != 1 {
                return Err(MediaError::MalformedMpeg4(
                    "missing marker in video packet header".to_string(),
                ));
            }
            self.reader.read_bits(2)?; // vop_coding_type
            self.reader.read_bits(3)?; // intra_dc_vlc_thr (repeat)
            self.reader.read_bits(3)?; // vop_fcode_forward (repeat)
        }
        Ok(BitSpan {
            start,
            len: self.reader.pos - start,
        })
    }
    fn parse_block(
        &mut self,
        intra: bool,
        coded: bool,
        use_dc_vlc: bool,
        chroma: bool,
        mut collect: Option<&mut Vec<TcoefEvent>>,
    ) -> Result<(), MediaError> {
        let t = tables();
        if intra && use_dc_vlc {
            let dc_table = if chroma { &t.dc_chrom } else { &t.dc_lum };
            let size = dc_table.decode(&mut self.reader, "intra DC size")?;
            if size > 0 {
                self.reader.read_bits(size as u32)?;
                if size > 8 && self.reader.read_bit()? != 1 {
                    return Err(MediaError::MalformedMpeg4(format!(
                        "missing intra DC marker bit at bit {}",
                        self.reader.pos
                    )));
                }
            }
        }
        if !coded {
            return Ok(());
        }
        let (table, last_split) = if intra {
            (&t.tcoef_intra, INTRA_TCOEF_LAST_SPLIT)
        } else {
            (&t.tcoef_inter, INTER_TCOEF_LAST_SPLIT)
        };
        debug_assert!(collect.is_none() || !intra, "events are inter-only");
        loop {
            // Decode one (last, run, level) event; `collect` (inter blocks in
            // DCT-edit mode) records the semantic values.
            let (last, run, level) = {
                let symbol = table.decode(&mut self.reader, "TCOEF")?;
                if symbol != TCOEF_ESCAPE {
                    let sign = self.reader.read_bit()?;
                    let last = symbol >= last_split;
                    let (run, level) = if collect.is_some() {
                        let level = INTER_TCOEF_LEVEL[symbol as usize] as i16;
                        (
                            INTER_TCOEF_RUN[symbol as usize],
                            if sign == 1 { -level } else { level },
                        )
                    } else {
                        (0, 1)
                    };
                    (last, run, level)
                } else if self.reader.read_bit()? == 0 {
                    // Escape 1: level offset by LMAX of the inner code.
                    let inner = table.decode(&mut self.reader, "TCOEF (escape 1)")?;
                    if inner == TCOEF_ESCAPE {
                        return Err(MediaError::MalformedMpeg4(format!(
                            "double TCOEF escape at bit {}",
                            self.reader.pos
                        )));
                    }
                    let sign = self.reader.read_bit()?;
                    let last = inner >= last_split;
                    let (run, level) = if collect.is_some() {
                        let run = INTER_TCOEF_RUN[inner as usize];
                        let level = INTER_TCOEF_LEVEL[inner as usize] as i16
                            + t.inter_rl.lmax[usize::from(last)][run as usize] as i16;
                        (run, if sign == 1 { -level } else { level })
                    } else {
                        (0, 1)
                    };
                    (last, run, level)
                } else if self.reader.read_bit()? == 0 {
                    // Escape 2: run offset by RMAX of the inner code, plus one.
                    let inner = table.decode(&mut self.reader, "TCOEF (escape 2)")?;
                    if inner == TCOEF_ESCAPE {
                        return Err(MediaError::MalformedMpeg4(format!(
                            "double TCOEF escape at bit {}",
                            self.reader.pos
                        )));
                    }
                    let sign = self.reader.read_bit()?;
                    let last = inner >= last_split;
                    let (run, level) = if collect.is_some() {
                        let level = INTER_TCOEF_LEVEL[inner as usize];
                        let run = INTER_TCOEF_RUN[inner as usize]
                            + t.inter_rl.rmax[usize::from(last)][level as usize]
                            + 1;
                        let level = level as i16;
                        (run, if sign == 1 { -level } else { level })
                    } else {
                        (0, 1)
                    };
                    (last, run, level)
                } else {
                    // Escape 3: fixed-length last/run/level.
                    let last = self.reader.read_bit()? == 1;
                    let run = self.reader.read_bits(6)? as u8;
                    if self.reader.read_bit()? != 1 {
                        return Err(MediaError::MalformedMpeg4(format!(
                            "missing marker before escape-3 level at bit {}",
                            self.reader.pos
                        )));
                    }
                    let level = sign_extend(self.reader.read_bits(12)? as i32, 12) as i16;
                    if self.reader.read_bit()? != 1 {
                        return Err(MediaError::MalformedMpeg4(format!(
                            "missing marker after escape-3 level at bit {}",
                            self.reader.pos
                        )));
                    }
                    if level == 0 && collect.is_some() {
                        return Err(MediaError::MalformedMpeg4(format!(
                            "zero escape-3 level at bit {}",
                            self.reader.pos
                        )));
                    }
                    (last, run, level)
                }
            };
            if let Some(events) = collect.as_deref_mut() {
                events.push(TcoefEvent { run, level });
            }
            if last {
                return Ok(());
            }
        }
    }

    fn apply_dquant(&mut self, code: u32) {
        const QUANT_TAB: [i32; 4] = [-1, -2, 1, 2];
        self.qscale = (self.qscale + QUANT_TAB[code as usize]).clamp(1, self.quant_max);
    }

    fn parse_mb(
        &mut self,
        grid: &mut MvGrid,
        mb_x: usize,
        mb_y: usize,
    ) -> Result<MbKind, MediaError> {
        let t = tables();
        let start = self.reader.pos;

        // The reference decoder's skip/stuffing loop: each iteration reads a
        // not_coded bit, then (if coded) an MCBPC that may be stuffing.
        let mcbpc = loop {
            if self.reader.read_bit()? == 1 {
                // Skipped macroblock: MV (0,0), grid entries stay zero.
                return Ok(MbKind::Copied(BitSpan {
                    start,
                    len: self.reader.pos - start,
                }));
            }
            let symbol = t.inter_mcbpc.decode(&mut self.reader, "P-VOP MCBPC")?;
            if symbol != MCBPC_STUFFING {
                break symbol;
            }
        };

        let dquant = mcbpc & 8 != 0;
        let intra = mcbpc & 4 != 0;
        let four_mv = mcbpc & 16 != 0;
        let cbpc = (mcbpc & 3) as u32;

        if intra {
            // ac_pred, CBPY (uninverted), dquant, then six intra blocks.
            self.reader.read_bit()?;
            let cbpy = t.cbpy.decode(&mut self.reader, "intra CBPY")? as u32;
            let cbp = cbpc | (cbpy << 2);
            let use_dc_vlc = self.qscale < self.dc_threshold as i32;
            if dquant {
                let code = self.reader.read_bits(2)?;
                self.apply_dquant(code);
            }
            for block in 0..6 {
                self.parse_block(true, cbp & (32 >> block) != 0, use_dc_vlc, block >= 4, None)?;
            }
            return Ok(MbKind::Copied(BitSpan {
                start,
                len: self.reader.pos - start,
            }));
        }

        let cbpy = (t.cbpy.decode(&mut self.reader, "inter CBPY")? as u32) ^ 0xf;
        let cbp = cbpc | (cbpy << 2);
        if dquant {
            let code = self.reader.read_bits(2)?;
            self.apply_dquant(code);
        }
        let head = BitSpan {
            start,
            len: self.reader.pos - start,
        };

        let mut mvs = [[0i32; 2]; 4];
        if four_mv {
            for (block, mv) in mvs.iter_mut().enumerate() {
                let (px, py) = grid.pred(mb_x, mb_y, block, &self.state);
                let mx = decode_motion(&mut self.reader, px, self.fcode)?;
                let my = decode_motion(&mut self.reader, py, self.fcode)?;
                *mv = [mx, my];
                grid.set_block(mb_x, mb_y, block, (mx, my));
            }
        } else {
            let (px, py) = grid.pred(mb_x, mb_y, 0, &self.state);
            let mx = decode_motion(&mut self.reader, px, self.fcode)?;
            let my = decode_motion(&mut self.reader, py, self.fcode)?;
            mvs[0] = [mx, my];
            grid.set_mb(mb_x, mb_y, (mx, my));
        }

        let body_start = self.reader.pos;
        let mut blocks = Vec::new();
        for block in 0..6 {
            let coded = cbp & (32 >> block) != 0;
            if self.collect_dct {
                let block_start = self.reader.pos;
                let mut events = Vec::new();
                self.parse_block(false, coded, false, block >= 4, Some(&mut events))?;
                blocks.push(InterBlock {
                    span: BitSpan {
                        start: block_start,
                        len: self.reader.pos - block_start,
                    },
                    events: coded.then_some(events),
                    dirty: false,
                });
            } else {
                self.parse_block(false, coded, false, block >= 4, None)?;
            }
        }

        Ok(MbKind::Inter {
            head,
            body: BitSpan {
                start: body_start,
                len: self.reader.pos - body_start,
            },
            four_mv,
            mvs,
            blocks,
        })
    }
}

/// Parse an editable P-VOP out of one video chunk. Returns `Ok(None)` for
/// chunks that are not coded P-VOPs (I-VOPs, `vop_coded == 0`, no VOP start
/// code) — those are passed through verbatim by the editor.
fn parse_pvop(
    chunk: &[u8],
    cfg: &VolConfig,
    collect_dct: bool,
) -> Result<Option<PvopParse>, MediaError> {
    let Some(vop_pos) = find_start_code(chunk, 0, |code| code == 0xb6) else {
        return Ok(None);
    };
    let mut r = BitReader::at(chunk, (vop_pos + 4) * 8);

    if r.read_bits(2)? != 1 {
        return Ok(None); // not a P-VOP
    }
    let mut modulo_guard = 0;
    while r.read_bit()? == 1 {
        modulo_guard += 1;
        if modulo_guard > 32 {
            return Err(MediaError::MalformedMpeg4(
                "runaway modulo_time_base in VOP header".to_string(),
            ));
        }
    }
    if r.read_bit()? != 1 {
        return Err(MediaError::MalformedMpeg4(
            "missing marker before vop_time_increment".to_string(),
        ));
    }
    r.read_bits(cfg.time_increment_bits)?;
    if r.read_bit()? != 1 {
        return Err(MediaError::MalformedMpeg4(
            "missing marker after vop_time_increment".to_string(),
        ));
    }
    if r.read_bit()? == 0 {
        return Ok(None); // vop_coded == 0: header-only frame
    }
    r.read_bit()?; // vop_rounding_type
    let dc_threshold = DC_VLC_THRESHOLD[r.read_bits(3)? as usize];
    let qscale = r.read_bits(cfg.quant_precision)? as i32;
    if qscale == 0 {
        return Err(MediaError::MalformedMpeg4("VOP quantizer is zero".to_string()));
    }
    let fcode = r.read_bits(3)? as u8;
    if fcode == 0 {
        return Err(MediaError::MalformedMpeg4("VOP fcode is zero".to_string()));
    }

    let header = BitSpan {
        start: 0,
        len: r.pos,
    };

    let mb_w = cfg.mb_width();
    let mb_h = cfg.mb_height();
    let mut grid = MvGrid::new(mb_w, mb_h);
    let mut parser = PvopParser {
        reader: r,
        fcode,
        qscale,
        dc_threshold,
        quant_max: (1i32 << cfg.quant_precision) - 1,
        quant_precision: cfg.quant_precision,
        time_increment_bits: cfg.time_increment_bits,
        mb_total: mb_w * mb_h,
        state: SliceState::frame_start(),
        collect_dct,
    };

    let mut mbs = Vec::with_capacity(mb_w * mb_h);
    for mb_index in 0..mb_w * mb_h {
        let mb_x = mb_index % mb_w;
        let mb_y = mb_index / mb_w;
        let packet = if mb_index > 0 && parser.peek_packet() {
            let span = parser.parse_packet(mb_index).map_err(|err| {
                MediaError::MalformedMpeg4(format!("video packet before MB ({mb_x},{mb_y}): {err}"))
            })?;
            parser.state.on_packet(mb_x, mb_y);
            Some(span)
        } else {
            None
        };
        parser.state.at_mb_start(mb_x, mb_y);
        let kind = parser.parse_mb(&mut grid, mb_x, mb_y).map_err(|err| {
            MediaError::MalformedMpeg4(format!("macroblock ({mb_x},{mb_y}): {err}"))
        })?;
        mbs.push(MbRecord { packet, kind });
    }

    let tail_start = parser.reader.pos;
    Ok(Some(PvopParse {
        header,
        tail: BitSpan {
            start: tail_start,
            len: chunk.len() * 8 - tail_start,
        },
        fcode,
        mbs,
    }))
}

// ---------------------------------------------------------------------------
// Re-emission
// ---------------------------------------------------------------------------

fn emit_pvop(
    chunk: &[u8],
    parse: &PvopParse,
    cfg: &VolConfig,
) -> Result<Vec<u8>, MediaError> {
    let mut w = BitWriter::default();
    w.copy_span(chunk, parse.header)?;

    let mb_w = cfg.mb_width();
    let mut grid = MvGrid::new(mb_w, cfg.mb_height());
    let (lo, hi) = mv_window(parse.fcode);
    let mut state = SliceState::frame_start();

    for (mb_index, mb) in parse.mbs.iter().enumerate() {
        let mb_x = mb_index % mb_w;
        let mb_y = mb_index / mb_w;
        if let Some(packet) = &mb.packet {
            // Regenerated stuffing re-aligns the copied (byte-aligned) header
            // even when earlier MV edits shifted the bit position.
            w.put_vop_stuffing();
            w.copy_span(chunk, *packet)?;
            state.on_packet(mb_x, mb_y);
        }
        state.at_mb_start(mb_x, mb_y);
        match &mb.kind {
            MbKind::Copied(span) => w.copy_span(chunk, *span)?,
            MbKind::Inter {
                head,
                body,
                four_mv,
                mvs,
                blocks,
            } => {
                w.copy_span(chunk, *head)?;
                if *four_mv {
                    for (block, mv) in mvs.iter().enumerate() {
                        let (px, py) = grid.pred(mb_x, mb_y, block, &state);
                        let mx = mv[0].clamp(lo, hi);
                        let my = mv[1].clamp(lo, hi);
                        encode_motion(&mut w, mx - px, parse.fcode);
                        encode_motion(&mut w, my - py, parse.fcode);
                        grid.set_block(mb_x, mb_y, block, (mx, my));
                    }
                } else {
                    let (px, py) = grid.pred(mb_x, mb_y, 0, &state);
                    let mx = mvs[0][0].clamp(lo, hi);
                    let my = mvs[0][1].clamp(lo, hi);
                    encode_motion(&mut w, mx - px, parse.fcode);
                    encode_motion(&mut w, my - py, parse.fcode);
                    grid.set_mb(mb_x, mb_y, (mx, my));
                }
                if blocks.is_empty() {
                    w.copy_span(chunk, *body)?;
                } else {
                    let rl = &tables().inter_rl;
                    for block in blocks {
                        match (&block.events, block.dirty) {
                            (Some(events), true) => {
                                for (index, event) in events.iter().enumerate() {
                                    rl.encode_event(
                                        &mut w,
                                        index + 1 == events.len(),
                                        event.run,
                                        event.level,
                                    );
                                }
                            }
                            _ => w.copy_span(chunk, block.span)?,
                        }
                    }
                }
            }
        }
    }

    if w.bit_len == parse.tail.start {
        // Bit length unchanged (e.g. no edits, or edits that re-encode to the
        // same widths): the original stuffing and trailing bytes still align.
        w.copy_span(chunk, parse.tail)?;
    } else {
        w.put_vop_stuffing();
    }
    Ok(w.into_bytes())
}

// ---------------------------------------------------------------------------
// Public editing API
// ---------------------------------------------------------------------------

/// A motion-vector edit applied to every coded inter macroblock of every P-VOP.
/// All units are half-pels (a pan of 2 shifts one full pixel per frame).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MvOperation {
    /// Set every motion vector to zero: motion freezes, residuals keep painting.
    Zero,
    /// Add a constant offset — the classic ffglitch pan/drift.
    Pan { dx: i32, dy: i32 },
    /// Multiply every vector (amplify > 1, dampen < 1, invert < 0).
    Scale { factor: f64 },
    /// Replace every vector with the running average of all original vectors
    /// seen so far — ffglitch's "average motion" melt.
    Sink,
    /// Position-dependent sinusoidal warp across the macroblock grid.
    /// `amp` is in half-pels, `period` in macroblocks (also the temporal period
    /// in P-frames).
    Sine { amp: f64, period: f64 },
}

/// Counters reported back to the CLI (and its sidecar) after an edit pass.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MvEditStats {
    /// Coded P-VOPs seen (skipped/intra-only frames still count).
    pub p_frames: usize,
    /// P-VOPs where at least one vector changed.
    pub edited_frames: usize,
    /// Total motion vectors visited (1 per 1MV macroblock, 4 per 4MV).
    pub visited_mvs: usize,
    /// Vectors whose value actually changed.
    pub changed_mvs: usize,
    /// Edited vectors clamped to the fcode window.
    pub clamped_mvs: usize,
}

fn round_half_away(value: f64) -> i32 {
    if value >= 0.0 {
        (value + 0.5).floor() as i32
    } else {
        (value - 0.5).ceil() as i32
    }
}

struct SinkState {
    sum_x: f64,
    sum_y: f64,
    count: u64,
}

fn apply_operation(
    op: &MvOperation,
    mv: [i32; 2],
    mb_x: usize,
    mb_y: usize,
    frame: usize,
    sink: &SinkState,
) -> [i32; 2] {
    match op {
        MvOperation::Zero => [0, 0],
        MvOperation::Pan { dx, dy } => [mv[0] + dx, mv[1] + dy],
        MvOperation::Scale { factor } => [
            round_half_away(mv[0] as f64 * factor),
            round_half_away(mv[1] as f64 * factor),
        ],
        MvOperation::Sink => {
            if sink.count == 0 {
                mv
            } else {
                [
                    round_half_away(sink.sum_x / sink.count as f64),
                    round_half_away(sink.sum_y / sink.count as f64),
                ]
            }
        }
        MvOperation::Sine { amp, period } => {
            let period = period.max(1.0);
            let tau = std::f64::consts::TAU;
            let phase = tau * frame as f64 / period;
            let dx = amp * (tau * mb_y as f64 / period + phase).sin();
            let dy = amp * (tau * mb_x as f64 / period + phase).sin();
            [mv[0] + round_half_away(dx), mv[1] + round_half_away(dy)]
        }
    }
}

/// Apply `op` to every P-VOP's motion vectors in an FFmpeg-encoded MPEG-4 AVI
/// (the datamosh-bitstream substrate). Returns the rebuilt AVI and edit stats.
/// If no vector changes (e.g. `Pan { 0, 0 }`), the input is returned verbatim —
/// the exact off case.
pub fn remix_motion_vectors(
    avi_bytes: &[u8],
    op: &MvOperation,
) -> Result<(Vec<u8>, MvEditStats), MediaError> {
    let payloads = avi::video_chunk_payloads(avi_bytes)?;
    let first = payloads
        .first()
        .ok_or_else(|| MediaError::MalformedAvi("AVI contains no video chunks".to_string()))?;
    let cfg = parse_vol_config(first)?;

    let mut stats = MvEditStats::default();
    let mut sink = SinkState {
        sum_x: 0.0,
        sum_y: 0.0,
        count: 0,
    };
    let mut p_frame_ordinal = 0usize;

    let edited = avi::edit_video_chunks(avi_bytes, |_, keyframe, payload| {
        if keyframe {
            return Ok(None);
        }
        let Some(mut parse) = parse_pvop(payload, &cfg, false)? else {
            return Ok(None);
        };
        let frame = p_frame_ordinal;
        p_frame_ordinal += 1;
        stats.p_frames += 1;

        // Sink accumulates the frame's *original* vectors before any are
        // replaced, so the melt direction is the clip's own cumulative motion.
        if *op == MvOperation::Sink {
            for mb in &parse.mbs {
                if let MbKind::Inter { four_mv, mvs, .. } = &mb.kind {
                    let count = if *four_mv { 4 } else { 1 };
                    for mv in mvs.iter().take(count) {
                        sink.sum_x += mv[0] as f64;
                        sink.sum_y += mv[1] as f64;
                        sink.count += 1;
                    }
                }
            }
        }

        let (lo, hi) = mv_window(parse.fcode);
        let mb_w = cfg.mb_width();
        let mut frame_changed = false;
        for (mb_index, mb) in parse.mbs.iter_mut().enumerate() {
            let mb_x = mb_index % mb_w;
            let mb_y = mb_index / mb_w;
            if let MbKind::Inter { four_mv, mvs, .. } = &mut mb.kind {
                let count = if *four_mv { 4 } else { 1 };
                for mv in mvs.iter_mut().take(count) {
                    stats.visited_mvs += 1;
                    let target = apply_operation(op, *mv, mb_x, mb_y, frame, &sink);
                    let clamped = [target[0].clamp(lo, hi), target[1].clamp(lo, hi)];
                    if clamped != target {
                        stats.clamped_mvs += 1;
                    }
                    if clamped != *mv {
                        stats.changed_mvs += 1;
                        frame_changed = true;
                        *mv = clamped;
                    }
                }
            }
        }

        if !frame_changed {
            return Ok(None);
        }
        stats.edited_frames += 1;
        Ok(Some(emit_pvop(payload, &parse, &cfg)?))
    })?;

    if stats.changed_mvs == 0 {
        return Ok((avi_bytes.to_vec(), stats));
    }
    Ok((edited, stats))
}

/// A DCT-coefficient edit applied to every coded inter block of every P-VOP
/// (intra macroblocks are left untouched; their DC path is coded differently
/// and they are rare inside P-frames). CBP is frozen in the copied MB header,
/// so a block never becomes empty — edits that would zero everything keep the
/// final coefficient at magnitude 1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DctOperation {
    /// Multiply every quantized level (clamped to ±2047). `1.0` is the exact
    /// off case; large factors overdrive into the classic rainbow ringing.
    Amp { factor: f64 },
    /// Keep only the first `keep` coefficient events per block (blocky
    /// mosaic/blur). `keep >= 64` is the off case.
    LoPass { keep: u32 },
    /// Zero the first `drop` events per block, preserving the positions of the
    /// rest (edge ghosts). `0` is the off case; the final event always
    /// survives.
    HiPass { drop: u32 },
    /// Add deterministic pseudo-random noise in `[-amount, amount]` to every
    /// level (hash of frame/MB/block/coefficient — reproducible surgery).
    /// `0` is the off case.
    Noise { amount: u32 },
}

/// Counters reported for a DCT edit pass.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DctEditStats {
    pub p_frames: usize,
    pub edited_frames: usize,
    /// Coded inter blocks visited.
    pub visited_blocks: usize,
    /// Blocks whose event list changed.
    pub changed_blocks: usize,
    /// Coefficient events changed, dropped, or truncated.
    pub changed_coeffs: usize,
    /// Levels clamped to the ±2047 escape-3 range.
    pub clamped_levels: usize,
}

/// splitmix64 finalizer over the coefficient's coordinates — deterministic
/// noise without any global RNG state.
fn dct_hash(frame: u64, mb: u64, block: u64, index: u64) -> u64 {
    let mut x = frame
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (mb << 32)
        ^ (block << 16)
        ^ index;
    x ^= x >> 30;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 31;
    x
}

/// Replace each event's level with `new_levels[i]`, dropping zeroed events by
/// merging their zero-runs into the next survivor. Never leaves the block
/// empty (CBP is frozen). Returns the number of changed/dropped events.
fn rebuild_block_events(events: &mut Vec<TcoefEvent>, new_levels: &[i16]) -> usize {
    let mut changed = 0usize;
    let mut out: Vec<TcoefEvent> = Vec::with_capacity(events.len());
    let mut carry: u16 = 0;
    for (event, &new_level) in events.iter().zip(new_levels) {
        if new_level == 0 {
            changed += 1;
            carry += event.run as u16 + 1;
            continue;
        }
        if new_level != event.level || carry != 0 {
            changed += 1;
        }
        out.push(TcoefEvent {
            // Positions within an 8x8 block bound the merged run to <= 63.
            run: (event.run as u16 + carry) as u8,
            level: new_level,
        });
        carry = 0;
    }
    if out.is_empty() {
        let last = events.last().copied().unwrap_or(TcoefEvent { run: 0, level: 1 });
        out.push(TcoefEvent {
            run: last.run,
            level: if last.level < 0 { -1 } else { 1 },
        });
    }
    *events = out;
    changed
}

/// Apply the operation to one block's events; returns `(changed, clamped)`.
fn apply_dct_operation(
    op: &DctOperation,
    events: &mut Vec<TcoefEvent>,
    frame: usize,
    mb_index: usize,
    block_index: usize,
    clamped: &mut usize,
) -> usize {
    const LEVEL_LIMIT: i32 = 2047;
    match op {
        DctOperation::Amp { factor } => {
            let new_levels: Vec<i16> = events
                .iter()
                .map(|event| {
                    let target = round_half_away(event.level as f64 * factor);
                    let limited = target.clamp(-LEVEL_LIMIT, LEVEL_LIMIT);
                    if limited != target {
                        *clamped += 1;
                    }
                    limited as i16
                })
                .collect();
            rebuild_block_events(events, &new_levels)
        }
        DctOperation::LoPass { keep } => {
            let keep = (*keep).max(1) as usize;
            if events.len() > keep {
                let dropped = events.len() - keep;
                events.truncate(keep);
                dropped
            } else {
                0
            }
        }
        DctOperation::HiPass { drop } => {
            let drop = (*drop as usize).min(events.len() - 1);
            if drop == 0 {
                return 0;
            }
            let removed: u16 = events[..drop]
                .iter()
                .map(|event| event.run as u16 + 1)
                .sum();
            events.drain(..drop);
            events[0].run = (events[0].run as u16 + removed) as u8;
            drop
        }
        DctOperation::Noise { amount } => {
            if *amount == 0 {
                return 0;
            }
            let span = 2 * *amount as u64 + 1;
            let new_levels: Vec<i16> = events
                .iter()
                .enumerate()
                .map(|(index, event)| {
                    let delta = (dct_hash(
                        frame as u64,
                        mb_index as u64,
                        block_index as u64,
                        index as u64,
                    ) % span) as i32
                        - *amount as i32;
                    let target = event.level as i32 + delta;
                    let limited = target.clamp(-LEVEL_LIMIT, LEVEL_LIMIT);
                    if limited != target {
                        *clamped += 1;
                    }
                    limited as i16
                })
                .collect();
            rebuild_block_events(events, &new_levels)
        }
    }
}

/// Apply `op` to every P-VOP's inter-block DCT coefficients in an
/// FFmpeg-encoded MPEG-4 AVI. Returns the rebuilt AVI and edit stats; identity
/// parameters return the input verbatim (the exact off case).
pub fn remix_dct_coefficients(
    avi_bytes: &[u8],
    op: &DctOperation,
) -> Result<(Vec<u8>, DctEditStats), MediaError> {
    let payloads = avi::video_chunk_payloads(avi_bytes)?;
    let first = payloads
        .first()
        .ok_or_else(|| MediaError::MalformedAvi("AVI contains no video chunks".to_string()))?;
    let cfg = parse_vol_config(first)?;

    let mut stats = DctEditStats::default();
    let mut p_frame_ordinal = 0usize;

    let edited = avi::edit_video_chunks(avi_bytes, |_, keyframe, payload| {
        if keyframe {
            return Ok(None);
        }
        let Some(mut parse) = parse_pvop(payload, &cfg, true)? else {
            return Ok(None);
        };
        let frame = p_frame_ordinal;
        p_frame_ordinal += 1;
        stats.p_frames += 1;

        let mut frame_changed = false;
        for (mb_index, mb) in parse.mbs.iter_mut().enumerate() {
            if let MbKind::Inter { blocks, .. } = &mut mb.kind {
                for (block_index, block) in blocks.iter_mut().enumerate() {
                    let Some(events) = &mut block.events else {
                        continue;
                    };
                    stats.visited_blocks += 1;
                    let changed = apply_dct_operation(
                        op,
                        events,
                        frame,
                        mb_index,
                        block_index,
                        &mut stats.clamped_levels,
                    );
                    if changed > 0 {
                        stats.changed_blocks += 1;
                        stats.changed_coeffs += changed;
                        block.dirty = true;
                        frame_changed = true;
                    }
                }
            }
        }

        if !frame_changed {
            return Ok(None);
        }
        stats.edited_frames += 1;
        Ok(Some(emit_pvop(payload, &parse, &cfg)?))
    })?;

    if stats.changed_blocks == 0 {
        return Ok((avi_bytes.to_vec(), stats));
    }
    Ok((edited, stats))
}

/// Prove the parser on a real file: parse and re-emit every P-VOP with no edits
/// and require the result to be byte-identical to the original chunk. Returns
/// the number of P-VOPs verified. This is the acceptance gate for the VLC
/// tables and syntax coverage.
pub fn verify_roundtrip(avi_bytes: &[u8]) -> Result<usize, MediaError> {
    verify_roundtrip_inner(avi_bytes, false)
}

/// As [`verify_roundtrip`], but decode every inter block's DCT coefficients to
/// events and force them through the canonical re-encoder instead of the span
/// copy — the acceptance gate proving the TCOEF event codec (tables, escape
/// selection, tie-breaking) matches the reference encoder bit-for-bit.
pub fn verify_roundtrip_dct(avi_bytes: &[u8]) -> Result<usize, MediaError> {
    verify_roundtrip_inner(avi_bytes, true)
}

fn verify_roundtrip_inner(avi_bytes: &[u8], force_dct: bool) -> Result<usize, MediaError> {
    let payloads = avi::video_chunk_payloads(avi_bytes)?;
    let first = payloads
        .first()
        .ok_or_else(|| MediaError::MalformedAvi("AVI contains no video chunks".to_string()))?;
    let cfg = parse_vol_config(first)?;

    let mut verified = 0usize;
    for (index, payload) in payloads.iter().enumerate() {
        let Some(mut parse) = parse_pvop(payload, &cfg, force_dct)? else {
            continue;
        };
        if force_dct {
            for mb in &mut parse.mbs {
                if let MbKind::Inter { blocks, .. } = &mut mb.kind {
                    for block in blocks {
                        if block.events.is_some() {
                            block.dirty = true;
                        }
                    }
                }
            }
        }
        let reemitted = emit_pvop(payload, &parse, &cfg)?;
        if reemitted.as_slice() != *payload {
            let diff_at = reemitted
                .iter()
                .zip(payload.iter())
                .position(|(a, b)| a != b)
                .unwrap_or_else(|| reemitted.len().min(payload.len()));
            return Err(MediaError::MalformedMpeg4(format!(
                "round-trip mismatch in video chunk {index}: re-emitted {} bytes vs original {}, \
                 first difference at byte {diff_at}",
                reemitted.len(),
                payload.len()
            )));
        }
        verified += 1;
    }
    Ok(verified)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn median3_matches_reference_mid_pred() {
        for a in [-5, 0, 3, 7] {
            for b in [-9, -5, 0, 2, 7] {
                for c in [-5, 1, 4, 7] {
                    let mut sorted = [a, b, c];
                    sorted.sort_unstable();
                    assert_eq!(median3(a, b, c), sorted[1], "median3({a},{b},{c})");
                }
            }
        }
    }

    #[test]
    fn motion_encode_decode_round_trips_across_fcodes() {
        for fcode in 1u8..=7 {
            let (lo, hi) = mv_window(fcode);
            let preds = [0, -7, 13, lo, hi];
            for &pred in &preds {
                for target in [lo, lo + 1, -33, -1, 0, 1, 17, hi - 1, hi] {
                    let target = target.clamp(lo, hi);
                    let mut w = BitWriter::default();
                    encode_motion(&mut w, target - pred, fcode);
                    w.put_vop_stuffing();
                    let bytes = w.into_bytes();
                    let mut r = BitReader::at(&bytes, 0);
                    let decoded = decode_motion(&mut r, pred, fcode)
                        .expect("decode_motion on freshly encoded bits");
                    assert_eq!(
                        decoded, target,
                        "fcode {fcode}, pred {pred}, target {target}"
                    );
                }
            }
        }
    }

    #[test]
    fn vop_stuffing_matches_reference_pattern() {
        // Already aligned: a full 0111_1111 byte.
        let mut w = BitWriter::default();
        w.put_bits(8, 0xAB);
        w.put_vop_stuffing();
        assert_eq!(w.into_bytes(), vec![0xAB, 0x7F]);

        // 3 bits used: stuffing is 0 + 4 ones.
        let mut w = BitWriter::default();
        w.put_bits(3, 0b101);
        w.put_vop_stuffing();
        assert_eq!(w.into_bytes(), vec![0b1010_1111]);
    }

    #[test]
    fn copy_span_is_bit_exact_at_odd_offsets() {
        let src = [0xDE, 0xAD, 0xBE, 0xEF, 0x01];
        let mut w = BitWriter::default();
        w.put_bits(3, 0); // misalign
        w.copy_span(&src, BitSpan { start: 5, len: 29 }).expect("copy");
        let out = w.into_bytes();
        let mut a = BitReader::at(&src, 5);
        let mut b = BitReader::at(&out, 3);
        for _ in 0..29 {
            assert_eq!(a.read_bit().unwrap(), b.read_bit().unwrap());
        }
    }

    #[test]
    fn predictor_grid_matches_decoder_border_rules() {
        let mut grid = MvGrid::new(3, 2);
        let mut st = SliceState::frame_start();
        // First MB row: block 0 predicts (0,0) at mb_x 0, then left-only.
        assert_eq!(grid.pred(0, 0, 0, &st), (0, 0));
        grid.set_mb(0, 0, (6, -4));
        assert_eq!(grid.pred(1, 0, 0, &st), (6, -4));
        grid.set_mb(1, 0, (2, 2));
        // Second row: median of left (border zero), above, above-right.
        grid.set_mb(2, 0, (10, 10));
        st.at_mb_start(0, 1);
        let (px, py) = grid.pred(0, 1, 0, &st);
        // A = border (0,0); B = above = (6,-4); C = above-right = (2,2).
        assert_eq!((px, py), (median3(0, 6, 2), median3(0, -4, 2)));
    }

    /// Hand-assemble a minimal single-MB P-VOP and check parse + identity emit.
    #[test]
    fn synthetic_single_mb_pvop_round_trips() {
        let cfg = VolConfig {
            width: 16,
            height: 16,
            time_increment_bits: 1,
            quant_precision: 5,
        };

        let mut w = BitWriter::default();
        w.put_bits(24, 1); // 00 00 01
        w.put_bits(8, 0xb6); // VOP start code
        w.put_bits(2, 1); // P-VOP
        w.put_bit(0); // modulo_time_base terminator
        w.put_bit(1); // marker
        w.put_bits(1, 0); // time increment
        w.put_bit(1); // marker
        w.put_bit(1); // vop_coded
        w.put_bit(0); // rounding
        w.put_bits(3, 0); // intra_dc_vlc_thr
        w.put_bits(5, 4); // vop_quant
        w.put_bits(3, 1); // fcode 1
        // One inter MB, cbp 0: not_coded=0, MCBPC inter/cbpc0 = '1',
        // CBPY sym 15 (inter pattern 0) = '11', MV diffs (3, -2).
        w.put_bit(0);
        w.put_bits(1, 1);
        w.put_bits(2, 3);
        encode_motion(&mut w, 3, 1);
        encode_motion(&mut w, -2, 1);
        w.put_vop_stuffing();
        let chunk = w.into_bytes();

        let parse = parse_pvop(&chunk, &cfg, false)
            .expect("parse")
            .expect("is a coded P-VOP");
        assert_eq!(parse.fcode, 1);
        assert_eq!(parse.mbs.len(), 1);
        match &parse.mbs[0].kind {
            MbKind::Inter { four_mv, mvs, .. } => {
                assert!(!four_mv);
                assert_eq!(mvs[0], [3, -2]);
            }
            MbKind::Copied(_) => panic!("expected inter MB"),
        }

        let reemitted = emit_pvop(&chunk, &parse, &cfg).expect("emit");
        assert_eq!(reemitted, chunk, "identity re-emit must be byte-exact");
    }

    /// Ignored dev harness: write edited variants of /tmp/fixture-plain.avi
    /// for manual ffmpeg decode inspection (the PNG-Read look loop).
    #[test]
    #[ignore]
    fn debug_write_edited_variants() {
        let Ok(bytes) = std::fs::read("/tmp/fixture-plain.avi") else {
            eprintln!("no fixture");
            return;
        };
        for (name, op) in [
            ("pan", MvOperation::Pan { dx: 6, dy: 3 }),
            ("zero", MvOperation::Zero),
            ("scale", MvOperation::Scale { factor: -1.5 }),
            ("sink", MvOperation::Sink),
            ("sine", MvOperation::Sine { amp: 8.0, period: 5.0 }),
        ] {
            let (out, stats) = remix_motion_vectors(&bytes, &op).expect(name);
            std::fs::write(format!("/tmp/fixture-{name}.avi"), &out).expect("write");
            eprintln!("{name}: {stats:?}");
        }
    }

    /// Ignored dev harness: trace the macroblock walk of the first P-frame in
    /// a scratch fixture (bit positions, MB kinds, MVs, packet boundaries) —
    /// the tool for diagnosing future syntax-coverage gaps. Run with
    /// `--nocapture`; point `MORPHOGEN_MPEG4_DEBUG_AVI` at the fixture.
    #[test]
    #[ignore]
    fn debug_trace_first_pframe() {
        let path = std::env::var("MORPHOGEN_MPEG4_DEBUG_AVI")
            .unwrap_or_else(|_| "/tmp/fixture-plain.avi".to_string());
        let Ok(bytes) = std::fs::read(&path) else {
            eprintln!("no fixture at {path}");
            return;
        };
        let payloads = avi::video_chunk_payloads(&bytes).expect("payloads");
        let cfg = parse_vol_config(payloads[0]).expect("vol");
        eprintln!("cfg: {cfg:?} mb_grid {}x{}", cfg.mb_width(), cfg.mb_height());
        let payload = payloads[1];
        let vop_pos = find_start_code(payload, 0, |c| c == 0xb6).expect("vop");
        let mut r = BitReader::at(payload, (vop_pos + 4) * 8);
        assert_eq!(r.read_bits(2).unwrap(), 1);
        while r.read_bit().unwrap() == 1 {}
        r.read_bit().unwrap();
        r.read_bits(cfg.time_increment_bits).unwrap();
        r.read_bit().unwrap();
        assert_eq!(r.read_bit().unwrap(), 1, "vop_coded");
        r.read_bit().unwrap();
        let thr = DC_VLC_THRESHOLD[r.read_bits(3).unwrap() as usize];
        let q = r.read_bits(cfg.quant_precision).unwrap() as i32;
        let fcode = r.read_bits(3).unwrap() as u8;
        eprintln!("qscale {q} fcode {fcode} thr {thr} header_end_bit {}", r.pos);
        let mb_w = cfg.mb_width();
        let mut grid = MvGrid::new(mb_w, cfg.mb_height());
        let mut parser = PvopParser {
            reader: r,
            fcode,
            qscale: q,
            dc_threshold: thr,
            quant_max: (1i32 << cfg.quant_precision) - 1,
            quant_precision: cfg.quant_precision,
            time_increment_bits: cfg.time_increment_bits,
            mb_total: mb_w * cfg.mb_height(),
            state: SliceState::frame_start(),
            collect_dct: false,
        };
        for mb_index in 0..mb_w * cfg.mb_height() {
            let mb_x = mb_index % mb_w;
            let mb_y = mb_index / mb_w;
            if mb_index > 0 && parser.peek_packet() {
                match parser.parse_packet(mb_index) {
                    Ok(span) => {
                        parser.state.on_packet(mb_x, mb_y);
                        eprintln!("packet before ({mb_x},{mb_y}) span {}+{}", span.start, span.len);
                    }
                    Err(err) => {
                        eprintln!("packet before ({mb_x},{mb_y}) FAILED: {err}");
                        return;
                    }
                }
            }
            parser.state.at_mb_start(mb_x, mb_y);
            let start = parser.reader.pos;
            match parser.parse_mb(&mut grid, mb_x, mb_y) {
                Ok(MbKind::Copied(span)) => {
                    eprintln!("mb ({mb_x},{mb_y}) copied bits {}..{}", span.start, span.start + span.len)
                }
                Ok(MbKind::Inter { four_mv, mvs, head, body, .. }) => eprintln!(
                    "mb ({mb_x},{mb_y}) inter 4mv={four_mv} mvs={mvs:?} head {}+{} body {}+{}",
                    head.start, head.len, body.start, body.len
                ),
                Err(err) => {
                    eprintln!("mb ({mb_x},{mb_y}) FAILED at start bit {start}: {err}");
                    let mut ctx = BitReader::at(payload, start);
                    let mut s = String::new();
                    for _ in 0..64 {
                        if let Ok(b) = ctx.read_bit() {
                            s.push(if b == 1 { '1' } else { '0' });
                        }
                    }
                    eprintln!("bits from mb start: {s}");
                    return;
                }
            }
        }
        eprintln!("all MBs parsed; tail starts at bit {}", parser.reader.pos);
    }

    /// Encode a real fixture with the external ffmpeg (skipped cleanly when the
    /// binary is absent) and require every P-VOP to re-emit byte-identical —
    /// the acceptance gate for the VLC tables and syntax coverage. Variants
    /// cover plain 1MV, 4MV (`+mv4`), and RD mode decision (more intra MBs and
    /// escape codes).
    #[test]
    fn ffmpeg_fixture_round_trips_bit_exact() {
        let probe = std::process::Command::new("ffmpeg").arg("-version").output();
        if probe.is_err() {
            eprintln!("skipping: ffmpeg not on PATH");
            return;
        }
        let dir = std::env::temp_dir().join(format!("morphogen-mpeg4-rt-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp dir");

        let variants: [(&str, &[&str]); 3] = [
            ("plain.avi", &[]),
            ("mv4.avi", &["-flags", "+mv4"]),
            ("rd.avi", &["-mbd", "rd"]),
        ];
        for (name, extra) in variants {
            let out = dir.join(name);
            let mut cmd = std::process::Command::new("ffmpeg");
            cmd.args([
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=160x120:rate=24:duration=3",
                "-c:v",
                "mpeg4",
                "-bf",
                "0",
                "-g",
                "999999",
                "-sc_threshold",
                "0",
                "-an",
            ]);
            cmd.args(extra);
            cmd.arg(&out);
            let status = cmd
                .stderr(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .status()
                .expect("run ffmpeg");
            assert!(status.success(), "ffmpeg encode failed for {name}");

            let bytes = std::fs::read(&out).expect("read fixture");
            let verified = verify_roundtrip(&bytes)
                .unwrap_or_else(|err| panic!("round-trip failed for {name}: {err}"));
            assert!(
                verified >= 60,
                "{name}: expected >= 60 P-VOPs verified, got {verified}"
            );

            // Identity edit: parameters that change nothing return the input
            // verbatim (the exact off case).
            let (unchanged, stats) =
                remix_motion_vectors(&bytes, &MvOperation::Pan { dx: 0, dy: 0 })
                    .expect("identity remix");
            assert_eq!(unchanged, bytes, "{name}: pan 0/0 must be the identity");
            assert_eq!(stats.changed_mvs, 0);
            assert!(stats.visited_mvs > 0, "{name}: fixture has no coded MVs");

            // Real edit: the rewritten stream must be self-consistent — our own
            // parser round-trips it byte-exactly (predictor chains re-encode).
            let (panned, stats) = remix_motion_vectors(&bytes, &MvOperation::Pan { dx: 6, dy: 3 })
                .expect("pan remix");
            assert!(stats.changed_mvs > 0, "{name}: pan changed nothing");
            assert_ne!(panned, bytes);
            let reverified = verify_roundtrip(&panned)
                .unwrap_or_else(|err| panic!("edited {name} does not re-parse: {err}"));
            assert!(reverified >= 60);

            // DCT gate: forcing every inter block through the canonical TCOEF
            // re-encoder must still be byte-identical to the encoder's output.
            let dct_verified = verify_roundtrip_dct(&bytes)
                .unwrap_or_else(|err| panic!("DCT re-encode round-trip failed for {name}: {err}"));
            assert!(dct_verified >= 60);

            // DCT identity params return the input verbatim.
            let (unchanged, dct_stats) =
                remix_dct_coefficients(&bytes, &DctOperation::Amp { factor: 1.0 })
                    .expect("identity dct remix");
            assert_eq!(unchanged, bytes, "{name}: amp 1.0 must be the identity");
            assert_eq!(dct_stats.changed_blocks, 0);
            assert!(dct_stats.visited_blocks > 0);

            // A real DCT edit re-parses byte-exactly too.
            let (amped, dct_stats) =
                remix_dct_coefficients(&bytes, &DctOperation::Amp { factor: 3.0 })
                    .expect("amp remix");
            assert!(dct_stats.changed_coeffs > 0, "{name}: amp changed nothing");
            assert_ne!(amped, bytes);
            let reverified = verify_roundtrip(&amped)
                .unwrap_or_else(|err| panic!("DCT-edited {name} does not re-parse: {err}"));
            assert!(reverified >= 60);
        }

        // Quarter-pel is outside the supported subset and must be rejected
        // with a clear error, not misparsed.
        let qpel = dir.join("qpel.avi");
        let status = std::process::Command::new("ffmpeg")
            .args([
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=160x120:rate=24:duration=1",
                "-c:v",
                "mpeg4",
                "-bf",
                "0",
                "-g",
                "999999",
                "-sc_threshold",
                "0",
                "-flags",
                "+qpel",
                "-an",
            ])
            .arg(&qpel)
            .stderr(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .status()
            .expect("run ffmpeg");
        assert!(status.success(), "ffmpeg qpel encode failed");
        let bytes = std::fs::read(&qpel).expect("read qpel fixture");
        let err = verify_roundtrip(&bytes).expect_err("qpel must be rejected");
        assert!(
            err.to_string().contains("quarter-pel"),
            "unexpected qpel rejection message: {err}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn round_half_away_ties() {
        assert_eq!(round_half_away(1.5), 2);
        assert_eq!(round_half_away(-1.5), -2);
        assert_eq!(round_half_away(0.4), 0);
        assert_eq!(round_half_away(-0.4), 0);
    }
}
