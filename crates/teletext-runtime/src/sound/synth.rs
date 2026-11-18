//! The synthesizers' sounds built (server.md sections 4.2 to 4.8): events
//! placed at their ticks, bank dumps lifted, blocks written through the
//! engine's writer with each part's restatement, live sounds held and
//! paced to the receiver's ring, banks held and sent before the sounds
//! that name them, and the records written through the engine's encoder.
//! Every block is read back by the engine's reader before it is sent
//! (`Sound::events`, `Sound::live_event_records`; SRV-54).

use alloc::collections::{BTreeMap, BTreeSet, VecDeque};
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use neoteletext_audio::encode::{EncodeError, Sound as Encoded};
use neoteletext_audio::midi::block::BlockWriter;
use neoteletext_audio::midi::{Banks, CHANNELS, MOST_SYSTEM_EXCLUSIVE_BYTES, status};
use neoteletext_audio::{LIVE_LOOKAHEAD_BLOCKS, LIVE_RING_BLOCKS};
use neoteletext_audio_instruments::synth6581::interface::{BlockBudget, accepts as synth6581_accepts};

use super::family::{Family, FamilyFault, Synth};
use super::midi_file;
use super::tracker::{TrackHint, Tracker};
use super::wire::{self, Wire};
use crate::protocol::{
    BLOCK_TICKS, BLOCK_US, BankName, ConnectionId, DroppedCounts, MOST_BLOCK_BYTES, MOST_INSTRUMENT_BLOCKS, MOST_LIVE_ADVANCE_US, MidiChannel, NoteFamily,
    Rule, SoundBuilt, SoundKind, SoundLength, SplitEvent, Synth6581Event, TICK_US,
};

/// A refusal of a sound message: the path, the rule and the detail
/// (SRV-50).
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct Refused {
    /// The path of the field at fault.
    pub(crate) path: String,
    /// The rule.
    pub(crate) rule: Rule,
    /// The detail, beginning with the requirement.
    pub(crate) detail: String,
}

/// A refusal at `path`.
pub(crate) fn refuse(path: impl Into<String>, rule: Rule, detail: impl Into<String>) -> Refused {
    Refused { path: path.into(), rule, detail: detail.into() }
}

fn family_fault(path: &str, fault: FamilyFault) -> Refused {
    refuse(path, fault.rule, fault.detail)
}

/// What the server calls in a family's interface for its own typed
/// patches, parameters and banks (SRV-55, SRV-88), in `sound/family/`.
pub(crate) trait FamilyCalls: NoteFamily {
    /// The family's bank type.
    type Bank;
    /// An inline patch's messages (MIDI-60) for `synth`, and what the
    /// tracker is told beside them.
    fn inline_patch(synth: Synth, event: &Self::PatchEvent) -> Result<(Vec<Wire>, Option<TrackHint>), FamilyFault>;
    /// A named parameter's messages (MIDI-18) for `synth`.
    fn parameter(synth: Synth, event: &Self::ParameterEvent) -> Result<Vec<Wire>, FamilyFault>;
    /// A bank's image (MIDI-56) for `synth`.
    fn bank_image(synth: Synth, bank: &Self::Bank) -> Result<Vec<u8>, FamilyFault>;
}

/// How far ahead of its due time a live block is sent: 25 blocks, 500
/// milliseconds (`integration.md` INT-18; SRV-77).
pub(crate) const LIVE_SEND_LEAD_BLOCKS: u64 = 25;
const _: () = assert!(LIVE_SEND_LEAD_BLOCKS < (LIVE_RING_BLOCKS - LIVE_LOOKAHEAD_BLOCKS) as u64);
/// A live sound's parts are restated at least once in this many blocks
/// (MIDI-96).
pub(crate) const RESTATE_EVERY_BLOCKS: u16 = 50;
/// ... and every this many where the blocks have room (MIDI-96).
pub(crate) const RESTATE_AIM_BLOCKS: u16 = 25;
/// Milliseconds a block lasts.
const BLOCK_MS: u64 = BLOCK_US / MICROSECONDS_PER_MILLISECOND;
/// Microseconds in a millisecond.
const MICROSECONDS_PER_MILLISECOND: u64 = 1_000;
/// The block number's modulus in a DATA record (MIDI-42): two bytes.
const BLOCK_NUMBER_MODULUS: u64 = 1 << 16;

/// The tick of a time (SRV-8; MIDI-17): rounded to the nearest, each
/// event's from its own time.
pub(crate) const fn tick_of(time_us: u64) -> u64 {
    neoteletext_audio::midi::tick_of_microseconds(time_us)
}

/// One message placed at its tick, with where it came from.
#[derive(Clone, Debug)]
pub(crate) struct Timed {
    /// The tick from the sound's start.
    pub(crate) tick: u64,
    /// The message.
    pub(crate) wire: Wire,
    /// The path of the event that gave it.
    pub(crate) path: String,
    /// What the tracker is told after the message (SRV-68).
    pub(crate) hint: Option<TrackHint>,
}

