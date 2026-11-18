//! `sound` (the engine's audio profile, DOC-1785) in program protocol 13:
//! Opus and PCM sounds and the commands as before, and the eleven
//! synthesizers' typed interface of neoteletext-common's
//! `docs/audio/server.md` (SRV-1 to SRV-48): a stored sound, a live sound
//! and an events message for every name, and a bank for every name whose
//! family has banks, 42 kinds in all (SRV-3). A program gives music as
//! timed MIDI events or a Standard MIDI File, a patch as named fields or
//! the adopted interface's bytes, and a bank as a list of patches or the
//! bank image (DEC-6462); the server builds every block, bank and record
//! (`crate::sound`).
//!
//! Every tagged choice (`kind`, `event`, `form`) is read by following the
//! tag by hand to the variant's own type, as the reader follows `type`,
//! so that a fault inside keeps its path (SRV-49 item 1).

use alloc::string::String;
use alloc::vec::Vec;

use schemars::JsonSchema;
use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::read::{Fault, INNER_PATH, typed};
use super::{Base64, Connections, PageNumber, SAFE_INTEGER_LIMIT};

mod fields;
mod ranged;

pub use fields::*;
pub use ranged::{Data, Data14, MidiChannel, U8In, U16In, U64In};

// ----------------------------------------------------------- the bounds

/// The longest a sound may be: two minutes at 48 kHz, the browser's own
/// limit for one sound.
pub const MOST_SAMPLES: usize = 48_000 * 120;
/// The sample rates of the audio profile (the engine's `audio.md`).
pub const SAMPLE_RATES: [u32; 8] = [8_000, 11_025, 16_000, 22_050, 24_000, 32_000, 44_100, 48_000];
/// The highest duck step (`audio.md` section 4.4: 0 to 6).
pub const MOST_DUCK: u8 = 6;
/// The last audio channel, 3 (`audio.md`).
const LAST_AUDIO_CHANNEL: u8 = 3;
/// The highest gain, 100.
const MOST_GAIN: u8 = neoteletext_audio::MAX_GAIN;
/// The longest Opus packet the profile's constant rate allows: 40 bytes
/// (decision 21; the engine's E.123).
pub const OPUS_MOST_PACKET_BYTES: u16 = 40;

/// A tick, 100 microseconds (MIDI-17: a tick is 0.1 millisecond).
pub const TICK_US: u64 = neoteletext_audio::midi::MICROSECONDS_PER_TICK;
/// The ticks of a block: the engine's (MIDI-25).
pub const BLOCK_TICKS: u64 = neoteletext_audio::midi::TICKS_PER_BLOCK as u64;
/// A block, 20,000 microseconds.
pub const BLOCK_US: u64 = TICK_US * BLOCK_TICKS;
/// The most blocks of a stored synthesizer sound: two minutes (MIDI-104),
/// the engine's.
pub const MOST_INSTRUMENT_BLOCKS: usize = neoteletext_audio::MOST_EVENT_SOUND_BLOCKS as usize;
/// The longest a stored sound's time may run, in microseconds: its most
/// blocks.
pub const MOST_STORED_TIME_US: u64 = MOST_INSTRUMENT_BLOCKS as u64 * BLOCK_US;
/// The most blocks one events message may complete (MIDI-20).
pub const MOST_LIVE_INSTRUMENT_BLOCKS: usize = 100;
/// The most one events message may advance `until_us` (MIDI-20).
pub const MOST_LIVE_ADVANCE_US: u64 = MOST_LIVE_INSTRUMENT_BLOCKS as u64 * BLOCK_US;
/// The most bytes of a block (MIDI-30), the engine's.
pub const MOST_BLOCK_BYTES: usize = neoteletext_audio::midi::MOST_BLOCK_BYTES;
/// The most events of a block (MIDI-31), the engine's.
pub const MOST_BLOCK_EVENTS: usize = neoteletext_audio::midi::MOST_EVENTS;
/// The most bytes of a stored sound's blocks (MIDI-41), the engine's.
pub const MOST_STORED_SOUND_BYTES: u32 = neoteletext_audio::MOST_EVENT_SOUND_BYTES;
/// The most events of a stored sound's music (MIDI-122).
pub const MOST_STORED_EVENTS: usize = 100_000;
/// The most events of one events message: 100 blocks of 256 (MIDI-122).
pub const MOST_LIVE_EVENTS: usize = MOST_LIVE_INSTRUMENT_BLOCKS * MOST_BLOCK_EVENTS;
/// The most bytes of a bank image (MIDI-55), the engine's.
pub const MOST_BANK_IMAGE_BYTES: usize = neoteletext_audio::midi::MOST_BANK_BYTES;
/// The most bytes an event's message may give: a bank dump to lift is the
/// longest message (SRV-90).
pub const MOST_EVENT_BYTES: usize = 16_384;
/// The most bytes of a patch's bytes form (MIDI-122).
pub const MOST_PATCH_BYTES: usize = 16_384;
/// The most patches of a bank (MIDI-56); each family's own lower most is
/// the engine's.
pub const MOST_BANK_PATCHES: usize = neoteletext_audio::midi::MOST_BANK_PATCHES;
/// The most banks a sound names (MIDI-57), the engine's.
pub const MOST_SOUND_BANKS: usize = neoteletext_audio::midi::MOST_BANKS;
/// The most bytes of a Standard MIDI File (MIDI-119).
pub const MOST_MIDI_FILE_BYTES: usize = 1_048_576;
/// The smallest Standard MIDI File: a header chunk, `MThd`, its 4-byte
/// length and 6 bytes (Standard MIDI Files 1.0, printed pages 3 and 4).
pub const LEAST_MIDI_FILE_BYTES: usize = 14;
/// The shortest system exclusive event: `F0`, one data byte, `F7`.
pub const LEAST_SYSTEM_EXCLUSIVE_BYTES: usize = 3;
/// The shortest ASID message: `F0 2D`, the command, `F7` (S6581-4).
pub const LEAST_ASID_BYTES: usize = 4;
/// The longest ASID message: the longest system exclusive message a block
/// carries (MIDI-28) with its `F0` (SRV-12).
pub const MOST_ASID_BYTES: usize = neoteletext_audio::midi::MOST_SYSTEM_EXCLUSIVE_BYTES + 1;
/// The most banks a stored sound may lift from its music: the most it may
/// name (SRV-15).
pub const MOST_LIFTED_BANKS: usize = MOST_SOUND_BANKS;

const _: () = assert!(BLOCK_US == 20_000);
const _: () = assert!(MOST_STORED_TIME_US == 120_000_000);
const _: () = assert!(MOST_LIVE_ADVANCE_US == 2_000_000);
const _: () = assert!(MOST_LIVE_EVENTS == 25_600);
const _: () = assert!(MOST_EVENT_BYTES == MOST_BANK_IMAGE_BYTES);
const _: () = assert!(MOST_INSTRUMENT_BLOCKS == 6_000);

// ------------------------------------------------- reading a tagged choice

/// The error that carries a fault inside a tagged choice out of serde,
/// with its path, as the reader's `fault_of` reads it back.
fn carried<E: de::Error>(fault: &Fault) -> E {
    E::custom(alloc::format!("{INNER_PATH}{}{INNER_PATH}{:?}{INNER_PATH}{}", fault.path, fault.rule, fault.detail))
}

