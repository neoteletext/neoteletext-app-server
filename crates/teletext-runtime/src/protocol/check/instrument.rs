//! The synthesizers' program-facing shape (server.md SRV-49 item 2,
//! SRV-53): what a schema cannot state of a synthesizer sound, checked once
//! it is read and before anything is built. Every rule of a family's
//! messages, patches and banks is the engine's and is checked where the
//! server builds them (`crate::sound`); here are only the lengths of lists
//! and bytes, times, the order of events, the banks a sound names and its
//! lifted banks' names, and the text of a synth7 voice's name.

use alloc::format;
use alloc::string::ToString;

use super::{Checked, Path, check_base64, check_connections, fault};
use crate::protocol::{
    BytesForm, LEAST_ASID_BYTES, LEAST_MIDI_FILE_BYTES, LEAST_SYSTEM_EXCLUSIVE_BYTES, MOST_ASID_BYTES, MOST_BANK_IMAGE_BYTES, MOST_EVENT_BYTES,
    MOST_LIFTED_BANKS, MOST_LIVE_EVENTS, MOST_MIDI_FILE_BYTES, MOST_PATCH_BYTES, MOST_SOUND_BANKS, MOST_STORED_EVENTS, MOST_STORED_TIME_US, NoteFamily, Rule,
    SYNTH7_BANK_VOICES, SYNTH8_BANK_PATCHES, SYNTH53_BANK_PROGRAMS, SharedEvent, Sound, SplitEvent, Synth7Family, Synth7Patch, Synth8Family, Synth53Family,
    Synth60Family, Synth80Family, Synth106Family, Synth6581Event, Synth6581Music, SynthDFamily,
};
use crate::protocol::{MOST_BANK_PATCHES, Synth7Bank, Synth8Bank, Synth53Bank, Synth60Bank, Synth80Bank, Synth106Bank, SynthDBank};

/// The most characters of a synth7 voice's name (S7-28, parameters 145 to
/// 154).
const SYNTH7_NAME_CHARACTERS: usize = 10;
/// The characters a synth7 voice's name may hold in the fields form:
/// printable ASCII (SRV-26).
const SYNTH7_NAME_CHARACTER: core::ops::RangeInclusive<char> = ' '..='~';

