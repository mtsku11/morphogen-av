//! Standard MIDI File (SMF) reader — pure Rust, minimal (~contract "~200
//! lines"), no new crate dependency. Formats 0 and 1, **PPQ division only**
//! (an SMPTE-division file is a clear error). Reads header/track chunks
//! (unknown chunks skipped), variable-length delta times, running status,
//! note-on/note-off (note-on velocity 0 IS note-off), control change, Set
//! Tempo meta, end-of-track. All other events are skipped after their length
//! is consumed. Malformed input never panics — every failure is an
//! `AudioError::Midi` (thiserror; no `unwrap()`).
//!
//! Contract: `docs/MIDI_MODULATION_MILESTONE.md`.

use std::fs;
use std::path::Path;

use crate::error::AudioError;

/// One channel/meta event on the merged, absolute-tick global timeline.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MidiEventKind {
    /// Velocity 0 note-on is folded into `NoteOff` at parse time (the classic
    /// SMF trap) so downstream extraction never special-cases it.
    NoteOn {
        channel: u8,
        key: u8,
        velocity: u8,
    },
    NoteOff {
        channel: u8,
        key: u8,
        velocity: u8,
    },
    ControlChange {
        channel: u8,
        controller: u8,
        value: u8,
    },
    SetTempo {
        micros_per_quarter: u32,
    },
    EndOfTrack,
}

/// One event at its absolute tick, tagged with the merge-order tiebreakers
/// (contract: sort by tick, ties by `(track, order)` — a total order).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MidiTimedEvent {
    pub tick: u64,
    pub track: usize,
    /// Position within the track's own event stream (the second tiebreaker).
    pub order: usize,
    pub kind: MidiEventKind,
}

/// A parsed Standard MIDI File: the PPQ division and the merged, globally
/// sorted event timeline (format 1's tempo events live on track 0 but apply
/// globally — merging by tick makes that automatic regardless of format).
#[derive(Debug, Clone)]
pub struct MidiFile {
    /// Ticks per quarter note (already validated as PPQ, not SMPTE).
    pub division: u16,
    /// Sorted by `(tick, track, order)` — deterministic regardless of chunk
    /// layout.
    pub events: Vec<MidiTimedEvent>,
}

impl MidiFile {
    /// Parse a Standard MIDI File (format 0 or 1) from raw bytes.
    pub fn parse(bytes: &[u8]) -> Result<Self, AudioError> {
        let mut cursor = 0usize;
        let (id, length, header) = read_chunk(bytes, &mut cursor)?;
        if &id != b"MThd" {
            return Err(AudioError::Midi(format!(
                "expected an 'MThd' header chunk, found {:?}",
                String::from_utf8_lossy(&id)
            )));
        }
        if length < 6 {
            return Err(AudioError::Midi(
                "truncated 'MThd' header chunk (expected at least 6 bytes)".to_string(),
            ));
        }
        let format = read_u16_be(header, 0)?;
        let ntrks = read_u16_be(header, 2)?;
        let division_raw = read_u16_be(header, 4)?;
        if format > 1 {
            return Err(AudioError::Midi(format!(
                "unsupported SMF format {format} (only format 0 and 1 are supported)"
            )));
        }
        if division_raw & 0x8000 != 0 {
            return Err(AudioError::Midi(
                "SMPTE time-code division is not supported (PPQ division only)".to_string(),
            ));
        }
        let division = division_raw;

        let mut events: Vec<MidiTimedEvent> = Vec::new();
        let mut track_index = 0usize;
        while track_index < ntrks as usize && cursor < bytes.len() {
            let (chunk_id, _chunk_len, body) = read_chunk(bytes, &mut cursor)?;
            if &chunk_id != b"MTrk" {
                // Unknown chunk where a track was expected: skip it safely and
                // keep looking for the next 'MTrk' (the contract's "unknown
                // chunks skipped" rule).
                continue;
            }
            parse_track(body, track_index, &mut events)?;
            track_index += 1;
        }
        events.sort_by_key(|event| (event.tick, event.track, event.order));

        Ok(MidiFile { division, events })
    }

