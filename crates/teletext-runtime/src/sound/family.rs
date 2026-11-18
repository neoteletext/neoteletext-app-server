//! Where the server calls each family's interface in the engine (server.md
//! SRV-55, SRV-88): which messages a family takes, its inline patches and
//! named parameters, its bank images, which system exclusive messages are
//! its inline patches, parameter changes and bank dumps, and its most
//! banks. The server holds none of these tables itself (SRV-53): each
//! family's module is one call site into the family's `interface` module
//! in the engine, so nothing can drift from the receiver (MIDI-21).
//!
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use neoteletext_audio::Codec;
use neoteletext_audio_instruments::synth53::interface::Variant;
use neoteletext_audio_instruments::synth106::interface::Model;

use super::tracker::Tracker;
use super::wire::Wire;
use crate::protocol::{Rule, SoundKind, Synth106Parameter};

pub(crate) mod synth106;
pub(crate) mod synth53;
pub(crate) mod synth6581;
pub(crate) mod synth7;
pub(crate) mod synth8;
pub(crate) mod synth80;
pub(crate) mod synthd;

/// A family of synthesizers: one codec, one interface (MIDI-1).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Family {
    /// synth6581 and synth8580 (codec 12).
    Synth6581,
    /// synth7 (codec 13).
    Synth7,
    /// synth106 and synth60 (codec 14).
    Synth106,
    /// synthD (codec 15).
    SynthD,
    /// synth53, synth52 and synth51 (codec 16).
    Synth53,
    /// synth8 (codec 17).
    Synth8,
    /// synth80 (codec 18).
    Synth80,
}

impl Family {
    /// Its codec (MIDI-107).
    pub(crate) const fn codec(self) -> Codec {
        match self {
            Self::Synth6581 => Codec::Synth6581,
            Self::Synth7 => Codec::Synth7,
            Self::Synth106 => Codec::Synth106,
            Self::SynthD => Codec::SynthD,
            Self::Synth53 => Codec::Synth53,
            Self::Synth8 => Codec::Synth8,
            Self::Synth80 => Codec::Synth80,
        }
    }

    /// Its name, for refusals.
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Synth6581 => "synth6581",
            Self::Synth7 => "synth7",
            Self::Synth106 => "synth106",
            Self::SynthD => "synthd",
            Self::Synth53 => "synth53",
            Self::Synth8 => "synth8",
            Self::Synth80 => "synth80",
        }
    }

    /// The most banks a sound of the family may name (SRV-58), the
    /// family's own constant in the engine.
    pub(crate) const fn most_banks(self) -> usize {
        match self {
            Self::Synth6581 => 0,
            Self::Synth7 => synth7::MOST_BANKS,
            Self::Synth106 => synth106::MOST_BANKS,
            Self::SynthD => synthd::MOST_BANKS,
            Self::Synth53 => synth53::MOST_BANKS,
            Self::Synth8 => synth8::MOST_BANKS,
            Self::Synth80 => synth80::MOST_BANKS,
        }
    }

    /// Whether a system exclusive message is one of the family's bank
    /// dumps, which a stored sound's music lifts into a bank (SRV-63):
    /// `Some` with the group it fills, so that one group's dumps make one
    /// bank (the synth53 family), or 0.
    pub(crate) fn bank_dump(self, bytes: &[u8]) -> Result<Option<u8>, FamilyFault> {
        match self {
            Self::Synth7 => synth7::bank_dump(bytes),
            Self::Synth53 => synth53::bank_dump(bytes),
            Self::Synth6581 | Self::Synth106 | Self::SynthD | Self::Synth8 | Self::Synth80 => Ok(None),
        }
    }
}

/// A kind of synthesizer sound: its family and the variant its OBJECT
/// names (MIDI-48 tags 3 and 5, written from the kind, never from a field).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Synth {
    /// The family.
    pub(crate) family: Family,
    /// The variant within it: tag 5's value for codecs 14 and 16, and tag
    /// 3's chip bit for codec 12.
    pub(crate) variant: u8,
}

