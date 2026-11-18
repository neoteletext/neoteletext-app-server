//! A page's rule table as the server keeps it (neoteletext-common
//! `docs/page-rules.md` RULE-36, RULE-40, RULE-59; REQ-6532): the body of
//! its RULES record, encoded and checked with the engine's
//! `neoteletext-rules`, the crate every receiver decodes it with, and the
//! names its program gave its rules, variables, timers and rectangles,
//! which no receiver is sent and by which the server names them back in
//! every message it gives the program.
//!
//! The server has no table of keys, of synthesizers or of their fields:
//! a key's name is the engine's (`neoteletext_input::keys`), and a
//! synthesizer's field is the engine's parameter table of its family
//! (`neoteletext_audio_instruments::watch::fields`, RULE-29).

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use neoteletext_audio_instruments::midi::{PatchField, family};
use neoteletext_audio_instruments::watch;
use neoteletext_input::keys;
use neoteletext_rules::limits;
use neoteletext_rules::table::{self, Fault, HEADER_BYTES, KeyKind, Table, Target, TargetKind, Value};

use crate::protocol::{
    self, AnyWord, ButtonValue, ControlRef, KeyName, KeyValue, LookName, NoFields, ObjectRef, PageWord, PointerProperty, PointerValue, RuleKey, RuleName,
    RuleTarget, RuleValue, SoundProperty, SoundValue, SynthFamily, SynthProperty, SynthValue, TargetProperty, TargetValue, TimerProperty, TimerValue, U8In,
    U16In, VariableValue,
};

/// The most pages whose table the server keeps for a connection
/// (RULE-40, RULE-57): the oldest is dropped first.
pub const MAX_KEPT_TABLES: usize = 64;

// What the schema states of a table is the engine's limit (RULE-57).
const _: () = assert!(protocol::MOST_RULES == limits::MAX_RULES);
const _: () = assert!(protocol::MOST_ACTIONS == limits::MAX_ACTIONS);
const _: () = assert!(protocol::MOST_VARIABLES == limits::MAX_VARIABLES);
const _: () = assert!(protocol::MOST_TIMERS == limits::MAX_TIMERS);
const _: () = assert!(protocol::MOST_RECTS == limits::MAX_RECTS);
const _: () = assert!(protocol::MOST_SUBPAGE_MASKS == limits::MAX_SUBPAGE_MASKS);
const _: () = assert!(protocol::MOST_KEYS == limits::MAX_KEYS_PER_TRIGGER);
const _: () = assert!(protocol::MOST_CHILDREN == limits::MAX_AND_OR_CHILDREN);
const _: () = assert!(protocol::FEWEST_CHILDREN == limits::MIN_AND_OR_CHILDREN);
const _: () = assert!(protocol::MOST_SET_MEMBERS == limits::MAX_SET_MEMBERS);
const _: () = assert!(protocol::MOST_TABLE_TARGETS == limits::MAX_TABLE_TARGETS);
const _: () = assert!(protocol::MOST_TABLE_VALUES == limits::MAX_TABLE_VALUES);
const _: () = assert!(protocol::MOST_OPERATIONS == limits::MAX_ANIMATE_OPERATIONS);

/// The bytes of a RULES body's page and generation, its first two fields
/// (RULE-41).
const PAGE_AND_GENERATION_BYTES: usize = 2 + 2;
const _: () = assert!(PAGE_AND_GENERATION_BYTES < HEADER_BYTES);

/// The names a program gave what its table declares, each list in the
/// table's order, so that a name's place is its number in the record
/// (RULE-36).
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub(crate) struct Names {
    /// The rules' names, in rule order.
    pub(crate) rules: Vec<String>,
    /// The variables' names.
    pub(crate) variables: Vec<String>,
    /// The timers' names.
    pub(crate) timers: Vec<String>,
    /// The rectangles' names.
    pub(crate) rects: Vec<String>,
}