/// Checks a synthesizer sound's shape; `None` for a sound of another kind.
#[allow(clippy::too_many_lines, reason = "one arm for each of the 42 synthesizer kinds")]
pub(super) fn check(root: &Path<'_>, sound: &Sound) -> Option<Checked> {
    Some(match sound {
        Sound::Synth6581(stored) | Sound::Synth8580(stored) => stored_shape(root, &stored.connections, &[], &[]).and_then(|()| match &stored.music {
            Synth6581Music::Events(list) => synth6581_events(&root.field("music").field("events"), &list.events, Some(MOST_STORED_EVENTS), None),
            Synth6581Music::MidiFile(file) => midi_file(&root.field("music").field("bytes"), &file.bytes),
        }),
        Sound::Synth6581Live(live) | Sound::Synth8580Live(live) => check_connections(&root.field("connections"), &live.connections),
        Sound::Synth6581Events(events) | Sound::Synth8580Events(events) => check_connections(&root.field("connections"), &events.connections)
            .and_then(|()| synth6581_events(&root.field("events"), &events.events, None, Some(events.until_us.get()))),
        Sound::Synth7(stored) => stored_sound::<Synth7Family>(root, &stored.connections, &stored.banks, &stored.lift_banks, &music_of_7(stored)),
        Sound::Synth106(stored) => stored_sound::<Synth106Family>(root, &stored.connections, &stored.banks, &stored.lift_banks, &music_of_106(stored)),
        Sound::Synth60(stored) => stored_sound::<Synth60Family>(root, &stored.connections, &stored.banks, &stored.lift_banks, &music_of_60(stored)),
        Sound::SynthD(stored) => stored_sound::<SynthDFamily>(root, &stored.connections, &stored.banks, &stored.lift_banks, &music_of_d(stored)),
        Sound::Synth53(stored) | Sound::Synth52(stored) | Sound::Synth51(stored) => {
            stored_sound::<Synth53Family>(root, &stored.connections, &stored.banks, &stored.lift_banks, &music_of_53(stored))
        }
        Sound::Synth8(stored) => stored_sound::<Synth8Family>(root, &stored.connections, &stored.banks, &stored.lift_banks, &music_of_8(stored)),
        Sound::Synth80(stored) => stored_sound::<Synth80Family>(root, &stored.connections, &stored.banks, &stored.lift_banks, &music_of_80(stored)),
        Sound::Synth7Live(live) => live_banks(root, &live.connections, &live.banks),
        Sound::Synth106Live(live) => live_banks(root, &live.connections, &live.banks),
        Sound::Synth60Live(live) => live_banks(root, &live.connections, &live.banks),
        Sound::SynthDLive(live) => live_banks(root, &live.connections, &live.banks),
        Sound::Synth53Live(live) | Sound::Synth52Live(live) | Sound::Synth51Live(live) => live_banks(root, &live.connections, &live.banks),
        Sound::Synth8Live(live) => live_banks(root, &live.connections, &live.banks),
        Sound::Synth80Live(live) => live_banks(root, &live.connections, &live.banks),
        Sound::Synth7Local(local) => local_sound::<Synth7Family>(root, &local.connections, &local.banks, &local.setup),
        Sound::Synth106Local(local) => local_sound::<Synth106Family>(root, &local.connections, &local.banks, &local.setup),
        Sound::Synth60Local(local) => local_sound::<Synth60Family>(root, &local.connections, &local.banks, &local.setup),
        Sound::SynthDLocal(local) => local_sound::<SynthDFamily>(root, &local.connections, &local.banks, &local.setup),
        Sound::Synth53Local(local) | Sound::Synth52Local(local) | Sound::Synth51Local(local) => {
            local_sound::<Synth53Family>(root, &local.connections, &local.banks, &local.setup)
        }
        Sound::Synth8Local(local) => local_sound::<Synth8Family>(root, &local.connections, &local.banks, &local.setup),
        Sound::Synth80Local(local) => local_sound::<Synth80Family>(root, &local.connections, &local.banks, &local.setup),
        Sound::Synth7Events(events) => live_events::<Synth7Family>(root, &events.connections, &events.events, events.until_us.get()),
        Sound::Synth106Events(events) => live_events::<Synth106Family>(root, &events.connections, &events.events, events.until_us.get()),
        Sound::Synth60Events(events) => live_events::<Synth60Family>(root, &events.connections, &events.events, events.until_us.get()),
        Sound::SynthDEvents(events) => live_events::<SynthDFamily>(root, &events.connections, &events.events, events.until_us.get()),
        Sound::Synth53Events(events) | Sound::Synth52Events(events) | Sound::Synth51Events(events) => {
            live_events::<Synth53Family>(root, &events.connections, &events.events, events.until_us.get())
        }
        Sound::Synth8Events(events) => live_events::<Synth8Family>(root, &events.connections, &events.events, events.until_us.get()),
        Sound::Synth80Events(events) => live_events::<Synth80Family>(root, &events.connections, &events.events, events.until_us.get()),
        Sound::Synth7Bank(bank) => check_connections(&root.field("connections"), &bank.connections).and_then(|()| synth7_bank(&root.field("bank"), &bank.bank)),
        Sound::Synth106Bank(bank) => check_connections(&root.field("connections"), &bank.connections).and_then(|()| match &bank.bank {
            Synth106Bank::Patches(list) => patch_count(&root.field("bank").field("patches"), list.patches.len(), MOST_BANK_PATCHES).and_then(|()| {
                patch_bytes_each(
                    &root.field("bank").field("patches"),
                    list.patches.iter().map(|patch| match patch {
                        crate::protocol::Synth106Patch::Bytes(bytes) => Some(bytes),
                        crate::protocol::Synth106Patch::Fields(_) => None,
                    }),
                )
            }),
            Synth106Bank::Bytes(bytes) => bank_bytes(&root.field("bank"), bytes),
        }),
        Sound::Synth60Bank(bank) => check_connections(&root.field("connections"), &bank.connections).and_then(|()| match &bank.bank {
            Synth60Bank::Patches(list) => patch_count(&root.field("bank").field("patches"), list.patches.len(), MOST_BANK_PATCHES).and_then(|()| {
                patch_bytes_each(
                    &root.field("bank").field("patches"),
                    list.patches.iter().map(|patch| match patch {
                        crate::protocol::Synth60Patch::Bytes(bytes) => Some(bytes),
                        crate::protocol::Synth60Patch::Fields(_) => None,
                    }),
                )
            }),
            Synth60Bank::Bytes(bytes) => bank_bytes(&root.field("bank"), bytes),
        }),
        Sound::SynthDBank(bank) => check_connections(&root.field("connections"), &bank.connections).and_then(|()| match &bank.bank {
            SynthDBank::Patches(list) => patch_count(&root.field("bank").field("patches"), list.patches.len(), MOST_BANK_PATCHES).and_then(|()| {
                patch_bytes_each(
                    &root.field("bank").field("patches"),
                    list.patches.iter().map(|patch| match patch {
                        crate::protocol::SynthDPatch::Bytes(bytes) => Some(bytes),
                        crate::protocol::SynthDPatch::Fields(_) => None,
                    }),
                )
            }),
            SynthDBank::Bytes(bytes) => bank_bytes(&root.field("bank"), bytes),
        }),
        Sound::Synth53Bank(bank) | Sound::Synth52Bank(bank) | Sound::Synth51Bank(bank) => check_connections(&root.field("connections"), &bank.connections)
            .and_then(|()| match &bank.bank {
                Synth53Bank::Patches(list) => patch_count(&root.field("bank").field("patches"), list.patches.len(), SYNTH53_BANK_PROGRAMS).and_then(|()| {
                    patch_bytes_each(
                        &root.field("bank").field("patches"),
                        list.patches.iter().map(|patch| match patch {
                            crate::protocol::Synth53Patch::Bytes(bytes) => Some(bytes),
                            crate::protocol::Synth53Patch::Fields(_) => None,
                        }),
                    )
                }),
                Synth53Bank::Bytes(bytes) => bank_bytes(&root.field("bank"), bytes),
            }),
        Sound::Synth8Bank(bank) => check_connections(&root.field("connections"), &bank.connections).and_then(|()| match &bank.bank {
            Synth8Bank::Patches(list) => patch_count(&root.field("bank").field("patches"), list.patches.len(), SYNTH8_BANK_PATCHES).and_then(|()| {
                patch_bytes_each(
                    &root.field("bank").field("patches"),
                    list.patches.iter().map(|patch| match patch {
                        crate::protocol::Synth8Patch::Bytes(bytes) => Some(bytes),
                        crate::protocol::Synth8Patch::Fields(_) => None,
                    }),
                )
            }),
            Synth8Bank::Bytes(bytes) => bank_bytes(&root.field("bank"), bytes),
        }),
        Sound::Synth80Bank(bank) => check_connections(&root.field("connections"), &bank.connections).and_then(|()| match &bank.bank {
            Synth80Bank::Patches(list) => patch_count(&root.field("bank").field("patches"), list.patches.len(), MOST_BANK_PATCHES).and_then(|()| {
                patch_bytes_each(
                    &root.field("bank").field("patches"),
                    list.patches.iter().map(|patch| match patch {
                        crate::protocol::Synth80Patch::Bytes(bytes) => Some(bytes),
                        crate::protocol::Synth80Patch::Fields(_) => None,
                    }),
                )
            }),
            Synth80Bank::Bytes(bytes) => bank_bytes(&root.field("bank"), bytes),
        }),
        _ => return None,
    })
}