/// Expands a note family's events into messages at their ticks, each
/// checked against the family (SRV-10, SRV-24).
pub(crate) fn place_notes<F: FamilyCalls>(synth: Synth, events: &[F::Event], at: &str, out: &mut Vec<Timed>) -> Result<(), Refused> {
    let mut wires = Vec::new();
    for (index, event) in events.iter().enumerate() {
        let path = format!("{at}[{index}]");
        let time_us = F::time_us(event);
        wires.clear();
        let mut hint = None;
        match F::split(event) {
            SplitEvent::Shared(shared) => wire::expand(shared, &mut wires).map_err(|(rule, detail)| refuse(&path, rule, detail))?,
            SplitEvent::Patch(inline) => {
                let (messages, told) = F::inline_patch(synth, inline).map_err(|fault| family_fault(&format!("{path}.patch"), fault))?;
                wires.extend(messages);
                hint = told;
            }
            SplitEvent::Parameter(parameter) => {
                wires.extend(F::parameter(synth, parameter).map_err(|fault| family_fault(&format!("{path}.set"), fault))?);
            }
        }
        let last = wires.len().saturating_sub(1);
        for (position, wire) in wires.drain(..).enumerate() {
            // A deferred message is checked once it is resolved.
            if !matches!(wire, Wire::Deferred(_)) {
                synth.takes(&wire).map_err(|detail| refuse(&path, Rule::NotTaken, detail))?;
            }
            let hint = if position == last { hint.take() } else { None };
            out.push(Timed { tick: tick_of(time_us), wire, path: path.clone(), hint });
        }
    }
    Ok(())
}

/// Expands synth6581 events (SRV-12): `register_update` through the
/// engine's packer, `asid` checked as one message.
pub(crate) fn place_synth6581(events: &[Synth6581Event], at: &str, out: &mut Vec<Timed>) -> Result<(), Refused> {
    for (index, event) in events.iter().enumerate() {
        let path = format!("{at}[{index}]");
        let wires = match event {
            Synth6581Event::RegisterUpdate(update) => super::family::synth6581::register_update(update),
            Synth6581Event::Asid(message) => super::family::synth6581::asid(&crate::protocol::check::bytes_of(&message.bytes)),
        }
        .map_err(|fault| family_fault(&path, fault))?;
        for wire in wires {
            super::family::synth6581::takes(&wire).map_err(|detail| refuse(&path, Rule::NotTaken, detail))?;
            out.push(Timed { tick: tick_of(event.time_us()), wire, path: path.clone(), hint: None });
        }
    }
    Ok(())
}

/// The opening S6581-45 requires: a full register update at tick 0 of
/// block 0 (SRV-25).
pub(crate) fn check_opening(events: &[Synth6581Event], at: &str) -> Result<(), Refused> {
    let first = events.first().ok_or_else(|| refuse(at, Rule::NoOpeningUpdate, "S6581-45: the sound opens with a full register update at tick 0"))?;
    if tick_of(first.time_us()) != 0 || !super::family::synth6581::is_full_update(first) {
        return Err(refuse(format!("{at}[0]"), Rule::NoOpeningUpdate, "S6581-45, S6581-50: the first event is a full register update (IDs 0 to 24) at tick 0"));
    }
    Ok(())
}

/// Reads a Standard MIDI File into placed messages (SRV-81 to SRV-86).
pub(crate) fn place_file(synth: Synth, bytes: &[u8], map: Option<&[Option<MidiChannel>; 16]>, at: &str) -> Result<(Vec<Timed>, u64, DroppedCounts), Refused> {
    let takes = |wire: &Wire| synth.takes(wire).is_ok();
    let music = midi_file::read(bytes, map, &takes).map_err(|(rule, detail)| refuse(at, rule, detail))?;
    let timed = music
        .events
        .into_iter()
        .map(|event| Timed {
            tick: event.tick,
            wire: event.wire,
            path: format!("{at} (track {}, event {}, byte {})", event.track, event.index, event.offset),
            hint: None,
        })
        .collect();
    Ok((timed, music.end_tick, music.dropped))
}

/// The bank dumps lifted from a stored sound's music (SRV-63): taken out
/// of `timed`, grouped by the group each fills, in the order they first
/// appear. Any other system exclusive message longer than an event is
/// refused (MIDI-28).
pub(crate) fn lift(family: Family, timed: &mut Vec<Timed>, live: bool) -> Result<Vec<Vec<Vec<u8>>>, Refused> {
    let mut groups: Vec<(u8, Vec<Vec<u8>>)> = Vec::new();
    let mut noted = false;
    let mut kept = Vec::with_capacity(timed.len());
    for event in timed.drain(..) {
        if let Wire::Channel(message) = &event.wire
            && message.kind() == status::NOTE_ON
            && message.data2 > 0
        {
            noted = true;
        }
        if let Wire::Sysex(bytes) = &event.wire {
            if let Some(group) = family.bank_dump(bytes).map_err(|fault| family_fault(&event.path, fault))? {
                if live || noted {
                    return Err(refuse(
                        &event.path,
                        Rule::BankDumpAfterNote,
                        format!("MIDI-58, MIDI-51: a bank dump at tick {} comes after the first note on, or in a live sound", event.tick),
                    ));
                }
                // SRV-63: the synth53 family's program dumps of one group make
                // one bank; every other family's dump is a bank of its own.
                let joined = if family == Family::Synth53 { groups.iter_mut().find(|(kept, _)| *kept == group) } else { None };
                match joined {
                    Some((_, dumps)) => dumps.push(bytes.clone()),
                    None => groups.push((group, alloc::vec![bytes.clone()])),
                }
                continue;
            }
            if bytes.len() > MOST_SYSTEM_EXCLUSIVE_BYTES {
                return Err(refuse(
                    &event.path,
                    Rule::SystemExclusiveTooLong,
                    format!("MIDI-28: {} bytes after 0xF0, at most {MOST_SYSTEM_EXCLUSIVE_BYTES}", bytes.len()),
                ));
            }
        }
        kept.push(event);
    }
    *timed = kept;
    Ok(groups.into_iter().map(|(_, dumps)| dumps).collect())
}