/// The names of the default table's rules (RULE-53), in the order the
/// engine's `default_send_all` writes them.
const DEFAULT_RULES: [&str; 13] =
    ["keys", "keys_up", "text", "press", "release", "move", "hover_in", "hover_out", "drawing_in", "drawing_out", "focus", "blur", "wheel"];

/// A page's table, ready to send: its RULES record's body and its names.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct Compiled {
    /// The record's body as `neoteletext_rules::encode` wrote it, for the
    /// first page at generation 1: [`Compiled::body`] writes the page and
    /// the generation it is sent with.
    body: Vec<u8>,
    /// The names, by number.
    names: Names,
}

/// A synthesizer by its name in the protocol, and its family's codec code
/// and variant in the record (RULE-28).
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

/// A property and its number in the record, for each kind of value.
const POINTER_PROPERTIES: [(PointerProperty, u16); 3] =
    [(PointerProperty::Over, table::POINTER_OVER), (PointerProperty::X, table::POINTER_X), (PointerProperty::Y, table::POINTER_Y)];
const TIMER_PROPERTIES: [(TimerProperty, u16); 2] = [(TimerProperty::Running, table::TIMER_RUNNING), (TimerProperty::Elapsed, table::TIMER_ELAPSED)];
const TARGET_PROPERTIES: [(TargetProperty, u16); 10] = [
    (TargetProperty::Exists, table::TARGET_EXISTS),
    (TargetProperty::Shown, table::TARGET_SHOWN),
    (TargetProperty::Frame, table::TARGET_FRAME),
    (TargetProperty::X, table::TARGET_X),
    (TargetProperty::Y, table::TARGET_Y),
    (TargetProperty::Look, table::TARGET_LOOK),
    (TargetProperty::Layer, table::TARGET_LAYER),
    (TargetProperty::Order, table::TARGET_ORDER),
    (TargetProperty::Cycling, table::TARGET_CYCLING),
    (TargetProperty::Inside, table::TARGET_INSIDE),
];
const SOUND_PROPERTIES: [(SoundProperty, u16); 5] = [
    (SoundProperty::Playing, table::SOUND_PLAYING),
    (SoundProperty::Level, table::SOUND_LEVEL),
    (SoundProperty::Handle, table::SOUND_HANDLE),
    (SoundProperty::Gain, table::SOUND_GAIN),
    (SoundProperty::Local, table::SOUND_LOCAL),
];
const SYNTH_PROPERTIES: [(SynthProperty, u8); 10] = [
    (SynthProperty::Program, table::CLASS_PROGRAM),
    (SynthProperty::Notes, table::CLASS_NOTES),
    (SynthProperty::Note, table::CLASS_NOTE),
    (SynthProperty::Velocity, table::CLASS_VELOCITY),
    (SynthProperty::Field, table::CLASS_FIELD),
    (SynthProperty::EnvelopeStage, table::CLASS_ENVELOPE_STAGE),
    (SynthProperty::EnvelopeLevel, table::CLASS_ENVELOPE_LEVEL),
    (SynthProperty::Voices, table::CLASS_VOICES),
    (SynthProperty::Controller, table::CLASS_CONTROLLER),
    (SynthProperty::Bend, table::CLASS_BEND),
];
/// The looks, by their number in the record (ANIM-6).
const LOOKS: [LookName; 4] = [LookName::Normal, LookName::Hover, LookName::Pressed, LookName::Alternative];

/// The number a name has in `pairs`.
fn number_of<T: Copy + PartialEq, N: Copy + Default>(pairs: &[(T, N)], name: T) -> N {
    pairs.iter().find(|(held, _)| *held == name).map_or_else(N::default, |(_, number)| *number)
}

/// The name a number has in `pairs`.
fn name_of<T: Copy, N: Copy + PartialEq>(pairs: &[(T, N)], number: N) -> Option<T> {
    pairs.iter().find(|(_, held)| *held == number).map(|(name, _)| *name)
}

