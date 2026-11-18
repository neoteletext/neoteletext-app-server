//! A page's rule table as the server keeps it while it is in force on a
//! connection (neoteletext-common `docs/page-rules.md` RULE-36, RULE-40,
//! RULE-59; REQ-6532): its image, encoded and checked with the engine's
//! `neoteletext-rules`, the crate every receiver decodes it with, for one
//! page number and sub-code (DEC-6599); and the names its program gave
//! its rules, groups, sequences and rectangles, which no receiver is sent
//! and by which the server names them back in every message it gives the
//! program.
//!
//! The server has no table of keys, of synthesizers or of their fields:
//! a key's name is the engine's (`neoteletext_input::keys`), a word of the
//! protocol is the engine's (`neoteletext_rules::names`), and a
//! synthesizer's field is the engine's parameter table of its family
//! (`neoteletext_audio_instruments::watch::fields`, RULE-29).

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use neoteletext_audio_instruments::midi::{PatchField, family};
use neoteletext_audio_instruments::watch;
use neoteletext_core::types::SubCode as EngineSubCode;
use neoteletext_input::keys;
use neoteletext_rules::table::{self, KeyKind, Table, Target, TargetKind, Value};
use neoteletext_rules::{limits, names};
use neoteletext_web::records::rules::{RulesRecord, encode as encode_rules, encode_image};
use neoteletext_web::records::write_packets;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::page::address;
use crate::protocol::{
    ButtonValue, FiredName, FiredValue, KeyName, KeyValue, LookName, NoFields, PageWord, PointerValue, RuleKey, RuleName, RuleTarget, RuleValue, SequenceValue,
    SoundValue, SynthFamily, SynthProperty, SynthValue, TargetNumber, TargetValue, TimerName, TimerRef, TimerValue, U8In, U16In, VariableName, VariableValue,
};
use crate::protocol::{CommandMessage, Refusal};

/// The audio and input service the records go on (the reference channel's,
/// DOC-1951).
const SERVICE: u8 = 0;

/// The names a program gave what its table declares, each list in the
/// table's order, so that a name's place is its number in the image
/// (RULE-36).
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub(crate) struct Names {
    /// The rules' names, in rule order.
    pub(crate) rules: Vec<String>,
    /// The named groups' names; the unnamed groups follow them.
    pub(crate) groups: Vec<String>,
    /// The sequences' names.
    pub(crate) sequences: Vec<String>,
    /// The rectangles' names.
    pub(crate) rects: Vec<String>,
}

/// The names of the default table's rules (RULE-53), in the order the
/// engine's `default_send_all` writes them.
const DEFAULT_RULES: [&str; 13] =
    ["keys", "keys_up", "text", "press", "release", "move", "hover_in", "hover_out", "drawing_in", "drawing_out", "focus", "blur", "wheel"];
/// The name of the default table's one group (RULE-53).
const DEFAULT_GROUP: &str = "send";

/// One page's table, ready to send: its image and its names.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct Compiled {
    page: u16,
    sub: u16,
    /// The image as `neoteletext_rules::encode` wrote it and
    /// `neoteletext_rules::check` passed it.
    image: Vec<u8>,
    /// The names, by number.
    names: Names,
    /// The table as its program gave it, kept only while its groups place
    /// NAPLPS objects, whose records name the generation of the stream
    /// each connection has (ANIM-11, ANIM-21; DEC-6654 item 11): the table
    /// is written again for that generation when it is sent.
    source: Option<alloc::sync::Arc<crate::protocol::RuleTable>>,
}

/// A synthesizer by its name in the protocol, and its family's codec code
/// and variant in the image (RULE-28).
const SYNTHESIZERS: [(SynthFamily, u8, u8); 11] = [
    (SynthFamily::Synth6581, family::SYNTH6581, 0),
    (SynthFamily::Synth8580, family::SYNTH6581, 1),
    (SynthFamily::Synth7, family::SYNTH7, 0),
    (SynthFamily::Synth106, family::SYNTH106, 0),
    (SynthFamily::Synth60, family::SYNTH106, 1),
    (SynthFamily::Synthd, family::SYNTHD, 0),
    (SynthFamily::Synth53, family::SYNTH53, 0),
    (SynthFamily::Synth52, family::SYNTH53, 1),
    (SynthFamily::Synth51, family::SYNTH53, 2),
    (SynthFamily::Synth8, family::SYNTH8, 0),
    (SynthFamily::Synth80, family::SYNTH80, 0),
];