/// A stored sound's blocks (SRV-9): from its length, or its last event's
/// tick and its tail.
pub(crate) fn stored_blocks(length: SoundLength, last_tick: Option<u64>) -> Result<u64, Refused> {
    let blocks = match length {
        SoundLength::LengthUs(length) => length.get().div_ceil(BLOCK_US),
        SoundLength::TailUs(tail) => match last_tick {
            Some(last) => last.saturating_mul(TICK_US).saturating_add(tail.get()).div_ceil(BLOCK_US).max((last / BLOCK_TICKS).saturating_add(1)),
            None => tail.get().div_ceil(BLOCK_US),
        },
    }
    .max(1);
    let most = MOST_INSTRUMENT_BLOCKS as u64;
    if blocks > most {
        return Err(refuse("length", Rule::BeyondLength, format!("SRV-9, MIDI-104: the sound needs {blocks} blocks, at most {most}")));
    }
    Ok(blocks)
}

/// The parts a stored sound uses (SRV-70): for synth7, each channel a
/// message addresses (a channel message's, or the channel byte of its
/// voice dump or parameter change, S7-38, S7-43); for a family of one part,
/// that part, when the sound has any message.
fn used_parts(synth: Synth, tracker: &Tracker, timed: &[Timed]) -> Vec<u8> {
    if tracker.parts() == 1 {
        return if timed.is_empty() { Vec::new() } else { alloc::vec![super::tracker::ONE_PART] };
    }
    let mut used = [false; CHANNELS];
    for event in timed {
        let part = match &event.wire {
            Wire::Channel(message) => Some(message.channel()),
            Wire::Sysex(bytes) if synth.family == Family::Synth7 => super::family::synth7::part_of(bytes),
            Wire::Sysex(_) | Wire::Deferred(_) => None,
        };
        if let Some(slot) = part.and_then(|part| used.get_mut(usize::from(part))) {
            *slot = true;
        }
    }
    (0_u8..).zip(used).filter(|(_, used)| *used).map(|(part, _)| part).collect()
}

/// Resolves a block's deferred messages from the tracker and gives the
/// tracker every message of the block in order (SRV-68): the block's
/// events as they are written.
fn resolve_and_track(synth: Synth, tracker: &mut Tracker, banks: &Banks<'_>, events: Vec<Timed>) -> Result<Vec<Timed>, Refused> {
    let mut written = Vec::with_capacity(events.len());
    for event in events {
        let Timed { tick, wire, path, hint } = event;
        let wires = match &wire {
            Wire::Deferred(deferred) => {
                let resolved = deferred.resolve(tracker).map_err(|fault| family_fault(&path, fault))?;
                for message in &resolved {
                    synth.takes(message).map_err(|detail| refuse(&path, Rule::NotTaken, detail))?;
                }
                resolved
            }
            Wire::Channel(_) | Wire::Sysex(_) => alloc::vec![wire],
        };
        let last = wires.len().saturating_sub(1);
        for (position, wire) in wires.into_iter().enumerate() {
            let hint = if position == last { hint.clone() } else { None };
            tracker.take(&wire, hint.as_ref(), banks);
            written.push(Timed { tick, wire, path: path.clone(), hint });
        }
    }
    Ok(written)
}

/// The synth6581 family's bound on a block (S6581-42, SRV-67): at most 20
/// register data messages and one start and one stop, the restatement
/// among them.
fn within_budget<'a>(family: Family, messages: impl Iterator<Item = &'a Wire>) -> bool {
    if family != Family::Synth6581 {
        return true;
    }
    let mut budget = BlockBudget::new();
    for wire in messages {
        if !synth6581_accepts(&wire.message()).is_ok_and(|asid| budget.admit(&asid)) {
            return false;
        }
    }
    true
}

