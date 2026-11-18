//! The server's Standard MIDI File reader (server.md SRV-81 to SRV-86;
//! MIDI-112 to MIDI-120): formats 0 and 1, the tempo map, meta events,
//! split system exclusive messages, escapes, the channel map and every
//! bound. It is the server's, not the engine's, because it holds a whole
//! file and merges its tracks, which needs allocation (SRV-81).
//!
//! Every length is checked against the bytes there are before it is
//! believed; every count is bounded by a named constant.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use neoteletext_audio::midi::{ChannelMessage, DATA_MAX, STATUS_BIT, data_bytes, status};

use super::wire::Wire;
use crate::protocol::{DroppedCounts, MidiChannel, Rule, TICK_US};

/// The most tracks a file may have (MIDI-119).
pub(crate) const MOST_MIDI_FILE_TRACKS: usize = 1_024;
/// The most events a file may hold, meta events and dropped messages
/// counted (MIDI-119).
pub(crate) const MOST_MIDI_FILE_EVENTS: usize = 100_000;
/// The most bytes of a variable-length quantity (Standard MIDI Files 1.0,
/// printed page 2: "the largest number which is allowed is 0FFFFFFF").
pub(crate) const MOST_FILE_VLQ_BYTES: usize = 4;
/// The most bytes of a system exclusive message joined from its parts
/// (MIDI-117).
pub(crate) const MOST_JOINED_SYSEX_BYTES: usize = 16_384;
/// The tempo before any set tempo event: 500,000 microseconds a quarter
/// note (Standard MIDI Files 1.0, printed page 10; MIDI-114).
pub(crate) const DEFAULT_TEMPO_US: u32 = 500_000;
/// The most a tempo may be: three bytes (MIDI-114).
pub(crate) const MOST_TEMPO_US: u32 = 0x00FF_FFFF;

/// The header chunk's type.
const HEADER: &[u8; 4] = b"MThd";
/// A track chunk's type.
const TRACK: &[u8; 4] = b"MTrk";
/// The least length of the header's data: format, tracks, division.
const HEADER_DATA: usize = 6;
/// A chunk's type and length.
const CHUNK_HEAD: usize = 8;
/// The meta event's status (Standard MIDI Files 1.0, printed page 8).
const META: u8 = 0xFF;
/// Set tempo, `FF 51 03 tttttt`.
const META_TEMPO: u8 = 0x51;
/// The bytes of set tempo's data.
const TEMPO_BYTES: usize = 3;
/// End of track, `FF 2F 00`.
const META_END_OF_TRACK: u8 = 0x2F;
/// The division's bit that means SMPTE time.
const DIVISION_SMPTE: u16 = 0x8000;
/// The formats read (MIDI-113).
const FORMAT_SINGLE: u16 = 0;
/// Format 1: simultaneous tracks.
const FORMAT_SIMULTANEOUS: u16 = 1;
/// Bits of a variable-length quantity's byte that are its value.
const SEVEN_BITS: u32 = 7;
/// Half a tick of rounding, in microseconds (MIDI-17).
const HALF_TICK_US: u128 = TICK_US as u128 / 2;

/// A file read: its messages with their ticks of 0.1 ms, in the order a
/// block writes them (SRV-65), the last end of track's tick, and what was
/// dropped.
#[derive(Debug, Default)]
pub(crate) struct FileMusic {
    /// The messages.
    pub(crate) events: Vec<FileEvent>,
    /// The latest end of track, in ticks of 0.1 ms (MIDI-120).
    pub(crate) end_tick: u64,
    /// What was dropped (MIDI-118).
    pub(crate) dropped: DroppedCounts,
}

/// One message of a file, placed.
#[derive(Debug)]
pub(crate) struct FileEvent {
    /// Its tick of 0.1 ms from the sound's start.
    pub(crate) tick: u64,
    /// The message, its channel mapped.
    pub(crate) wire: Wire,
    /// The track it is in, counted from 0.
    pub(crate) track: usize,
    /// Its index within the track.
    pub(crate) index: usize,
    /// Its byte offset in the file.
    pub(crate) offset: usize,
}

/// A file's fault: the rule and the detail.
pub(crate) type FileFault = (Rule, String);

fn malformed(detail: String) -> FileFault {
    (Rule::MidiFileMalformed, detail)
}

/// One event of a track before the tempo map places it.
struct Raw {
    file_tick: u64,
    track: usize,
    index: usize,
    offset: usize,
    kind: RawKind,
}

enum RawKind {
    Message(Wire),
    Tempo(u32),
}

/// A cursor over a chunk's bytes, at their offset in the file.
struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
    base: usize,
}