/// Reads a JSON object whose tag `name` chooses its type, by `choose`,
/// from what is left once the tag is taken out (SRV-49 item 1).
fn tagged<'de, D: Deserializer<'de>, T>(
    deserializer: D,
    name: &'static str,
    choose: impl FnOnce(&str, Map<String, Value>) -> Result<T, Fault>,
) -> Result<T, D::Error> {
    let Value::Object(mut object) = Value::deserialize(deserializer)? else {
        return Err(de::Error::invalid_type(de::Unexpected::Other("not an object"), &"an object with its tag"));
    };
    let tag = match object.remove(name) {
        None => return Err(carried(&Fault::new("", super::Rule::MissingField, alloc::format!("missing field `{name}`")))),
        Some(Value::String(tag)) => tag,
        Some(_) => return Err(carried(&Fault::new(name, super::Rule::WrongType, alloc::format!("`{name}` is a string")))),
    };
    choose(&tag, object).map_err(|fault| carried(&fault))
}

/// A tag's value not in its closed list.
fn not_listed(name: &str, value: &str, listed: &[&str]) -> Fault {
    Fault::new(name, super::Rule::NotListed, alloc::format!("unknown variant `{value}`, expected one of {listed:?}"))
}

/// A tagged choice of newtype variants, read by [`tagged`].
macro_rules! tagged_choice {
    ($type:ident, $tag:literal, { $($name:literal => $variant:ident),+ $(,)? }) => {
        impl<'de> Deserialize<'de> for $type {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                tagged(deserializer, $tag, |tag, object| {
                    Ok(match tag {
                        $($name => Self::$variant(typed(object, "")?),)+
                        other => return Err(not_listed($tag, other, &[$($name),+])),
                    })
                })
            }
        }
    };
}

// ---------------------------------------------------------- the kinds

/// The kinds of `sound`, one list from which the [`Sound`] enum, the closed
/// list [`SoundKind`], the reader's dispatch on `kind` and each kind's name
/// are all made, so that they cannot disagree.
macro_rules! sound_kinds {
    ($($variant:ident($type:ident) = $name:literal, $doc:literal;)+) => {
        /// `sound`, by `kind` (the engine's audio profile, DOC-1785; server.md
        /// SRV-3 and SRV-46).
        // Read by the reader, which follows `kind` to the sound's own type
        // ([`read_sound`]).
        #[derive(Clone, PartialEq, Eq, Debug, JsonSchema)]
        #[serde(tag = "kind")]
        pub enum Sound {
            $(
                #[doc = $doc]
                #[serde(rename = $name)]
                $variant($type),
            )+
        }

        /// Every `kind` of `sound`: the 42 synthesizer kinds (server.md
        /// SRV-3), the six of Opus and PCM and the three commands (SRV-46).
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash, Serialize, Deserialize, JsonSchema)]
        pub enum SoundKind {
            $(
                #[doc = $doc]
                #[serde(rename = $name)]
                $variant,
            )+
        }

        impl SoundKind {
            /// Every kind, in the order of [`Sound`].
            pub const ALL: [Self; [$(stringify!($variant)),+].len()] = [$(Self::$variant),+];

            /// Its name, as `kind` gives it.
            #[must_use]
            pub const fn name(self) -> &'static str {
                match self {
                    $(Self::$variant => $name,)+
                }
            }
        }

        impl Sound {
            /// Its kind.
            #[must_use]
            pub const fn kind(&self) -> SoundKind {
                match self {
                    $(Self::$variant(_) => SoundKind::$variant,)+
                }
            }

            /// Who it is for.
            #[must_use]
            pub const fn connections(&self) -> &Connections {
                match self {
                    $(Self::$variant(sound) => &sound.connections,)+
                }
            }
        }

        /// The sound of `kind` from the rest of its object.
        pub(super) fn read_sound(kind: SoundKind, object: Map<String, Value>) -> Result<Sound, Fault> {
            Ok(match kind {
                $(SoundKind::$variant => Sound::$variant(typed(object, "")?),)+
            })
        }
    };
}

sound_kinds! {
    Opus(OpusSound) = "opus", "A stored Opus sound (decision 21).";
    OpusLive(OpusLiveSound) = "opus_live", "A live Opus sound's object.";
    OpusPackets(OpusPacketsSound) = "opus_packets", "A live Opus sound's next packets.";
    Object(ObjectSound) = "object", "A stored sound as 16-bit samples, which the server encodes as IMA ADPCM, for receivers that decode no Opus.";
    Live(LiveSound) = "live", "A live sound's object.";
    Blocks(BlocksSound) = "blocks", "A live sound's next blocks of samples.";
    Synth6581(Synth6581Sound) = "synth6581", "A stored synth6581 sound: ASID register updates, timed (server.md SRV-12, SRV-16).";
    Synth6581Live(Synth6581LiveSound) = "synth6581_live", "A live synth6581 sound, announced (SRV-17).";
    Synth6581Events(Synth6581EventsSound) = "synth6581_events", "A live synth6581 sound's next events (SRV-18).";
    Synth8580(Synth6581Sound) = "synth8580", "A stored synth8580 sound: ASID register updates, timed (SRV-12, SRV-16).";
    Synth8580Live(Synth6581LiveSound) = "synth8580_live", "A live synth8580 sound, announced (SRV-17).";
    Synth8580Events(Synth6581EventsSound) = "synth8580_events", "A live synth8580 sound's next events (SRV-18).";
    Synth7(Synth7Sound) = "synth7", "A stored synth7 sound: timed MIDI events or a Standard MIDI File (SRV-14, SRV-16).";
    Synth7Live(Synth7LiveSound) = "synth7_live", "A live synth7 sound, announced (SRV-17).";
    Synth7Events(Synth7EventsSound) = "synth7_events", "A live synth7 sound's next events (SRV-18).";
    Synth7Bank(Synth7BankSound) = "synth7_bank", "A bank of synth7 voices, which synth7 sounds name (SRV-19, SRV-27).";
    Synth106(Synth106Sound) = "synth106", "A stored synth106 sound (SRV-16).";
    Synth106Live(Synth106LiveSound) = "synth106_live", "A live synth106 sound, announced (SRV-17).";
    Synth106Events(Synth106EventsSound) = "synth106_events", "A live synth106 sound's next events (SRV-18).";
    Synth106Bank(Synth106BankSound) = "synth106_bank", "A bank of synth106 patches (SRV-19, SRV-32).";
    Synth60(Synth60Sound) = "synth60", "A stored synth60 sound (SRV-16).";
    Synth60Live(Synth60LiveSound) = "synth60_live", "A live synth60 sound, announced (SRV-17).";
    Synth60Events(Synth60EventsSound) = "synth60_events", "A live synth60 sound's next events (SRV-18).";
    Synth60Bank(Synth60BankSound) = "synth60_bank", "A bank of synth60 patches (SRV-19, SRV-32).";
    SynthD(SynthDSound) = "synthd", "A stored synthD sound (SRV-16).";
    SynthDLive(SynthDLiveSound) = "synthd_live", "A live synthD sound, announced (SRV-17).";
    SynthDEvents(SynthDEventsSound) = "synthd_events", "A live synthD sound's next events (SRV-18).";
    SynthDBank(SynthDBankSound) = "synthd_bank", "A bank of synthD patches (SRV-19, SRV-35).";
    Synth53(Synth53Sound) = "synth53", "A stored synth53 sound (SRV-16).";
    Synth53Live(Synth53LiveSound) = "synth53_live", "A live synth53 sound, announced (SRV-17).";
    Synth53Events(Synth53EventsSound) = "synth53_events", "A live synth53 sound's next events (SRV-18).";
    Synth53Bank(Synth53BankSound) = "synth53_bank", "A bank of synth53 programs, one group (SRV-19, SRV-38).";
    Synth52(Synth53Sound) = "synth52", "A stored synth52 sound (SRV-16).";
    Synth52Live(Synth53LiveSound) = "synth52_live", "A live synth52 sound, announced (SRV-17).";
    Synth52Events(Synth53EventsSound) = "synth52_events", "A live synth52 sound's next events (SRV-18).";
    Synth52Bank(Synth53BankSound) = "synth52_bank", "A bank of synth52 programs, one group (SRV-19, SRV-38).";
    Synth51(Synth53Sound) = "synth51", "A stored synth51 sound (SRV-16).";
    Synth51Live(Synth53LiveSound) = "synth51_live", "A live synth51 sound, announced (SRV-17).";
    Synth51Events(Synth53EventsSound) = "synth51_events", "A live synth51 sound's next events (SRV-18).";
    Synth51Bank(Synth53BankSound) = "synth51_bank", "A bank of synth51 programs, one group (SRV-19, SRV-38).";
    Synth8(Synth8Sound) = "synth8", "A stored synth8 sound (SRV-16).";
    Synth8Live(Synth8LiveSound) = "synth8_live", "A live synth8 sound, announced (SRV-17).";
    Synth8Events(Synth8EventsSound) = "synth8_events", "A live synth8 sound's next events (SRV-18).";
    Synth8Bank(Synth8BankSound) = "synth8_bank", "A bank of synth8 patches (SRV-19, SRV-41).";
    Synth80(Synth80Sound) = "synth80", "A stored synth80 sound (SRV-16).";
    Synth80Live(Synth80LiveSound) = "synth80_live", "A live synth80 sound, announced (SRV-17).";
    Synth80Events(Synth80EventsSound) = "synth80_events", "A live synth80 sound's next events (SRV-18).";
    Synth80Bank(Synth80BankSound) = "synth80_bank", "A bank of synth80 patches (SRV-19, SRV-44).";
    Play(PlaySound) = "play", "Play a sound on an audio channel.";
    Stop(StopSound) = "stop", "Stop an audio channel.";
    Gain(GainSound) = "gain", "Change an audio channel's gain.";
}