/// A stored sound's music, as the shape check reads it.
enum MusicShape<'a, E> {
    /// Timed events.
    Events(&'a [E]),
    /// A Standard MIDI File's bytes.
    File(&'a str),
}

macro_rules! music_of {
    ($name:ident, $sound:ty, $music:ident, $family:ty) => {
        fn $name(sound: &$sound) -> MusicShape<'_, <$family as NoteFamily>::Event> {
            match &sound.music {
                crate::protocol::$music::Events(list) => MusicShape::Events(&list.events),
                crate::protocol::$music::MidiFile(file) => MusicShape::File(&file.bytes),
            }
        }
    };
}

music_of!(music_of_7, crate::protocol::Synth7Sound, Synth7Music, Synth7Family);
music_of!(music_of_106, crate::protocol::Synth106Sound, Synth106Music, Synth106Family);
music_of!(music_of_60, crate::protocol::Synth60Sound, Synth60Music, Synth60Family);
music_of!(music_of_d, crate::protocol::SynthDSound, SynthDMusic, SynthDFamily);
music_of!(music_of_53, crate::protocol::Synth53Sound, Synth53Music, Synth53Family);
music_of!(music_of_8, crate::protocol::Synth8Sound, Synth8Music, Synth8Family);
music_of!(music_of_80, crate::protocol::Synth80Sound, Synth80Music, Synth80Family);