impl Cursor<'_> {
    fn offset(&self) -> usize {
        self.base.saturating_add(self.at)
    }

    fn byte(&mut self) -> Result<u8, FileFault> {
        let byte = self.bytes.get(self.at).copied().ok_or_else(|| malformed(format!("MIDI-119: an event passes its track's end at byte {}", self.offset())))?;
        self.at = self.at.saturating_add(1);
        Ok(byte)
    }

    fn take(&mut self, count: usize) -> Result<&[u8], FileFault> {
        let end = self.at.saturating_add(count);
        let taken =
            self.bytes.get(self.at..end).ok_or_else(|| malformed(format!("MIDI-119: a length of {count} at byte {} passes its track's end", self.offset())))?;
        self.at = end;
        Ok(taken)
    }

    /// A variable-length quantity of at most four bytes (MIDI-119).
    fn quantity(&mut self) -> Result<u32, FileFault> {
        let mut value: u32 = 0;
        for _ in 0..MOST_FILE_VLQ_BYTES {
            let byte = self.byte()?;
            value = (value << SEVEN_BITS) | u32::from(byte & DATA_MAX);
            if byte & STATUS_BIT == 0 {
                return Ok(value);
            }
        }
        Err(malformed(format!("MIDI-119: a variable-length quantity of more than {MOST_FILE_VLQ_BYTES} bytes at byte {}", self.offset())))
    }

    fn length(&mut self) -> Result<usize, FileFault> {
        self.quantity().map(|value| usize::try_from(value).unwrap_or(usize::MAX))
    }
}

fn be32(bytes: &[u8]) -> usize {
    bytes.iter().take(4).fold(0_usize, |value, &byte| (value << 8) | usize::from(byte))
}

fn be16(bytes: &[u8]) -> u16 {
    bytes.iter().take(2).fold(0_u16, |value, &byte| (value << 8) | u16::from(byte))
}

/// Reads `file`, mapping each channel message's channel by `map` and
/// keeping only what `takes` accepts, the rest counted as dropped.
///
/// # Errors
/// The first fault, with its rule of SRV-51.
pub(crate) fn read(file: &[u8], map: Option<&[Option<MidiChannel>; 16]>, takes: &dyn Fn(&Wire) -> bool) -> Result<FileMusic, FileFault> {
    if file.len() > crate::protocol::MOST_MIDI_FILE_BYTES {
        return Err((Rule::MidiFileTooLarge, format!("MIDI-119: {} bytes, at most {}", file.len(), crate::protocol::MOST_MIDI_FILE_BYTES)));
    }
    let head = file.get(..CHUNK_HEAD).ok_or_else(|| (Rule::MidiFileUnsupported, "MIDI-113: no header chunk".into()))?;
    if head.get(..4) != Some(HEADER.as_slice()) {
        return Err((Rule::MidiFileUnsupported, "MIDI-113: the first chunk is not MThd".into()));
    }
    let header_length = be32(head.get(4..).unwrap_or(&[]));
    if header_length < HEADER_DATA {
        return Err((Rule::MidiFileUnsupported, format!("MIDI-113: a header of {header_length} bytes, at least {HEADER_DATA}")));
    }
    let header = file
        .get(CHUNK_HEAD..CHUNK_HEAD.saturating_add(header_length))
        .ok_or_else(|| malformed("MIDI-119: the header's length passes the file's end".into()))?;
    let format = be16(header);
    let tracks = usize::from(be16(header.get(2..).unwrap_or(&[])));
    let division = be16(header.get(4..).unwrap_or(&[]));
    if format != FORMAT_SINGLE && format != FORMAT_SIMULTANEOUS {
        return Err((Rule::MidiFileUnsupported, format!("MIDI-113: format {format}; formats 0 and 1 are read")));
    }
    if division & DIVISION_SMPTE != 0 || division == 0 {
        return Err((Rule::MidiFileUnsupported, format!("MIDI-113: division 0x{division:04X}; ticks a quarter note, not 0 and not SMPTE")));
    }
    if format == FORMAT_SINGLE && tracks != 1 {
        return Err((Rule::MidiFileUnsupported, format!("MIDI-113: format 0 with {tracks} tracks")));
    }
    if tracks > MOST_MIDI_FILE_TRACKS {
        return Err((Rule::MidiFileTooLarge, format!("MIDI-119: {tracks} tracks, at most {MOST_MIDI_FILE_TRACKS}")));
    }
    let mut at = CHUNK_HEAD.saturating_add(header_length);
    let mut raws: Vec<Raw> = Vec::new();
    let mut dropped = DroppedCounts::default();
    let mut found: usize = 0;
    let mut end_file_tick: u64 = 0;
    let mut counted: usize = 0;
    while at < file.len() {
        let head =
            file.get(at..at.saturating_add(CHUNK_HEAD)).ok_or_else(|| malformed(format!("MIDI-119: a chunk's head at byte {at} passes the file's end")))?;
        let length = be32(head.get(4..).unwrap_or(&[]));
        let start = at.saturating_add(CHUNK_HEAD);
        let body = file.get(start..start.saturating_add(length)).ok_or_else(|| malformed(format!("MIDI-119: the chunk at byte {at} passes the file's end")))?;
        if head.get(..4) == Some(TRACK.as_slice()) {
            if found >= tracks {
                return Err(malformed(format!("MIDI-113: more MTrk chunks than the header's {tracks}")));
            }
            let end = track(Cursor { bytes: body, at: 0, base: start }, found, map, takes, &mut raws, &mut dropped, &mut counted)?;
            end_file_tick = end_file_tick.max(end);
            found = found.saturating_add(1);
        }
        at = start.saturating_add(length);
    }
    if found != tracks {
        return Err(malformed(format!("MIDI-113: {found} MTrk chunks; the header says {tracks}")));
    }
    // SRV-65: by time, then track 0 first, then each track's own order.
    raws.sort_by_key(|raw| (raw.file_tick, raw.track, raw.index));
    Ok(place(raws, division, end_file_tick, dropped))
}