/// Writes one block: the restatements that fit, at tick 0 before
/// everything (SRV-69), then the program's events (SRV-65). Gives the
/// block and which of `restate`'s parts were written.
fn write_block(family: Family, index: u64, restate: &[(u8, Vec<Wire>)], events: &[&Timed]) -> Result<(Vec<u8>, Vec<u8>), Refused> {
    let mut buffer = [0_u8; MOST_BLOCK_BYTES];
    let mut fit = restate.len();
    loop {
        let mut writer = BlockWriter::new(&mut buffer);
        let mut failed = None;
        let restated = restate.iter().take(fit).flat_map(|(_, wires)| wires.iter());
        let over_budget = !within_budget(family, restated.chain(events.iter().map(|event| &event.wire)));
        if over_budget {
            failed = Some(None);
        }
        for (_, wires) in restate.iter().take(fit) {
            for wire in wires {
                if writer.push(0, &wire.message()).is_err() {
                    failed = Some(None);
                }
            }
        }
        if failed.is_none() {
            for event in events {
                let time = u8::try_from(event.tick % BLOCK_TICKS).unwrap_or(u8::MAX);
                if let Err(error) = writer.push(time, &event.wire.message()) {
                    failed = Some(Some((error, *event)));
                    break;
                }
            }
        }
        match failed {
            None => return Ok((writer.bytes().to_vec(), restate.iter().take(fit).map(|(part, _)| *part).collect())),
            Some(_) if fit == 0 && over_budget => {
                let path = events.last().map_or(String::new(), |event| event.path.clone());
                return Err(refuse(
                    path,
                    Rule::BlockFull,
                    format!("S6581-42, SRV-67: block {index} would hold more than 20 register data messages, or more than one start or one stop"),
                ));
            }
            Some(fault) if fit == 0 => {
                let (error, event) = fault.map_or((None, None), |(error, event)| (Some(error), Some(event)));
                let path = event.map_or(String::new(), |event| event.path.clone());
                let rule = match error {
                    Some(neoteletext_audio::midi::block::WriteError::Message) => Rule::MalformedMidi,
                    _ => Rule::BlockFull,
                };
                return Err(refuse(
                    path,
                    rule,
                    format!(
                        "MIDI-30, MIDI-31: block {index} would hold more than {MOST_BLOCK_BYTES} bytes or 256 events ({} events given), or a message it does not carry",
                        events.len()
                    ),
                ));
            }
            Some(_) => fit = fit.saturating_sub(1),
        }
    }
}

fn encode_fault(error: EncodeError) -> Refused {
    match error {
        EncodeError::Total | EncodeError::Length => refuse("", Rule::BeyondLength, "MIDI-41, MIDI-104: more than 6,000 blocks or 960,000 bytes"),
        EncodeError::Banks => refuse("banks", Rule::TooManyBanks, "MIDI-57: 1 to 10 banks"),
        EncodeError::Block { index, fault } => {
            refuse("", Rule::MalformedMidi, format!("MIDI-21: the receiver would not wholly act on block {index}: {fault:?}"))
        }
        other => refuse("", Rule::MalformedMidi, format!("MIDI-21: the encoder refused the sound: {other:?}")),
    }
}

/// A sound's OBJECT settings (MIDI-48): set from its kind and fields.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub(crate) struct Settings {
    /// Tag 3: the chip and clock (codec 12).
    pub(crate) chip: Option<u8>,
    /// Tag 7: synth8's key mode and split key.
    pub(crate) keys: Option<[u8; 2]>,
    /// Tag 4: the banks, bank index 0 first.
    pub(crate) banks: Vec<u8>,
}

impl Settings {
    fn apply(&self, synth: Synth, sound: &mut Encoded) {
        sound.chip = self.chip;
        sound.keys = self.keys;
        sound.banks.clone_from(&self.banks);
        sound.variant = synth.variant_tag();
    }
}

/// A stored synthesizer sound, everything known: what [`Synths::stored`]
/// builds.
pub(crate) struct Stored<'a> {
    /// Its kind.
    pub(crate) kind: SoundKind,
    /// Its synthesizer.
    pub(crate) synth: Synth,
    /// Its handle.
    pub(crate) handle: u8,
    /// Its generation.
    pub(crate) generation: u16,
    /// Its settings; its banks are those it names.
    pub(crate) settings: Settings,
    /// The names of the banks lifted from its music.
    pub(crate) lift: &'a [BankName],
    /// Its length.
    pub(crate) length: SoundLength,
    /// Its music, placed.
    pub(crate) timed: Vec<Timed>,
    /// A file's last end of track.
    pub(crate) end_tick: Option<u64>,
    /// What a file held that was dropped.
    pub(crate) dropped: DroppedCounts,
}

/// One bank the server holds for a program (SRV-59).
#[derive(Clone, Debug)]
struct Bank {
    generation: u16,
    family: Family,
    records: Vec<Vec<u8>>,
    /// Its image, which the trackers of the sounds naming it read
    /// (SRV-68; MIDI-61).
    image: Vec<u8>,
}

/// What a sound message sends: records for each connection, in order, and
/// `sound_built` where a stored sound was built.
#[derive(Debug, Default)]
pub(crate) struct Sent {
    /// Records for each connection.
    pub(crate) records: Vec<(ConnectionId, Vec<Vec<u8>>)>,
    /// `sound_built`, for a stored sound.
    pub(crate) built: Option<SoundBuilt>,
}

impl Sent {
    fn add(&mut self, connection: ConnectionId, records: Vec<Vec<u8>>) {
        if records.is_empty() {
            return;
        }
        match self.records.iter_mut().find(|(kept, _)| *kept == connection) {
            Some((_, kept)) => kept.extend(records),
            None => self.records.push((connection, records)),
        }
    }
}

