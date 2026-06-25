//! Pure-Rust AVI (RIFF) bitstream surgery for the *experimental* real-datamosh
//! path. AVI is a RIFF container (the same byte format we already parse by hand for
//! WAV), so the P-frame "bloom" — duplicating a P-frame's compressed chunk so its
//! motion vectors re-apply on every redecode — is a structural edit we can do in
//! safe Rust without ffmpeg.
//!
//! **Invariant carve-out.** This module is the surgery half of a path that is
//! deliberately *non-deterministic*: the surrounding encode/decode is delegated to
//! the external ffmpeg, whose MPEG-4 encoder/decoder is version- and build-
//! dependent, so the end-to-end output is **not** bit-reproducible. It lives
//! outside the deterministic render graph (no parity gate, no determinism asserts)
//! by design. The surgery here, however, is fully deterministic and unit-tested.

use crate::MediaError;

const FOURCC_RIFF: &[u8; 4] = b"RIFF";
const FOURCC_AVI: &[u8; 4] = b"AVI ";
const FOURCC_LIST: &[u8; 4] = b"LIST";
const FOURCC_HDRL: &[u8; 4] = b"hdrl";
const FOURCC_MOVI: &[u8; 4] = b"movi";
const FOURCC_STRL: &[u8; 4] = b"strl";
const FOURCC_AVIH: &[u8; 4] = b"avih";
const FOURCC_STRH: &[u8; 4] = b"strh";
const FOURCC_VIDS: &[u8; 4] = b"vids";
const FOURCC_IDX1: &[u8; 4] = b"idx1";