/// Reads one track's events.
#[allow(clippy::too_many_lines, reason = "one arm for each kind of a track's event, as Standard MIDI Files 1.0 lists them")]
#[allow(clippy::too_many_arguments, reason = "the reader's state, passed to the one function that reads a track")]
fn track(
    mut cursor: Cursor<'_>,
    number: usize,
    map: Option<&[Option<MidiChannel>; 16]>,
    takes: &dyn Fn(&Wire) -> bool,
    raws: &mut Vec<Raw>,
    dropped: &mut DroppedCounts,
    counted: &mut usize,
) -> Result<u64, FileFault> {
    let mut tick: u64 = 0;
    let mut running: Option<u8> = None;
    // A system exclusive message waiting for its continuations: its bytes
    // after F0 so far, and where it began.
    let mut joining: Option<(Vec<u8>, u64, usize, usize)> = None;
    let mut index: usize = 0;
    let mut end: u64 = 0;
    while cursor.at < cursor.bytes.len() {
        *counted = counted.saturating_add(1);
        if *counted > MOST_MIDI_FILE_EVENTS {
            return Err((Rule::MidiFileTooLarge, format!("MIDI-119: more than {MOST_MIDI_FILE_EVENTS} events")));
        }
        tick = tick.saturating_add(u64::from(cursor.quantity()?));
        let offset = cursor.offset();
        let first = cursor.byte()?;
        let push = |raws: &mut Vec<Raw>, kind: RawKind| raws.push(Raw { file_tick: tick, track: number, index, offset, kind });
        match first {
            META => {
                running = None;
                let kind = cursor.byte()?;
                if kind & STATUS_BIT != 0 {
                    return Err(malformed(format!("MIDI-115: a meta event's type 0x{kind:02X} at byte {offset}")));
                }
                let length = cursor.length()?;
                let data = cursor.take(length)?;
                match kind {
                    META_TEMPO if data.len() == TEMPO_BYTES => {
                        let tempo = data.iter().fold(0_u32, |value, &byte| (value << 8) | u32::from(byte));
                        if tempo == 0 {
                            return Err(malformed(format!("MIDI-114: a tempo of 0 at byte {offset}")));
                        }
                        push(raws, RawKind::Tempo(tempo.min(MOST_TEMPO_US)));
                    }
                    META_END_OF_TRACK => end = end.max(tick),
                    _ => dropped.meta_events = dropped.meta_events.saturating_add(1),
                }
            }
            status::SYSTEM_EXCLUSIVE => {
                running = None;
                let length = cursor.length()?;
                let data = cursor.take(length)?.to_vec();
                if data.len() > MOST_JOINED_SYSEX_BYTES {
                    return Err((
                        Rule::MidiFileTooLarge,
                        format!("MIDI-117: a system exclusive message of more than {MOST_JOINED_SYSEX_BYTES} bytes at byte {offset}"),
                    ));
                }
                if data.last() == Some(&status::END_OF_EXCLUSIVE) {
                    push(raws, RawKind::Message(Wire::Sysex(data)));
                } else {
                    joining = Some((data, tick, index, offset));
                }
            }
            status::END_OF_EXCLUSIVE => {
                running = None;
                let length = cursor.length()?;
                let data = cursor.take(length)?;
                if let Some((mut joined, start_tick, start_index, start_offset)) = joining.take() {
                    joined.extend_from_slice(data);
                    if joined.len() > MOST_JOINED_SYSEX_BYTES {
                        return Err((
                            Rule::MidiFileTooLarge,
                            format!("MIDI-117: a joined system exclusive message of more than {MOST_JOINED_SYSEX_BYTES} bytes"),
                        ));
                    }
                    if joined.last() == Some(&status::END_OF_EXCLUSIVE) {
                        raws.push(Raw {
                            file_tick: start_tick,
                            track: number,
                            index: start_index,
                            offset: start_offset,
                            kind: RawKind::Message(Wire::Sysex(joined)),
                        });
                    } else {
                        joining = Some((joined, start_tick, start_index, start_offset));
                    }
                } else if data.first().is_some_and(|&byte| byte > status::SYSTEM_EXCLUSIVE) {
                    dropped.system_messages = dropped.system_messages.saturating_add(1);
                } else {
                    dropped.escapes = dropped.escapes.saturating_add(1);
                }
            }
            _ => {
                let (status_byte, first_data) = if first & STATUS_BIT != 0 {
                    (first, None)
                } else {
                    let running = running.ok_or_else(|| malformed(format!("MIDI-119: a data byte with no status at byte {offset}")))?;
                    (running, Some(first))
                };
                let count = data_bytes(status_byte)
                    .ok_or_else(|| malformed(format!("MIDI-113: status 0x{status_byte:02X} at byte {offset} is no event of a track")))?;
                running = Some(status_byte);
                let mut data = [0_u8; 2];
                for place in 0..count {
                    let byte = match (place, first_data) {
                        (0, Some(byte)) => byte,
                        _ => cursor.byte()?,
                    };
                    if byte > DATA_MAX {
                        return Err(malformed(format!("MIDI-11: byte 0x{byte:02X} of the message at byte {offset} is not a data byte")));
                    }
                    if let Some(slot) = data.get_mut(place) {
                        *slot = byte;
                    }
                }
                let [data1, data2] = data;
                let mut message = ChannelMessage { status: status_byte, data1, data2 };
                if let Some(map) = map {
                    let Some(channel) = map.get(usize::from(message.channel())).copied().flatten() else {
                        dropped.unmapped_channel = dropped.unmapped_channel.saturating_add(1);
                        index = index.saturating_add(1);
                        continue;
                    };
                    message = ChannelMessage::new(message.kind(), channel.get().saturating_sub(1), data1, data2);
                }
                let wire = Wire::Channel(message);
                if super::wire::never_carried(&wire).is_ok() && takes(&wire) {
                    push(raws, RawKind::Message(wire));
                } else {
                    dropped.not_taken = dropped.not_taken.saturating_add(1);
                }
            }
        }
        index = index.saturating_add(1);
    }
    if joining.is_some() {
        return Err(malformed(format!("MIDI-117: a system exclusive message in track {number} never ends")));
    }
    Ok(end.max(tick))
}