/// What every stored sound's names must meet: its connections, and its
/// banks and lifted banks within the most a sound names (SRV-15, SRV-16).
fn stored_shape(root: &Path<'_>, connections: &crate::protocol::Connections, banks: &[u8], lift: &[crate::protocol::BankName]) -> Checked {
    check_connections(&root.field("connections"), connections)?;
    if banks.len() > MOST_SOUND_BANKS {
        return Err(fault(&root.field("banks"), Rule::TooLong, format!("SRV-58: {} banks, at most {MOST_SOUND_BANKS}", banks.len())));
    }
    if lift.len() > MOST_LIFTED_BANKS {
        return Err(fault(&root.field("lift_banks"), Rule::TooLong, format!("SRV-15: {} names, at most {MOST_LIFTED_BANKS}", lift.len())));
    }
    for (index, name) in lift.iter().enumerate() {
        if lift.iter().take(index).any(|earlier| earlier.handle == name.handle) || banks.contains(&name.handle) {
            return Err(fault(&root.field("lift_banks").item(index), Rule::Duplicate, format!("SRV-15: handle {} is named twice", name.handle)));
        }
    }
    Ok(())
}

fn stored_sound<F: NoteFamily>(
    root: &Path<'_>,
    connections: &crate::protocol::Connections,
    banks: &[u8],
    lift: &[crate::protocol::BankName],
    music: &MusicShape<'_, F::Event>,
) -> Checked
where
    F::PatchEvent: PatchShape,
{
    stored_shape(root, connections, banks, lift)?;
    if banks.is_empty() && lift.is_empty() {
        return Err(fault(&root.field("banks"), Rule::TooShort, "SRV-16, MIDI-57: a sound names at least one bank, in banks or lift_banks".to_string()));
    }
    match music {
        MusicShape::Events(events) => note_events::<F>(&root.field("music").field("events"), events, Some(MOST_STORED_EVENTS), None),
        MusicShape::File(bytes) => midi_file(&root.field("music").field("bytes"), bytes),
    }
}

fn live_banks(root: &Path<'_>, connections: &crate::protocol::Connections, banks: &[u8]) -> Checked {
    check_connections(&root.field("connections"), connections)?;
    if banks.is_empty() {
        return Err(fault(&root.field("banks"), Rule::TooShort, "SRV-17: a live sound names at least one bank".to_string()));
    }
    if banks.len() > MOST_SOUND_BANKS {
        return Err(fault(&root.field("banks"), Rule::TooLong, format!("SRV-58: {} banks, at most {MOST_SOUND_BANKS}", banks.len())));
    }
    Ok(())
}