    /// Read and parse a Standard MIDI File from disk.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, AudioError> {
        let bytes = fs::read(path)?;
        Self::parse(&bytes)
    }

    /// Ascending, unique-tick tempo-map segments: `(start_tick,
    /// micros_per_quarter)`. Always starts at tick 0 (the default 500,000
    /// µs/quarter unless a `Set Tempo` event sits there). Two tempo events at
    /// the same tick: the later one in merge order wins.
    pub fn tempo_segments(&self) -> Vec<(u64, u32)> {
        let mut segments: Vec<(u64, u32)> = vec![(0, 500_000)];
        for event in &self.events {
            if let MidiEventKind::SetTempo { micros_per_quarter } = event.kind {
                match segments.last_mut() {
                    Some(last) if last.0 == event.tick => last.1 = micros_per_quarter,
                    _ => segments.push((event.tick, micros_per_quarter)),
                }
            }
        }
        segments
    }

    /// The exact tempo-mapped seconds for `tick` (contract formula: piecewise
    /// f64 arithmetic over tempo segments).
    pub fn seconds_for_tick(&self, tick: u64) -> f64 {
        seconds_for_tick(&self.tempo_segments(), self.division, tick)
    }

    /// The file's total duration in seconds: the tempo-mapped time of the
    /// latest event's tick (0 for an empty file).
    pub fn duration_seconds(&self) -> f64 {
        let max_tick = self
            .events
            .iter()
            .map(|event| event.tick)
            .max()
            .unwrap_or(0);
        self.seconds_for_tick(max_tick)
    }
}

/// The contract's exact piecewise conversion: `seconds(tick) = Σ over tempo
/// segments of segment_ticks * (µs_per_quarter / division) / 1e6`.
pub fn seconds_for_tick(segments: &[(u64, u32)], division: u16, tick: u64) -> f64 {
    let mut seconds = 0.0f64;
    for (index, &(start_tick, micros_per_quarter)) in segments.iter().enumerate() {
        if start_tick >= tick {
            break;
        }
        let next_start = segments.get(index + 1).map(|s| s.0).unwrap_or(u64::MAX);
        let segment_end = next_start.min(tick);
        let segment_ticks = segment_end.saturating_sub(start_tick);
        seconds +=
            segment_ticks as f64 * (micros_per_quarter as f64 / division as f64) / 1_000_000.0;
    }
    seconds
}

/// Read one chunk (`id`, declared `length`, and its body slice) at `cursor`,
/// advancing `cursor` past the body. A declared length exceeding the
/// remaining bytes is the truncated-file error.
fn read_chunk<'a>(
    bytes: &'a [u8],
    cursor: &mut usize,
) -> Result<([u8; 4], usize, &'a [u8]), AudioError> {
    if cursor.saturating_add(8) > bytes.len() {
        return Err(AudioError::Midi(
            "truncated chunk header (expected 8-byte id + length)".to_string(),
        ));
    }
    let mut id = [0u8; 4];
    id.copy_from_slice(&bytes[*cursor..*cursor + 4]);
    let length = read_u32_be(bytes, *cursor + 4)? as usize;
    let body_start = *cursor + 8;
    let body_end = body_start.checked_add(length).ok_or_else(|| {
        AudioError::Midi(format!(
            "'{}' chunk length overflows the file",
            String::from_utf8_lossy(&id)
        ))
    })?;
    if body_end > bytes.len() {
        return Err(AudioError::Midi(format!(
            "truncated '{}' chunk: declared length {length} exceeds the remaining {} bytes",
            String::from_utf8_lossy(&id),
            bytes.len() - body_start.min(bytes.len())
        )));
    }
    *cursor = body_end;
    Ok((id, length, &bytes[body_start..body_end]))
}

fn read_u16_be(bytes: &[u8], offset: usize) -> Result<u16, AudioError> {
    bytes
        .get(offset..offset + 2)
        .map(|s| u16::from_be_bytes([s[0], s[1]]))
        .ok_or_else(|| AudioError::Midi("truncated 16-bit field".to_string()))
}