/// The codec code and variant of a synthesizer (RULE-28).
pub(crate) fn synthesizer_code(name: SynthFamily) -> (u8, u8) {
    SYNTHESIZERS.iter().find(|(held, _, _)| *held == name).map_or((family::SYNTH6581, 0), |(_, code, variant)| (*code, *variant))
}

/// The readable patch fields of a synthesizer, in parameter order
/// (RULE-29; page-rules.md appendix A).
pub(crate) fn fields_of(name: SynthFamily) -> &'static [PatchField] {
    let (code, variant) = synthesizer_code(name);
    watch::fields(code, variant).unwrap_or(&[])
}

/// The protocol's word for `item`: its JSON string, which is the word the
/// engine's `names` tables give the same thing.
pub(crate) fn word_of<T: Serialize>(item: &T) -> String {
    match serde_json::to_value(item) {
        Ok(serde_json::Value::String(word)) => word,
        _ => String::new(),
    }
}

/// The protocol's item whose word is `word`.
fn item_of<T: DeserializeOwned>(word: &str) -> Option<T> {
    serde_json::from_value(serde_json::Value::String(word.to_string())).ok()
}

/// The number the engine's table `pairs` gives `item`'s word.
pub(crate) fn code_of<T: Serialize, N: Copy>(pairs: &[(&str, N)], item: &T) -> Option<N> {
    names::code(pairs, &word_of(item))
}