// ------------------------------------------------- what the server says

/// A bank's name: the handle and generation of a bank lifted from a stored
/// sound's music (server.md SRV-15).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BankName {
    /// Its handle.
    pub handle: u8,
    /// Its generation.
    pub generation: u16,
}

/// What a Standard MIDI File held that the server dropped, by kind
/// (MIDI-118; server.md SRV-47): every count 0 for a sound given as events.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DroppedCounts {
    /// Messages the family document does not list.
    pub not_taken: u32,
    /// Messages of a channel the channel map maps to none.
    pub unmapped_channel: u32,
    /// System common and real-time messages inside `F7` escape events.
    pub system_messages: u32,
    /// Other `F7` escape events.
    pub escapes: u32,
    /// Meta events other than set tempo and end of track.
    pub meta_events: u32,
}

/// `sound_built`: a stored synthesizer sound accepted and sent, after its
/// records were given to every connection named (server.md SRV-47).
#[derive(Clone, PartialEq, Eq, Debug, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SoundBuilt {
    /// The stored sound's kind.
    pub kind: SoundKind,
    /// Its handle.
    pub handle: u8,
    /// Its generation.
    pub generation: u16,
    /// Its blocks, 1 to 6,000.
    #[schemars(range(min = 1, max = MOST_INSTRUMENT_BLOCKS))]
    pub blocks: u16,
    /// The total bytes of its blocks, 0 to 960,000 (MIDI-41's tag 8).
    #[schemars(range(max = MOST_STORED_SOUND_BYTES))]
    pub bytes: u32,
    /// Its longest block, 0 to 1,005: the OBJECT's block bytes (MIDI-40).
    #[schemars(range(max = MOST_BLOCK_BYTES))]
    pub longest_block: u16,
    /// The banks lifted from its music, in order (SRV-15).
    pub banks_lifted: Vec<BankName>,
    /// What a Standard MIDI File held that was dropped (MIDI-118).
    pub dropped: DroppedCounts,
}

/// The sound a refusal concerns, named by its kind, handle and generation
/// where the refused line holds them (server.md SRV-48; DEC-6477).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RefusedSound {
    /// Its kind.
    pub kind: SoundKind,
    /// Its handle, where the line holds one within its type.
    pub handle: Option<u8>,
    /// Its generation, where the line holds one within its type.
    pub generation: Option<u16>,
}

/// The sound a refused line names, whatever its fault (SRV-48): none when
/// the line is not a JSON object or its `kind` is missing or not listed.
#[must_use]
pub fn sound_of(line: &[u8]) -> Option<RefusedSound> {
    let value: Value = serde_json::from_slice(line).ok()?;
    let object = value.as_object()?;
    if object.get("type").and_then(Value::as_str) != Some("sound") {
        return None;
    }
    let kind = SoundKind::deserialize(object.get("kind")?.clone()).ok()?;
    let handle = object.get("handle").and_then(Value::as_u64).and_then(|handle| u8::try_from(handle).ok());
    let generation = object.get("generation").and_then(Value::as_u64).and_then(|generation| u16::try_from(generation).ok());
    Some(RefusedSound { kind, handle, generation })
}

// ------------------------------------------------- the common fields

/// The clock the chip of synth6581 or synth8580 runs from, which sets the
/// pitch of a frequency register's value (S6581-39).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Synth6581Clock {
    /// PAL's, 985,248 Hz.
    Pal,
    /// NTSC's, 1,022,727 Hz.
    Ntsc,
}

/// How synth8's keyboard plays its two patches, the upper chosen by
/// program changes on MIDI channel 1 and the lower on channel 2 (S8-18).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Synth8KeyMode {
    /// Every key plays the upper patch, on eight voices.
    Whole,
    /// Every key plays both patches, four voices each.
    Dual,
    /// Keys below `split_key` play the lower patch and the others the
    /// upper, four voices each.
    Split,
}

/// A stored sound's length (server.md SRV-9): exactly one of its length,
/// or how long it rings after its last event.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum SoundLength {
    /// The sound's length in microseconds, 1 to 120,000,000: its blocks are
    /// `ceil(n / 20,000)`.
    LengthUs(U64In<1, MOST_STORED_TIME_US>),
    /// How long the sound rings after its last event, 0 to 120,000,000
    /// microseconds: its blocks reach past the last event's block by it.
    TailUs(U64In<0, MOST_STORED_TIME_US>),
}

/// A live events message's `until_us`: 1 to 2^53 less one. The server
/// sends the blocks a message completes at most 500 milliseconds ahead of
/// their time, and a live sound with no new block for 2,000 milliseconds
/// stops at the receiver, so a program should advance `until_us` at least
/// every 2,000 milliseconds (SRV-77).
pub type UntilUs = U64In<1, SAFE_INTEGER_LIMIT>;