fn read_u32_be(bytes: &[u8], offset: usize) -> Result<u32, AudioError> {
    bytes
        .get(offset..offset + 4)
        .map(|s| u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
        .ok_or_else(|| AudioError::Midi("truncated 32-bit field".to_string()))
}

/// Read a variable-length quantity (VLQ), advancing `pos`. SMF VLQs never
/// exceed 4 bytes (28 bits); a 5th continuation byte is malformed input.
fn read_vlq(body: &[u8], pos: &mut usize) -> Result<u32, AudioError> {
    let mut value: u32 = 0;
    for _ in 0..4 {
        let byte = *body
            .get(*pos)
            .ok_or_else(|| AudioError::Midi("truncated variable-length quantity".to_string()))?;
        *pos += 1;
        value = (value << 7) | u32::from(byte & 0x7F);
        if byte & 0x80 == 0 {
            return Ok(value);
        }
    }
    Err(AudioError::Midi(
        "variable-length quantity exceeds the 4-byte SMF limit".to_string(),
    ))
}

fn read_data_byte(body: &[u8], pos: &mut usize, track_index: usize) -> Result<u8, AudioError> {
    let byte = *body.get(*pos).ok_or_else(|| {
        AudioError::Midi(format!("track {track_index}: truncated channel message"))
    })?;
    *pos += 1;
    Ok(byte)
}

/// Parse one `MTrk` body into absolute-tick events appended to `events`
/// (tick and in-track `order` set; global `(tick, track, order)` sort happens
/// once, after all tracks are parsed).
fn parse_track(
    body: &[u8],
    track_index: usize,
    events: &mut Vec<MidiTimedEvent>,
) -> Result<(), AudioError> {
    let mut pos = 0usize;
    let mut tick: u64 = 0;
    let mut running_status: Option<u8> = None;
    let mut order = 0usize;

    while pos < body.len() {
        let delta = read_vlq(body, &mut pos)?;
        tick += u64::from(delta);

        let mut status = *body.get(pos).ok_or_else(|| {
            AudioError::Midi(format!(
                "track {track_index}: truncated event after delta-time"
            ))
        })?;
        if status < 0x80 {
            // Running status: this byte is actually the first data byte of a
            // repeated channel message — don't consume it as a status byte.
            status = running_status.ok_or_else(|| {
                AudioError::Midi(format!(
                    "track {track_index}: running status used before any status byte"
                ))
            })?;
        } else {
            pos += 1;
        }

        match status {
            0xFF => {
                let meta_type = *body.get(pos).ok_or_else(|| {
                    AudioError::Midi(format!("track {track_index}: truncated meta event"))
                })?;
                pos += 1;
                let len = read_vlq(body, &mut pos)? as usize;
                let data_end = pos.checked_add(len).ok_or_else(|| {
                    AudioError::Midi(format!("track {track_index}: meta event length overflow"))
                })?;
                let data = body.get(pos..data_end).ok_or_else(|| {
                    AudioError::Midi(format!("track {track_index}: truncated meta event data"))
                })?;
                pos = data_end;
                match meta_type {
                    0x51 if data.len() == 3 => {
                        let micros_per_quarter = (u32::from(data[0]) << 16)
                            | (u32::from(data[1]) << 8)
                            | u32::from(data[2]);
                        events.push(MidiTimedEvent {
                            tick,
                            track: track_index,
                            order,
                            kind: MidiEventKind::SetTempo { micros_per_quarter },
                        });
                        order += 1;
                    }
                    0x2F => {
                        events.push(MidiTimedEvent {
                            tick,
                            track: track_index,
                            order,
                            kind: MidiEventKind::EndOfTrack,
                        });
                        order += 1;
                    }
                    _ => {} // other meta events skipped (length already consumed)
                }
            }
            0xF0 | 0xF7 => {
                // Sysex: variable-length data, skipped.
                let len = read_vlq(body, &mut pos)? as usize;
                let data_end = pos.checked_add(len).ok_or_else(|| {
                    AudioError::Midi(format!("track {track_index}: sysex length overflow"))
                })?;
                if data_end > body.len() {
                    return Err(AudioError::Midi(format!(
                        "track {track_index}: truncated sysex event"
                    )));
                }
                pos = data_end;
            }
            0x80..=0xEF => {
                running_status = Some(status);
                let channel = status & 0x0F;
                match status & 0xF0 {
                    0x80 => {
                        let key = read_data_byte(body, &mut pos, track_index)?;
                        let velocity = read_data_byte(body, &mut pos, track_index)?;
                        events.push(MidiTimedEvent {
                            tick,
                            track: track_index,
                            order,
                            kind: MidiEventKind::NoteOff {
                                channel,
                                key,
                                velocity,
                            },
                        });
                        order += 1;
                    }
                    0x90 => {
                        let key = read_data_byte(body, &mut pos, track_index)?;
                        let velocity = read_data_byte(body, &mut pos, track_index)?;
                        // Note-on velocity 0 IS note-off — the classic SMF trap.
                        let kind = if velocity == 0 {
                            MidiEventKind::NoteOff {
                                channel,
                                key,
                                velocity: 0,
                            }
                        } else {
                            MidiEventKind::NoteOn {
                                channel,
                                key,
                                velocity,
                            }
                        };
                        events.push(MidiTimedEvent {
                            tick,
                            track: track_index,
                            order,
                            kind,
                        });
                        order += 1;
                    }
                    0xB0 => {
                        let controller = read_data_byte(body, &mut pos, track_index)?;
                        let value = read_data_byte(body, &mut pos, track_index)?;
                        events.push(MidiTimedEvent {
                            tick,
                            track: track_index,
                            order,
                            kind: MidiEventKind::ControlChange {
                                channel,
                                controller,
                                value,
                            },
                        });
                        order += 1;
                    }
                    // Poly aftertouch (0xA0, 2 data bytes), program change
                    // (0xC0, 1), channel aftertouch (0xD0, 1), pitch bend
                    // (0xE0, 2): consumed and skipped (not a modulation source
                    // this milestone extracts).
                    0xA0 | 0xE0 => {
                        read_data_byte(body, &mut pos, track_index)?;
                        read_data_byte(body, &mut pos, track_index)?;
                    }
                    0xC0 | 0xD0 => {
                        read_data_byte(body, &mut pos, track_index)?;
                    }
                    _ => unreachable!("status & 0xF0 only takes the 8 channel-message values"),
                }
            }
            other => {
                return Err(AudioError::Midi(format!(
                    "track {track_index}: unexpected status byte 0x{other:02X}"
                )));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Encode a variable-length quantity (test-side mirror of `read_vlq`,
    /// written independently so an encoder bug can't hide a decoder bug).
    fn vlq(mut value: u32) -> Vec<u8> {
        let mut bytes = vec![(value & 0x7F) as u8];
        value >>= 7;
        while value > 0 {
            bytes.push(((value & 0x7F) as u8) | 0x80);
            value >>= 7;
        }
        bytes.reverse();
        bytes
    }

    fn chunk(id: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(id);
        out.extend_from_slice(&(body.len() as u32).to_be_bytes());
        out.extend_from_slice(body);
        out
    }

    fn header(format: u16, ntrks: u16, division: u16) -> Vec<u8> {
        let mut body = Vec::new();
        body.extend_from_slice(&format.to_be_bytes());
        body.extend_from_slice(&ntrks.to_be_bytes());
        body.extend_from_slice(&division.to_be_bytes());
        chunk(b"MThd", &body)
    }

    /// One `(delta_ticks, event_bytes)` pair flattened into a track body.
    fn track(events: &[(u32, Vec<u8>)]) -> Vec<u8> {
        let mut body = Vec::new();
        for (delta, bytes) in events {
            body.extend_from_slice(&vlq(*delta));
            body.extend_from_slice(bytes);
        }
        chunk(b"MTrk", &body)
    }

    fn set_tempo(micros_per_quarter: u32) -> Vec<u8> {
        let bytes = micros_per_quarter.to_be_bytes();
        vec![0xFF, 0x51, 0x03, bytes[1], bytes[2], bytes[3]]
    }

    fn end_of_track() -> Vec<u8> {
        vec![0xFF, 0x2F, 0x00]
    }

    fn note_on(channel: u8, key: u8, velocity: u8) -> Vec<u8> {
        vec![0x90 | channel, key, velocity]
    }

    fn note_off(channel: u8, key: u8, velocity: u8) -> Vec<u8> {
        vec![0x80 | channel, key, velocity]
    }

    fn control_change(channel: u8, controller: u8, value: u8) -> Vec<u8> {
        vec![0xB0 | channel, controller, value]
    }

    /// Format 0, single track, PPQ 480: 120 BPM (500,000 µs/quarter) for one
    /// quarter note, then a Set Tempo to 60 BPM (1,000,000 µs/quarter), then
    /// one more quarter note before a note-on that anchors the assertion.
    fn tempo_change_fixture() -> Vec<u8> {
        let division = 480u16;
        let events: Vec<(u32, Vec<u8>)> = vec![
            (0, set_tempo(500_000)),
            (480, set_tempo(1_000_000)),
            (480, note_on(0, 60, 100)),
            (0, end_of_track()),
        ];
        let mut bytes = header(0, 1, division);
        bytes.extend_from_slice(&track(&events));
        bytes
    }

    #[test]
    fn tempo_exactness_across_a_mid_file_bpm_change() {
        let midi = MidiFile::parse(&tempo_change_fixture()).expect("valid fixture parses");
        let note_on_event = midi
            .events
            .iter()
            .find(|e| matches!(e.kind, MidiEventKind::NoteOn { .. }))
            .expect("note-on event present");
        assert_eq!(note_on_event.tick, 960);

        // Hand-computed: segment 1 is ticks [0, 480) at 500,000 µs/quarter;
        // segment 2 is ticks [480, 960) at 1,000,000 µs/quarter — the exact
        // pinned formula from `docs/MIDI_MODULATION_MILESTONE.md`.
        let division = 480.0_f64;
        let expected = 480.0_f64 * (500_000.0_f64 / division) / 1_000_000.0
            + 480.0_f64 * (1_000_000.0_f64 / division) / 1_000_000.0;
        assert_eq!(midi.seconds_for_tick(960), expected);
        // Sanity: 120 BPM's quarter note is 0.5s, 60 BPM's is 1.0s — the exact
        // value is within float epsilon of the musically obvious 1.5s.
        assert!((expected - 1.5).abs() < 1e-9);
    }

    #[test]
    fn note_on_velocity_zero_is_note_off() {
        let events: Vec<(u32, Vec<u8>)> = vec![
            (0, note_on(0, 64, 100)),
            (240, note_on(0, 64, 0)), // the classic trap
            (0, end_of_track()),
        ];
        let mut bytes = header(0, 1, 480);
        bytes.extend_from_slice(&track(&events));
        let midi = MidiFile::parse(&bytes).expect("valid fixture parses");

        let kinds: Vec<&MidiEventKind> = midi.events.iter().map(|e| &e.kind).collect();
        assert!(matches!(
            kinds[0],
            MidiEventKind::NoteOn {
                channel: 0,
                key: 64,
                velocity: 100
            }
        ));
        assert!(matches!(
            kinds[1],
            MidiEventKind::NoteOff {
                channel: 0,
                key: 64,
                velocity: 0
            }
        ));
    }

    #[test]
    fn explicit_note_off_event_is_parsed() {
        let events: Vec<(u32, Vec<u8>)> = vec![
            (0, note_on(1, 40, 90)),
            (200, note_off(1, 40, 64)), // explicit 0x8n note-off, nonzero velocity
            (0, end_of_track()),
        ];
        let mut bytes = header(0, 1, 480);
        bytes.extend_from_slice(&track(&events));
        let midi = MidiFile::parse(&bytes).expect("valid fixture parses");
        let kinds: Vec<&MidiEventKind> = midi.events.iter().map(|e| &e.kind).collect();
        assert!(matches!(
            kinds[1],
            MidiEventKind::NoteOff {
                channel: 1,
                key: 40,
                velocity: 64
            }
        ));
    }

    #[test]
    fn parsing_is_deterministic() {
        let bytes = tempo_change_fixture();
        let first = MidiFile::parse(&bytes).expect("first parse");
        let second = MidiFile::parse(&bytes).expect("second parse");
        assert_eq!(first.division, second.division);
        assert_eq!(first.events, second.events);
    }

    #[test]
    fn running_status_repeats_the_previous_channel_message() {
        // A control-change status byte, then a second CC omitting the status
        // byte (running status) — both must decode to ControlChange events.
        let mut body = Vec::new();
        body.extend_from_slice(&vlq(0));
        body.push(0xB0);
        body.push(74);
        body.push(10);
        body.extend_from_slice(&vlq(120));
        // Running status: no leading 0xB0 — the first byte here is data.
        body.push(74);
        body.push(20);
        body.extend_from_slice(&vlq(0));
        body.extend_from_slice(&end_of_track());
        let mut bytes = header(0, 1, 480);
        bytes.extend_from_slice(&chunk(b"MTrk", &body));

        let midi = MidiFile::parse(&bytes).expect("valid running-status fixture parses");
        let ccs: Vec<&MidiEventKind> = midi
            .events
            .iter()
            .map(|e| &e.kind)
            .filter(|k| matches!(k, MidiEventKind::ControlChange { .. }))
            .collect();
        assert_eq!(ccs.len(), 2);
        assert!(matches!(
            ccs[0],
            MidiEventKind::ControlChange {
                channel: 0,
                controller: 74,
                value: 10
            }
        ));
        assert!(matches!(
            ccs[1],
            MidiEventKind::ControlChange {
                channel: 0,
                controller: 74,
                value: 20
            }
        ));
    }

    #[test]
    fn control_change_events_are_merged_by_tick() {
        let events: Vec<(u32, Vec<u8>)> = vec![
            (0, control_change(0, 74, 0)),
            (100, control_change(0, 74, 64)),
            (100, control_change(0, 74, 127)),
            (0, end_of_track()),
        ];
        let mut bytes = header(0, 1, 480);
        bytes.extend_from_slice(&track(&events));
        let midi = MidiFile::parse(&bytes).expect("valid fixture parses");
        let values: Vec<u8> = midi
            .events
            .iter()
            .filter_map(|e| match e.kind {
                MidiEventKind::ControlChange { value, .. } => Some(value),
                _ => None,
            })
            .collect();
        assert_eq!(values, vec![0, 64, 127]);
    }

    #[test]
    fn rejects_smpte_division() {
        let bytes = header(0, 1, 0xE250); // top bit set => SMPTE, not PPQ
        let err = MidiFile::parse(&bytes).unwrap_err();
        assert!(err.to_string().to_lowercase().contains("smpte"));
    }

    #[test]
    fn rejects_truncated_chunk() {
        let mut bytes = header(0, 1, 480);
        // Declare a track chunk with a length that overruns the actual bytes.
        bytes.extend_from_slice(b"MTrk");
        bytes.extend_from_slice(&100u32.to_be_bytes());
        bytes.extend_from_slice(&[0, 0x90, 60, 100]); // far fewer than 100 bytes
        let err = MidiFile::parse(&bytes).unwrap_err();
        assert!(err.to_string().to_lowercase().contains("truncated"));
    }

    #[test]
    fn rejects_bad_header_id() {
        let bytes = chunk(b"XXXX", &[0, 0, 0, 0, 0x01, 0xE0]);
        let err = MidiFile::parse(&bytes).unwrap_err();
        assert!(err.to_string().to_lowercase().contains("mthd"));
    }

    #[test]
    fn multi_track_format_1_merges_by_tick_then_track_then_order() {
        // Track 0: tempo-only. Track 1: two note-ons that straddle track 0's
        // single event's tick — the merge must interleave by tick, not emit
        // track 0 fully before track 1 (format 1's global-timeline rule).
        let track0: Vec<(u32, Vec<u8>)> = vec![(100, set_tempo(400_000)), (0, end_of_track())];
        let track1: Vec<(u32, Vec<u8>)> = vec![
            (50, note_on(0, 60, 100)),
            (100, note_on(0, 64, 100)),
            (0, end_of_track()),
        ];
        let mut bytes = header(1, 2, 480);
        bytes.extend_from_slice(&track(&track0));
        bytes.extend_from_slice(&track(&track1));
        let midi = MidiFile::parse(&bytes).expect("valid multi-track fixture parses");

        let ordered: Vec<(u64, usize)> = midi
            .events
            .iter()
            .filter(|e| !matches!(e.kind, MidiEventKind::EndOfTrack))
            .map(|e| (e.tick, e.track))
            .collect();
        assert_eq!(ordered, vec![(50, 1), (100, 0), (150, 1)]);
    }
}