/// A live synthesizer sound the server holds (SRV-72).
#[derive(Clone, Debug)]
struct Live {
    kind: SoundKind,
    synth: Synth,
    generation: u16,
    settings: Settings,
    encoded: Encoded,
    object: Vec<u8>,
    announced: BTreeSet<ConnectionId>,
    until_us: u64,
    latest_us: u64,
    next_write: u64,
    pending: Vec<Timed>,
    /// The images of its banks, bank index 0 first, as it was announced.
    images: Vec<Vec<u8>>,
    tracker: Tracker,
    since: Vec<u16>,
    opened: bool,
    held: VecDeque<(u64, Vec<u8>, Vec<ConnectionId>)>,
    clock_start_ms: Option<u64>,
}

/// The program's banks and live sounds, and what each connection has been
/// sent (SRV-59, SRV-61, SRV-72).
#[derive(Default)]
pub(crate) struct Synths {
    banks: BTreeMap<u8, Bank>,
    live: BTreeMap<u8, Live>,
    sent_banks: BTreeMap<ConnectionId, BTreeSet<(u8, u16)>>,
}

impl Synths {
    /// Forgets what a closed connection was sent.
    pub(crate) fn forget(&mut self, connection: ConnectionId) {
        self.sent_banks.remove(&connection);
        for live in self.live.values_mut() {
            live.announced.remove(&connection);
        }
    }

    /// Whether `handle` names a bank (SRV-46).
    pub(crate) fn is_bank(&self, handle: u8) -> bool {
        self.banks.contains_key(&handle)
    }

    /// A handle now names another object: its bank and its live sound go
    /// (SRV-59, SRV-72).
    pub(crate) fn replaced(&mut self, handle: u8) {
        self.banks.remove(&handle);
        self.live.remove(&handle);
    }

    /// The banks a sound names, checked against the bank table (SRV-58,
    /// SRV-61).
    fn check_banks(&self, family: Family, banks: &[u8], lifted: usize) -> Result<(), Refused> {
        let total = banks.len().saturating_add(lifted);
        if total > family.most_banks() {
            return Err(refuse("banks", Rule::TooManyBanks, format!("SRV-58: {total} banks; a {} sound names at most {}", family.name(), family.most_banks())));
        }
        for (index, handle) in banks.iter().enumerate() {
            let bank = self
                .banks
                .get(handle)
                .ok_or_else(|| refuse(format!("banks[{index}]"), Rule::UnknownBank, format!("MIDI-57: handle {handle} holds no bank")))?;
            if bank.family != family {
                return Err(refuse(
                    format!("banks[{index}]"),
                    Rule::WrongFamily,
                    format!("MIDI-57: bank {handle} is for {}, not {}", bank.family.name(), family.name()),
                ));
            }
        }
        Ok(())
    }

    /// The images of the banks `banks` names, bank index 0 first.
    fn images(&self, banks: &[u8]) -> Vec<Vec<u8>> {
        banks.iter().filter_map(|handle| self.banks.get(handle)).map(|bank| bank.image.clone()).collect()
    }

    /// Each target's records of the banks it has not been sent, in bank
    /// index order (SRV-61), noting them sent.
    fn banks_for(&mut self, connection: ConnectionId, banks: &[u8]) -> Vec<Vec<u8>> {
        let mut records = Vec::new();
        let sent = self.sent_banks.entry(connection).or_default();
        for handle in banks {
            if let Some(bank) = self.banks.get(handle)
                && sent.insert((*handle, bank.generation))
            {
                records.extend(bank.records.iter().cloned());
            }
        }
        records
    }

    /// The records of a bank (SRV-60).
    fn bank_records(family: Family, handle: u8, generation: u16, image: &[u8]) -> Result<Vec<Vec<u8>>, Refused> {
        let bank = Encoded::bank(handle, generation, family.codec(), image)
            .map_err(|error| refuse("bank", Rule::InvalidBank, format!("MIDI-55: a bank image of 1 to 16,384 bytes ({} given): {error:?}", image.len())))?;
        let mut records = alloc::vec![bank.object_record().map_err(encode_fault)?];
        records.extend(bank.data_records());
        Ok(records)
    }

    /// A bank message (SRV-60, SRV-62): checked, held and sent.
    pub(crate) fn bank(&mut self, synth: Synth, handle: u8, generation: u16, image: &[u8], targets: &[ConnectionId]) -> Result<Sent, Refused> {
        let family = synth.family;
        synth.check_bank(image).map_err(|fault| family_fault("bank", fault))?;
        let records = Self::bank_records(family, handle, generation, image)?;
        self.live.remove(&handle);
        self.banks.insert(handle, Bank { generation, family, records: records.clone(), image: image.to_vec() });
        let mut sent = Sent::default();
        for &connection in targets {
            self.sent_banks.entry(connection).or_default().insert((handle, generation));
            sent.add(connection, records.clone());
        }
        Ok(sent)
    }