/// Byte offset of `dwTotalFrames` within an `avih` chunk's data payload.
const AVIH_TOTAL_FRAMES_OFFSET: usize = 16;
/// Byte offset of `dwLength` within a video-stream `strh` chunk's data payload.
const STRH_LENGTH_OFFSET: usize = 32;
/// `AVIIF_KEYFRAME` — the keyframe flag in an `idx1` entry.
const AVIIF_KEYFRAME: u32 = 0x10;
/// Size of a single `idx1` index entry: ckid(4) + flags(4) + offset(4) + size(4).
const IDX1_ENTRY_LEN: usize = 16;

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, MediaError> {
    bytes
        .get(offset..offset + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
        .ok_or_else(|| {
            MediaError::MalformedAvi(format!("truncated 32-bit read at offset {offset}"))
        })
}

fn write_u32(bytes: &mut [u8], offset: usize, value: u32) -> Result<(), MediaError> {
    let slot = bytes.get_mut(offset..offset + 4).ok_or_else(|| {
        MediaError::MalformedAvi(format!("truncated 32-bit write at offset {offset}"))
    })?;
    slot.copy_from_slice(&value.to_le_bytes());
    Ok(())
}

fn fourcc_at(bytes: &[u8], offset: usize) -> Result<[u8; 4], MediaError> {
    bytes
        .get(offset..offset + 4)
        .map(|s| [s[0], s[1], s[2], s[3]])
        .ok_or_else(|| MediaError::MalformedAvi(format!("truncated FourCC at offset {offset}")))
}

/// AVI chunk payloads are padded to an even byte boundary.
fn padded_len(data_size: usize) -> usize {
    data_size + (data_size & 1)
}

/// One compressed video frame chunk inside the `movi` list.
#[derive(Debug, Clone)]
struct VideoChunk {
    /// Absolute offset of the chunk's FourCC in the source buffer.
    start: usize,
    /// Total bytes: 8-byte header + padded data.
    total_len: usize,
    /// Unpadded data size (the value stored in the chunk header and `idx1`).
    data_size: u32,
    fourcc: [u8; 4],
    /// True when flagged `AVIIF_KEYFRAME` in `idx1` (the I-frame).
    keyframe: bool,
}

/// The parts of the AVI we need to rebuild after inserting duplicate chunks.
#[derive(Debug)]
struct AviLayout {
    /// Absolute offset of the movi `LIST` FourCC (start of the movi chunk).
    movi_list_start: usize,
    /// Ordered video chunks inside `movi`.
    chunks: Vec<VideoChunk>,
    /// First `idx1` entry offset — preserved verbatim so the rebuilt index uses the
    /// encoder's own offset convention (movi-relative vs absolute), whatever it was.
    idx1_base_offset: u32,
    /// Absolute byte right after the original `idx1` chunk (start of any trailing
    /// top-level data; usually EOF for a simple AVI 1.0 file).
    suffix_start: usize,
    /// Absolute offset of `dwTotalFrames` in the `avih` payload.
    avih_total_frames_pos: usize,
    /// Absolute offset of `dwLength` in the video `strh` payload, if present.
    strh_length_pos: Option<usize>,
}

#[derive(Debug)]
struct OutChunk {
    fourcc: [u8; 4],
    data_size: u32,
    total_len: usize,
    keyframe: bool,
}

fn parse(bytes: &[u8]) -> Result<AviLayout, MediaError> {
    if fourcc_at(bytes, 0)? != *FOURCC_RIFF {
        return Err(MediaError::MalformedAvi("missing RIFF header".to_string()));
    }
    if fourcc_at(bytes, 8)? != *FOURCC_AVI {
        return Err(MediaError::MalformedAvi("not an AVI RIFF form".to_string()));
    }

    let mut movi_list_start = None;
    let mut chunks = Vec::new();
    let mut idx1_entries = Vec::new();
    let mut suffix_start = bytes.len();
    let mut avih_total_frames_pos = None;
    let mut strh_length_pos = None;

    // Walk the top-level chunks under the RIFF form (start at 12: after RIFF+size+"AVI ").
    let mut p = 12;
    while p + 8 <= bytes.len() {
        let cc = fourcc_at(bytes, p)?;
        let size = read_u32(bytes, p + 4)? as usize;
        let data = p + 8;
        if data + size > bytes.len() {
            return Err(MediaError::MalformedAvi(format!(
                "chunk at {p} runs past end of file"
            )));
        }

        if cc == *FOURCC_LIST {
            let list_type = fourcc_at(bytes, data)?;
            if list_type == *FOURCC_HDRL {
                parse_hdrl(
                    bytes,
                    data + 4,
                    data + size,
                    &mut avih_total_frames_pos,
                    &mut strh_length_pos,
                )?;
            } else if list_type == *FOURCC_MOVI {
                movi_list_start = Some(p);
                parse_movi(bytes, data + 4, data + size, &mut chunks)?;
            }
        } else if cc == *FOURCC_IDX1 {
            parse_idx1(bytes, data, size, &mut idx1_entries)?;
            suffix_start = data + padded_len(size);
        }

        p = data + padded_len(size);
    }

    let movi_list_start =
        movi_list_start.ok_or_else(|| MediaError::MalformedAvi("no movi list".to_string()))?;
    let avih_total_frames_pos = avih_total_frames_pos
        .ok_or_else(|| MediaError::MalformedAvi("no avih header".to_string()))?;

    if idx1_entries.is_empty() {
        return Err(MediaError::MalformedAvi(
            "no idx1 index (required to derive keyframe flags and offset convention)".to_string(),
        ));
    }
    if idx1_entries.len() != chunks.len() {
        return Err(MediaError::MalformedAvi(format!(
            "idx1 entry count {} does not match {} movi chunks (audio present? use -an)",
            idx1_entries.len(),
            chunks.len()
        )));
    }

    let idx1_base_offset = idx1_entries[0].1;
    for (chunk, (ckid, _offset, flags)) in chunks.iter_mut().zip(idx1_entries.iter()) {
        if *ckid != chunk.fourcc {
            return Err(MediaError::MalformedAvi(
                "idx1 entry order does not match movi chunk order".to_string(),
            ));
        }
        chunk.keyframe = (flags & AVIIF_KEYFRAME) != 0;
    }

    Ok(AviLayout {
        movi_list_start,
        chunks,
        idx1_base_offset,
        suffix_start,
        avih_total_frames_pos,
        strh_length_pos,
    })
}

fn parse_hdrl(
    bytes: &[u8],
    start: usize,
    end: usize,
    avih_total_frames_pos: &mut Option<usize>,
    strh_length_pos: &mut Option<usize>,
) -> Result<(), MediaError> {
    let mut q = start;
    while q + 8 <= end {
        let cc = fourcc_at(bytes, q)?;
        let size = read_u32(bytes, q + 4)? as usize;
        let data = q + 8;
        if cc == *FOURCC_AVIH {
            *avih_total_frames_pos = Some(data + AVIH_TOTAL_FRAMES_OFFSET);
        } else if cc == *FOURCC_LIST && fourcc_at(bytes, data)? == *FOURCC_STRL {
            parse_strl(bytes, data + 4, data + size, strh_length_pos)?;
        }
        q = data + padded_len(size);
    }
    Ok(())
}

fn parse_strl(
    bytes: &[u8],
    start: usize,
    end: usize,
    strh_length_pos: &mut Option<usize>,
) -> Result<(), MediaError> {
    let mut q = start;
    while q + 8 <= end {
        let cc = fourcc_at(bytes, q)?;
        let size = read_u32(bytes, q + 4)? as usize;
        let data = q + 8;
        if cc == *FOURCC_STRH
            && strh_length_pos.is_none()
            && fourcc_at(bytes, data)? == *FOURCC_VIDS
        {
            *strh_length_pos = Some(data + STRH_LENGTH_OFFSET);
        }
        q = data + padded_len(size);
    }
    Ok(())
}

fn parse_movi(
    bytes: &[u8],
    start: usize,
    end: usize,
    chunks: &mut Vec<VideoChunk>,
) -> Result<(), MediaError> {
    let mut q = start;
    while q + 8 <= end {
        let cc = fourcc_at(bytes, q)?;
        if cc == *FOURCC_LIST {
            return Err(MediaError::MalformedAvi(
                "OpenDML 'rec ' chunk grouping in movi is unsupported".to_string(),
            ));
        }
        let size = read_u32(bytes, q + 4)? as usize;
        let total_len = 8 + padded_len(size);
        chunks.push(VideoChunk {
            start: q,
            total_len,
            data_size: size as u32,
            fourcc: cc,
            keyframe: false,
        });
        q += total_len;
    }
    Ok(())
}

/// Returns `(ckid, offset, flags)` per entry.
fn parse_idx1(
    bytes: &[u8],
    data: usize,
    size: usize,
    entries: &mut Vec<([u8; 4], u32, u32)>,
) -> Result<(), MediaError> {
    let count = size / IDX1_ENTRY_LEN;
    for i in 0..count {
        let base = data + i * IDX1_ENTRY_LEN;
        let ckid = fourcc_at(bytes, base)?;
        let flags = read_u32(bytes, base + 4)?;
        let offset = read_u32(bytes, base + 8)?;
        entries.push((ckid, offset, flags));
    }
    Ok(())
}

/// Duplicate one P-frame's compressed chunk `count` extra times — the "bloom".
///
/// `p_frame_index` is the 0-based ordinal **among P-frames** (P-frame 0 is the
/// second video frame; the leading I-frame is never a valid target). `count == 0`
/// is the identity (returns the input verbatim — the off case). The returned buffer
/// is a valid AVI with rebuilt `movi`, `idx1`, and frame-count headers.
pub fn duplicate_p_frame(
    bytes: &[u8],
    p_frame_index: u32,
    count: u32,
) -> Result<Vec<u8>, MediaError> {
    if count == 0 {
        return Ok(bytes.to_vec());
    }

    let layout = parse(bytes)?;
    let n = layout.chunks.len();

    // Map the P-frame ordinal to a movi chunk index (skip the leading I-frame).
    let chunk_idx = (p_frame_index as usize) + 1;
    if chunk_idx >= n {
        return Err(MediaError::InvalidRequest(format!(
            "p-frame-index {p_frame_index} out of range: clip has {} P-frames",
            n.saturating_sub(1)
        )));
    }
    let target = &layout.chunks[chunk_idx];
    if target.keyframe {
        return Err(MediaError::InvalidRequest(
            "target frame is a keyframe, not a P-frame; choose a later frame".to_string(),
        ));
    }
    let dup_bytes = &bytes[target.start..target.start + target.total_len];

    // Build the new movi chunk sequence (descriptors) and the raw movi data.
    let mut out_chunks: Vec<OutChunk> = Vec::with_capacity(n + count as usize);
    let mut movi_data: Vec<u8> = Vec::new();
    for (i, chunk) in layout.chunks.iter().enumerate() {
        let raw = &bytes[chunk.start..chunk.start + chunk.total_len];
        movi_data.extend_from_slice(raw);
        out_chunks.push(OutChunk {
            fourcc: chunk.fourcc,
            data_size: chunk.data_size,
            total_len: chunk.total_len,
            keyframe: chunk.keyframe,
        });
        if i == chunk_idx {
            for _ in 0..count {
                movi_data.extend_from_slice(dup_bytes);
                out_chunks.push(OutChunk {
                    fourcc: target.fourcc,
                    data_size: target.data_size,
                    total_len: target.total_len,
                    keyframe: false,
                });
            }
        }
    }

    rebuild_avi(bytes, &layout, &out_chunks, &movi_data)
}

/// Remove the leading keyframe from the controlled P-frame-only MPEG-4 AVI
/// substrate — the "void/transition mosh" operation.
///
/// The resulting AVI intentionally starts with prediction frames and may decode
/// differently across ffmpeg builds. This is therefore only for the experimental
/// bitstream path, never the deterministic render graph.
pub fn remove_leading_keyframe(bytes: &[u8]) -> Result<Vec<u8>, MediaError> {
    let layout = parse(bytes)?;
    if layout.chunks.len() <= 1 {
        return Err(MediaError::InvalidRequest(
            "keyframe removal needs at least one P-frame after the leading keyframe".to_string(),
        ));
    }
    let first = &layout.chunks[0];
    if !first.keyframe {
        return Err(MediaError::InvalidRequest(
            "the first video chunk is not marked as a keyframe".to_string(),
        ));
    }

    let mut out_chunks: Vec<OutChunk> = Vec::with_capacity(layout.chunks.len() - 1);
    let mut movi_data: Vec<u8> = Vec::new();
    for chunk in layout.chunks.iter().skip(1) {
        let raw = &bytes[chunk.start..chunk.start + chunk.total_len];
        movi_data.extend_from_slice(raw);
        out_chunks.push(OutChunk {
            fourcc: chunk.fourcc,
            data_size: chunk.data_size,
            total_len: chunk.total_len,
            keyframe: chunk.keyframe,
        });
    }

    rebuild_avi(bytes, &layout, &out_chunks, &movi_data)
}

/// Read `(dwWidth, dwHeight)` from the `avih` main header. The macroblock grid is
/// fixed by these dimensions, so motion-transfer requires the carrier and modulator
/// to agree (otherwise the spliced P-frames address a grid that no longer exists).
pub fn avi_dimensions(bytes: &[u8]) -> Result<(u32, u32), MediaError> {
    let layout = parse(bytes)?;
    // `avih` layout: dwTotalFrames at +16, dwWidth at +32, dwHeight at +36, so the
    // dimensions sit +16 / +20 past the total-frames slot we already located.
    let width = read_u32(bytes, layout.avih_total_frames_pos + 16)?;
    let height = read_u32(bytes, layout.avih_total_frames_pos + 20)?;
    Ok((width, height))
}

/// Transfer the *modulator*'s motion onto the *carrier*'s content — the canonical
/// "motion-transfer" mosh. The output keeps the carrier's leading I-frame
/// (its appearance) and then replays the modulator's P-frames (its motion vectors +
/// residuals), so the carrier's pixels are pushed around by motion that never
/// belonged to them. Pure chunk surgery (no FFglitch): both clips are decoded by
/// the same external MPEG-4 codec, so this stays in the experimental, non-
/// deterministic carve-out.
///
/// `keep_carrier_frames` (clamped to `>= 1`) is how many leading carrier frames to
/// keep before switching to the modulator's motion: `1` keeps only the I-frame
/// (pure transfer); higher values keep some of the carrier's own motion first. The
/// carrier supplies the rebuilt headers, so the output inherits its dimensions and
/// `idx1` offset convention.
pub fn transfer_motion(
    carrier: &[u8],
    modulator: &[u8],
    keep_carrier_frames: u32,
) -> Result<Vec<u8>, MediaError> {
    let carrier_layout = parse(carrier)?;
    let modulator_layout = parse(modulator)?;

    let (cw, ch) = avi_dimensions(carrier)?;
    let (mw, mh) = avi_dimensions(modulator)?;
    if (cw, ch) != (mw, mh) {
        return Err(MediaError::InvalidRequest(format!(
            "carrier ({cw}x{ch}) and modulator ({mw}x{mh}) dimensions differ; \
             encode both at the same size before motion transfer"
        )));
    }

    if !carrier_layout
        .chunks
        .first()
        .map(|c| c.keyframe)
        .unwrap_or(false)
    {
        return Err(MediaError::InvalidRequest(
            "the carrier's first video chunk is not a keyframe seed".to_string(),
        ));
    }
    if modulator_layout.chunks.len() < 2 {
        return Err(MediaError::InvalidRequest(
            "the modulator needs at least one P-frame after its keyframe to transfer motion"
                .to_string(),
        ));
    }

    let keep = (keep_carrier_frames.max(1) as usize).min(carrier_layout.chunks.len());

    let mut out_chunks: Vec<OutChunk> =
        Vec::with_capacity(keep + modulator_layout.chunks.len() - 1);
    let mut movi_data: Vec<u8> = Vec::new();

    // Carrier seed: the I-frame (and any requested extra carrier frames).
    for chunk in carrier_layout.chunks.iter().take(keep) {
        movi_data.extend_from_slice(&carrier[chunk.start..chunk.start + chunk.total_len]);
        out_chunks.push(OutChunk {
            fourcc: chunk.fourcc,
            data_size: chunk.data_size,
            total_len: chunk.total_len,
            keyframe: chunk.keyframe,
        });
    }
    // Modulator motion: replay its P-frames, skipping its own leading I-frame.
    for chunk in modulator_layout.chunks.iter().skip(1) {
        movi_data.extend_from_slice(&modulator[chunk.start..chunk.start + chunk.total_len]);
        out_chunks.push(OutChunk {
            fourcc: chunk.fourcc,
            data_size: chunk.data_size,
            total_len: chunk.total_len,
            keyframe: false,
        });
    }

    rebuild_avi(carrier, &carrier_layout, &out_chunks, &movi_data)
}

fn rebuild_avi(
    bytes: &[u8],
    layout: &AviLayout,
    out_chunks: &[OutChunk],
    movi_data: &[u8],
) -> Result<Vec<u8>, MediaError> {
    // Rebuild idx1, preserving the source's offset convention via idx1_base_offset.
    let mut idx1 = Vec::with_capacity(out_chunks.len() * IDX1_ENTRY_LEN);
    let mut rel: u32 = 0;
    for chunk in out_chunks {
        idx1.extend_from_slice(&chunk.fourcc);
        let flags = if chunk.keyframe { AVIIF_KEYFRAME } else { 0 };
        idx1.extend_from_slice(&flags.to_le_bytes());
        idx1.extend_from_slice(&layout.idx1_base_offset.wrapping_add(rel).to_le_bytes());
        idx1.extend_from_slice(&chunk.data_size.to_le_bytes());
        rel = rel.wrapping_add(chunk.total_len as u32);
    }

    // Prefix = everything before the movi list (hdrl + any JUNK padding), with the
    // frame-count headers patched to the new total.
    let new_frame_count = out_chunks.len() as u32;
    let mut out = bytes[..layout.movi_list_start].to_vec();
    write_u32(&mut out, layout.avih_total_frames_pos, new_frame_count)?;
    if let Some(pos) = layout.strh_length_pos {
        write_u32(&mut out, pos, new_frame_count)?;
    }

    // New movi LIST: "LIST" + size + "movi" + data.
    let movi_size = (4 + movi_data.len()) as u32;
    out.extend_from_slice(FOURCC_LIST);
    out.extend_from_slice(&movi_size.to_le_bytes());
    out.extend_from_slice(FOURCC_MOVI);
    out.extend_from_slice(movi_data);

    // New idx1.
    out.extend_from_slice(FOURCC_IDX1);
    out.extend_from_slice(&(idx1.len() as u32).to_le_bytes());
    out.extend_from_slice(&idx1);

    // Any trailing top-level data after the original idx1 (usually none).
    out.extend_from_slice(&bytes[layout.suffix_start..]);

    // Patch the RIFF form size.
    let riff_size = (out.len() - 8) as u32;
    write_u32(&mut out, 4, riff_size)?;

    Ok(out)
}

/// Number of P-frames in an AVI (video frames after the leading I-frame). Useful
/// for the CLI to validate `--p-frame-index` and report the available range.
pub fn count_p_frames(bytes: &[u8]) -> Result<u32, MediaError> {
    let layout = parse(bytes)?;
    Ok(layout.chunks.len().saturating_sub(1) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn push_chunk(buf: &mut Vec<u8>, fourcc: &[u8; 4], data: &[u8]) {
        buf.extend_from_slice(fourcc);
        buf.extend_from_slice(&(data.len() as u32).to_le_bytes());
        buf.extend_from_slice(data);
        if data.len() % 2 == 1 {
            buf.push(0);
        }
    }

    fn push_list(buf: &mut Vec<u8>, list_type: &[u8; 4], content: &[u8]) {
        let size = (4 + content.len()) as u32;
        buf.extend_from_slice(FOURCC_LIST);
        buf.extend_from_slice(&size.to_le_bytes());
        buf.extend_from_slice(list_type);
        buf.extend_from_slice(content);
        if content.len() % 2 == 1 {
            buf.push(0);
        }
    }

    /// Build a minimal valid AVI: one video stream, `payloads.len()` frames
    /// (frame 0 = keyframe, rest = P-frames), with a movi-relative idx1 (base 4).
    fn synthetic_avi(payloads: &[Vec<u8>]) -> Vec<u8> {
        synthetic_avi_dims(payloads, 16, 16)
    }

    /// As `synthetic_avi`, but with explicit `avih` dwWidth/dwHeight (offsets 32/36)
    /// so the motion-transfer dimension guard can be exercised.
    fn synthetic_avi_dims(payloads: &[Vec<u8>], width: u32, height: u32) -> Vec<u8> {
        let k = payloads.len();

        let mut avih = vec![0u8; 56];
        avih[AVIH_TOTAL_FRAMES_OFFSET..AVIH_TOTAL_FRAMES_OFFSET + 4]
            .copy_from_slice(&(k as u32).to_le_bytes());
        avih[32..36].copy_from_slice(&width.to_le_bytes());
        avih[36..40].copy_from_slice(&height.to_le_bytes());

        let mut strh = vec![0u8; 56];
        strh[0..4].copy_from_slice(FOURCC_VIDS);
        strh[STRH_LENGTH_OFFSET..STRH_LENGTH_OFFSET + 4].copy_from_slice(&(k as u32).to_le_bytes());
        let strf = vec![0u8; 40];

        let mut strl_inner = Vec::new();
        push_chunk(&mut strl_inner, FOURCC_STRH, &strh);
        push_chunk(&mut strl_inner, b"strf", &strf);

        let mut hdrl_inner = Vec::new();
        push_chunk(&mut hdrl_inner, FOURCC_AVIH, &avih);
        push_list(&mut hdrl_inner, FOURCC_STRL, &strl_inner);

        let mut movi_inner = Vec::new();
        let mut rel: u32 = 0;
        let mut offsets = Vec::new();
        for payload in payloads {
            offsets.push(4 + rel); // base-4, movi-relative convention
            rel += (8 + padded_len(payload.len())) as u32;
            push_chunk(&mut movi_inner, b"00dc", payload);
        }

        let mut idx1 = Vec::new();
        for (i, payload) in payloads.iter().enumerate() {
            idx1.extend_from_slice(b"00dc");
            let flags: u32 = if i == 0 { AVIIF_KEYFRAME } else { 0 };
            idx1.extend_from_slice(&flags.to_le_bytes());
            idx1.extend_from_slice(&offsets[i].to_le_bytes());
            idx1.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        }

        let mut body = Vec::new();
        body.extend_from_slice(FOURCC_AVI);
        push_list(&mut body, FOURCC_HDRL, &hdrl_inner);
        push_list(&mut body, FOURCC_MOVI, &movi_inner);
        push_chunk(&mut body, FOURCC_IDX1, &idx1);

        let mut file = Vec::new();
        file.extend_from_slice(FOURCC_RIFF);
        file.extend_from_slice(&(body.len() as u32).to_le_bytes());
        file.extend_from_slice(&body);
        file
    }

    fn sample() -> Vec<u8> {
        // Frame 0 keyframe (odd-length, to exercise padding), then 3 P-frames.
        synthetic_avi(&[
            vec![1, 2, 3],
            vec![10, 11, 12, 13],
            vec![20, 21, 22, 23],
            vec![30, 31],
        ])
    }

    #[test]
    fn parse_finds_chunks_and_keyframe_flags() {
        let avi = sample();
        let layout = parse(&avi).expect("parse");
        assert_eq!(layout.chunks.len(), 4);
        assert!(layout.chunks[0].keyframe);
        assert!(!layout.chunks[1].keyframe);
        assert_eq!(layout.chunks[1].data_size, 4);
        assert_eq!(count_p_frames(&avi).expect("count"), 3);
    }

    #[test]
    fn duplicate_count_zero_is_identity() {
        let avi = sample();
        let out = duplicate_p_frame(&avi, 0, 0).expect("dup");
        assert_eq!(out, avi);
    }

    #[test]
    fn duplicate_inserts_copies_and_updates_headers() {
        let avi = sample();
        // Bloom P-frame ordinal 1 (the 3rd video frame, payload [20,21,22,23]) x2.
        let out = duplicate_p_frame(&avi, 1, 2).expect("dup");
        let layout = parse(&out).expect("reparse");
        assert_eq!(layout.chunks.len(), 6);

        // The duplicated chunks sit right after the original target, with its bytes.
        for idx in 2..=4 {
            assert!(!layout.chunks[idx].keyframe);
            assert_eq!(layout.chunks[idx].data_size, 4);
            let raw = &out[layout.chunks[idx].start..layout.chunks[idx].start + 4 + 8];
            assert_eq!(&raw[8..12], &[20, 21, 22, 23]);
        }
        // Frame-count headers reflect the 2 inserted frames (4 -> 6).
        assert_eq!(read_u32(&out, layout.avih_total_frames_pos).unwrap(), 6);
        assert_eq!(read_u32(&out, layout.strh_length_pos.unwrap()).unwrap(), 6);
        // idx1 offsets stay monotonically increasing and self-consistent.
        assert!(count_p_frames(&out).unwrap() == 5);
    }

    #[test]
    fn duplicate_rejects_out_of_range_index() {
        let avi = sample();
        let err = duplicate_p_frame(&avi, 9, 1).expect_err("should reject");
        assert!(matches!(err, MediaError::InvalidRequest(_)));
    }

    #[test]
    fn remove_leading_keyframe_drops_first_chunk_and_updates_headers() {
        let avi = sample();
        let out = remove_leading_keyframe(&avi).expect("remove keyframe");
        let layout = parse(&out).expect("reparse");
        assert_eq!(layout.chunks.len(), 3);
        assert!(!layout.chunks[0].keyframe);
        assert_eq!(layout.chunks[0].data_size, 4);
        let raw = &out[layout.chunks[0].start..layout.chunks[0].start + 8 + 4];
        assert_eq!(&raw[8..12], &[10, 11, 12, 13]);
        assert_eq!(read_u32(&out, layout.avih_total_frames_pos).unwrap(), 3);
        assert_eq!(read_u32(&out, layout.strh_length_pos.unwrap()).unwrap(), 3);
    }

    #[test]
    fn remove_leading_keyframe_rejects_single_frame_avi() {
        let avi = synthetic_avi(&[vec![1, 2, 3]]);
        let err = remove_leading_keyframe(&avi).expect_err("should reject");
        assert!(matches!(err, MediaError::InvalidRequest(_)));
    }

    #[test]
    fn malformed_input_errors_cleanly() {
        let err = duplicate_p_frame(b"not an avi at all", 0, 1).expect_err("should reject");
        assert!(matches!(err, MediaError::MalformedAvi(_)));
    }

    #[test]
    fn avi_dimensions_reads_avih_width_height() {
        let avi = synthetic_avi_dims(&[vec![1, 2, 3], vec![4, 5]], 128, 96);
        assert_eq!(avi_dimensions(&avi).expect("dims"), (128, 96));
    }

    #[test]
    fn transfer_motion_keeps_carrier_seed_then_modulator_pframes() {
        // Carrier: I-frame + 1 P-frame; modulator: I-frame + 3 P-frames.
        let carrier = synthetic_avi(&[vec![100, 101, 102], vec![110, 111]]);
        let modulator = synthetic_avi(&[
            vec![1, 2, 3, 4],
            vec![20, 21],
            vec![30, 31, 32],
            vec![40, 41],
        ]);
        let out = transfer_motion(&carrier, &modulator, 1).expect("transfer");
        let layout = parse(&out).expect("reparse");

        // 1 carrier seed frame + 3 modulator P-frames.
        assert_eq!(layout.chunks.len(), 4);
        // Frame 0 is the carrier's I-frame (its appearance), still a keyframe.
        assert!(layout.chunks[0].keyframe);
        let seed = &out[layout.chunks[0].start..layout.chunks[0].start + 8 + 3];
        assert_eq!(&seed[8..11], &[100, 101, 102]);
        // The rest are the modulator's P-frames (its motion), in order, never keyframes.
        let p1 = &out[layout.chunks[1].start..layout.chunks[1].start + 8 + 2];
        assert_eq!(&p1[8..10], &[20, 21]);
        assert!(layout.chunks[1..].iter().all(|c| !c.keyframe));
        // Headers reflect the spliced length.
        assert_eq!(read_u32(&out, layout.avih_total_frames_pos).unwrap(), 4);
        assert_eq!(read_u32(&out, layout.strh_length_pos.unwrap()).unwrap(), 4);
    }

    #[test]
    fn transfer_motion_keep_carrier_frames_retains_carrier_motion() {
        let carrier = synthetic_avi(&[vec![100, 101], vec![103, 104], vec![105, 106]]);
        let modulator = synthetic_avi(&[vec![1, 2], vec![20, 21], vec![30, 31]]);
        // Keep 2 carrier frames (I-frame + 1 carrier P-frame), then the modulator's.
        let out = transfer_motion(&carrier, &modulator, 2).expect("transfer");
        let layout = parse(&out).expect("reparse");
        assert_eq!(layout.chunks.len(), 4); // 2 carrier + 2 modulator P-frames
        let carrier_p = &out[layout.chunks[1].start..layout.chunks[1].start + 8 + 2];
        assert_eq!(&carrier_p[8..10], &[103, 104]); // carrier's own first P-frame retained
    }

    #[test]
    fn transfer_motion_rejects_dimension_mismatch() {
        let carrier = synthetic_avi_dims(&[vec![1, 2], vec![3, 4]], 128, 96);
        let modulator = synthetic_avi_dims(&[vec![1, 2], vec![3, 4]], 64, 64);
        let err = transfer_motion(&carrier, &modulator, 1).expect_err("should reject");
        assert!(matches!(err, MediaError::InvalidRequest(_)));
    }

    #[test]
    fn transfer_motion_rejects_modulator_without_pframes() {
        let carrier = synthetic_avi(&[vec![1, 2], vec![3, 4]]);
        let modulator = synthetic_avi(&[vec![1, 2]]); // I-frame only
        let err = transfer_motion(&carrier, &modulator, 1).expect_err("should reject");
        assert!(matches!(err, MediaError::InvalidRequest(_)));
    }
}