/// The protocol's item that the engine's table `pairs` names `number`.
fn named<T: DeserializeOwned, N: Copy + PartialEq>(pairs: &[(&'static str, N)], number: N) -> Option<T> {
    item_of(names::name(pairs, number)?)
}

/// A look's number (ANIM-6).
pub(crate) fn look_number(look: LookName) -> u8 {
    code_of(&names::LOOKS, &look).unwrap_or(0)
}

/// The look a number is, or `normal` for one no look has.
pub(crate) fn look_name(number: u8) -> LookName {
    named(&names::LOOKS, number).unwrap_or(LookName::Normal)
}

/// The key a kind and a code name, as the protocol writes it.
fn key_of(kind: KeyKind, code: u32) -> Option<RuleKey> {
    let character = || char::from_u32(code).map(|character| character.to_string());
    Some(match kind {
        KeyKind::Character => RuleKey::Char { char: character()? },
        KeyKind::Physical => RuleKey::Physical { physical: character()? },
        KeyKind::Named => RuleKey::Name { name: KeyName(keys::name(code)?.to_string()) },
    })
}

/// The packets of `write`'s records, numbered from `record_id`, which is
/// moved past them.
fn packets(record_id: &mut u8, write: impl FnOnce(&mut dyn FnMut(usize) -> u8, &mut dyn FnMut(neoteletext_core::packet::Packet)) -> bool) -> Vec<u8> {
    let first = *record_id;
    let mut used = 0_usize;
    let mut bytes = Vec::new();
    let written = write(
        &mut |number| {
            used = used.max(number.saturating_add(1));
            first.wrapping_add(u8::try_from(number).unwrap_or(u8::MAX))
        },
        &mut |packet| bytes.extend_from_slice(&packet.bytes),
    );
    if !written {
        return Vec::new();
    }
    *record_id = first.wrapping_add(u8::try_from(used.max(1)).unwrap_or(u8::MAX));
    bytes
}

/// The RULES record that says `page` and `sub` have no table, at
/// `generation` (RULE-41, RULE-53).
pub(crate) fn no_table_records(page: u16, sub: u16, generation: u16, record_id: &mut u8) -> Vec<u8> {
    let record = RulesRecord { page: address(page), subcode: EngineSubCode::new(sub), generation, fragment: None };
    packets(record_id, |id, out| encode_rules(&record, SERVICE, id(0), out).is_ok())
}

impl Compiled {
    /// `table`, for `page` and `sub`, with its `names`, encoded with the
    /// engine's crate and checked by the decoder every receiver has
    /// (RULE-59).
    ///
    /// # Errors
    /// What the engine's encoder or check refuses.
    pub(crate) fn of(table: &Table, names: Names) -> Result<Self, table::Fault> {
        let mut image = alloc::vec![0_u8; limits::MAX_TABLE_BYTES];
        let length = neoteletext_rules::encode(table, neoteletext_web::records::body_check, &mut image)?;
        image.truncate(length);
        neoteletext_rules::check(&image, table.page, table.sub, neoteletext_web::records::body_check)?;
        Ok(Self { page: table.page, sub: table.sub, image, names, source: None })
    }

    /// The default table of a channel whose configuration says
    /// `"default_rules": "send_all"` (RULE-53), for one page: the engine's,
    /// so its bytes are the same everywhere, with the `wheel` rule when the
    /// channel uses the wheel.
    pub(crate) fn send_all(wheel: bool, page: u16, sub: u16) -> Option<Self> {
        let mut table = alloc::boxed::Box::new(Table::new(page, sub));
        neoteletext_rules::default_send_all(wheel, page, sub, &mut table);
        let rules = DEFAULT_RULES.iter().take(table.rules.len()).map(ToString::to_string).collect();
        let names = Names { rules, groups: alloc::vec![DEFAULT_GROUP.to_string()], ..Names::default() };
        Self::of(&table, names).ok()
    }

    /// This table, noted as written from `source`, to be written again for
    /// each connection's NAPLPS stream generation.
    pub(crate) fn recompiled_from(self, source: alloc::sync::Arc<crate::protocol::RuleTable>) -> Self {
        Self { source: Some(source), ..self }
    }

    /// This table as it is sent to a connection whose NAPLPS stream of the
    /// page has `generation` (ANIM-21): itself, or, when its groups place
    /// NAPLPS objects, written again for that generation. The program's
    /// table compiled once already, so it compiles again; were it not to,
    /// this table is sent as it is.
    pub(crate) fn for_naplps_generation(self: &alloc::sync::Arc<Self>, generation: u16) -> alloc::sync::Arc<Self> {
        let Some(source) = &self.source else { return alloc::sync::Arc::clone(self) };
        crate::protocol::check::recompiled(source, (self.page, self.sub), generation).map_or_else(|| alloc::sync::Arc::clone(self), alloc::sync::Arc::new)
    }

    /// The image, as the RULES records carry it.
    #[cfg(test)]
    pub(crate) fn image(&self) -> &[u8] {
        &self.image
    }

    /// The names of its rules, groups, sequences and rectangles.
    #[cfg(test)]
    pub(crate) const fn names(&self) -> &Names {
        &self.names
    }

    /// The RULES records that give its page this table at `generation`
    /// (RULE-41): as many as the image needs.
    pub(crate) fn records(&self, generation: u16, record_id: &mut u8) -> Vec<u8> {
        packets(record_id, |id, out| encode_image(address(self.page), EngineSubCode::new(self.sub), generation, &self.image, SERVICE, id, out).is_ok())
    }

    /// The name of rule `number`.
    pub(crate) fn rule(&self, number: usize) -> Option<RuleName> {
        self.names.rules.get(number).cloned().map(RuleName)
    }

    /// The name of sequence `number`.
    pub(crate) fn sequence_name(&self, number: usize) -> Option<RuleName> {
        self.names.sequences.get(number).cloned().map(RuleName)
    }

    /// A target as the protocol writes it, a rectangle by its name.
    pub(crate) fn target(&self, target: Target) -> Option<RuleTarget> {
        Some(match target.kind {
            TargetKind::Page => RuleTarget::Page(PageWord::Page),
            TargetKind::Control if target.number == 0 => RuleTarget::Control { control: TargetNumber::Any },
            TargetKind::Control => RuleTarget::Control { control: TargetNumber::Numbered(U16In(target.number)) },
            TargetKind::Object if target.number == 0 => RuleTarget::Object { object: TargetNumber::Any },
            TargetKind::Object => RuleTarget::Object { object: TargetNumber::Numbered(U16In(target.number)) },
            TargetKind::Rect => RuleTarget::Rect { rect: RuleName(self.names.rects.get(usize::from(target.number))?.clone()) },
        })
    }

    /// A value as the table's program named it (RULE-36): what a state
    /// answer and a `report` of a value give it as.
    pub(crate) fn value(&self, value: Value) -> Option<RuleValue> {
        let channel = U8In(value.a);
        Some(match value.space {
            table::SPACE_VARIABLE => RuleValue::Variable(VariableValue { variable: VariableName(value.a) }),
            table::SPACE_INPUT => match value.a {
                table::INPUT_BUTTON => RuleValue::Button(ButtonValue { button: U8In(u8::try_from(value.b).ok()?) }),
                table::INPUT_POINTER => RuleValue::Pointer(PointerValue { property: named(&names::POINTER_PROPERTIES, value.b)? }),
                table::INPUT_KEYS_DOWN => RuleValue::KeysDown(NoFields {}),
                kind => RuleValue::Key(KeyValue { key: key_of(KeyKind::from_code(kind)?, value.key_code())? }),
            },
            table::SPACE_TIMER => {
                let timer = if value.a == limits::PAGE_TIMER { TimerRef::Page } else { TimerRef::Timer(TimerName(value.a)) };
                let property = named(&names::TIMER_PROPERTIES, value.b)?;
                // A phase's period is in `c` (high) and `d` (low).
                let period = (u32::from(value.c) << u16::BITS) | u32::from(value.d);
                let period = (period != 0).then_some(crate::protocol::U64In(u64::from(period)));
                RuleValue::Timer(TimerValue { timer, property, period })
            }
            table::SPACE_TARGET => {
                RuleValue::Target(TargetValue { target: self.target(value.named_target()?)?, property: named(&names::TARGET_PROPERTIES, value.c)? })
            }
            table::SPACE_SOUND => RuleValue::Sound(SoundValue { channel, property: named(&names::SOUND_PROPERTIES, value.b)? }),
            table::SPACE_SYNTH => {
                let (family, ..) = *SYNTHESIZERS.iter().find(|(_, code, variant)| *code == value.family() && *variant == value.variant())?;
                let property: SynthProperty = named(&names::SYNTH_PROPERTIES, value.class())?;
                let midi_number = || u8::try_from(value.d).ok().map(U8In);
                let (mut key, mut field, mut controller) = (None, None, None);
                match property {
                    SynthProperty::Note | SynthProperty::Velocity => key = Some(midi_number()?),
                    SynthProperty::Controller => controller = Some(midi_number()?),
                    SynthProperty::Field => field = Some(fields_of(family).get(usize::from(value.d))?.name.to_string()),
                    _ => {}
                }
                RuleValue::Synth(SynthValue { channel, family, part: U8In(value.part().checked_add(1)?), property, key, field, controller })
            }
            table::SPACE_FIRED => RuleValue::Fired(FiredValue { property: named::<FiredName, u16>(&names::FIRED_PROPERTIES, u16::from(value.a))? }),
            table::SPACE_SEQUENCE => RuleValue::Sequence(SequenceValue {
                sequence: self.sequence_name(usize::from(value.a))?,
                property: named(&names::SEQUENCE_PROPERTIES, value.b)?,
            }),
            _ => return None,
        })
    }
}

/// A command sent directly (RULE-114), as the record of its page for one
/// connection whose table in force is `table`: a `run` or a `sequence`
/// names a group or a sequence of that table.
///
/// # Errors
/// The first fault of the command; `unknown_name` for a name the table
/// does not have, or for a `run` or a `sequence` with no table.
pub(crate) fn direct(message: &CommandMessage, table: Option<&Compiled>, record_id: &mut u8) -> Result<Vec<u8>, Refusal> {
    let none = Names::default();
    let names = table.map_or(&none, |table| &table.names);
    let built = crate::protocol::check::direct_record(message, names)?;
    let mut bytes = Vec::new();
    write_packets(built.kind, &built.body, SERVICE, *record_id, &mut |packet| bytes.extend_from_slice(&packet.bytes))
        .ok_or_else(|| Refusal::whole(Some(message.kind()), crate::protocol::Rule::InvalidCommand, String::from("the record does not fit its frame")))?;
    *record_id = record_id.wrapping_add(1);
    Ok(bytes)
}