    /// A stored sound (SRV-63, SRV-70, SRV-79): built whole, then sent.
    pub(crate) fn stored(&mut self, mut sound: Stored<'_>, targets: &[ConnectionId]) -> Result<Sent, Refused> {
        let family = sound.synth.family;
        let dumps = lift(family, &mut sound.timed, false)?;
        if dumps.len() > sound.lift.len() {
            return Err(refuse(
                "music",
                Rule::BankNotNamed,
                format!("MIDI-58, SRV-15: the music holds {} bank dumps to lift; lift_banks names {}", dumps.len(), sound.lift.len()),
            ));
        }
        if dumps.len() < sound.lift.len() {
            return Err(refuse(
                "lift_banks",
                Rule::TooLong,
                format!("SRV-15: lift_banks names {}; the music holds {} bank dumps", sound.lift.len(), dumps.len()),
            ));
        }
        if family != Family::Synth6581 {
            self.check_banks(family, &sound.settings.banks, dumps.len())?;
        }
        let mut lifted = Vec::new();
        for (dump, name) in dumps.iter().zip(sound.lift) {
            let image = sound.synth.lifted_bank(dump).map_err(|fault| family_fault("music", fault))?;
            lifted.push((*name, Self::bank_records(family, name.handle, name.generation, &image)?, image));
        }
        let last = sound.timed.iter().map(|event| event.tick).max();
        let last = match (last, sound.end_tick) {
            (Some(last), Some(end)) => Some(last.max(end)),
            (last, end) => last.or(end),
        };
        let blocks = stored_blocks(sound.length, last)?;
        if let Some(event) = sound.timed.iter().find(|event| event.tick / BLOCK_TICKS >= blocks) {
            return Err(refuse(
                &event.path,
                Rule::BeyondLength,
                format!("SRV-8, SRV-14: tick {} is in block {}; the sound has {blocks}", event.tick, event.tick / BLOCK_TICKS),
            ));
        }
        if let Some(end) = sound.end_tick
            && matches!(sound.length, SoundLength::LengthUs(_))
            && end / BLOCK_TICKS >= blocks
        {
            return Err(refuse("music", Rule::BeyondLength, format!("SRV-85: the file's end of track at tick {end} is past the sound's {blocks} blocks")));
        }
        // The sound's banks as its tracker reads them: those it names, then
        // those lifted from its music (MIDI-57, MIDI-58).
        let mut images = self.images(&sound.settings.banks);
        images.extend(lifted.iter().map(|(_, _, image)| image.clone()));
        let image_slices: Vec<&[u8]> = images.iter().map(Vec::as_slice).collect();
        let banks = Banks::new(&image_slices);
        let mut tracker = Tracker::new(sound.synth, &banks);
        // SRV-70: synth6581's and synth8580's own first event is their
        // restatement (S6581-45).
        let restate: Vec<(u8, Vec<Wire>)> = if family == Family::Synth6581 {
            Vec::new()
        } else {
            used_parts(sound.synth, &tracker, &sound.timed).into_iter().map(|part| (part, tracker.restate(part))).collect()
        };
        let mut written = Vec::with_capacity(usize::try_from(blocks).unwrap_or(0));
        let mut timed = core::mem::take(&mut sound.timed).into_iter().peekable();
        for index in 0..blocks {
            let mut block_events = Vec::new();
            while let Some(event) = timed.next_if(|event| event.tick / BLOCK_TICKS == index) {
                block_events.push(event);
            }
            let block_events = resolve_and_track(sound.synth, &mut tracker, &banks, block_events)?;
            let events: Vec<&Timed> = block_events.iter().collect();
            let opening: &[(u8, Vec<Wire>)] = if index == 0 { &restate } else { &[] };
            let (block, restated) = write_block(family, index, opening, &events)?;
            if restated.len() != opening.len() {
                return Err(refuse("music", Rule::BlockFull, "SRV-70, MIDI-99: block 0 has no room for every part's restatement"));
            }
            written.push(block);
        }
        let mut settings = sound.settings.clone();
        settings.banks.extend(lifted.iter().map(|(name, _, _)| name.handle));
        let mut encoded = Encoded::events(sound.handle, sound.generation, family.codec(), &written).map_err(encode_fault)?;
        settings.apply(sound.synth, &mut encoded);
        let mut records = alloc::vec![encoded.object_record().map_err(encode_fault)?];
        records.extend(encoded.data_records());
        // Only now, with nothing refused, does any table change (SRV-49).
        self.replaced(sound.handle);
        for (name, bank_records, image) in lifted.iter().cloned() {
            self.live.remove(&name.handle);
            self.banks.insert(name.handle, Bank { generation: name.generation, family, records: bank_records, image });
        }
        let mut sent = Sent::default();
        for &connection in targets {
            let mut out = self.banks_for(connection, &settings.banks);
            out.extend(records.iter().cloned());
            sent.add(connection, out);
        }
        let bytes: usize = written.iter().map(Vec::len).sum();
        sent.built = Some(SoundBuilt {
            kind: sound.kind,
            handle: sound.handle,
            generation: sound.generation,
            blocks: u16::try_from(blocks).unwrap_or(u16::MAX),
            bytes: u32::try_from(bytes).unwrap_or(u32::MAX),
            longest_block: encoded.block_bytes,
            banks_lifted: lifted.iter().map(|(name, _, _)| *name).collect(),
            dropped: sound.dropped,
        });
        Ok(sent)
    }