/// A timed event's time: whole microseconds from the sound's start, where
/// 0 is the first sample of block 0 (SRV-8). A stored sound's times are
/// below 120,000,000.
pub type TimeUs = U64In<0, SAFE_INTEGER_LIMIT>;

/// A patch or a bank given as the adopted interface's own bytes (SRV-20,
/// SRV-21).
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BytesForm {
    /// The bytes, exact base64: 1 to 16,384 once decoded.
    pub bytes: Base64,
}

/// A Standard MIDI File as a stored sound's music (server.md SRV-14,
/// SRV-23): the file's bytes and, optionally, where each of its channels
/// goes.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MidiFile {
    /// The file, exact base64: 14 to 1,048,576 bytes once decoded.
    pub bytes: Base64,
    /// Entry n for the file's channel n + 1: the sound's channel 1 to 16 it
    /// plays on, or null to drop that channel's messages. Without a map each
    /// message keeps its channel (MIDI-116).
    #[serde(default)]
    pub channel_map: Option<[Option<MidiChannel>; 16]>,
}

/// A Standard MIDI File as a synth6581 or synth8580 sound's music (SRV-14).
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Synth6581MidiFile {
    /// The file, exact base64: 14 to 1,048,576 bytes once decoded.
    pub bytes: Base64,
}

// ---------------------------------------------------------- the events

/// `note_off`: `8n key velocity`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NoteOff {
    /// Microseconds from the sound's start (SRV-8).
    pub time_us: TimeUs,
    /// The MIDI channel, 1 to 16 (SRV-7).
    pub channel: MidiChannel,
    /// The key, 0 to 127.
    pub key: Data,
    /// The release velocity, 0 to 127.
    pub velocity: Data,
}

/// `note_on`: `9n key velocity`; a velocity of 0 is a note off (MIDI-88).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NoteOn {
    /// Microseconds from the sound's start (SRV-8).
    pub time_us: TimeUs,
    /// The MIDI channel, 1 to 16 (SRV-7).
    pub channel: MidiChannel,
    /// The key, 0 to 127.
    pub key: Data,
    /// The velocity, 0 to 127.
    pub velocity: Data,
}

/// `key_pressure`: `An key pressure`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct KeyPressure {
    /// Microseconds from the sound's start (SRV-8).
    pub time_us: TimeUs,
    /// The MIDI channel, 1 to 16 (SRV-7).
    pub channel: MidiChannel,
    /// The key, 0 to 127.
    pub key: Data,
    /// The pressure, 0 to 127.
    pub pressure: Data,
}

/// `control_change`: `Bn controller value`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ControlChange {
    /// Microseconds from the sound's start (SRV-8).
    pub time_us: TimeUs,
    /// The MIDI channel, 1 to 16 (SRV-7).
    pub channel: MidiChannel,
    /// The controller, 0 to 127.
    pub controller: Data,
    /// Its value, 0 to 127.
    pub value: Data,
}

/// `program_change`: `Bn 00 msb` and `Bn 20 lsb` where given, then
/// `Cn program`, at one tick in one block (MIDI-37 (a)).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProgramChange {
    /// Microseconds from the sound's start (SRV-8).
    pub time_us: TimeUs,
    /// The MIDI channel, 1 to 16 (SRV-7).
    pub channel: MidiChannel,
    /// The program, 0 to 127.
    pub program: Data,
    /// Bank select's most significant byte (controller 0), or none.
    #[serde(default)]
    pub bank_msb: Option<Data>,
    /// Bank select's least significant byte (controller 32), or none.
    #[serde(default)]
    pub bank_lsb: Option<Data>,
}

/// `channel_pressure`: `Dn pressure`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChannelPressure {
    /// Microseconds from the sound's start (SRV-8).
    pub time_us: TimeUs,
    /// The MIDI channel, 1 to 16 (SRV-7).
    pub channel: MidiChannel,
    /// The pressure, 0 to 127.
    pub pressure: Data,
}

/// `pitch_bend`: `En lsb msb`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PitchBend {
    /// Microseconds from the sound's start (SRV-8).
    pub time_us: TimeUs,
    /// The MIDI channel, 1 to 16 (SRV-7).
    pub channel: MidiChannel,
    /// The bend, 0 to 16,383, centre 8,192.
    pub value: Data14,
}

/// `rpn` or `nrpn`: the parameter number's two controllers, then data
/// entry's coarse and fine, at one tick in one block (MIDI-37 (c)).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ParameterNumber {
    /// Microseconds from the sound's start (SRV-8).
    pub time_us: TimeUs,
    /// The MIDI channel, 1 to 16 (SRV-7).
    pub channel: MidiChannel,
    /// The parameter number, 0 to 16,383.
    pub parameter: Data14,
    /// Its value, 0 to 16,383.
    pub value: Data14,
}

/// Which way a parameter step moves (controllers 96 and 97).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ParameterDirection {
    /// Data increment, controller 96.
    Increment,
    /// Data decrement, controller 97.
    Decrement,
}

/// `rpn_step` or `nrpn_step`: the parameter number's two controllers, then
/// data increment or decrement with the value 0 (SRV-11).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ParameterStep {
    /// Microseconds from the sound's start (SRV-8).
    pub time_us: TimeUs,
    /// The MIDI channel, 1 to 16 (SRV-7).
    pub channel: MidiChannel,
    /// The parameter number, 0 to 16,383.
    pub parameter: Data14,
    /// Which way.
    pub direction: ParameterDirection,
}

/// `system_exclusive`: one whole message, `F0` to `F7`.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SystemExclusive {
    /// Microseconds from the sound's start (SRV-8).
    pub time_us: TimeUs,
    /// The message from `F0` to `F7`, exact base64: 3 to 16,384 bytes once
    /// decoded.
    pub bytes: Base64,
}

/// `raw`: one or more whole MIDI messages back to back, running status
/// allowed within the event, its first message beginning with a status
/// byte (SRV-13).
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RawMidi {
    /// Microseconds from the sound's start (SRV-8).
    pub time_us: TimeUs,
    /// The messages, exact base64: 1 to 16,384 bytes once decoded.
    pub bytes: Base64,
}