/// A pointer property's number.
pub(crate) fn pointer_property(name: PointerProperty) -> u16 {
    number_of(&POINTER_PROPERTIES, name)
}

/// A timer property's number.
pub(crate) fn timer_property(name: TimerProperty) -> u16 {
    number_of(&TIMER_PROPERTIES, name)
}

/// A target property's number.
pub(crate) fn target_property(name: TargetProperty) -> u16 {
    number_of(&TARGET_PROPERTIES, name)
}

/// A sound property's number.
pub(crate) fn sound_property(name: SoundProperty) -> u16 {
    number_of(&SOUND_PROPERTIES, name)
}

/// A synthesizer property's class.
pub(crate) fn synth_class(name: SynthProperty) -> u8 {
    number_of(&SYNTH_PROPERTIES, name)
}

/// A look's number.
pub(crate) fn look_number(name: LookName) -> u8 {
    LOOKS.iter().position(|held| *held == name).and_then(|place| u8::try_from(place).ok()).unwrap_or(0)
}

/// The look a number is, or `normal` for one no look has.
pub(crate) fn look_name(number: u8) -> LookName {
    LOOKS.get(usize::from(number)).copied().unwrap_or(LookName::Normal)
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

impl Compiled {
    /// The table `table` with its `names`, encoded with the engine's crate
    /// and so checked by the decoder every receiver has (RULE-59).
    ///
    /// # Errors
    /// What the engine's encoder or decoder refuses.
    pub(crate) fn of(table: &Table, names: Names) -> Result<Self, Fault> {
        let mut body = alloc::vec![0_u8; limits::MAX_TABLE_BYTES];
        let length = neoteletext_rules::encode(limits::FIRST_PAGE, 1, Some(table), &mut body)?;
        body.truncate(length);
        Ok(Self { body, names })
    }

    /// The default table of a channel whose configuration says
    /// `"default_rules": "send_all"` (RULE-53): the engine's, so its bytes
    /// are the same everywhere, with the `wheel` rule when the channel
    /// uses the wheel.
    pub(crate) fn send_all(wheel: bool) -> Option<Self> {
        let mut table = alloc::boxed::Box::new(Table::new());
        neoteletext_rules::default_send_all(wheel, &mut table);
        let rules = DEFAULT_RULES.iter().take(table.rules.len()).map(ToString::to_string).collect();
        Self::of(&table, Names { rules, ..Names::default() }).ok()
    }

    /// The body of the RULES record that gives `page` this table at
    /// `generation` (RULE-41): the page and the generation are its first
    /// four bytes.
    pub(crate) fn body(&self, page: u16, generation: u16) -> Vec<u8> {
        let mut body = self.body.clone();
        for (slot, byte) in body.iter_mut().zip(page.to_be_bytes().into_iter().chain(generation.to_be_bytes())) {
            *slot = byte;
        }
        body
    }

    /// Whether `other` is the same table: the same record but for its page
    /// and generation, and the same names.
    pub(crate) fn is_same(&self, other: &Self) -> bool {
        self.names == other.names && self.body.get(PAGE_AND_GENERATION_BYTES..) == other.body.get(PAGE_AND_GENERATION_BYTES..)
    }

    /// The names.
    pub(crate) const fn names(&self) -> &Names {
        &self.names
    }

    /// The name of rule `number`.
    pub(crate) fn rule(&self, number: usize) -> Option<RuleName> {
        self.names.rules.get(number).cloned().map(RuleName)
    }

    /// A target as the protocol writes it, a rectangle by its name.
    pub(crate) fn target(&self, target: Target) -> Option<RuleTarget> {
        Some(match target.kind {
            TargetKind::Page => RuleTarget::Page(PageWord::Page),
            TargetKind::Control if target.number == 0 => RuleTarget::Control { control: ControlRef::Any(AnyWord::Any) },
            TargetKind::Control => RuleTarget::Control { control: ControlRef::Numbered(U8In(u8::try_from(target.number).ok()?)) },
            TargetKind::Object if target.number == 0 => RuleTarget::Object { object: ObjectRef::Any(AnyWord::Any) },
            TargetKind::Object => RuleTarget::Object { object: ObjectRef::Numbered(U16In(target.number)) },
            TargetKind::Rect => RuleTarget::Rect { rect: RuleName(self.names.rects.get(usize::from(target.number))?.clone()) },
        })
    }

    /// A value as the table's program named it (RULE-36): what a state
    /// answer and a `send` of a value report it as.
    pub(crate) fn value(&self, value: Value) -> Option<RuleValue> {
        let channel = U8In(value.a);
        Some(match value.space {
            table::SPACE_VARIABLE => RuleValue::Variable(VariableValue { name: RuleName(self.names.variables.get(usize::from(value.a))?.clone()) }),
            table::SPACE_INPUT => match value.a {
                table::INPUT_BUTTON => RuleValue::Button(ButtonValue { button: U8In(u8::try_from(value.b).ok()?) }),
                table::INPUT_POINTER => RuleValue::Pointer(PointerValue { property: name_of(&POINTER_PROPERTIES, value.b)? }),
                table::INPUT_KEYS_DOWN => RuleValue::KeysDown(NoFields {}),
                kind => RuleValue::Key(KeyValue { key: key_of(KeyKind::from_code(kind)?, value.key_code())? }),
            },
            table::SPACE_TIMER => {
                let name = if value.a == limits::PAGE_TIMER { None } else { Some(RuleName(self.names.timers.get(usize::from(value.a))?.clone())) };
                RuleValue::Timer(TimerValue { name, property: name_of(&TIMER_PROPERTIES, value.b)? })
            }
            table::SPACE_TARGET => {
                RuleValue::Target(TargetValue { target: self.target(value.named_target()?)?, property: name_of(&TARGET_PROPERTIES, value.c)? })
            }
            table::SPACE_SOUND => RuleValue::Sound(SoundValue { channel, property: name_of(&SOUND_PROPERTIES, value.b)? }),
            table::SPACE_SYNTH => {
                let (family, ..) = *SYNTHESIZERS.iter().find(|(_, code, variant)| *code == value.family() && *variant == value.variant())?;
                let property = name_of(&SYNTH_PROPERTIES, value.class())?;
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
            _ => return None,
        })
    }
}

/// The body of the RULES record that says `page` has no table, at
/// `generation` (RULE-41, RULE-53).
pub(crate) fn no_table(page: u16, generation: u16) -> Vec<u8> {
    let mut body = [0_u8; HEADER_BYTES];
    let length = neoteletext_rules::encode(page, generation, None, &mut body).unwrap_or(0);
    body.get(..length).unwrap_or(&[]).to_vec()
}

/// A limit of RULE-57 by the name the engine's faults give it, in words
/// with its value, for a refusal's detail (RULE-39).
pub(crate) fn limit_in_words(name: &str) -> String {
    let (value, what) = match name {
        "MAX_CONDITION_TERMS" => (limits::MAX_CONDITION_TERMS, "terms in a condition"),
        "MAX_CONDITION_NODES" => (limits::MAX_CONDITION_NODES, "nodes in a condition"),
        "MAX_CONDITION_DEPTH" => (limits::MAX_CONDITION_DEPTH, "levels of and, or and not in a condition"),
        "MAX_TABLE_VALUES" => (limits::MAX_TABLE_VALUES, "distinct values read by a table"),
        "MAX_TABLE_TARGETS" => (limits::MAX_TABLE_TARGETS, "distinct controls and objects named by a table"),
        "MAX_WATCHED" => (limits::MAX_WATCHED, "distinct metered values read by a table"),
        "MAX_TABLE_BYTES" => (limits::MAX_TABLE_BYTES, "bytes of an encoded table"),
        other => return alloc::format!("{other}: over a limit of a rule table"),
    };
    alloc::format!("{name}: at most {value} {what}")
}