    /// A live sound announced (SRV-73).
    pub(crate) fn announce(
        &mut self,
        kind: SoundKind,
        synth: Synth,
        handle: u8,
        generation: u16,
        settings: Settings,
        targets: &[ConnectionId],
    ) -> Result<Sent, Refused> {
        let mut sent = Sent::default();
        if let Some(live) = self.live.get(&handle)
            && live.generation == generation
        {
            if live.kind != kind || live.settings != settings {
                return Err(refuse(
                    "",
                    Rule::LiveSoundChanged,
                    format!("SRV-73: handle {handle} generation {generation} is a live sound of another kind or settings"),
                ));
            }
            let object = live.object.clone();
            let banks = live.settings.banks.clone();
            let new: Vec<ConnectionId> = targets.iter().copied().filter(|connection| !live.announced.contains(connection)).collect();
            for connection in new {
                let mut out = self.banks_for(connection, &banks);
                out.push(object.clone());
                sent.add(connection, out);
                if let Some(live) = self.live.get_mut(&handle) {
                    live.announced.insert(connection);
                }
            }
            return Ok(sent);
        }
        if synth.family != Family::Synth6581 {
            self.check_banks(synth.family, &settings.banks, 0)?;
        }
        let mut encoded = Encoded::live_events(handle, generation, synth.family.codec()).map_err(encode_fault)?;
        settings.apply(synth, &mut encoded);
        let object = encoded.object_record().map_err(encode_fault)?;
        let images = self.images(&settings.banks);
        let tracker = {
            let image_slices: Vec<&[u8]> = images.iter().map(Vec::as_slice).collect();
            Tracker::new(synth, &Banks::new(&image_slices))
        };
        self.replaced(handle);
        for &connection in targets {
            let mut out = self.banks_for(connection, &settings.banks);
            out.push(object.clone());
            sent.add(connection, out);
        }
        let since = alloc::vec![0; tracker.parts()];
        self.live.insert(
            handle,
            Live {
                kind,
                synth,
                generation,
                settings,
                encoded,
                object,
                announced: targets.iter().copied().collect(),
                until_us: 0,
                latest_us: 0,
                next_write: 0,
                pending: Vec::new(),
                images,
                tracker,
                since,
                opened: false,
                held: VecDeque::new(),
                clock_start_ms: None,
            },
        );
        Ok(sent)
    }

