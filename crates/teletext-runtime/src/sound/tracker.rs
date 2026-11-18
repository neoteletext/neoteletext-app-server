//! Each sound's tracker: the engine's record of the sound's parts as the
//! server writes its messages, from which each part's restatement is
//! written (server.md SRV-68, SRV-69; MIDI-93 to MIDI-99). The server
//! keeps none of a family's state itself: every family's tracker is the
//! one in its engine interface module, and this is only the choice
//! between them.

use alloc::boxed::Box;
use alloc::vec::Vec;

use neoteletext_audio::midi::{Banks, CHANNELS, Message};
use neoteletext_audio_instruments::{synth7, synth8, synth53, synth80, synth106, synth6581, synthd};

use super::family::{Family, Synth};
use super::wire::Wire;

/// What the tracker is told beside a message, where taking the message
/// alone would leave the state right but restate it otherwise (SRV-68).
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) enum TrackHint {
    #[expect(dead_code, reason = "written by synth80's call site, which a following commit gives the engine's interface")]
    /// The message ends a synth80 inline patch of this patch: restated as
    /// the patch, not as 85 controllers moved (S80-40, S80-46).
    Synth80Patch(Box<synth80::interface::Patch>),
}

/// One sound's tracker, of its family.
#[derive(Clone, Debug)]
pub(crate) enum Tracker {
    /// synth6581 and synth8580: the 25 registers (S6581-46).
    Synth6581(Box<synth6581::interface::Tracker>),
    /// synth7: sixteen parts (S7-49 item 4).
    Synth7(Box<synth7::interface::Tracker>),
    /// synth106 and synth60: one part on every channel (S106-2, S106-40).
    Synth106(Box<synth106::interface::Tracker>),
    /// synthD: one part (SD-28).
    SynthD(Box<synthd::interface::Tracker>),
    /// synth53, synth52 and synth51: one part on every channel (S53-3,
    /// S53-46).
    Synth53(Box<synth53::interface::Part>),
    /// synth8: its upper and lower halves (S8-29 (e)).
    Synth8(Box<synth8::interface::Tracker>),
    /// synth80: one part on every channel (S80-2, S80-46).
    Synth80(Box<synth80::interface::Tracker>),
}

/// The part a family of one part restates as (MIDI-67): its tracker
/// writes the restatement on the family's own channel.
pub(crate) const ONE_PART: u8 = 0;

impl Tracker {
    /// A sound's tracker as the sound starts, with its banks (MIDI-61):
    /// each part at program 0 of bank index 0, or the family's default.
    pub(crate) fn new(synth: Synth, banks: &Banks<'_>) -> Self {
        match synth.family {
            Family::Synth6581 => Self::Synth6581(Box::new(synth6581::interface::Tracker::new())),
            Family::Synth7 => Self::Synth7(Box::new(synth7::interface::Tracker::new(banks))),
            Family::Synth106 => {
                let mut tracker = synth106::interface::Tracker::new(synth.model());
                // What a bank's program 0 is does not refuse the sound: the
                // receiver plays the default where it is faulty (MIDI-63).
                let _outcome = tracker.start(banks);
                Self::Synth106(Box::new(tracker))
            }
            Family::SynthD => Self::SynthD(Box::new(synthd::interface::Tracker::new(banks))),
            Family::Synth53 => {
                let mut part = synth53::interface::Part::new(synth.synth53_variant());
                let _outcome = part.start(banks);
                Self::Synth53(Box::new(part))
            }
            Family::Synth8 => Self::Synth8(Box::new(synth8::interface::Tracker::new())),
            Family::Synth80 => Self::Synth80(Box::new(synth80::interface::Tracker::new())),
        }
    }

    /// Takes one message the server wrote, in order (SRV-68), and what it
    /// was written for.
    pub(crate) fn take(&mut self, wire: &Wire, hint: Option<&TrackHint>, banks: &Banks<'_>) {
        let message = wire.message();
        match self {
            Self::Synth6581(tracker) => {
                if let Ok(asid) = synth6581::interface::accepts(&message) {
                    tracker.take(&asid);
                }
            }
            Self::Synth7(tracker) => tracker.take(&message, banks),
            Self::Synth106(tracker) => {
                let _taken = tracker.take(&message, banks);
            }
            Self::SynthD(tracker) => tracker.take(&message, banks),
            Self::Synth53(part) => {
                let _change = part.take(&message, banks);
            }
            Self::Synth8(tracker) => tracker.take(&message),
            Self::Synth80(tracker) => {
                tracker.take(&message);
                if let Some(TrackHint::Synth80Patch(patch)) = hint {
                    tracker.inline_patch(patch);
                }
            }
        }
    }

    /// Whether each of the sixteen channels is a part of its own, restated
    /// on its own (synth7); every other family is one part (MIDI-67).
    pub(crate) const fn parts(&self) -> usize {
        match self {
            Self::Synth7(_) => CHANNELS,
            _ => 1,
        }
    }

    /// Part `part`'s restatement, as the engine writes it from the tracker
    /// (SRV-69): for a family of one part, [`ONE_PART`] on the family's own
    /// channel.
    pub(crate) fn restate(&self, part: u8) -> Vec<Wire> {
        let mut out = Vec::new();
        let mut push = |message: Message<'_>| {
            out.push(match message {
                Message::Channel(channel) => Wire::Channel(channel),
                Message::SystemExclusive(bytes) => Wire::Sysex(bytes.to_vec()),
            });
        };
        match self {
            // Before the program's opening full update the registers are
            // not yet known, so there is nothing to restate (S6581-45).
            Self::Synth6581(tracker) => {
                if tracker.opened() {
                    tracker.restate(&mut push);
                }
            }
            Self::Synth7(tracker) => tracker.restate(part, &mut push),
            Self::Synth106(tracker) => tracker.restate(ONE_PART, &mut push),
            Self::SynthD(tracker) => tracker.restate(&mut push),
            Self::Synth53(part_state) => part_state.restate(ONE_PART, &mut push),
            Self::Synth8(tracker) => tracker.restate(&mut push),
            Self::Synth80(tracker) => tracker.restate(&mut push),
        }
        out
    }

    /// The synth106 family's tracker, which a named switch parameter is
    /// written from (SRV-33, SRV-68).
    pub(crate) fn synth106(&self) -> Option<&synth106::interface::Tracker> {
        match self {
            Self::Synth106(tracker) => Some(tracker),
            _ => None,
        }
    }
}