/// The variant of synth60 within its family (MIDI-48 tag 5).
const VARIANT_SYNTH60: u8 = 1;
/// The variant of synth52 within its family (MIDI-48 tag 5).
const VARIANT_SYNTH52: u8 = 1;
/// The variant of synth51 within its family (MIDI-48 tag 5).
const VARIANT_SYNTH51: u8 = 2;

impl Synth {
    /// The synthesizer a kind is for, or none for a kind of no synthesizer.
    pub(crate) const fn of(kind: SoundKind) -> Option<Self> {
        use SoundKind as K;
        let (family, variant) = match kind {
            K::Synth6581 | K::Synth6581Live | K::Synth6581Events => (Family::Synth6581, 0),
            K::Synth8580 | K::Synth8580Live | K::Synth8580Events => (Family::Synth6581, neoteletext_audio::record::SYNTH6581_CHIP_8580),
            K::Synth7 | K::Synth7Live | K::Synth7Events | K::Synth7Bank => (Family::Synth7, 0),
            K::Synth106 | K::Synth106Live | K::Synth106Events | K::Synth106Bank => (Family::Synth106, 0),
            K::Synth60 | K::Synth60Live | K::Synth60Events | K::Synth60Bank => (Family::Synth106, VARIANT_SYNTH60),
            K::SynthD | K::SynthDLive | K::SynthDEvents | K::SynthDBank => (Family::SynthD, 0),
            K::Synth53 | K::Synth53Live | K::Synth53Events | K::Synth53Bank => (Family::Synth53, 0),
            K::Synth52 | K::Synth52Live | K::Synth52Events | K::Synth52Bank => (Family::Synth53, VARIANT_SYNTH52),
            K::Synth51 | K::Synth51Live | K::Synth51Events | K::Synth51Bank => (Family::Synth53, VARIANT_SYNTH51),
            K::Synth8 | K::Synth8Live | K::Synth8Events | K::Synth8Bank => (Family::Synth8, 0),
            K::Synth80 | K::Synth80Live | K::Synth80Events | K::Synth80Bank => (Family::Synth80, 0),
            K::Opus | K::OpusLive | K::OpusPackets | K::Object | K::Live | K::Blocks | K::Play | K::Stop | K::Gain => return None,
        };
        Some(Self { family, variant })
    }

    /// The OBJECT's variant tag (MIDI-48 tag 5): on codecs 14 and 16 only.
    pub(crate) const fn variant_tag(self) -> Option<u8> {
        match self.family {
            Family::Synth106 | Family::Synth53 => Some(self.variant),
            _ => None,
        }
    }
    /// The model of the synth106 family it is (S106-1).
    pub(crate) const fn model(self) -> Model {
        if self.variant == VARIANT_SYNTH60 { Model::Synth60 } else { Model::Synth106 }
    }

    /// The variant of the synth53 family it is (S53-1).
    pub(crate) const fn synth53_variant(self) -> Variant {
        match Variant::from_tag(self.variant) {
            Some(variant) => variant,
            None => Variant::Synth53,
        }
    }

    /// Whether the synthesizer takes `message` (SRV-24; MIDI-10), by its
    /// family's table in the engine: `Err` names the family document's
    /// requirement.
    pub(crate) fn takes(self, message: &Wire) -> Result<(), String> {
        match self.family {
            Family::Synth6581 => synth6581::takes(message),
            Family::Synth7 => synth7::takes(message),
            Family::Synth106 => synth106::takes(self, message),
            Family::SynthD => synthd::takes(message),
            Family::Synth53 => synth53::takes(message),
            Family::Synth8 => synth8::takes(message),
            Family::Synth80 => synth80::takes(message),
        }
    }