/// Places every event by the tempo map, exactly, rounding once (MIDI-114;
/// SRV-82): an event's time is the sum of its ticks' durations at each
/// tempo, kept as a numerator over the division.
fn place(raws: Vec<Raw>, division: u16, end_file_tick: u64, dropped: DroppedCounts) -> FileMusic {
    let division = u128::from(division);
    let mut tempo = u128::from(DEFAULT_TEMPO_US);
    let mut last_tick: u64 = 0;
    // Microseconds since the start, times the division.
    let mut numerator: u128 = 0;
    let to_tick = |numerator: u128| -> u64 {
        let tick = numerator.saturating_add(HALF_TICK_US.saturating_mul(division)).checked_div(u128::from(TICK_US).saturating_mul(division)).unwrap_or(0);
        u64::try_from(tick).unwrap_or(u64::MAX)
    };
    let mut music = FileMusic { dropped, ..FileMusic::default() };
    for raw in raws {
        numerator = numerator.saturating_add(u128::from(raw.file_tick.saturating_sub(last_tick)).saturating_mul(tempo));
        last_tick = raw.file_tick;
        match raw.kind {
            RawKind::Tempo(new) => tempo = u128::from(new),
            RawKind::Message(wire) => music.events.push(FileEvent { tick: to_tick(numerator), wire, track: raw.track, index: raw.index, offset: raw.offset }),
        }
    }
    numerator = numerator.saturating_add(u128::from(end_file_tick.saturating_sub(last_tick)).saturating_mul(tempo));
    music.end_tick = to_tick(numerator);
    music
}