    /// An events message (SRV-74 to SRV-77): `timed` its events placed,
    /// `first_time` its first event's time. Every check is made on a copy
    /// of the live sound, which replaces it only when nothing is refused
    /// (SRV-49).
    #[allow(clippy::too_many_arguments, reason = "an events message's fields, unpacked once by its caller")]
    #[allow(clippy::too_many_lines, reason = "SRV-74's steps in order, each refusing before anything changes")]
    pub(crate) fn events(
        &mut self,
        live_kind: SoundKind,
        handle: u8,
        generation: u16,
        until_us: u64,
        mut timed: Vec<Timed>,
        firsts: (Option<u64>, Option<bool>),
        targets: &[ConnectionId],
        now_ms: u64,
    ) -> Result<Sent, Refused> {
        let (first_time, opening) = firsts;
        let mut live = match self.live.get(&handle) {
            Some(live) if live.generation == generation && live.kind == live_kind => live.clone(),
            _ => {
                return Err(refuse(
                    "handle",
                    Rule::UnknownLiveSound,
                    format!("SRV-72: handle {handle} generation {generation} is no live sound of kind {}", live_kind.name()),
                ));
            }
        };
        if until_us < live.until_us {
            return Err(refuse("until_us", Rule::UntilMovedBack, format!("SRV-74: until_us {until_us} is before the last, {}", live.until_us)));
        }
        if until_us.saturating_sub(live.until_us) > MOST_LIVE_ADVANCE_US {
            return Err(refuse(
                "until_us",
                Rule::AdvanceTooFar,
                format!("MIDI-20: until_us moves {} microseconds, at most {MOST_LIVE_ADVANCE_US}", until_us.saturating_sub(live.until_us)),
            ));
        }
        if let Some(first) = first_time
            && first < live.latest_us
        {
            return Err(refuse("events[0]", Rule::NotAscending, format!("SRV-75: {first} us is before the sound's latest event, at {} us", live.latest_us)));
        }
        let family = live.synth.family;
        lift(family, &mut timed, true)?;
        if let Some(late) = timed.iter().find(|event| event.tick / BLOCK_TICKS < live.next_write) {
            return Err(refuse(&late.path, Rule::LateEvent, format!("MIDI-20: tick {} is in block {}, already sent", late.tick, late.tick / BLOCK_TICKS)));
        }
        if family == Family::Synth6581 && !live.opened {
            match opening {
                Some(true) => live.opened = true,
                Some(false) => {
                    return Err(refuse("events[0]", Rule::NoOpeningUpdate, "S6581-45, S6581-54 item 8: the first event is a full register update at tick 0"));
                }
                None if (live.next_write.saturating_add(1)).saturating_mul(BLOCK_US) <= until_us => {
                    return Err(refuse("events", Rule::NoOpeningUpdate, "S6581-45, S6581-54 item 8: the first block opens with a full register update"));
                }
                None => {}
            }
        }
        if let Some(last) = timed.last() {
            live.latest_us = live.latest_us.max(last.tick.saturating_mul(TICK_US));
        }
        if let Some(first) = first_time {
            live.latest_us = live.latest_us.max(first);
        }
        live.pending.extend(timed);
        live.until_us = until_us;
        let named: Vec<ConnectionId> = targets.to_vec();
        let images = core::mem::take(&mut live.images);
        let image_slices: Vec<&[u8]> = images.iter().map(Vec::as_slice).collect();
        let banks = Banks::new(&image_slices);
        while (live.next_write.saturating_add(1)).saturating_mul(BLOCK_US) <= until_us {
            let index = live.next_write;
            let split = live.pending.iter().take_while(|event| event.tick / BLOCK_TICKS == index).count();
            let events: Vec<Timed> = live.pending.drain(..split).collect();
            // SRV-69, S6581-46: every part's restatement in the first block,
            // then each where the blocks have room once 25 have passed,
            // carrying the state before this block's events.
            let candidates: Vec<(u8, Vec<Wire>)> = (0_u8..)
                .zip(&live.since)
                .filter(|(_, since)| index == 0 || **since >= RESTATE_AIM_BLOCKS)
                .map(|(part, _)| (part, live.tracker.restate(part)))
                .collect();
            let events = resolve_and_track(live.synth, &mut live.tracker, &banks, events)?;
            let references: Vec<&Timed> = events.iter().collect();
            let (block, restated) = write_block(family, index, &candidates, &references)?;
            for (part, since) in (0_u8..).zip(live.since.iter_mut()) {
                if restated.contains(&part) {
                    *since = 0;
                } else {
                    *since = since.saturating_add(1);
                    if *since >= RESTATE_EVERY_BLOCKS {
                        return Err(refuse(
                            "events",
                            Rule::NoRoomToRestate,
                            format!("MIDI-98: {RESTATE_EVERY_BLOCKS} blocks up to block {index} leave no room to restate part {part}"),
                        ));
                    }
                }
            }
            live.encoded.live_event_records(0, &[&block]).map_err(encode_fault)?;
            live.held.push_back((index, block, named.clone()));
            live.next_write = index.saturating_add(1);
        }
        // Nothing refused: the copy replaces the live sound.
        drop(image_slices);
        live.images = images;
        let mut sent = Sent::default();
        let new: Vec<ConnectionId> = named.iter().copied().filter(|connection| !live.announced.contains(connection)).collect();
        let banks = live.settings.banks.clone();
        let object = live.object.clone();
        live.announced.extend(new.iter().copied());
        self.live.insert(handle, live);
        for connection in new {
            let mut out = self.banks_for(connection, &banks);
            out.push(object.clone());
            sent.add(connection, out);
        }
        for (connection, records) in self.due(now_ms) {
            sent.add(connection, records);
        }
        Ok(sent)
    }

    /// The live blocks due now (SRV-77): each held block sent once its time
    /// is within [`LIVE_SEND_LEAD_BLOCKS`] of the live sound's clock, which
    /// starts when its first block is sent.
    pub(crate) fn due(&mut self, now_ms: u64) -> Vec<(ConnectionId, Vec<Vec<u8>>)> {
        let mut out: Vec<(ConnectionId, Vec<Vec<u8>>)> = Vec::new();
        for live in self.live.values_mut() {
            let start = match (live.clock_start_ms, live.held.is_empty()) {
                (Some(start), _) => start,
                (None, false) => *live.clock_start_ms.insert(now_ms),
                (None, true) => continue,
            };
            while let Some((index, _, _)) = live.held.front() {
                let due = start.saturating_add(index.saturating_mul(BLOCK_MS));
                if due > now_ms.saturating_add(LIVE_SEND_LEAD_BLOCKS.saturating_mul(BLOCK_MS)) {
                    break;
                }
                let Some((index, block, targets)) = live.held.pop_front() else { break };
                let number = u16::try_from(index % BLOCK_NUMBER_MODULUS).unwrap_or(0);
                let Ok(records) = live.encoded.live_event_records(number, &[&block]) else { continue };
                for connection in targets {
                    match out.iter_mut().find(|(kept, _)| *kept == connection) {
                        Some((_, kept)) => kept.extend(records.iter().cloned()),
                        None => out.push((connection, records.clone())),
                    }
                }
            }
        }
        out
    }

    /// When the next held live block is due, in milliseconds (SRV-77).
    pub(crate) fn next_due(&self) -> Option<u64> {
        self.live
            .values()
            .filter_map(|live| {
                let (index, _, _) = live.held.front()?;
                let start = live.clock_start_ms?;
                Some(start.saturating_add(index.saturating_mul(BLOCK_MS)).saturating_sub(LIVE_SEND_LEAD_BLOCKS.saturating_mul(BLOCK_MS)))
            })
            .min()
    }
}