    /// The bank image made of lifted dumps, checked as a bank (SRV-60,
    /// SRV-63).
    pub(crate) fn lifted_bank(self, dumps: &[Vec<u8>]) -> Result<Vec<u8>, FamilyFault> {
        let mut image = Vec::new();
        for dump in dumps {
            image.push(neoteletext_audio::midi::status::SYSTEM_EXCLUSIVE);
            image.extend_from_slice(dump);
        }
        self.check_bank(&image)?;
        Ok(image)
    }

    /// Checks a bank image as the family's bank reader reads it (SRV-60).
    pub(crate) fn check_bank(self, image: &[u8]) -> Result<(), FamilyFault> {
        match self.family {
            Family::Synth6581 => Err(FamilyFault::new(Rule::InvalidBank, "S6581-51: the synth6581 family has no bank")),
            Family::Synth7 => synth7::check_bank(image),
            Family::Synth106 => synth106::check_bank(self, image),
            Family::SynthD => synthd::check_bank(image),
            Family::Synth53 => synth53::check_bank(image),
            Family::Synth8 => synth8::check_bank(image),
            Family::Synth80 => synth80::check_bank(image),
        }
    }
}

/// A message written from the tracked state of its part at its tick, so
/// resolved only as its block is written (SRV-33, SRV-68).
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) enum Deferred {
    /// A named synth106 switch parameter on channel `channel` (0 to 15):
    /// the whole switch byte from the tracked panel with the one field
    /// changed (SRV-33 item 1).
    Synth106Switch {
        /// The channel.
        channel: u8,
        /// The parameter.
        parameter: Synth106Parameter,
    },
}

impl Deferred {
    /// The messages it stands for, from `tracker` as it is at the
    /// message's tick.
    pub(crate) fn resolve(&self, tracker: &Tracker) -> Result<Vec<Wire>, FamilyFault> {
        match self {
            Self::Synth106Switch { channel, parameter } => synth106::switch_message(tracker.synth106(), *channel, *parameter),
        }
    }
}

/// Why a family's interface refused something: the rule of SRV-51 and the
/// detail, which begins with the requirement broken (SRV-50).
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct FamilyFault {
    /// The rule.
    pub(crate) rule: Rule,
    /// The detail.
    pub(crate) detail: String,
}

impl FamilyFault {
    /// A fault of `rule`.
    pub(crate) fn new(rule: Rule, detail: impl Into<String>) -> Self {
        Self { rule, detail: detail.into() }
    }

    /// What a temporary call site answers where the family's engine
    /// function is not yet in the pinned engine (SRV-55): the message is
    /// refused, never built by a second implementation here.
    pub(crate) fn unavailable(rule: Rule, family: &str, function: &str) -> Self {
        Self::new(
            rule,
            format!(
                "SRV-55: {family}'s {function} is not yet in the engine this server pins; it comes with the engine's branch family-{family} \
                 (sound/family/{family}.rs is its temporary call site)"
            ),
        )
    }
}

/// The checks every family's system exclusive inline patch meets before
/// the family's own check (SRV-57): one whole message, `F0` to `F7`, that
/// fits one event (MIDI-27, MIDI-28). Gives its bytes after `F0`.
pub(crate) fn whole_message(bytes: &[u8]) -> Result<Vec<u8>, FamilyFault> {
    let after = bytes.strip_prefix(&[neoteletext_audio::midi::status::SYSTEM_EXCLUSIVE]).unwrap_or(&[]);
    if !neoteletext_audio::midi::block::whole_system_exclusive(after) {
        return Err(FamilyFault::new(
            Rule::InvalidPatch,
            format!("MIDI-27, MIDI-28: {} bytes are not one whole system exclusive message of at most 257", bytes.len()),
        ));
    }
    Ok(after.to_vec())
}

/// A whole system exclusive message as one inline patch event (MIDI-60).
pub(crate) fn sysex_patch(bytes: &[u8]) -> Result<Vec<Wire>, FamilyFault> {
    Ok(alloc::vec![Wire::Sysex(whole_message(bytes)?)])
}