/// A note-playing family's event, seen without its family's own two
/// variants: what the server expands alike for every family.
#[derive(Clone, Copy, Debug)]
pub enum SharedEvent<'a> {
    /// `note_off`.
    NoteOff(&'a NoteOff),
    /// `note_on`.
    NoteOn(&'a NoteOn),
    /// `key_pressure`.
    KeyPressure(&'a KeyPressure),
    /// `control_change`.
    ControlChange(&'a ControlChange),
    /// `program_change`.
    ProgramChange(&'a ProgramChange),
    /// `channel_pressure`.
    ChannelPressure(&'a ChannelPressure),
    /// `pitch_bend`.
    PitchBend(&'a PitchBend),
    /// `rpn`.
    Rpn(&'a ParameterNumber),
    /// `nrpn`.
    Nrpn(&'a ParameterNumber),
    /// `rpn_step`.
    RpnStep(&'a ParameterStep),
    /// `nrpn_step`.
    NrpnStep(&'a ParameterStep),
    /// `system_exclusive`.
    SystemExclusive(&'a SystemExclusive),
    /// `raw`.
    Raw(&'a RawMidi),
}

/// A note-playing family's event, split into what every family shares and
/// the family's own patch and parameter events.
#[derive(Clone, Copy, Debug)]
pub enum SplitEvent<'a, P, Q> {
    /// One of the shared variants.
    Shared(SharedEvent<'a>),
    /// `patch`: the family's inline patch.
    Patch(&'a P),
    /// `parameter`: the family's named parameter.
    Parameter(&'a Q),
}

/// What the server reads of every note-playing family's events, sounds and
/// banks alike.
pub trait NoteFamily {
    /// The family's `patch` event.
    type PatchEvent;
    /// The family's `parameter` event.
    type ParameterEvent;
    /// The family's event.
    type Event;
    /// Its time in microseconds.
    fn time_us(event: &Self::Event) -> u64;
    /// The event, split.
    fn split(event: &Self::Event) -> SplitEvent<'_, Self::PatchEvent, Self::ParameterEvent>;
}

// ------------------------------------------------- the note families

/// One note-playing family's types (server.md SRV-10, SRV-14 to SRV-22):
/// its event, its patch and parameter events, its patch, its music, its
/// stored sound, live sound, events message, and bank, each a named,
/// concrete type (SRV-3).
macro_rules! note_family {
    (
        $family:ident, $name:literal,
        event $event:ident, $patch_event:ident, $parameter_event:ident,
        patch $patch:ident($fields:ty), parameter $parameter:ty,
        music $music:ident, $event_list:ident,
        sounds $sound:ident, $live:ident, $events:ident,
        settings { $($(#[$setting_meta:meta])* $setting:ident: $setting_type:ty,)* },
        bank $bank_sound:ident, $bank:ident($patch_list:ty)
    ) => {
        #[doc = concat!("The ", $name, " family's events, sounds and banks, as the server reads them.")]
        #[derive(Clone, Copy, Debug)]
        pub struct $family;

        #[doc = concat!("`patch`: an inline ", $name, " patch on a channel, written as the family's inline form at one tick in one block (MIDI-60; SRV-22).")]
        #[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
        #[serde(deny_unknown_fields)]
        pub struct $patch_event {
            /// Microseconds from the sound's start (SRV-8).
            pub time_us: TimeUs,
            /// The MIDI channel, 1 to 16 (SRV-7).
            pub channel: MidiChannel,
            /// The patch, as named fields or as the interface's bytes.
            pub patch: $patch,
        }

        #[doc = concat!("`parameter`: one of ", $name, "'s named parameters set on a channel, written as the one message the family document names for its real-time change (MIDI-18; SRV-22).")]
        #[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
        #[serde(deny_unknown_fields)]
        pub struct $parameter_event {
            /// Microseconds from the sound's start (SRV-8).
            pub time_us: TimeUs,
            /// The MIDI channel, 1 to 16 (SRV-7).
            pub channel: MidiChannel,
            /// The parameter and its value: `{"name": value}`.
            pub set: $parameter,
        }

        #[doc = concat!("A ", $name, " patch: its named fields, or the adopted interface's own bytes (SRV-21).")]
        #[derive(Clone, PartialEq, Eq, Debug, JsonSchema)]
        #[serde(tag = "form", rename_all = "snake_case", deny_unknown_fields)]
        pub enum $patch {
            /// The named fields; the server writes canonical values.
            Fields($fields),
            /// The interface's own bytes, carried byte for byte once checked.
            Bytes(BytesForm),
        }

        tagged_choice!($patch, "form", { "fields" => Fields, "bytes" => Bytes });

        #[doc = concat!("One timed event of a ", $name, " sound (SRV-10).")]
        #[derive(Clone, PartialEq, Eq, Debug, JsonSchema)]
        #[serde(tag = "event", rename_all = "snake_case", deny_unknown_fields)]
        pub enum $event {
            /// `8n key velocity`.
            NoteOff(NoteOff),
            /// `9n key velocity`.
            NoteOn(NoteOn),
            /// `An key pressure`.
            KeyPressure(KeyPressure),
            /// `Bn controller value`.
            ControlChange(ControlChange),
            /// A program change, with its bank select where given.
            ProgramChange(ProgramChange),
            /// `Dn pressure`.
            ChannelPressure(ChannelPressure),
            /// `En lsb msb`.
            PitchBend(PitchBend),
            /// A registered parameter number and its value.
            Rpn(ParameterNumber),
            /// A non-registered parameter number and its value.
            Nrpn(ParameterNumber),
            /// A registered parameter number and a step.
            RpnStep(ParameterStep),
            /// A non-registered parameter number and a step.
            NrpnStep(ParameterStep),
            /// One whole system exclusive message.
            SystemExclusive(SystemExclusive),
            /// Whole MIDI messages as bytes.
            Raw(RawMidi),
            /// An inline patch.
            Patch($patch_event),
            /// A named parameter.
            Parameter($parameter_event),
        }

        tagged_choice!($event, "event", {
            "note_off" => NoteOff,
            "note_on" => NoteOn,
            "key_pressure" => KeyPressure,
            "control_change" => ControlChange,
            "program_change" => ProgramChange,
            "channel_pressure" => ChannelPressure,
            "pitch_bend" => PitchBend,
            "rpn" => Rpn,
            "nrpn" => Nrpn,
            "rpn_step" => RpnStep,
            "nrpn_step" => NrpnStep,
            "system_exclusive" => SystemExclusive,
            "raw" => Raw,
            "patch" => Patch,
            "parameter" => Parameter,
        });

        impl NoteFamily for $family {
            type PatchEvent = $patch_event;
            type ParameterEvent = $parameter_event;
            type Event = $event;

            fn time_us(event: &$event) -> u64 {
                match event {
                    $event::NoteOff(NoteOff { time_us, .. })
                    | $event::NoteOn(NoteOn { time_us, .. })
                    | $event::KeyPressure(KeyPressure { time_us, .. })
                    | $event::ControlChange(ControlChange { time_us, .. })
                    | $event::ProgramChange(ProgramChange { time_us, .. })
                    | $event::ChannelPressure(ChannelPressure { time_us, .. })
                    | $event::PitchBend(PitchBend { time_us, .. })
                    | $event::Rpn(ParameterNumber { time_us, .. })
                    | $event::Nrpn(ParameterNumber { time_us, .. })
                    | $event::RpnStep(ParameterStep { time_us, .. })
                    | $event::NrpnStep(ParameterStep { time_us, .. })
                    | $event::SystemExclusive(SystemExclusive { time_us, .. })
                    | $event::Raw(RawMidi { time_us, .. })
                    | $event::Patch($patch_event { time_us, .. })
                    | $event::Parameter($parameter_event { time_us, .. }) => time_us.get(),
                }
            }

            fn split(event: &$event) -> SplitEvent<'_, $patch_event, $parameter_event> {
                SplitEvent::Shared(match event {
                    $event::NoteOff(event) => SharedEvent::NoteOff(event),
                    $event::NoteOn(event) => SharedEvent::NoteOn(event),
                    $event::KeyPressure(event) => SharedEvent::KeyPressure(event),
                    $event::ControlChange(event) => SharedEvent::ControlChange(event),
                    $event::ProgramChange(event) => SharedEvent::ProgramChange(event),
                    $event::ChannelPressure(event) => SharedEvent::ChannelPressure(event),
                    $event::PitchBend(event) => SharedEvent::PitchBend(event),
                    $event::Rpn(event) => SharedEvent::Rpn(event),
                    $event::Nrpn(event) => SharedEvent::Nrpn(event),
                    $event::RpnStep(event) => SharedEvent::RpnStep(event),
                    $event::NrpnStep(event) => SharedEvent::NrpnStep(event),
                    $event::SystemExclusive(event) => SharedEvent::SystemExclusive(event),
                    $event::Raw(event) => SharedEvent::Raw(event),
                    $event::Patch(event) => return SplitEvent::Patch(event),
                    $event::Parameter(event) => return SplitEvent::Parameter(event),
                })
            }
        }

        #[doc = concat!("A ", $name, " sound's music as timed events (SRV-14).")]
        #[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
        #[serde(deny_unknown_fields)]
        pub struct $event_list {
            /// 0 to 100,000 events, in non-decreasing order of `time_us`.
            #[schemars(length(max = MOST_STORED_EVENTS))]
            pub events: Vec<$event>,
        }

        #[doc = concat!("A stored ", $name, " sound's music: timed events or a Standard MIDI File (SRV-14).")]
        #[derive(Clone, PartialEq, Eq, Debug, JsonSchema)]
        #[serde(tag = "form", rename_all = "snake_case", deny_unknown_fields)]
        pub enum $music {
            /// Timed events.
            Events($event_list),
            /// A Standard MIDI File, format 0 or 1.
            MidiFile(MidiFile),
        }

        tagged_choice!($music, "form", { "events" => Events, "midi_file" => MidiFile });

        #[doc = concat!("A stored ", $name, " sound (SRV-16).")]
        #[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
        #[serde(deny_unknown_fields)]
        pub struct $sound {
            /// Who it is for.
            pub connections: Connections,
            /// Its handle.
            pub handle: u8,
            /// Its generation.
            pub generation: u16,
            $($(#[$setting_meta])* pub $setting: $setting_type,)*
            /// The handles of its banks, bank index 0 first; banks lifted from
            /// its music follow them (MIDI-57, MIDI-58).
            #[schemars(length(max = MOST_SOUND_BANKS))]
            pub banks: Vec<u8>,
            /// The names the banks lifted from its music take, in order
            /// (SRV-15).
            #[serde(default)]
            #[schemars(length(max = MOST_LIFTED_BANKS))]
            pub lift_banks: Vec<BankName>,
            /// Its length (SRV-9).
            pub length: SoundLength,
            /// Its music.
            pub music: $music,
        }

        #[doc = concat!("A live ", $name, " sound, announced once (SRV-17); its events come in events messages.")]
        #[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
        #[serde(deny_unknown_fields)]
        pub struct $live {
            /// Who it is for.
            pub connections: Connections,
            /// Its handle.
            pub handle: u8,
            /// Its generation.
            pub generation: u16,
            $($(#[$setting_meta])* pub $setting: $setting_type,)*
            /// The handles of its banks, bank index 0 first: at least one.
            #[schemars(length(min = 1, max = MOST_SOUND_BANKS))]
            pub banks: Vec<u8>,
        }

        #[doc = concat!("A live ", $name, " sound's next events (SRV-18).")]
        #[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
        #[serde(deny_unknown_fields)]
        pub struct $events {
            /// Who it is for.
            pub connections: Connections,
            /// Its handle.
            pub handle: u8,
            /// Its generation.
            pub generation: u16,
            /// The time, in microseconds from the sound's start, up to which
            /// the program has given every event: every event is before it.
            pub until_us: UntilUs,
            /// 0 to 25,600 events, in non-decreasing order of `time_us`.
            #[schemars(length(max = MOST_LIVE_EVENTS))]
            pub events: Vec<$event>,
        }

        #[doc = concat!("A ", $name, " bank: a list of patches, or the bank image's bytes (SRV-20).")]
        #[derive(Clone, PartialEq, Eq, Debug, JsonSchema)]
        #[serde(tag = "form", rename_all = "snake_case", deny_unknown_fields)]
        pub enum $bank {
            /// Patches, patch k being program k of the bank.
            Patches($patch_list),
            /// The bank image's bytes, as the family document defines it.
            Bytes(BytesForm),
        }

        tagged_choice!($bank, "form", { "patches" => Patches, "bytes" => Bytes });

        #[doc = concat!("A bank of ", $name, " patches, which sounds of its family name by its handle (SRV-19).")]
        #[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
        #[serde(deny_unknown_fields)]
        pub struct $bank_sound {
            /// Who it is for.
            pub connections: Connections,
            /// Its handle.
            pub handle: u8,
            /// Its generation.
            pub generation: u16,
            /// The bank.
            pub bank: $bank,
        }
    };
}

/// A list of patches of one family, patch k being program k (SRV-20).
macro_rules! patch_list {
    ($list:ident, $patch:ident, $name:literal, $most:expr) => {
        #[doc = concat!("A ", $name, " bank as its patches, patch k being program k (SRV-20).")]
        #[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
        #[serde(deny_unknown_fields)]
        pub struct $list {
            /// The patches.
            #[schemars(length(min = 1, max = $most))]
            pub patches: Vec<$patch>,
        }
    };
}

/// The most voices of a synth7 bank: one 32-voice dump (S7-41).
pub const SYNTH7_BANK_VOICES: usize = 32;
/// The most programs of a synth53 bank: one group (S53-26).
pub const SYNTH53_BANK_PROGRAMS: usize = 40;
/// The most patches of a synth8 bank (S8-29).
pub const SYNTH8_BANK_PATCHES: usize = 64;
/// The last group of a synth53 bank, 9 (S53-13).
pub const SYNTH53_LAST_GROUP: u8 = 9;

patch_list!(Synth7PatchList, Synth7Patch, "synth7", SYNTH7_BANK_VOICES);
patch_list!(Synth106PatchList, Synth106Patch, "synth106", MOST_BANK_PATCHES);
patch_list!(Synth60PatchList, Synth60Patch, "synth60", MOST_BANK_PATCHES);
patch_list!(SynthDPatchList, SynthDPatch, "synthD", MOST_BANK_PATCHES);
patch_list!(Synth8PatchList, Synth8Patch, "synth8", SYNTH8_BANK_PATCHES);
patch_list!(Synth80PatchList, Synth80Patch, "synth80", MOST_BANK_PATCHES);

/// A synth53 bank as its programs: one group's programs, program k being
/// program k of group `group` (SRV-38).
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Synth53PatchList {
    /// The group, 0 to 9.
    pub group: U8In<0, SYNTH53_LAST_GROUP>,
    /// The programs.
    #[schemars(length(min = 1, max = SYNTH53_BANK_PROGRAMS))]
    pub patches: Vec<Synth53Patch>,
}

/// A synth7 voice's name in the fields form: 0 to 10 characters, each
/// U+0020 to U+007E (SRV-26).
pub type Synth7Name = String;

note_family!(
    Synth7Family, "synth7",
    event Synth7Event, Synth7PatchEvent, Synth7ParameterEvent,
    patch Synth7Patch(Synth7Voice), parameter Synth7Parameter,
    music Synth7Music, Synth7EventList,
    sounds Synth7Sound, Synth7LiveSound, Synth7EventsSound,
    settings {},
    bank Synth7BankSound, Synth7Bank(Synth7PatchList)
);
note_family!(
    Synth106Family, "synth106",
    event Synth106Event, Synth106PatchEvent, Synth106ParameterEvent,
    patch Synth106Patch(Synth106Panel), parameter Synth106Parameter,
    music Synth106Music, Synth106EventList,
    sounds Synth106Sound, Synth106LiveSound, Synth106EventsSound,
    settings {},
    bank Synth106BankSound, Synth106Bank(Synth106PatchList)
);
note_family!(
    Synth60Family, "synth60",
    event Synth60Event, Synth60PatchEvent, Synth60ParameterEvent,
    patch Synth60Patch(Synth60Panel), parameter Synth60Parameter,
    music Synth60Music, Synth60EventList,
    sounds Synth60Sound, Synth60LiveSound, Synth60EventsSound,
    settings {},
    bank Synth60BankSound, Synth60Bank(Synth60PatchList)
);
note_family!(
    SynthDFamily, "synthD",
    event SynthDEvent, SynthDPatchEvent, SynthDParameterEvent,
    patch SynthDPatch(SynthDPanel), parameter SynthDParameter,
    music SynthDMusic, SynthDEventList,
    sounds SynthDSound, SynthDLiveSound, SynthDEventsSound,
    settings {},
    bank SynthDBankSound, SynthDBank(SynthDPatchList)
);
note_family!(
    Synth53Family, "synth53",
    event Synth53Event, Synth53PatchEvent, Synth53ParameterEvent,
    patch Synth53Patch(Synth53Program), parameter Synth53Parameter,
    music Synth53Music, Synth53EventList,
    sounds Synth53Sound, Synth53LiveSound, Synth53EventsSound,
    settings {},
    bank Synth53BankSound, Synth53Bank(Synth53PatchList)
);
note_family!(
    Synth8Family, "synth8",
    event Synth8Event, Synth8PatchEvent, Synth8ParameterEvent,
    patch Synth8Patch(Synth8Panel), parameter Synth8Parameter,
    music Synth8Music, Synth8EventList,
    sounds Synth8Sound, Synth8LiveSound, Synth8EventsSound,
    settings {
        /// How the keyboard plays the upper and lower patches (S8-18).
        key_mode: Synth8KeyMode,
        /// The lowest key of the upper patch in the split mode, 0 to 127;
        /// sent, and unused, in the others.
        split_key: Data,
    },
    bank Synth8BankSound, Synth8Bank(Synth8PatchList)
);
note_family!(
    Synth80Family, "synth80",
    event Synth80Event, Synth80PatchEvent, Synth80ParameterEvent,
    patch Synth80Patch(Synth80Panel), parameter Synth80Parameter,
    music Synth80Music, Synth80EventList,
    sounds Synth80Sound, Synth80LiveSound, Synth80EventsSound,
    settings {},
    bank Synth80BankSound, Synth80Bank(Synth80PatchList)
);

// ------------------------------------------- the families' own choices

/// synth106's range switch, or synth60's octave transpose switch (S106-22).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Synth106Range {
    /// 16 feet; on synth60, DOWN.
    #[serde(rename = "feet_16")]
    Feet16,
    /// 8 feet; on synth60, NORMAL.
    #[serde(rename = "feet_8")]
    Feet8,
    /// 4 feet; on synth60, UP.
    #[serde(rename = "feet_4")]
    Feet4,
}

/// synth106's chorus.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Synth106Chorus {
    /// Off.
    Off,
    /// Chorus I.
    One,
    /// Chorus II.
    Two,
}

/// synth60's chorus.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Synth60Chorus {
    /// Off.
    Off,
    /// Chorus I.
    One,
    /// Chorus II.
    Two,
    /// Chorus I and II.
    Both,
}

/// synth106's PWM source.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Synth106Pwm {
    /// The LFO.
    Lfo,
    /// The manual slider.
    Manual,
}

/// synth60's PWM source.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Synth60Pwm {
    /// The LFO.
    Lfo,
    /// The manual slider.
    Manual,
    /// The envelope.
    Envelope,
}

/// The VCF envelope's polarity.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Synth106Polarity {
    /// Positive.
    Positive,
    /// Negative.
    Negative,
}

/// What controls the VCA.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Synth106Vca {
    /// The envelope.
    Envelope,
    /// The gate.
    Gate,
}

/// A synth80 channel's tone: its fields, or one of the 22 preset tones
/// (S80-42; SRV-43).
#[derive(Clone, PartialEq, Eq, Debug, JsonSchema)]
#[serde(tag = "form", rename_all = "snake_case", deny_unknown_fields)]
pub enum Synth80Tone {
    /// The tone's 26 fields.
    Fields(Synth80ToneFields),
    /// A preset tone, its 26 values the engine's.
    Preset(Synth80PresetTone),
}

tagged_choice!(Synth80Tone, "form", { "fields" => Fields, "preset" => Preset });

/// A preset tone by name.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Synth80PresetTone {
    /// Its name.
    pub name: Synth80Preset,
}

/// The 22 preset tones of S80-42, either channel's (S80-42: "MAY be
/// accepted for the other").
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[allow(missing_docs, reason = "the preset tones' names, S80-42's lists")]
pub enum Synth80Preset {
    #[serde(rename = "string_1")]
    String1,
    #[serde(rename = "string_3")]
    String3,
    #[serde(rename = "brass_1")]
    Brass1,
    Flute,
    ElectricPiano,
    #[serde(rename = "clavichord_1")]
    Clavichord1,
    #[serde(rename = "harpsichord_1")]
    Harpsichord1,
    #[serde(rename = "organ_1")]
    Organ1,
    #[serde(rename = "guitar_1")]
    Guitar1,
    #[serde(rename = "funky_1")]
    Funky1,
    #[serde(rename = "funky_3")]
    Funky3,
    #[serde(rename = "string_2")]
    String2,
    #[serde(rename = "string_4")]
    String4,
    #[serde(rename = "brass_2")]
    Brass2,
    #[serde(rename = "brass_3")]
    Brass3,
    Bass,
    #[serde(rename = "clavichord_2")]
    Clavichord2,
    #[serde(rename = "harpsichord_2")]
    Harpsichord2,
    #[serde(rename = "organ_2")]
    Organ2,
    #[serde(rename = "guitar_2")]
    Guitar2,
    #[serde(rename = "funky_2")]
    Funky2,
    #[serde(rename = "funky_4")]
    Funky4,
}

// ------------------------------------------------- synth6581 and synth8580

/// `register_update`: an ASID register data message (command 0x4E) given
/// by named fields (SRV-12).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RegisterUpdate {
    /// Microseconds from the sound's start (SRV-8).
    pub time_us: TimeUs,
    /// The registers it writes: at least one.
    pub registers: Synth6581Registers,
}

/// `asid`: one ASID message in its MIDI form, `F0 2D`, the command, the
/// payload, `F7` (S6581-4).
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AsidMessage {
    /// Microseconds from the sound's start (SRV-8).
    pub time_us: TimeUs,
    /// The message, exact base64: 4 to 257 bytes once decoded.
    pub bytes: Base64,
}

/// One timed event of a synth6581 or synth8580 sound (SRV-12).
#[derive(Clone, PartialEq, Eq, Debug, JsonSchema)]
#[serde(tag = "event", rename_all = "snake_case", deny_unknown_fields)]
pub enum Synth6581Event {
    /// Register data by named fields.
    RegisterUpdate(RegisterUpdate),
    /// An ASID message as its bytes.
    Asid(AsidMessage),
}

tagged_choice!(Synth6581Event, "event", { "register_update" => RegisterUpdate, "asid" => Asid });

impl Synth6581Event {
    /// Its time in microseconds.
    #[must_use]
    pub const fn time_us(&self) -> u64 {
        match self {
            Self::RegisterUpdate(RegisterUpdate { time_us, .. }) | Self::Asid(AsidMessage { time_us, .. }) => time_us.get(),
        }
    }
}

/// A synth6581 or synth8580 sound's music as timed events (SRV-14).
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Synth6581EventList {
    /// 0 to 100,000 events, in non-decreasing order of `time_us`.
    #[schemars(length(max = MOST_STORED_EVENTS))]
    pub events: Vec<Synth6581Event>,
}

/// A stored synth6581 or synth8580 sound's music (SRV-14).
#[derive(Clone, PartialEq, Eq, Debug, JsonSchema)]
#[serde(tag = "form", rename_all = "snake_case", deny_unknown_fields)]
pub enum Synth6581Music {
    /// Timed events.
    Events(Synth6581EventList),
    /// A Standard MIDI File, format 0 or 1.
    MidiFile(Synth6581MidiFile),
}

tagged_choice!(Synth6581Music, "form", { "events" => Events, "midi_file" => MidiFile });

/// A stored sound of synth6581 or synth8580, whichever its kind names
/// (SRV-16).
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Synth6581Sound {
    /// Who it is for.
    pub connections: Connections,
    /// Its handle.
    pub handle: u8,
    /// Its generation.
    pub generation: u16,
    /// The chip's clock (S6581-39).
    pub clock: Synth6581Clock,
    /// Its length (SRV-9).
    pub length: SoundLength,
    /// Its music, whose first event is a full register update at tick 0
    /// (S6581-45).
    pub music: Synth6581Music,
}

/// A live sound of synth6581 or synth8580, announced once (SRV-17).
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Synth6581LiveSound {
    /// Who it is for.
    pub connections: Connections,
    /// Its handle.
    pub handle: u8,
    /// Its generation.
    pub generation: u16,
    /// The chip's clock (S6581-39).
    pub clock: Synth6581Clock,
}

/// A live synth6581 or synth8580 sound's next events (SRV-18).
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Synth6581EventsSound {
    /// Who it is for.
    pub connections: Connections,
    /// Its handle.
    pub handle: u8,
    /// Its generation.
    pub generation: u16,
    /// The time up to which the program has given every event.
    pub until_us: UntilUs,
    /// 0 to 25,600 events, in non-decreasing order of `time_us`.
    #[schemars(length(max = MOST_LIVE_EVENTS))]
    pub events: Vec<Synth6581Event>,
}

// ------------------------------------------- Opus, PCM and the commands

/// A stored Opus sound (decision 21).
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpusSound {
    /// Who it is for.
    pub connections: Connections,
    /// Its handle.
    pub handle: u8,
    /// Its generation.
    pub generation: u16,
    /// Every packet's length, 1 to 40 bytes.
    #[schemars(range(min = 1, max = OPUS_MOST_PACKET_BYTES))]
    pub packet_bytes: u16,
    /// Its length in samples at 48 kHz.
    #[schemars(range(max = MOST_SAMPLES))]
    pub samples: u32,
    /// Its packets back to back, exact base64, at most two minutes.
    pub packets: Base64,
}

/// A live Opus sound's object.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpusLiveSound {
    /// Who it is for.
    pub connections: Connections,
    /// Its handle.
    pub handle: u8,
    /// Its generation.
    pub generation: u16,
    /// Every packet's length.
    #[schemars(range(min = 1, max = OPUS_MOST_PACKET_BYTES))]
    pub packet_bytes: u16,
}

/// A live Opus sound's next packets.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpusPacketsSound {
    /// Who it is for.
    pub connections: Connections,
    /// Its handle.
    pub handle: u8,
    /// Its generation.
    pub generation: u16,
    /// Every packet's length.
    #[schemars(range(min = 1, max = OPUS_MOST_PACKET_BYTES))]
    pub packet_bytes: u16,
    /// The first packet's block number.
    pub first: u16,
    /// The packets, exact base64.
    pub packets: Base64,
}

/// A stored sound as 16-bit samples, which the server encodes as IMA
/// ADPCM, for receivers that decode no Opus.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ObjectSound {
    /// Who it is for.
    pub connections: Connections,
    /// Its handle.
    pub handle: u8,
    /// Its generation.
    pub generation: u16,
    /// Its sample rate, one of the profile's.
    #[schemars(extend("enum" = SAMPLE_RATES))]
    pub rate: u32,
    /// 16-bit little-endian mono samples, exact base64, at most two
    /// minutes.
    pub pcm: Base64,
}

/// A live sound's object.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LiveSound {
    /// Who it is for.
    pub connections: Connections,
    /// Its handle.
    pub handle: u8,
    /// Its generation.
    pub generation: u16,
    /// Its sample rate.
    #[schemars(extend("enum" = SAMPLE_RATES))]
    pub rate: u32,
}

/// A live sound's next blocks of samples.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BlocksSound {
    /// Who it is for.
    pub connections: Connections,
    /// Its handle.
    pub handle: u8,
    /// Its generation.
    pub generation: u16,
    /// Its sample rate.
    #[schemars(extend("enum" = SAMPLE_RATES))]
    pub rate: u32,
    /// The first block's number.
    pub first: u16,
    /// The samples, exact base64.
    pub pcm: Base64,
}

/// Play a sound on an audio channel.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlaySound {
    /// Who it is for.
    pub connections: Connections,
    /// The audio channel, 0 to 3.
    #[schemars(range(max = LAST_AUDIO_CHANNEL))]
    pub channel: u8,
    /// The sound's handle.
    pub handle: u8,
    /// Its generation.
    pub generation: u16,
    /// The gain, 0 to 100.
    #[schemars(range(max = MOST_GAIN))]
    pub gain: u8,
    /// The fade in, in milliseconds.
    pub fade_ms: u16,
    /// It loops.
    pub r#loop: bool,
    /// It plays after the sound playing ends.
    pub after: bool,
    /// How far other channels duck under it, 0 to 6.
    #[schemars(range(max = MOST_DUCK))]
    pub duck: u8,
    /// The page it belongs to, or none for the whole service.
    pub page: Option<PageNumber>,
}

/// Stop an audio channel.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StopSound {
    /// Who it is for.
    pub connections: Connections,
    /// The audio channel, 0 to 3.
    #[schemars(range(max = LAST_AUDIO_CHANNEL))]
    pub channel: u8,
    /// The fade out, in milliseconds.
    pub fade_ms: u16,
}

/// Change an audio channel's gain.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GainSound {
    /// Who it is for.
    pub connections: Connections,
    /// The audio channel, 0 to 3.
    #[schemars(range(max = LAST_AUDIO_CHANNEL))]
    pub channel: u8,
    /// The gain, 0 to 100.
    #[schemars(range(max = MOST_GAIN))]
    pub gain: u8,
    /// The fade, in milliseconds.
    pub fade_ms: u16,
}