/// A local sound (page-rules.md RULE-61): its banks as a live sound's, and
/// a setup of at most 64 events, each at time 0, since the setup is applied
/// at once when the instrument is made.
fn local_sound<F: NoteFamily>(root: &Path<'_>, connections: &crate::protocol::Connections, banks: &[u8], setup: &[F::Event]) -> Checked
where
    F::PatchEvent: PatchShape,
{
    live_banks(root, connections, banks)?;
    let path = root.field("setup");
    note_events::<F>(&path, setup, Some(crate::protocol::MOST_SETUP_EVENTS), None)?;
    match setup.iter().position(|event| F::time_us(event) != 0) {
        Some(late) => Err(fault(&path.item(late).field("time_us"), Rule::OutOfRange, "RULE-61: every event of a local sound's setup is at time 0".to_string())),
        None => Ok(()),
    }
}

fn live_events<F: NoteFamily>(root: &Path<'_>, connections: &crate::protocol::Connections, events: &[F::Event], until_us: u64) -> Checked
where
    F::PatchEvent: PatchShape,
{
    check_connections(&root.field("connections"), connections)?;
    note_events::<F>(&root.field("events"), events, Some(MOST_LIVE_EVENTS), Some(until_us))
}

/// The shape of an inline patch: its bytes form's length, and for synth7
/// its voice's name (SRV-21, SRV-26).
pub(crate) trait PatchShape {
    /// Checks it at `path`, the patch's own.
    fn shape(&self, path: &Path<'_>) -> Checked;
}

macro_rules! patch_shape {
    ($($event:ident: $patch:ident),+) => {
        $(impl PatchShape for crate::protocol::$event {
            fn shape(&self, path: &Path<'_>) -> Checked {
                match &self.patch {
                    crate::protocol::$patch::Bytes(bytes) => bytes_within(&path.field("bytes"), &bytes.bytes, 1, MOST_PATCH_BYTES),
                    crate::protocol::$patch::Fields(_) => Ok(()),
                }
            }
        })+
    };
}

patch_shape!(
    Synth106PatchEvent: Synth106Patch,
    Synth60PatchEvent: Synth60Patch,
    SynthDPatchEvent: SynthDPatch,
    Synth53PatchEvent: Synth53Patch,
    Synth8PatchEvent: Synth8Patch,
    Synth80PatchEvent: Synth80Patch
);

impl PatchShape for crate::protocol::Synth7PatchEvent {
    fn shape(&self, path: &Path<'_>) -> Checked {
        synth7_patch(path, &self.patch)
    }
}

/// A time's order and bounds: not before the one before it (SRV-14,
/// SRV-18), a stored sound's within two minutes (SRV-8), a live message's
/// before its `until_us` (SRV-74).
fn check_time(path: &Path<'_>, index: usize, time_us: u64, previous: u64, until_us: Option<u64>) -> Checked {
    if time_us < previous {
        return Err(fault(path, Rule::NotAscending, format!("SRV-14: event {index} at {time_us} us is before the one before it, at {previous} us")));
    }
    match until_us {
        None if time_us >= MOST_STORED_TIME_US => {
            Err(fault(&path.field("time_us"), Rule::OutOfRange, format!("SRV-8: {time_us} us; a stored sound's times are below {MOST_STORED_TIME_US}")))
        }
        Some(until) if time_us >= until => {
            Err(fault(&path.field("time_us"), Rule::NotBeforeUntil, format!("SRV-74: event {index} at {time_us} us is not before until_us {until}")))
        }
        _ => Ok(()),
    }
}

fn event_count(path: &Path<'_>, count: usize, most: Option<usize>) -> Checked {
    let most = most.unwrap_or(MOST_LIVE_EVENTS);
    if count > most {
        return Err(fault(path, Rule::TooLong, format!("SRV-14, SRV-18: {count} events, at most {most}")));
    }
    Ok(())
}

fn note_events<F: NoteFamily>(path: &Path<'_>, events: &[F::Event], most: Option<usize>, until_us: Option<u64>) -> Checked
where
    F::PatchEvent: PatchShape,
{
    event_count(path, events.len(), most)?;
    let mut previous = 0;
    for (index, event) in events.iter().enumerate() {
        let at = path.item(index);
        let time_us = F::time_us(event);
        check_time(&at, index, time_us, previous, until_us)?;
        previous = time_us;
        match F::split(event) {
            SplitEvent::Shared(SharedEvent::SystemExclusive(event)) => {
                bytes_within(&at.field("bytes"), &event.bytes, LEAST_SYSTEM_EXCLUSIVE_BYTES, MOST_EVENT_BYTES)?;
            }
            SplitEvent::Shared(SharedEvent::Raw(event)) => bytes_within(&at.field("bytes"), &event.bytes, 1, MOST_EVENT_BYTES)?,
            SplitEvent::Patch(event) => event.shape(&at.field("patch"))?,
            SplitEvent::Shared(_) | SplitEvent::Parameter(_) => {}
        }
    }
    Ok(())
}

fn synth6581_events(path: &Path<'_>, events: &[Synth6581Event], most: Option<usize>, until_us: Option<u64>) -> Checked {
    event_count(path, events.len(), most)?;
    let mut previous = 0;
    for (index, event) in events.iter().enumerate() {
        let at = path.item(index);
        check_time(&at, index, event.time_us(), previous, until_us)?;
        previous = event.time_us();
        match event {
            Synth6581Event::RegisterUpdate(update) => {
                if update.registers.by_id().iter().all(Option::is_none) {
                    return Err(fault(&at.field("registers"), Rule::TooShort, "SRV-12, S6581-16: an update writes at least one register".to_string()));
                }
            }
            Synth6581Event::Asid(message) => bytes_within(&at.field("bytes"), &message.bytes, LEAST_ASID_BYTES, MOST_ASID_BYTES)?,
        }
    }
    Ok(())
}

/// Base64 of `least` to `most` bytes once decoded (SRV-6).
fn bytes_within(path: &Path<'_>, text: &str, least: usize, most: usize) -> Checked {
    let bytes = check_base64(path, text, most)?;
    if bytes.len() < least {
        return Err(fault(path, Rule::TooShort, format!("SRV-6: {} bytes, at least {least}", bytes.len())));
    }
    Ok(())
}

fn midi_file(path: &Path<'_>, bytes: &str) -> Checked {
    let decoded = check_base64(path, bytes, MOST_MIDI_FILE_BYTES).map_err(|mut fault| {
        if fault.rule == Rule::TooLong {
            fault.rule = Rule::MidiFileTooLarge;
            fault.detail = format!("MIDI-119: {}", fault.detail);
        }
        fault
    })?;
    if decoded.len() < LEAST_MIDI_FILE_BYTES {
        return Err(fault(path, Rule::MidiFileUnsupported, format!("MIDI-113: {} bytes hold no header chunk of 14", decoded.len())));
    }
    Ok(())
}

fn patch_count(path: &Path<'_>, count: usize, most: usize) -> Checked {
    if count == 0 {
        return Err(fault(path, Rule::TooShort, "SRV-20: a bank holds at least one patch".to_string()));
    }
    if count > most {
        return Err(fault(path, Rule::TooLong, format!("SRV-20: {count} patches, at most {most}")));
    }
    Ok(())
}

fn patch_bytes_each<'a>(path: &Path<'_>, patches: impl Iterator<Item = Option<&'a BytesForm>>) -> Checked {
    for (index, bytes) in patches.enumerate() {
        if let Some(bytes) = bytes {
            bytes_within(&path.item(index).field("bytes"), &bytes.bytes, 1, MOST_PATCH_BYTES)?;
        }
    }
    Ok(())
}

fn bank_bytes(path: &Path<'_>, bytes: &BytesForm) -> Checked {
    bytes_within(&path.field("bytes"), &bytes.bytes, 1, MOST_BANK_IMAGE_BYTES)
}

fn synth7_bank(path: &Path<'_>, bank: &Synth7Bank) -> Checked {
    match bank {
        Synth7Bank::Patches(list) => {
            patch_count(&path.field("patches"), list.patches.len(), SYNTH7_BANK_VOICES)?;
            for (index, patch) in list.patches.iter().enumerate() {
                synth7_patch(&path.field("patches").item(index), patch)?;
            }
            Ok(())
        }
        Synth7Bank::Bytes(bytes) => bank_bytes(path, bytes),
    }
}

/// A synth7 voice's name in the fields form: at most ten printable ASCII
/// characters (SRV-26).
pub(crate) fn synth7_patch(path: &Path<'_>, voice_patch: &Synth7Patch) -> Checked {
    match voice_patch {
        Synth7Patch::Fields(voice) => {
            let name = &voice.name;
            if let Some(bad) = name.chars().find(|character| !SYNTH7_NAME_CHARACTER.contains(character)) {
                return Err(fault(&path.field("name"), Rule::ForbiddenCharacter, format!("SRV-26: U+{:04X}; a name is U+0020 to U+007E", u32::from(bad))));
            }
            if name.chars().count() > SYNTH7_NAME_CHARACTERS {
                return Err(fault(
                    &path.field("name"),
                    Rule::TooLong,
                    format!("SRV-26: {} characters, at most {SYNTH7_NAME_CHARACTERS}", name.chars().count()),
                ));
            }
            Ok(())
        }
        Synth7Patch::Bytes(bytes) => bytes_within(&path.field("bytes"), &bytes.bytes, 1, MOST_PATCH_BYTES),
    }
}
