//! The page rule table in the program protocol, version 15
//! (neoteletext-common `docs/page-rules.md` section 7, RULE-34 to RULE-40
//! and RULE-114; REQ-6532; DEC-6587, DEC-6599, DEC-6635): the `rules` and
//! `query_state` messages a program sends, the six command messages
//! (`local_midi`, `variable`, `timer`, `report`, `run`, `sequence`), and the
//! `rule_event`, `page_state` and `rules_fault` messages the server gives
//! it. A table is written with names, which exist only between a program
//! and the server (RULE-36): the server turns each into its number when it
//! encodes the table ([`crate::rules`]).
//!
//! Every limit is the engine's constant of `neoteletext_rules::limits`,
//! used by its name (REQ-6608, RULE-57): no figure of a limit is written
//! here.
//!
//! The shapes that a tag chooses (`event`, `of`, `action`, `type`, `kind`)
//! are read by following the tag, as a sound's are, so that a fault inside
//! one keeps its path; the shapes that their keys choose (a target, a key,
//! a condition, an amount) are read from the JSON value by hand for the
//! same reason.

use alloc::borrow::Cow;
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use neoteletext_rules::limits::{
    self, MAX_AND_OR_CHILDREN, MAX_CUE, MAX_FADE_MS, MAX_GAIN, MAX_GROUP_COMMANDS, MAX_GROUPS, MAX_KEYS_PER_TRIGGER, MAX_LAYER, MAX_LOOK, MAX_ROWS, MAX_RULES,
    MAX_SEQUENCE_REPEAT, MAX_SEQUENCE_STEPS, MAX_SEQUENCE_TICKS, MAX_SEQUENCES, MAX_SET_MEMBERS, MAX_TABLE_TARGETS, MAX_TABLE_VALUES, MAX_TIMER_TICKS,
    MAX_TRIGGERS_PER_RULE, MIN_AND_OR_CHILDREN, TIMERS, VARIABLES,
};
use neoteletext_rules::names;
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};

use super::animation::{MOST_NAPLPS_OBJECTS, MOST_OPERATIONS, MOST_SPRITES, NaplpsObject, Sprite};
use super::read::{Fault, join, typed, typed_value};
use super::sound::{MOST_DUCK, carried, not_listed, tagged, tagged_choice};
use super::{
    AnimationOp, ConnectionId, Connections, Data, I32In, KeyName, MidiChannel, Page, PageNumber, Rule, SAFE_INTEGER_LIMIT, SubCode, U8In, U16In, U64In,
};

/// The most characters of a rule's, a group's, a sequence's, a row set's
/// or a rectangle's name (RULE-36).
pub const NAME_CHARACTERS: usize = 32;
/// What a name matches (RULE-36).
const NAME_PATTERN: &str = "^[a-z][a-z0-9_]{0,31}$";
/// The name a rectangle may not have (RULE-36): it names the page target,
/// as it names the page timer.
pub const PAGE_NAME: &str = names::PAGE_TIMER_NAME;
/// The one parameter of a binding row, as a group or a trigger names it
/// (DEC-6635 item 8).
pub const ROW_PARAMETER_NAME: &str = "p";
/// The word for every variable, which `clear` alone takes (RULE-99).
pub const ALL_NAME: &str = "all";

/// The highest audio channel: one of the four voices of the audio
/// profile, not a MIDI channel.
const LAST_AUDIO_CHANNEL: u8 = neoteletext_audio::CHANNELS - 1;
/// An audio channel.
pub type AudioChannel = U8In<0, LAST_AUDIO_CHANNEL>;
/// Any byte, 0 to 255.
pub type Byte = U8In<0, { u8::MAX }>;
/// Any 16-bit number.
pub type Word = U16In<0, { u16::MAX }>;
/// A signed 32-bit constant.
pub type Constant = I32In<{ i32::MIN }, { i32::MAX }>;
/// A signed 16-bit number: a position, or the constant added to the row's
/// parameter for a target's number.
pub type Offset = I32In<{ i16::MIN as i32 }, { i16::MAX as i32 }>;
/// An order within a layer.
pub type OrderInLayer = I32In<{ i8::MIN as i32 }, { i8::MAX as i32 }>;
/// A layer.
pub type Layer = U8In<0, MAX_LAYER>;
/// A frame number.
pub type FrameNumber = Limited<U8In<0, { limits::MAX_FRAME }>, MaxFrame>;
/// A cycle's period, in ticks.
pub type CyclePeriod = Limited<U16In<1, { limits::MAX_CYCLE_PERIOD }>, MaxCyclePeriod>;
/// The ticks of `every` and `after`, and a beat's period.
pub type TimerTicks = Limited<U64In<1, { MAX_TIMER_TICKS as u64 }>, MaxTimerTicks>;
/// A pointer button: 0 left, 1 middle, 2 right.
pub type Button = U8In<0, { limits::MAX_BUTTON }>;
/// A part of a synthesizer sound: its MIDI channel, 1 to 16.
pub type Part = U8In<1, { limits::MAX_PART + 1 }>;
/// A cue's number (RULE-109).
pub type CueNumber = Limited<U8In<0, MAX_CUE>, MaxCue>;
/// A tick of a sequence: a step's offset, or its length (RULE-103).
pub type SequenceTicks<const LEAST: u64> = Limited<U64In<LEAST, { MAX_SEQUENCE_TICKS as u64 }>, MaxSequenceTicks>;
/// A sequence's stated repeats, or 0 for until stopped (RULE-103).
pub type SequenceRepeat = Limited<U8In<0, MAX_SEQUENCE_REPEAT>, MaxSequenceRepeat>;

// ---------------------------------------------------------- limits

/// A limit of RULE-57 that bounds a value's range, by its constant's name
/// in `neoteletext_rules::limits` (REQ-6608).
pub trait LimitName {
    /// The constant's name, or none for a range no limit of RULE-57 sets.
    const NAME: Option<&'static str>;
}

/// Declares the marker of one limit of RULE-57.
macro_rules! limit_name {
    ($($(#[$meta:meta])* $marker:ident = $name:literal;)*) => {$(
        $(#[$meta])*
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
        pub enum $marker {}

        impl LimitName for $marker {
            const NAME: Option<&'static str> = Some($name);
        }
    )*};
}

limit_name! {
    /// `MAX_FRAME`.
    MaxFrame = "MAX_FRAME";
    /// `MAX_CYCLE_PERIOD`.
    MaxCyclePeriod = "MAX_CYCLE_PERIOD";
    /// `MAX_TIMER_TICKS`.
    MaxTimerTicks = "MAX_TIMER_TICKS";
    /// `MAX_CUE`.
    MaxCue = "MAX_CUE";
    /// `MAX_SEQUENCE_TICKS`.
    MaxSequenceTicks = "MAX_SEQUENCE_TICKS";
    /// `MAX_SEQUENCE_REPEAT`.
    MaxSequenceRepeat = "MAX_SEQUENCE_REPEAT";
}

/// No limit of RULE-57: a range of the record's own.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub enum NoLimit {}

impl LimitName for NoLimit {
    const NAME: Option<&'static str> = None;
}

/// `fault` naming the limit `L` when it is a refusal of a value's range
/// (REQ-6608: a refusal of a limit names its constant and its value).
fn naming<L: LimitName>(mut fault: Fault) -> Fault {
    if let (Rule::OutOfRange, Some(name)) = (fault.rule, L::NAME) {
        let value = limits::value_of(name).unwrap_or_default();
        fault.detail = format!("{}; {name}: at most {value}", fault.detail);
    }
    fault
}

/// A value of `R` whose range a limit of RULE-57 bounds (REQ-6608): read
/// and written in the schema as `R`, a refusal of its range naming the
/// limit's constant and its value.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub struct Limited<R, L>(pub R, core::marker::PhantomData<L>);

impl<R, L> Limited<R, L> {
    /// `value`, held.
    pub const fn new(value: R) -> Self {
        Self(value, core::marker::PhantomData)
    }
}

impl<R, L> core::ops::Deref for Limited<R, L> {
    type Target = R;

    fn deref(&self) -> &R {
        &self.0
    }
}

impl<'de, R: serde::de::DeserializeOwned, L: LimitName> Deserialize<'de> for Limited<R, L> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        read_with(deserializer, |value| typed_value::<R>(value, "").map(Self::new).map_err(naming::<L>))
    }
}

impl<R: Serialize, L> Serialize for Limited<R, L> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<R: JsonSchema, L> JsonSchema for Limited<R, L> {
    fn inline_schema() -> bool {
        R::inline_schema()
    }

    fn schema_name() -> Cow<'static, str> {
        R::schema_name()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        R::json_schema(generator)
    }
}
/// A page tick.
pub type PageTick = U64In<0, { u32::MAX as u64 }>;
/// The highest control number (RULE-12).
const LAST_CONTROL: u16 = limits::MAX_CONTROL;

/// Reads a value whose shape its keys choose, by `read`, carrying a fault
/// out of serde with its path.
fn read_with<'de, D: Deserializer<'de>, T>(deserializer: D, read: impl FnOnce(Value) -> Result<T, Fault>) -> Result<T, D::Error> {
    read(Value::deserialize(deserializer)?).map_err(|fault| carried(&fault))
}

/// The one key and value of an object that must hold exactly one of
/// `keys`.
fn one_key(object: Map<String, Value>, keys: &[&str]) -> Result<(String, Value), Fault> {
    let mut fields = object.into_iter();
    let (Some((key, value)), None) = (fields.next(), fields.next()) else {
        return Err(Fault::new("", Rule::WrongType, format!("an object with one of {keys:?}")));
    };
    if keys.contains(&key.as_str()) {
        Ok((key, value))
    } else {
        Err(Fault::new(&key, Rule::UnknownField, format!("unknown field `{key}`, expected one of {keys:?}")))
    }
}

/// The fields of `object` beyond `known`, as the refusal of the first.
fn no_other(object: &Map<String, Value>, known: &[&str]) -> Result<(), Fault> {
    match object.keys().find(|key| !known.contains(&key.as_str())) {
        Some(extra) => Err(Fault::new(extra, Rule::UnknownField, format!("unknown field `{extra}`, expected one of {known:?}"))),
        None => Ok(()),
    }
}

/// A list of `least` to `most` items, each read by `item` at its place
/// under `at`. When `most` is a limit of RULE-57, `limit` names its
/// constant, which the refusal of a longer list names with its value
/// (REQ-6608).
fn list<T>(
    value: Value,
    at: &str,
    least: usize,
    (most, limit): (usize, Option<&'static str>),
    item: impl Fn(Value, &str) -> Result<T, Fault>,
) -> Result<Vec<T>, Fault> {
    let Value::Array(items) = value else { return Err(Fault::new(at, Rule::WrongType, format!("`{at}` is a list"))) };
    if items.len() < least {
        return Err(Fault::new(at, Rule::TooShort, format!("{} items, at least {least}", items.len())));
    }
    if items.len() > most {
        let named = limit.map(|name| format!("; {name}: at most {}", limits::value_of(name).unwrap_or_default())).unwrap_or_default();
        return Err(Fault::new(at, Rule::TooLong, format!("{} items, at most {most}{named}", items.len())));
    }
    items.into_iter().enumerate().map(|(index, value)| item(value, &join(at, &format!("[{index}]")))).collect()
}

// ------------------------------------------------------------ names

/// A rule's, a group's, a sequence's, a row set's or a rectangle's name
/// (RULE-36): a lower case letter, then lower case letters, digits and
/// `_`, at most 32 characters. Names are distinct within their kind in a
/// table, and no name is sent to a receiver.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Serialize)]
#[serde(transparent)]
pub struct RuleName(pub String);

impl RuleName {
    /// Whether `text` is a name.
    #[must_use]
    pub fn is_name(text: &str) -> bool {
        let bytes = text.as_bytes();
        bytes.len() <= NAME_CHARACTERS
            && bytes.first().is_some_and(u8::is_ascii_lowercase)
            && bytes.iter().all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'_')
    }
}

impl<'de> Deserialize<'de> for RuleName {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        if Self::is_name(&text) {
            Ok(Self(text))
        } else {
            Err(carried(&Fault::new(
                "",
                Rule::WrongType,
                format!("a name is a lower case letter, then lower case letters, digits and _, at most {NAME_CHARACTERS} characters"),
            )))
        }
    }
}

impl JsonSchema for RuleName {
    fn schema_name() -> Cow<'static, str> {
        "RuleName".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "description": "A rule's, a group's, a sequence's, a row set's or a rectangle's name (RULE-36): distinct within its kind in a table, and never sent to a receiver.",
            "type": "string",
            "pattern": NAME_PATTERN
        })
    }
}

/// Reads one of `words`, giving its place, or the refusal naming them.
fn word_of(text: &str, words: &[&str]) -> Result<u8, Fault> {
    words
        .iter()
        .position(|word| *word == text)
        .and_then(|place| u8::try_from(place).ok())
        .ok_or_else(|| Fault::new("", Rule::NotListed, format!("`{text}`, expected one of {words:?}")))
}

/// `fault`, of a designator that is none of a page's fixed `name`
/// (`VARIABLES` or `TIMERS`, RULE-14, RULE-15), naming that limit and its
/// value (REQ-6608).
fn fixed_count(mut fault: Fault, name: &'static str) -> Fault {
    fault.detail = format!("{}; {name}: {}", fault.detail, limits::value_of(name).unwrap_or_default());
    fault
}

/// A variable, `"v0"` to `"v15"` (RULE-14): its number. Every page has the
/// same sixteen, and no table declares or names one otherwise.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct VariableName(pub u8);

impl<'de> Deserialize<'de> for VariableName {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        word_of(&text, &names::VARIABLE_NAMES).map(Self).map_err(|fault| carried(&fixed_count(fault, "VARIABLES")))
    }
}

impl Serialize for VariableName {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(names::VARIABLE_NAMES.get(usize::from(self.0)).copied().unwrap_or_default())
    }
}

impl JsonSchema for VariableName {
    fn schema_name() -> Cow<'static, str> {
        "VariableName".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "description": "One of the sixteen variables every page has (RULE-14): a signed 32-bit integer, zero when the page opens.",
            "type": "string",
            "enum": names::VARIABLE_NAMES
        })
    }
}

/// A variable, or `"all"`, which `clear` alone takes (RULE-99).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum VariableOrAll {
    /// One variable.
    One(VariableName),
    /// All sixteen.
    All,
}

impl<'de> Deserialize<'de> for VariableOrAll {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        if text == ALL_NAME {
            return Ok(Self::All);
        }
        word_of(&text, &names::VARIABLE_NAMES).map(|number| Self::One(VariableName(number))).map_err(|fault| carried(&fixed_count(fault, "VARIABLES")))
    }
}

impl JsonSchema for VariableOrAll {
    fn schema_name() -> Cow<'static, str> {
        "VariableOrAll".into()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        let one = generator.subschema_for::<VariableName>();
        json_schema!({ "anyOf": [one, { "const": ALL_NAME }] })
    }
}

/// A timer a command starts or stops, `"t0"` to `"t7"` (RULE-15).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TimerName(pub u8);

impl<'de> Deserialize<'de> for TimerName {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        word_of(&text, &names::TIMER_NAMES).map(Self).map_err(|fault| carried(&fixed_count(fault, "TIMERS")))
    }
}

impl JsonSchema for TimerName {
    fn schema_name() -> Cow<'static, str> {
        "TimerName".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "description": "One of the eight timers every page has (RULE-15), stopped when the page opens.",
            "type": "string",
            "enum": names::TIMER_NAMES
        })
    }
}

/// A timer a trigger or a value reads: `"t0"` to `"t7"`, or `"page"`, the
/// page timer, which runs from the page's opening (RULE-15).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum TimerRef {
    /// The page timer.
    #[default]
    Page,
    /// One of the eight.
    Timer(TimerName),
}

impl TimerRef {
    /// Its number in a record: the page timer's is `PAGE_TIMER`.
    #[must_use]
    pub const fn number(self) -> u8 {
        match self {
            Self::Page => limits::PAGE_TIMER,
            Self::Timer(timer) => timer.0,
        }
    }
}

impl<'de> Deserialize<'de> for TimerRef {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        if text == names::PAGE_TIMER_NAME {
            return Ok(Self::Page);
        }
        word_of(&text, &names::TIMER_NAMES).map(|number| Self::Timer(TimerName(number))).map_err(|fault| carried(&fault))
    }
}

impl Serialize for TimerRef {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Page => serializer.serialize_str(names::PAGE_TIMER_NAME),
            Self::Timer(timer) => serializer.serialize_str(names::TIMER_NAMES.get(usize::from(timer.0)).copied().unwrap_or_default()),
        }
    }
}

impl JsonSchema for TimerRef {
    fn schema_name() -> Cow<'static, str> {
        "TimerRef".into()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        let timer = generator.subschema_for::<TimerName>();
        json_schema!({ "anyOf": [timer, { "const": names::PAGE_TIMER_NAME }] })
    }
}

/// `"any"`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AnyWord {
    /// Any one.
    Any,
}

/// `"page"`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PageWord {
    /// The whole page's picture.
    Page,
}

/// `"p"`: the row's one parameter (RULE-105, DEC-6635 item 8).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, JsonSchema)]
pub enum RowWord {
    /// The parameter of the firing row; 0 for a rule without rows.
    #[serde(rename = "p")]
    P,
}

/// Nothing: a tagged shape with no field beside its tag.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NoFields {}

/// A value of the firing (RULE-116): what the event and the row that fired
/// the rule give.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FiredName {
    /// The code of the key whose event fired the rule; 0 when none did.
    Key,
    /// The number of the control, object or rectangle the pointer event
    /// was inside; 0 otherwise.
    Target,
    /// The place of the firing row within the rule's rows; 0 without rows.
    Row,
    /// The number of the cue that fired the rule; 0 otherwise.
    Cue,
    /// The row's parameter; 0 when none.
    P,
}

// ---------------------------------------------------------- targets

/// What stands for a target's number beside a constant (RULE-105,
/// RULE-106): the row's parameter, or in an operation the firing's target
/// or row.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TargetSource {
    /// `{"row":"p"}`: the row's parameter.
    Row,
    /// `{"fired":"target"}`: the target the viewer's own pointer event
    /// named.
    FiredTarget,
    /// `{"fired":"row"}`: the firing row's place.
    FiredRow,
}

/// A control's or an object's number: one number, `"any"` in a pointer
/// trigger, or a constant plus a source (RULE-12, RULE-105, RULE-106).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TargetNumber<const MOST: u16> {
    /// Any: the rule fires once for each control or object concerned.
    Any,
    /// One number.
    Numbered(U16In<1, MOST>),
    /// A constant plus a source.
    Bound {
        /// The constant, a signed 16-bit integer.
        plus: i32,
        /// What is added to it.
        source: TargetSource,
    },
}

impl<const MOST: u16> TargetNumber<MOST> {
    /// The keys a bound number's object holds.
    const KEYS: [&'static str; 3] = ["plus", "row", "fired"];

    fn from_value(value: Value, at: &str) -> Result<Self, Fault> {
        match value {
            Value::String(word) if word == "any" => Ok(Self::Any),
            Value::String(word) => Err(Fault::new(at, Rule::NotListed, format!("`{word}`: a number, \"any\", or a number from the row or the firing"))),
            Value::Object(mut object) => {
                // RULE-106: a variable choosing which object is acted on
                // would be a computed address (RULE-1).
                if object.contains_key("variable") {
                    return Err(Fault::new(
                        &join(at, "variable"),
                        Rule::InvalidGroup,
                        "a variable never gives a target's number: a row's or the firing's does",
                    ));
                }
                no_other(&object, &Self::KEYS).map_err(|fault| Fault::new(&join(at, &fault.path), fault.rule, fault.detail))?;
                let plus: Offset = object.remove("plus").map_or(Ok(I32In(0)), |plus| typed_value(plus, &join(at, "plus")))?;
                let source = match (object.remove("row"), object.remove("fired")) {
                    (Some(row), None) => {
                        let _: RowWord = typed_value(row, &join(at, "row"))?;
                        TargetSource::Row
                    }
                    (None, Some(fired)) => match typed_value::<FiredName>(fired, &join(at, "fired"))? {
                        FiredName::Target => TargetSource::FiredTarget,
                        FiredName::Row => TargetSource::FiredRow,
                        _ => return Err(Fault::new(&join(at, "fired"), Rule::NotListed, "a target's number is from the firing's `target` or `row`")),
                    },
                    _ => return Err(Fault::new(at, Rule::MissingField, "a number from the row or the firing has one of `row` and `fired`")),
                };
                Ok(Self::Bound { plus: plus.get(), source })
            }
            number => typed_value(number, at).map(Self::Numbered),
        }
    }
}

impl<const MOST: u16> Serialize for TargetNumber<MOST> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Any => serializer.serialize_str("any"),
            Self::Numbered(number) => serializer.serialize_u16(number.get()),
            Self::Bound { plus, source } => {
                let mut object = Map::new();
                object.insert("plus".to_string(), (*plus).into());
                let (key, word) = match source {
                    TargetSource::Row => ("row", ROW_PARAMETER_NAME),
                    TargetSource::FiredTarget => ("fired", "target"),
                    TargetSource::FiredRow => ("fired", "row"),
                };
                object.insert(key.to_string(), word.into());
                Value::Object(object).serialize(serializer)
            }
        }
    }
}

impl<const MOST: u16> JsonSchema for TargetNumber<MOST> {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> Cow<'static, str> {
        format!("TargetNumber_{MOST}").into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "anyOf": [
                { "type": "integer", "format": "uint16", "minimum": 1, "maximum": MOST },
                { "const": "any" },
                {
                    "type": "object",
                    "properties": {
                        "plus": { "type": "integer", "format": "int16", "minimum": i16::MIN, "maximum": i16::MAX },
                        "row": { "const": ROW_PARAMETER_NAME },
                        "fired": { "enum": ["target", "row"] }
                    },
                    "oneOf": [{ "required": ["row"] }, { "required": ["fired"] }],
                    "additionalProperties": false
                }
            ]
        })
    }
}

/// Something on the page (RULE-12): `"page"`, `{"control": n}`,
/// `{"object": n}` or `{"rect": "name"}`. A control or an object may be
/// `"any"` in the six pointer triggers, and a constant plus the row's
/// parameter (in a rule with rows, and in an operation of a group) or plus
/// the firing's target or row (in an operation of a group).
#[derive(Clone, PartialEq, Eq, Debug, Serialize, JsonSchema)]
#[serde(untagged, deny_unknown_fields)]
pub enum RuleTarget {
    /// The whole page's picture.
    Page(PageWord),
    /// A teletext control of the page.
    Control {
        /// Which.
        control: TargetNumber<LAST_CONTROL>,
    },
    /// An animation object of the page.
    Object {
        /// Which.
        object: TargetNumber<{ u16::MAX }>,
    },
    /// A rectangle the table declares.
    Rect {
        /// Its name.
        rect: RuleName,
    },
}

impl RuleTarget {
    /// The keys one of which a target's object holds.
    const KEYS: [&'static str; 3] = ["control", "object", "rect"];

    fn from_value(value: Value) -> Result<Self, Fault> {
        match value {
            Value::String(word) if word == PAGE_NAME => Ok(Self::Page(PageWord::Page)),
            Value::String(word) => Err(Fault::new("", Rule::NotListed, format!("`{word}`: \"page\", or an object with one of {:?}", Self::KEYS))),
            Value::Object(object) => {
                let (key, value) = one_key(object, &Self::KEYS)?;
                Ok(match key.as_str() {
                    "control" => Self::Control { control: TargetNumber::from_value(value, "control")? },
                    "object" => Self::Object { object: TargetNumber::from_value(value, "object")? },
                    _ => Self::Rect { rect: typed_value(value, "rect")? },
                })
            }
            _ => Err(Fault::new("", Rule::WrongType, format!("a target is \"page\", or an object with one of {:?}", Self::KEYS))),
        }
    }
}

impl<'de> Deserialize<'de> for RuleTarget {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        read_with(deserializer, Self::from_value)
    }
}

// -------------------------------------------------------------- keys

/// One key (RULE-10), named in one of the input protocol's three ways.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, JsonSchema)]
#[serde(untagged, deny_unknown_fields)]
pub enum RuleKey {
    /// A character key: the character the active keyboard layout gives the
    /// key without Shift, a letter in lower case.
    Char {
        /// One Unicode scalar value.
        #[schemars(length(min = 1, max = 1))]
        char: String,
    },
    /// A named key, by its name in the engine's table.
    Name {
        /// The name, such as `Enter` or `ArrowLeft`.
        name: KeyName,
    },
    /// A physical key: the key that carries the character on a United
    /// States layout, whatever the active layout.
    Physical {
        /// One Unicode scalar value.
        #[schemars(length(min = 1, max = 1))]
        physical: String,
    },
}

impl RuleKey {
    /// The keys one of which a key's object holds.
    const KEYS: [&'static str; 3] = ["char", "name", "physical"];

    fn from_value(value: Value) -> Result<Self, Fault> {
        let Value::Object(object) = value else {
            return Err(Fault::new("", Rule::WrongType, format!("a key is an object with one of {:?}", Self::KEYS)));
        };
        let (key, value) = one_key(object, &Self::KEYS)?;
        let Value::String(text) = value else {
            return Err(Fault::new(&key, Rule::WrongType, format!("`{key}` is a string")));
        };
        let one_character = |text: &str| {
            let mut characters = text.chars();
            match (characters.next(), characters.next()) {
                (Some(_), None) => Ok(()),
                (None, _) => Err(Fault::new(&key, Rule::TooShort, "one character".to_string())),
                _ => Err(Fault::new(&key, Rule::TooLong, "one character".to_string())),
            }
        };
        Ok(match key.as_str() {
            "char" => {
                one_character(&text)?;
                Self::Char { char: text }
            }
            "physical" => {
                one_character(&text)?;
                Self::Physical { physical: text }
            }
            _ => Self::Name { name: KeyName(text) },
        })
    }
}

impl<'de> Deserialize<'de> for RuleKey {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        read_with(deserializer, Self::from_value)
    }
}

/// A key set (RULE-10): `"any"`, or 1 to `MAX_KEYS_PER_TRIGGER` keys, any
/// of which matches, or with `by_row` the key of each row in turn.
#[derive(Clone, PartialEq, Eq, Debug, JsonSchema)]
#[serde(untagged)]
pub enum KeySet {
    /// Any key.
    Any(AnyWord),
    /// The keys named.
    Keys(#[schemars(length(min = 1, max = MAX_KEYS_PER_TRIGGER))] Vec<RuleKey>),
}

impl<'de> Deserialize<'de> for KeySet {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        read_with(deserializer, |value| match value {
            Value::String(word) if word == "any" => Ok(Self::Any(AnyWord::Any)),
            Value::String(word) => Err(Fault::new("", Rule::NotListed, format!("`{word}`: \"any\", or a list of keys"))),
            Value::Array(keys) if keys.is_empty() => Err(Fault::new("", Rule::TooShort, "no key; \"any\" names every one".to_string())),
            keys @ Value::Array(_) => list(keys, "", 1, (MAX_KEYS_PER_TRIGGER, Some("MAX_KEYS_PER_TRIGGER")), typed_value).map(Self::Keys),
            _ => Err(Fault::new("", Rule::WrongType, "a key set is \"any\", or a list of keys".to_string())),
        })
    }
}

/// Which modifier keys a key trigger needs (RULE-9): it matches when
/// `(mods AND mask) = value`, on the input protocol's modifier bits
/// (DOC-1935: Shift 1, Alt 2, Control 4, Super 8, Hyper 16, Meta 32, Caps
/// Lock 64, Num Lock 128). Mask 0 ignores modifiers.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Mods {
    /// The bits compared.
    pub mask: Byte,
    /// What they must be.
    pub value: Byte,
}

impl Default for Mods {
    fn default() -> Self {
        Self { mask: U8In(0), value: U8In(0) }
    }
}

/// `"any"`, or a number read as `T` at `at`.
fn any_or<T: serde::de::DeserializeOwned, R>(value: Value, at: &str, any: R, numbered: impl FnOnce(T) -> R) -> Result<R, Fault> {
    match value {
        Value::String(word) if word == "any" => Ok(any),
        Value::String(word) => Err(Fault::new(at, Rule::NotListed, format!("`{word}`: a number, or \"any\""))),
        number => typed_value(number, at).map(numbered),
    }
}

/// The button of a pointer trigger: 0 left (or a touch contact), 1
/// middle, 2 right, or `"any"`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, JsonSchema)]
#[serde(untagged)]
pub enum ButtonChoice {
    /// Any button.
    Any(AnyWord),
    /// One button.
    Numbered(Button),
}

impl Default for ButtonChoice {
    fn default() -> Self {
        Self::Any(AnyWord::Any)
    }
}

impl<'de> Deserialize<'de> for ButtonChoice {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        read_with(deserializer, |value| any_or(value, "", Self::Any(AnyWord::Any), Self::Numbered))
    }
}

/// The cue of a `cue` trigger: a number, or `"any"`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, JsonSchema)]
#[serde(untagged)]
pub enum CueChoice {
    /// Any cue.
    Any(AnyWord),
    /// One cue.
    Numbered(CueNumber),
}

impl Default for CueChoice {
    fn default() -> Self {
        Self::Any(AnyWord::Any)
    }
}

impl<'de> Deserialize<'de> for CueChoice {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        read_with(deserializer, |value| any_or(value, "", Self::Any(AnyWord::Any), Self::Numbered))
    }
}

// ---------------------------------------------------------- triggers

/// `key`: a key of the set is pressed.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct KeyTrigger {
    /// The keys.
    pub keys: KeySet,
    /// Whether each automatic repeat fires too.
    #[serde(default)]
    pub repeat: bool,
    /// One key a row (RULE-105): the set holds as many keys as the rule's
    /// row set has rows, and the row at each place is matched by the key at
    /// the same place. Only a rule with `rows` has it.
    #[serde(default)]
    pub by_row: bool,
    /// The modifier keys it needs.
    #[serde(default)]
    pub mods: Mods,
}

/// `key_up`: a key of the set is released, or lost.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct KeyUpTrigger {
    /// The keys.
    pub keys: KeySet,
    /// One key a row, as for `key`.
    #[serde(default)]
    pub by_row: bool,
    /// The modifier keys it needs.
    #[serde(default)]
    pub mods: Mods,
}

/// `press` and `release`: a pointer button or a touch contact goes down
/// inside the target, or one that went down inside it comes up.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ButtonTrigger {
    /// Where.
    pub target: RuleTarget,
    /// Which button.
    #[serde(default)]
    pub button: ButtonChoice,
}

/// `enter`, `leave` and `wheel`: the pointer comes inside the target, goes
/// outside it, or the wheel turns with the pointer inside it.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TargetTrigger {
    /// Where.
    pub target: RuleTarget,
}

/// `move`: the pointer moves to another pixel inside the target.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MoveTrigger {
    /// Where.
    pub target: RuleTarget,
    /// Only while a button is down.
    #[serde(default)]
    pub held: bool,
}

/// `every` and `after`: each `ticks` ticks after the timer started while
/// it runs, or once, `ticks` ticks after it started (RULE-15).
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TimerTrigger {
    /// How many ticks, sixty a second.
    pub ticks: TimerTicks,
    /// The timer; the page timer when left out.
    #[serde(default)]
    pub timer: TimerRef,
}

/// `cue`: a cue of the sound playing on an audio channel is heard
/// (RULE-109).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CueTrigger {
    /// The audio channel.
    pub channel: AudioChannel,
    /// The cue; any when left out.
    #[serde(default)]
    pub number: CueChoice,
}

/// A rule's trigger (RULE-9, RULE-35), by `event`: the event that can
/// make the rule fire.
#[derive(Clone, PartialEq, Eq, Debug, JsonSchema)]
#[serde(tag = "event", rename_all = "snake_case", deny_unknown_fields)]
pub enum RuleTrigger {
    /// A key of the set is pressed.
    Key(KeyTrigger),
    /// A key of the set is released, or lost.
    KeyUp(KeyUpTrigger),
    /// A button or a contact goes down inside the target.
    Press(ButtonTrigger),
    /// A button or a contact that went down inside the target comes up.
    Release(ButtonTrigger),
    /// The pointer comes inside the target.
    Enter(TargetTrigger),
    /// The pointer goes outside the target, or leaves the page.
    Leave(TargetTrigger),
    /// The wheel turns with the pointer inside the target.
    Wheel(TargetTrigger),
    /// The pointer moves to another pixel inside the target.
    Move(MoveTrigger),
    /// The page opens: page tick 0.
    Open(NoFields),
    /// The page is closed while its connection stands.
    Close(NoFields),
    /// Text arrives that is not a key: a paste, an input method's
    /// composition or its committed text.
    Text(NoFields),
    /// The page gains the keyboard.
    Focus(NoFields),
    /// The page loses the keyboard.
    Blur(NoFields),
    /// The rule's `when` becomes true: its only trigger.
    Becomes(NoFields),
    /// Each so many ticks after a timer started.
    Every(TimerTrigger),
    /// Once, so many ticks after a timer started.
    After(TimerTrigger),
    /// A cue of a playing sound is heard.
    Cue(CueTrigger),
}

/// The triggers DEC-6635 item 7 withdraws, refused by name.
const WITHDRAWN_TRIGGERS: [&str; 4] = ["held", "double", "press_held", "double_press"];

impl<'de> Deserialize<'de> for RuleTrigger {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        tagged(deserializer, "event", |tag, object| {
            Ok(match tag {
                "key" => Self::Key(typed(object, "")?),
                "key_up" => Self::KeyUp(typed(object, "")?),
                "press" => Self::Press(typed(object, "")?),
                "release" => Self::Release(typed(object, "")?),
                "enter" => Self::Enter(typed(object, "")?),
                "leave" => Self::Leave(typed(object, "")?),
                "wheel" => Self::Wheel(typed(object, "")?),
                "move" => Self::Move(typed(object, "")?),
                "open" => Self::Open(typed(object, "")?),
                "close" => Self::Close(typed(object, "")?),
                "text" => Self::Text(typed(object, "")?),
                "focus" => Self::Focus(typed(object, "")?),
                "blur" => Self::Blur(typed(object, "")?),
                "becomes" => Self::Becomes(typed(object, "")?),
                "every" => Self::Every(typed(object, "")?),
                "after" => Self::After(typed(object, "")?),
                "cue" => Self::Cue(typed(object, "")?),
                withdrawn if WITHDRAWN_TRIGGERS.contains(&withdrawn) => {
                    return Err(Fault::new(
                        "event",
                        Rule::WrongType,
                        format!("`{withdrawn}` is withdrawn (DEC-6635 item 7): a long or a double press is a timer and a variable in two or three rules"),
                    ));
                }
                other => {
                    return Err(not_listed(
                        "event",
                        other,
                        &[
                            "key", "key_up", "press", "release", "enter", "leave", "wheel", "move", "open", "close", "text", "focus", "blur", "becomes",
                            "every", "after", "cue",
                        ],
                    ));
                }
            })
        })
    }
}

/// A rule's `on`: one trigger, or a list of 1 to `MAX_TRIGGERS_PER_RULE`,
/// any of which fires the rule (RULE-107).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Triggers(pub Vec<RuleTrigger>);

impl<'de> Deserialize<'de> for Triggers {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        read_with(deserializer, |value| match value {
            many @ Value::Array(_) => list(many, "", 1, (MAX_TRIGGERS_PER_RULE, Some("MAX_TRIGGERS_PER_RULE")), typed_value).map(Self),
            one => typed_value(one, "").map(|trigger| Self(alloc::vec![trigger])),
        })
    }
}

impl JsonSchema for Triggers {
    fn schema_name() -> Cow<'static, str> {
        "Triggers".into()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        let trigger = generator.subschema_for::<RuleTrigger>();
        json_schema!({
            "description": "One trigger, or a list of triggers, any of which fires the rule (RULE-107).",
            "anyOf": [trigger, { "type": "array", "items": trigger, "minItems": 1, "maxItems": MAX_TRIGGERS_PER_RULE }]
        })
    }
}

// ------------------------------------------------------------ values

/// `variable`: a variable's value (RULE-23).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VariableValue {
    /// The variable.
    pub variable: VariableName,
}

/// `key`: 1 while the key is down, else 0 (RULE-24).
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct KeyValue {
    /// The key.
    pub key: RuleKey,
}

/// `button`: 1 while the pointer button is down over the page, else 0.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ButtonValue {
    /// The button: 0 left, 1 middle, 2 right.
    pub button: Button,
}

/// What `pointer` reads.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PointerProperty {
    /// 1 while the pointer is over the page, else 0.
    Over,
    /// Its x pixel in the picture, 0 while it is not over the page.
    X,
    /// Its y pixel.
    Y,
}

/// `pointer`: where the pointer is.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PointerValue {
    /// What is read.
    pub property: PointerProperty,
}

/// What `timer` reads (RULE-25).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TimerProperty {
    /// 1 while it runs, else 0.
    Running,
    /// The ticks since it started, 0 while it is stopped.
    Elapsed,
    /// Where it stands in a period of `period` ticks: the elapsed ticks
    /// less the greatest multiple of the period (DEC-6635 item 6).
    Phase,
}

/// `timer`: a timer's state (RULE-25).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TimerValue {
    /// The timer; the page timer when left out.
    #[serde(default)]
    pub timer: TimerRef,
    /// What is read.
    pub property: TimerProperty,
    /// The period, with `phase` only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub period: Option<TimerTicks>,
}

/// What `target` reads (RULE-26).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TargetProperty {
    /// 1 when it exists on the page, else 0.
    Exists,
    /// 1 while it is shown.
    Shown,
    /// Its frame in force.
    Frame,
    /// Its position across, in its renderer's units; a control's first
    /// column.
    X,
    /// Its position down; a control's row.
    Y,
    /// The look in force: 0 normal, 1 hover, 2 pressed, 3 alternative.
    Look,
    /// Its layer.
    Layer,
    /// Its order within the layer.
    Order,
    /// 1 while its frames cycle.
    Cycling,
    /// 1 while the pointer is inside it.
    Inside,
}

/// `target`: the state of a control or an object.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TargetValue {
    /// One control or one object.
    pub target: RuleTarget,
    /// What is read.
    pub property: TargetProperty,
}

/// What `sound` reads (RULE-27).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SoundProperty {
    /// 1 while any sound plays on the channel in this view, else 0.
    Playing,
    /// The channel's output level over the last tick, 0 to 255, linear.
    Level,
    /// The handle of the sound playing, or −1 when none plays.
    Handle,
    /// The channel's gain as last commanded.
    Gain,
    /// 1 while the sound playing is a local sound, else 0.
    Local,
}

/// `sound`: the sound on an audio channel.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SoundValue {
    /// The audio channel.
    pub channel: AudioChannel,
    /// What is read.
    pub property: SoundProperty,
}

/// The synthesizer a `synth` value expects on the audio channel
/// (RULE-28): when the sound playing there is another's, every property
/// reads 0.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[allow(missing_docs, reason = "the synthesizers' names")]
pub enum SynthFamily {
    Synth6581,
    Synth8580,
    Synth7,
    Synth106,
    Synth60,
    Synthd,
    Synth53,
    Synth52,
    Synth51,
    Synth8,
    Synth80,
}

/// What `synth` reads (RULE-28).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SynthProperty {
    /// The part's program, 0 to 127.
    Program,
    /// How many of the part's keys are down.
    Notes,
    /// 1 while `key` is down on the part, else 0.
    Note,
    /// The velocity of `key`'s note on; 0 while it is up.
    Velocity,
    /// The present value of the patch field `field` on the part.
    Field,
    /// Of the part's newest voice: 0 idle, 1 attack, 2 decay, 3 sustain,
    /// 4 release.
    EnvelopeStage,
    /// The same voice's amplitude envelope, 0 to 255.
    EnvelopeLevel,
    /// How many of the instrument's voices are not idle.
    Voices,
    /// The last value of controller `controller` on the part.
    Controller,
    /// The part's pitch bend, 0 to 16,383, 8,192 at rest.
    Bend,
}

/// The longest name of a synthesizer's patch field.
pub const FIELD_NAME_CHARACTERS: usize = 64;

/// `synth`: a synthesizer's state (RULE-28).
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SynthValue {
    /// The audio channel.
    pub channel: AudioChannel,
    /// The synthesizer expected there.
    pub family: SynthFamily,
    /// The part, 1 to 16; for synth6581 and synth8580 the voice, 1 to 3.
    pub part: Part,
    /// What is read.
    pub property: SynthProperty,
    /// The key, with `note` and `velocity` only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<Data>,
    /// The patch field's name, with `field` only: one of the family's
    /// readable fields (page-rules.md appendix A).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(min = 1, max = FIELD_NAME_CHARACTERS))]
    pub field: Option<String>,
    /// The controller, with `controller` only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub controller: Option<Data>,
}

/// `fired`: a value of the firing (RULE-116).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FiredValue {
    /// What is read.
    pub property: FiredName,
}

/// What `sequence` reads (RULE-117).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SequenceProperty {
    /// 1 while it runs, else 0.
    Running,
    /// The passes completed since its start.
    Passes,
    /// Its position in ticks within the pass; 0 while stopped.
    Position,
    /// 1 while it waits for the tick it starts at, else 0.
    Waiting,
}

/// `sequence`: a sequence's state (RULE-117).
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SequenceValue {
    /// The sequence's name.
    pub sequence: RuleName,
    /// What is read.
    pub property: SequenceProperty,
}

/// Something a condition, a `report` or a `set` reads (section 6), by
/// `of`. Every value is a signed 32-bit integer.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, JsonSchema)]
#[serde(tag = "of", rename_all = "snake_case", deny_unknown_fields)]
pub enum RuleValue {
    /// A variable.
    Variable(VariableValue),
    /// Whether a key is down.
    Key(KeyValue),
    /// Whether a pointer button is down.
    Button(ButtonValue),
    /// Where the pointer is.
    Pointer(PointerValue),
    /// How many keys are down.
    KeysDown(NoFields),
    /// A timer's state.
    Timer(TimerValue),
    /// The state of a control or an object.
    Target(TargetValue),
    /// The sound on an audio channel.
    Sound(SoundValue),
    /// A synthesizer's state.
    Synth(SynthValue),
    /// A value of the firing.
    Fired(FiredValue),
    /// A sequence's state.
    Sequence(SequenceValue),
}

tagged_choice!(RuleValue, "of", {
    "variable" => Variable,
    "key" => Key,
    "button" => Button,
    "pointer" => Pointer,
    "keys_down" => KeysDown,
    "timer" => Timer,
    "target" => Target,
    "sound" => Sound,
    "synth" => Synth,
    "fired" => Fired,
    "sequence" => Sequence,
});

// -------------------------------------------------------- conditions

/// How a value is compared with a constant.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[allow(missing_docs, reason = "the six comparisons")]
pub enum CompareOp {
    #[serde(rename = "==")]
    Equal,
    #[serde(rename = "!=")]
    NotEqual,
    #[serde(rename = "<")]
    Less,
    #[serde(rename = "<=")]
    LessOrEqual,
    #[serde(rename = ">")]
    Greater,
    #[serde(rename = ">=")]
    GreaterOrEqual,
}

/// What a value is compared with: a constant, or in a rule with rows a
/// constant plus the row's parameter (RULE-18, RULE-105).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CompareValue {
    /// A constant.
    Constant(i32),
    /// `{"plus":k,"row":"p"}`.
    PlusRow(i32),
}

impl<'de> Deserialize<'de> for CompareValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        read_with(deserializer, |value| match value {
            Value::Object(mut object) => {
                no_other(&object, &["plus", "row"])?;
                let plus: Constant = object.remove("plus").map_or(Ok(I32In(0)), |plus| typed_value(plus, "plus"))?;
                let row = object.remove("row").ok_or_else(|| Fault::new("row", Rule::MissingField, "missing field `row`"))?;
                let _: RowWord = typed_value(row, "row")?;
                Ok(Self::PlusRow(plus.get()))
            }
            constant => typed_value::<Constant>(constant, "").map(|constant| Self::Constant(constant.get())),
        })
    }
}

impl JsonSchema for CompareValue {
    fn schema_name() -> Cow<'static, str> {
        "CompareValue".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "anyOf": [
                { "type": "integer", "format": "int32", "minimum": i32::MIN, "maximum": i32::MAX },
                {
                    "type": "object",
                    "properties": {
                        "plus": { "type": "integer", "format": "int32", "minimum": i32::MIN, "maximum": i32::MAX },
                        "row": { "const": ROW_PARAMETER_NAME }
                    },
                    "required": ["row"],
                    "additionalProperties": false
                }
            ]
        })
    }
}

/// A condition (RULE-18): a tree of at most `MAX_CONDITION_NODES` nodes,
/// of which at most `MAX_CONDITION_TERMS` are terms, with `and`, `or` and
/// `not` nested at most `MAX_CONDITION_DEPTH` deep. Every comparison is of
/// a value with a constant.
#[derive(Clone, PartialEq, Eq, Debug, JsonSchema)]
#[serde(untagged, deny_unknown_fields)]
pub enum RuleCondition {
    /// True when all of its children are.
    And {
        /// The children.
        #[schemars(length(min = MIN_AND_OR_CHILDREN, max = MAX_AND_OR_CHILDREN))]
        and: Vec<RuleCondition>,
    },
    /// True when any of its children is.
    Or {
        /// The children.
        #[schemars(length(min = MIN_AND_OR_CHILDREN, max = MAX_AND_OR_CHILDREN))]
        or: Vec<RuleCondition>,
    },
    /// True when its child is false.
    Not {
        /// The child.
        not: Box<RuleCondition>,
    },
    /// A value compared with a constant.
    Compare {
        /// What is read.
        read: RuleValue,
        /// How it is compared.
        op: CompareOp,
        /// With what.
        value: CompareValue,
    },
    /// True when `low ≤ value ≤ high`.
    InRange {
        /// What is read.
        read: RuleValue,
        /// `[low, high]`, `low` at most `high`.
        in_range: [Constant; 2],
    },
    /// True when the value equals one of the constants.
    InSet {
        /// What is read.
        read: RuleValue,
        /// The constants.
        #[schemars(length(min = 1, max = MAX_SET_MEMBERS))]
        in_set: Vec<Constant>,
    },
    /// True when the two targets' bounding rectangles share a pixel
    /// (RULE-19).
    Overlaps {
        /// The two targets.
        overlaps: [RuleTarget; 2],
    },
    /// True when the first target's bounding rectangle lies wholly within
    /// the second's.
    Inside {
        /// The two targets.
        inside: [RuleTarget; 2],
    },
}

impl RuleCondition {
    /// The keys that choose a condition's shape.
    const KEYS: [&'static str; 6] = ["and", "or", "not", "read", "overlaps", "inside"];

    /// Exactly two of what `item` reads.
    fn pair<T>(value: Value, at: &str, item: impl Fn(Value, &str) -> Result<T, Fault>) -> Result<[T; 2], Fault> {
        let mut items = list(value, at, 2, (2, None), item)?.into_iter();
        match (items.next(), items.next()) {
            (Some(first), Some(second)) => Ok([first, second]),
            _ => Err(Fault::new(at, Rule::TooShort, "two items".to_string())),
        }
    }

    /// The condition under `at`.
    fn at(value: Value, at: &str) -> Result<Self, Fault> {
        Self::from_value(value).map_err(|fault| Fault::new(&join(at, &fault.path), fault.rule, fault.detail))
    }

    fn from_value(value: Value) -> Result<Self, Fault> {
        let Value::Object(mut object) = value else {
            return Err(Fault::new("", Rule::WrongType, format!("a condition is an object with one of {:?}", Self::KEYS)));
        };
        let Some(key) = Self::KEYS.into_iter().find(|key| object.contains_key(*key)) else {
            return Err(Fault::new("", Rule::MissingField, format!("a condition has one of {:?}", Self::KEYS)));
        };
        let value = object.remove(key).unwrap_or(Value::Null);
        let mut take = |name: &str| object.remove(name);
        let condition = match key {
            "and" => Self::And { and: list(value, "and", MIN_AND_OR_CHILDREN, (MAX_AND_OR_CHILDREN, Some("MAX_AND_OR_CHILDREN")), Self::at)? },
            "or" => Self::Or { or: list(value, "or", MIN_AND_OR_CHILDREN, (MAX_AND_OR_CHILDREN, Some("MAX_AND_OR_CHILDREN")), Self::at)? },
            "not" => Self::Not { not: Box::new(Self::at(value, "not")?) },
            "overlaps" => Self::Overlaps { overlaps: Self::pair(value, "overlaps", typed_value)? },
            "inside" => Self::Inside { inside: Self::pair(value, "inside", typed_value)? },
            _ => {
                let read: RuleValue = typed_value(value, "read")?;
                if let Some(op) = take("op") {
                    let op = typed_value(op, "op")?;
                    let value = take("value").ok_or_else(|| Fault::new("value", Rule::MissingField, "missing field `value`"))?;
                    Self::Compare { read, op, value: typed_value(value, "value")? }
                } else if let Some(range) = take("in_range") {
                    Self::InRange { read, in_range: Self::pair(range, "in_range", typed_value)? }
                } else if let Some(set) = take("in_set") {
                    Self::InSet { read, in_set: list(set, "in_set", 1, (MAX_SET_MEMBERS, Some("MAX_SET_MEMBERS")), typed_value)? }
                } else {
                    return Err(Fault::new("", Rule::MissingField, "a term that reads a value has `op` and `value`, `in_range` or `in_set`"));
                }
            }
        };
        no_other(&object, &[])?;
        Ok(condition)
    }
}

impl<'de> Deserialize<'de> for RuleCondition {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        read_with(deserializer, Self::from_value)
    }
}

// ----------------------------------------------------------- amounts

/// What an amount adds to its constant (RULE-106).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AmountSource {
    /// A variable's value as it stands when the command is applied.
    Variable(VariableName),
    /// `{"row":"p"}`: the row's parameter.
    Row,
    /// A value of the firing.
    Fired(FiredName),
}

/// A field's value in a command (RULE-106): an integer from `LEAST` to
/// `MOST`, or `{"plus":k,…}` with one source, `k` in the field's range,
/// the sum held to that range when the command runs. Only in a group: a
/// command sent directly has constants alone (`invalid_command`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Amount<const LEAST: i32, const MOST: i32> {
    /// A constant.
    Constant(i32),
    /// A constant plus a source.
    Plus {
        /// The constant.
        plus: i32,
        /// What is added to it.
        source: AmountSource,
    },
}

impl<const LEAST: i32, const MOST: i32> Amount<LEAST, MOST> {
    /// The keys an amount's object holds.
    const KEYS: [&'static str; 4] = ["plus", "variable", "row", "fired"];

    /// Its constant: the whole value of a constant, the constant of a sum.
    #[must_use]
    pub const fn constant(self) -> i32 {
        match self {
            Self::Constant(constant) | Self::Plus { plus: constant, .. } => constant,
        }
    }

    /// Its source, when it has one.
    #[must_use]
    pub const fn source(self) -> Option<AmountSource> {
        match self {
            Self::Constant(_) => None,
            Self::Plus { source, .. } => Some(source),
        }
    }

    fn from_value(value: Value) -> Result<Self, Fault> {
        let Value::Object(mut object) = value else {
            return typed_value::<I32In<LEAST, MOST>>(value, "").map(|constant| Self::Constant(constant.get()));
        };
        no_other(&object, &Self::KEYS)?;
        let plus: I32In<LEAST, MOST> = match object.remove("plus") {
            Some(plus) => typed_value(plus, "plus")?,
            // `plus` defaults to 0, which must be within the field's range.
            None => typed_value(Value::from(0), "plus")?,
        };
        let source = match (object.remove("variable"), object.remove("row"), object.remove("fired")) {
            (Some(variable), None, None) => AmountSource::Variable(typed_value(variable, "variable")?),
            (None, Some(row), None) => {
                let _: RowWord = typed_value(row, "row")?;
                AmountSource::Row
            }
            (None, None, Some(fired)) => AmountSource::Fired(typed_value(fired, "fired")?),
            _ => return Err(Fault::new("", Rule::WrongType, "an amount is a constant `plus` and exactly one of `variable`, `row` and `fired`")),
        };
        Ok(Self::Plus { plus: plus.get(), source })
    }
}

impl<'de, const LEAST: i32, const MOST: i32> Deserialize<'de> for Amount<LEAST, MOST> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        read_with(deserializer, Self::from_value)
    }
}

impl<const LEAST: i32, const MOST: i32> JsonSchema for Amount<LEAST, MOST> {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> Cow<'static, str> {
        format!("Amount_{LEAST}_{MOST}").into()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        let variable = generator.subschema_for::<VariableName>();
        let fired = generator.subschema_for::<FiredName>();
        json_schema!({
            "anyOf": [
                { "type": "integer", "format": "int32", "minimum": LEAST, "maximum": MOST },
                {
                    "type": "object",
                    "description": "An amount (RULE-106): `plus` and one source, in a group only.",
                    "properties": {
                        "plus": { "type": "integer", "format": "int32", "minimum": LEAST, "maximum": MOST },
                        "variable": variable,
                        "row": { "const": ROW_PARAMETER_NAME },
                        "fired": fired
                    },
                    "oneOf": [{ "required": ["variable"] }, { "required": ["row"] }, { "required": ["fired"] }],
                    "additionalProperties": false
                }
            ]
        })
    }
}

/// A MIDI data byte as an amount.
pub type DataAmount = Amount<0, { limits::MAX_MIDI_DATA as i32 }>;
/// A pitch bend as an amount.
pub type BendAmount = Amount<0, { limits::MAX_PITCH_BEND as i32 }>;
/// A gain as an amount.
pub type GainAmount = Amount<0, { MAX_GAIN as i32 }>;
/// A fade, in milliseconds, as an amount.
pub type FadeAmount = Amount<0, { MAX_FADE_MS as i32 }>;
/// A position as an amount.
pub type OffsetAmount = Amount<{ i16::MIN as i32 }, { i16::MAX as i32 }>;
/// An order within a layer as an amount.
pub type OrderAmount = Amount<{ i8::MIN as i32 }, { i8::MAX as i32 }>;
/// A frame as an amount.
pub type FrameAmount = Limited<Amount<0, { limits::MAX_FRAME as i32 }>, MaxFrame>;
/// A cycle's period as an amount.
pub type PeriodAmount = Limited<Amount<1, { limits::MAX_CYCLE_PERIOD as i32 }>, MaxCyclePeriod>;
/// A look's number as an amount.
pub type LookAmount = Amount<0, { MAX_LOOK as i32 }>;

// ------------------------------------------------- animation operations

/// `show`, `hide`, `stop_cycle` and `reset`: an operation that names its
/// target and nothing else.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TargetOp {
    /// One control or one object; `stop_cycle` takes an object only.
    pub target: RuleTarget,
}

/// A look (ANIM-6).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LookName {
    /// No look is forced: the pointer decides it.
    Normal,
    /// The hover look.
    Hover,
    /// The pressed look.
    Pressed,
    /// The alternative look.
    Alternative,
}

/// A look, by its name, or in a group as an amount of its number 0 to 3
/// (RULE-106).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LookChoice {
    /// A look by its name.
    Named(LookName),
    /// Its number as an amount.
    Amount(LookAmount),
}

impl<'de> Deserialize<'de> for LookChoice {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        read_with(deserializer, |value| match value {
            word @ Value::String(_) => typed_value(word, "").map(Self::Named),
            other => LookAmount::from_value(other).map(Self::Amount),
        })
    }
}

impl JsonSchema for LookChoice {
    fn schema_name() -> Cow<'static, str> {
        "LookChoice".into()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        let named = generator.subschema_for::<LookName>();
        let amount = generator.subschema_for::<LookAmount>();
        json_schema!({ "anyOf": [named, amount] })
    }
}

/// `look`: forces a look on a control or an object.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LookOp {
    /// One control or one object.
    pub target: RuleTarget,
    /// The look.
    pub look: LookChoice,
}

/// `move`: puts an object at a position, or moves it by an amount, in its
/// renderer's units (ANIM-6).
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MoveOp {
    /// One object.
    pub target: RuleTarget,
    /// Across.
    pub x: OffsetAmount,
    /// Down.
    pub y: OffsetAmount,
    /// Whether `x` and `y` are added to the position in force.
    #[serde(default)]
    pub relative: bool,
}

/// `order`: sets an object's layer and its order within the layer
/// (ANIM-12).
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OrderOp {
    /// One object.
    pub target: RuleTarget,
    /// The layer, or null to keep it.
    #[serde(default)]
    pub layer: Option<Layer>,
    /// The order within the layer, lowest drawn first.
    pub order: OrderAmount,
}

/// `frame`: shows one frame of an object, and stops any cycle (ANIM-14).
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FrameOp {
    /// One object.
    pub target: RuleTarget,
    /// The frame.
    pub frame: FrameAmount,
}

/// `cycle`: cycles an object's frames from `first` to `last`, one frame
/// each `period` ticks (ANIM-14).
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CycleOp {
    /// One object.
    pub target: RuleTarget,
    /// The first frame.
    pub first: FrameNumber,
    /// The last frame, at least `first`.
    pub last: FrameNumber,
    /// Ticks a frame.
    pub period: PeriodAmount,
    /// Stop at `last` rather than begin again.
    #[serde(default)]
    pub once: bool,
}

// ---------------------------------------------------------- commands

/// `play` in a group: starts a sound the page already has on an audio
/// channel. It takes its page from the table and never plays for the whole
/// service or after the current sound (RULE-119).
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GroupPlay {
    /// The audio channel.
    pub channel: AudioChannel,
    /// The sound's handle.
    pub handle: Byte,
    /// Its generation.
    pub generation: Word,
    /// The gain.
    pub gain: GainAmount,
    /// The fade in, in milliseconds.
    pub fade_ms: FadeAmount,
    /// How far other channels duck under it.
    #[serde(default = "no_duck")]
    pub duck: U8In<0, MOST_DUCK>,
    /// It loops.
    #[serde(default)]
    pub r#loop: bool,
    /// Always false in a group: refused when true (`invalid_group`).
    #[serde(default)]
    pub after: bool,
    /// Never given in a group: the table's page is the play's
    /// (`invalid_group`).
    #[serde(default)]
    pub page: Option<PageNumber>,
}

/// No ducking.
const fn no_duck() -> U8In<0, MOST_DUCK> {
    U8In(0)
}

/// `stop` in a group.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GroupStop {
    /// The audio channel.
    pub channel: AudioChannel,
    /// The fade out, in milliseconds.
    pub fade_ms: FadeAmount,
}

/// `gain` in a group.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GroupGain {
    /// The audio channel.
    pub channel: AudioChannel,
    /// The gain.
    pub gain: GainAmount,
    /// The ramp, in milliseconds.
    pub fade_ms: FadeAmount,
}

/// A `sound` command of a group, by `kind`: the audio profile's `CONTROL`
/// record (RULE-119).
#[derive(Clone, PartialEq, Eq, Debug, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum GroupSound {
    /// Starts a sound.
    Play(GroupPlay),
    /// Stops an audio channel.
    Stop(GroupStop),
    /// Changes an audio channel's gain.
    Gain(GroupGain),
}

tagged_choice!(GroupSound, "kind", { "play" => Play, "stop" => Stop, "gain" => Gain });

/// A note on or a note off of a `local_midi` message.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LocalNote {
    /// The MIDI channel, 1 to 16: the part.
    pub channel: MidiChannel,
    /// The key.
    pub key: DataAmount,
    /// The velocity.
    pub velocity: DataAmount,
}

/// A control change of a `local_midi` message.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LocalControlChange {
    /// The MIDI channel.
    pub channel: MidiChannel,
    /// The controller.
    pub controller: DataAmount,
    /// Its value.
    pub value: DataAmount,
}

/// A program change of a `local_midi` message.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LocalProgramChange {
    /// The MIDI channel.
    pub channel: MidiChannel,
    /// The program.
    pub program: DataAmount,
}

/// A pitch bend of a `local_midi` message.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LocalPitchBend {
    /// The MIDI channel.
    pub channel: MidiChannel,
    /// The 14-bit bend; 8,192 is none.
    pub value: BendAmount,
}

/// The one MIDI channel voice message of a `local_midi` message (RULE-21),
/// by `event`: a family's event shapes, without a time.
#[derive(Clone, Copy, PartialEq, Eq, Debug, JsonSchema)]
#[serde(tag = "event", rename_all = "snake_case", deny_unknown_fields)]
pub enum LocalMidiEvent {
    /// `9n key velocity`.
    NoteOn(LocalNote),
    /// `8n key velocity`.
    NoteOff(LocalNote),
    /// `Bn controller value`.
    ControlChange(LocalControlChange),
    /// `Cn program`.
    ProgramChange(LocalProgramChange),
    /// `En lsb msb`.
    PitchBend(LocalPitchBend),
}

tagged_choice!(LocalMidiEvent, "event", {
    "note_on" => NoteOn,
    "note_off" => NoteOff,
    "control_change" => ControlChange,
    "program_change" => ProgramChange,
    "pitch_bend" => PitchBend,
});

/// What a `variable` command does (RULE-99).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum VariableOp {
    /// `v` becomes the value.
    Set,
    /// `v + 1`, saturating.
    Inc,
    /// `v − 1`, saturating.
    Dec,
    /// `v` plus the value, saturating.
    Add,
    /// `v` minus the value, saturating.
    Sub,
    /// 0 becomes 1, anything else 0.
    Toggle,
    /// `v` held to at most the value.
    Min,
    /// `v` held to at least the value.
    Max,
    /// Above `high` becomes `low`, below `low` becomes `high`.
    Wrap,
    /// `v` becomes 0; with `"all"`, every variable does.
    Clear,
}

/// The value of a `variable` command (RULE-99): an integer, a constant
/// plus a variable, the row's parameter or a value of the firing, or for
/// `set` alone a value read.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum VariableOperand {
    /// An integer, or a constant plus a source.
    Amount(Amount<{ i32::MIN }, { i32::MAX }>),
    /// `{"read": v}`: a value as it stands.
    Read(RuleValue),
}

impl<'de> Deserialize<'de> for VariableOperand {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        read_with(deserializer, |value| match value {
            Value::Object(mut object) if object.contains_key("read") => {
                no_other(&object, &["read"])?;
                let read = object.remove("read").unwrap_or(Value::Null);
                typed_value(read, "read").map(Self::Read)
            }
            other => Amount::from_value(other).map(Self::Amount),
        })
    }
}

impl JsonSchema for VariableOperand {
    fn schema_name() -> Cow<'static, str> {
        "VariableOperand".into()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        let amount = generator.subschema_for::<Amount<{ i32::MIN }, { i32::MAX }>>();
        let read = generator.subschema_for::<RuleValue>();
        json_schema!({
            "anyOf": [amount, { "type": "object", "properties": { "read": read }, "required": ["read"], "additionalProperties": false }]
        })
    }
}

/// Start or stop.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StartStop {
    /// Start, replacing a run in progress.
    Start,
    /// Stop.
    Stop,
}

/// A timer's next expiry of a period (RULE-15, RULE-103).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Beat {
    /// The timer; the page timer when left out.
    #[serde(default)]
    pub timer: TimerRef,
    /// The period, in ticks.
    pub ticks: TimerTicks,
}

/// When a sequence that is started begins (RULE-103, DEC-6635 item 6):
/// `{"tick":n}`, a page tick, or `{"beat":{…}}`, a timer's next expiry of
/// a period; the next tick when null.
#[derive(Clone, Copy, PartialEq, Eq, Debug, JsonSchema)]
#[serde(untagged, deny_unknown_fields)]
pub enum SequenceAt {
    /// At a page tick: a tick passed means the next tick.
    Tick {
        /// The page tick.
        tick: PageTick,
    },
    /// At the timer's next expiry of a period.
    Beat {
        /// The timer and the period.
        beat: Beat,
    },
}

impl<'de> Deserialize<'de> for SequenceAt {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        read_with(deserializer, |value| {
            let Value::Object(object) = value else { return Err(Fault::new("", Rule::WrongType, "`at` is {\"tick\":n} or {\"beat\":{…}}, or null")) };
            let (key, value) = one_key(object, &["tick", "beat"])?;
            if key == "tick" { typed_value(value, "tick").map(|tick| Self::Tick { tick }) } else { typed_value(value, "beat").map(|beat| Self::Beat { beat }) }
        })
    }
}

/// Generates a command a group may hold and the message of the same
/// fields that a program sends directly with `connections`, `page` and
/// `sub` (RULE-114): one list of fields, so that the two cannot differ.
macro_rules! command_pair {
    (
        $(#[$message_meta:meta])* $message:ident,
        $(#[$command_meta:meta])* $command:ident,
        { $($(#[$field_meta:meta])* $field:ident: $field_type:ty,)* }
    ) => {
        $(#[$command_meta])*
        #[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
        #[serde(deny_unknown_fields)]
        pub struct $command {
            $($(#[$field_meta])* pub $field: $field_type,)*
        }

        $(#[$message_meta])*
        #[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
        #[serde(deny_unknown_fields)]
        pub struct $message {
            /// Who it is for: of them, those that have the page selected.
            pub connections: Connections,
            /// The page.
            pub page: PageNumber,
            /// The sub-page, never 3F7F (DEC-6599).
            pub sub: SubCode,
            $($(#[$field_meta])* pub $field: $field_type,)*
        }

        impl $message {
            /// The command it carries.
            #[must_use]
            pub fn command(&self) -> $command {
                $command { $($field: self.$field.clone(),)* }
            }
        }
    };
}

command_pair!(
    /// `local_midi` (RULE-21, RULE-114): one MIDI message to the local
    /// sound on an audio channel of the page.
    LocalMidiMessage,
    /// `local_midi` in a group.
    LocalMidiCommand,
    {
        /// The audio channel.
        channel: AudioChannel,
        /// The message.
        message: LocalMidiEvent,
    }
);

command_pair!(
    /// `variable` (RULE-99, RULE-114): changes one variable of the page.
    VariableMessage,
    /// `variable` in a group.
    VariableCommand,
    {
        /// What it does.
        op: VariableOp,
        /// The variable; `"all"` with `clear` alone.
        variable: VariableOrAll,
        /// The value, for `set`, `add`, `sub`, `min` and `max`.
        #[serde(default)]
        value: Option<VariableOperand>,
        /// The range's least, for `wrap`.
        #[serde(default)]
        low: Option<Constant>,
        /// The range's greatest, for `wrap`.
        #[serde(default)]
        high: Option<Constant>,
    }
);

command_pair!(
    /// `timer` (RULE-100, RULE-114): starts or stops a timer of the page.
    TimerMessage,
    /// `timer` in a group.
    TimerCommand,
    {
        /// Start or stop.
        op: StartStop,
        /// The timer; never the page timer.
        timer: TimerName,
    }
);

command_pair!(
    /// `report` (RULE-22, RULE-114): reports a value of the page to its
    /// channel.
    ReportMessage,
    /// `report` in a group.
    ReportCommand,
    {
        /// The value reported, or null (only in a group) for the firing.
        #[serde(default)]
        value: Option<RuleValue>,
    }
);

command_pair!(
    /// `run` (RULE-102, RULE-114): runs a group of the page's table now.
    RunMessage,
    /// `run` in a group: runs another group in place.
    RunCommand,
    {
        /// The group's name.
        group: RuleName,
    }
);

command_pair!(
    /// `sequence` (RULE-103, RULE-114): starts or stops a sequence of the
    /// page's table.
    SequenceMessage,
    /// `sequence` in a group.
    SequenceCommand,
    {
        /// Start or stop.
        op: StartStop,
        /// The sequence's name, or null with `stop` for every sequence.
        #[serde(default)]
        sequence: Option<RuleName>,
        /// With `start`, when it begins: the next tick when null.
        #[serde(default)]
        at: Option<SequenceAt>,
    }
);

/// `animate` in a group (ANIM-6).
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AnimateCommand {
    /// The operations, applied in order.
    #[schemars(length(min = 1, max = MOST_OPERATIONS))]
    pub ops: Vec<AnimationOp>,
}

/// `sprites` in a group (ANIM-7): replaces the page's teletext sprites.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SpritesCommand {
    /// The sprites.
    #[schemars(length(max = MOST_SPRITES))]
    pub sprites: Vec<Sprite>,
}

/// `naplps_objects` in a group (ANIM-11): replaces the page's NAPLPS
/// objects.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NaplpsObjectsCommand {
    /// The objects.
    #[schemars(length(max = MOST_NAPLPS_OBJECTS))]
    pub objects: Vec<NaplpsObject>,
}

/// A command of a group (RULE-35, RULE-118, RULE-119), by `type`: a
/// program message without its `connections`, `page` and `sub`, which the
/// table supplies.
#[derive(Clone, PartialEq, Eq, Debug, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
#[schemars(transform = super::close_nested_branches)]
pub enum RuleCommand {
    /// A `play`, `stop` or `gain`.
    Sound(GroupSound),
    /// A MIDI message to a local sound.
    LocalMidi(LocalMidiCommand),
    /// Animation operations.
    Animate(AnimateCommand),
    /// The page's teletext sprites.
    Sprites(SpritesCommand),
    /// The page's NAPLPS objects.
    NaplpsObjects(NaplpsObjectsCommand),
    /// A variable changed.
    Variable(VariableCommand),
    /// A timer started or stopped.
    Timer(TimerCommand),
    /// A report to the channel.
    Report(ReportCommand),
    /// Another group run in place.
    Run(RunCommand),
    /// A sequence started or stopped.
    Sequence(SequenceCommand),
}

/// The message types a group may hold (RULE-119); any other is
/// `invalid_group`.
const COMMAND_TYPES: [&str; 10] = ["sound", "local_midi", "animate", "sprites", "naplps_objects", "variable", "timer", "report", "run", "sequence"];

impl<'de> Deserialize<'de> for RuleCommand {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        tagged(deserializer, "type", |tag, object| {
            Ok(match tag {
                "sound" => Self::Sound(typed(object, "")?),
                "local_midi" => Self::LocalMidi(typed(object, "")?),
                "animate" => Self::Animate(typed(object, "")?),
                "sprites" => Self::Sprites(typed(object, "")?),
                "naplps_objects" => Self::NaplpsObjects(typed(object, "")?),
                "variable" => Self::Variable(typed(object, "")?),
                "timer" => Self::Timer(typed(object, "")?),
                "report" => Self::Report(typed(object, "")?),
                "run" => Self::Run(typed(object, "")?),
                "sequence" => Self::Sequence(typed(object, "")?),
                other => {
                    return Err(Fault::new(
                        "type",
                        Rule::InvalidGroup,
                        format!("`{other}` is no command a group may hold (RULE-119): one of {COMMAND_TYPES:?}"),
                    ));
                }
            })
        })
    }
}

/// A command sent directly (RULE-114), by its message.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum CommandMessage {
    /// `local_midi`.
    LocalMidi(LocalMidiMessage),
    /// `variable`.
    Variable(VariableMessage),
    /// `timer`.
    Timer(TimerMessage),
    /// `report`.
    Report(ReportMessage),
    /// `run`.
    Run(RunMessage),
    /// `sequence`.
    Sequence(SequenceMessage),
}

impl CommandMessage {
    /// Its page and sub-code.
    #[must_use]
    pub const fn page(&self) -> (u16, u16) {
        let (page, sub) = match self {
            Self::LocalMidi(message) => (message.page, message.sub),
            Self::Variable(message) => (message.page, message.sub),
            Self::Timer(message) => (message.page, message.sub),
            Self::Report(message) => (message.page, message.sub),
            Self::Run(message) => (message.page, message.sub),
            Self::Sequence(message) => (message.page, message.sub),
        };
        (page.0, sub.0)
    }

    /// Its audience.
    #[must_use]
    pub const fn connections(&self) -> &Connections {
        match self {
            Self::LocalMidi(message) => &message.connections,
            Self::Variable(message) => &message.connections,
            Self::Timer(message) => &message.connections,
            Self::Report(message) => &message.connections,
            Self::Run(message) => &message.connections,
            Self::Sequence(message) => &message.connections,
        }
    }

    /// Its type.
    #[must_use]
    pub const fn kind(&self) -> super::MessageType {
        match self {
            Self::LocalMidi(_) => super::MessageType::LocalMidi,
            Self::Variable(_) => super::MessageType::Variable,
            Self::Timer(_) => super::MessageType::Timer,
            Self::Report(_) => super::MessageType::Report,
            Self::Run(_) => super::MessageType::Run,
            Self::Sequence(_) => super::MessageType::Sequence,
        }
    }

    /// Where its table's name is: what an `unknown_name` names.
    #[must_use]
    pub const fn name_path(&self) -> &'static str {
        match self {
            Self::Run(_) => "group",
            Self::Sequence(_) => "sequence",
            _ => "",
        }
    }

    /// The command it carries, as a group would hold it.
    #[must_use]
    pub fn command(&self) -> RuleCommand {
        match self {
            Self::LocalMidi(message) => RuleCommand::LocalMidi(message.command()),
            Self::Variable(message) => RuleCommand::Variable(message.command()),
            Self::Timer(message) => RuleCommand::Timer(message.command()),
            Self::Report(message) => RuleCommand::Report(message.command()),
            Self::Run(message) => RuleCommand::Run(message.command()),
            Self::Sequence(message) => RuleCommand::Sequence(message.command()),
        }
    }
}

// ---------------------------------------------------------- the table

/// A rectangle's declaration (RULE-13), in the pixels of the page's
/// picture from its top left: a teletext page is 480 by 500, a ReGIS
/// picture 800 by 480, a NAPLPS picture 640 by 480.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RectDeclaration {
    /// Its name; never `page`.
    pub name: RuleName,
    /// The left edge.
    pub x: Word,
    /// The top edge.
    pub y: Word,
    /// The width, at least 1.
    pub width: U16In<1, { u16::MAX }>,
    /// The height, at least 1.
    pub height: U16In<1, { u16::MAX }>,
}

/// A binding row (RULE-105, DEC-6635 item 8): its one parameter `p`, a
/// signed 32-bit integer, or a key, which stands for that key's code.
#[derive(Clone, PartialEq, Eq, Debug, JsonSchema)]
#[serde(untagged)]
pub enum RuleRow {
    /// An integer.
    Integer(Constant),
    /// A key's code.
    Key(RuleKey),
}

impl<'de> Deserialize<'de> for RuleRow {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        read_with(deserializer, |value| match value {
            key @ Value::Object(_) => typed_value(key, "").map(Self::Key),
            integer => typed_value(integer, "").map(Self::Integer),
        })
    }
}

/// A group of commands (RULE-118), named.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GroupDeclaration {
    /// Its name.
    pub name: RuleName,
    /// Its commands, applied in order.
    #[schemars(length(min = 1, max = MAX_GROUP_COMMANDS))]
    pub commands: Vec<RuleCommand>,
}

/// One step of a sequence: a group run at a tick offset.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SequenceStep {
    /// The offset in ticks within a pass, below the length.
    pub tick: SequenceTicks<0>,
    /// The group's name.
    pub run: RuleName,
}

/// A sequence (RULE-103): steps a receiver plays on its own clock.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SequenceDeclaration {
    /// Its name.
    pub name: RuleName,
    /// A pass's length in ticks.
    pub length: SequenceTicks<1>,
    /// How many passes it plays, or 0 until stopped.
    pub repeat: SequenceRepeat,
    /// Its steps, in ascending `tick`.
    #[schemars(length(min = 1, max = MAX_SEQUENCE_STEPS))]
    pub steps: Vec<SequenceStep>,
}

/// A rule (RULE-3, RULE-35): its triggers, its condition, its rows, and
/// exactly one of `run` and `do`.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PageRule {
    /// Its name, by which `input` and `rule_event` name it.
    pub name: RuleName,
    /// Its trigger or triggers.
    pub on: Triggers,
    /// What must hold for it to fire, or null; for `becomes`, the
    /// condition whose becoming true fires it.
    #[serde(default)]
    pub when: Option<RuleCondition>,
    /// The name of the row set it is a template of.
    #[serde(default)]
    pub rows: Option<RuleName>,
    /// The name of the group it runs.
    #[serde(default)]
    pub run: Option<RuleName>,
    /// The commands it runs, which the server makes into an unnamed group.
    #[serde(default)]
    #[schemars(length(min = 1, max = MAX_GROUP_COMMANDS))]
    pub r#do: Option<Vec<RuleCommand>>,
    /// After the group, a `report` of the firing.
    #[serde(default)]
    pub echo: bool,
}

/// A page's rule table (RULE-2, RULE-35): one for one page number and
/// sub-code (DEC-6599).
#[derive(Clone, PartialEq, Eq, Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RuleTable {
    /// The rectangles.
    #[serde(default)]
    #[schemars(length(max = limits::MAX_RECTS))]
    pub rects: Vec<RectDeclaration>,
    /// The row sets, by name, laid out in the order of their names, with
    /// `MAX_ROWS` rows in all.
    #[serde(default)]
    pub rows: BTreeMap<RuleName, Vec<RuleRow>>,
    /// The named groups.
    #[serde(default)]
    #[schemars(length(max = MAX_GROUPS))]
    pub groups: Vec<GroupDeclaration>,
    /// The sequences.
    #[serde(default)]
    #[schemars(length(max = MAX_SEQUENCES))]
    pub sequences: Vec<SequenceDeclaration>,
    /// The rules, in rule order.
    #[schemars(length(max = MAX_RULES))]
    pub rules: Vec<PageRule>,
}

/// A table, or null: a field that must be given, so that leaving it out is
/// never read as removing the page's table.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TableOrNull(pub Option<Box<RuleTable>>);

impl<'de> Deserialize<'de> for TableOrNull {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Read as a JSON value, which a missing field is not; an `Option`
        // would read a missing field as null.
        read_with(deserializer, |value| match value {
            Value::Null => Ok(Self(None)),
            Value::Object(object) => typed(object, "").map(|table| Self(Some(Box::new(table)))),
            _ => Err(Fault::new("", Rule::WrongType, "a table, or null")),
        })
    }
}

impl JsonSchema for TableOrNull {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> Cow<'static, str> {
        "TableOrNull".into()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        let table = generator.subschema_for::<RuleTable>();
        json_schema!({ "anyOf": [table, { "type": "null" }] })
    }
}

/// `rules`: gives one page (page number and sub-code) its rule table, for
/// the connections named that have it selected, or with a null table
/// removes it, which puts the channel's default table back where the
/// channel has one (RULE-35, RULE-53). The server keeps no table for a
/// connection that selects the page later (RULE-40).
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RulesMessage {
    /// Who it is for.
    pub connections: Connections,
    /// The page.
    pub page: PageNumber,
    /// The sub-page, never 3F7F.
    pub sub: SubCode,
    /// The table, or null.
    pub table: TableOrNull,
}

/// `query_state`: asks one connection for one page's local state, once
/// (RULE-38). The server answers `page_state`, or `refused` with
/// `wrong_page`, `too_frequent` or `not_answered` and this `request`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct QueryState {
    /// The connection.
    #[schemars(range(min = 1, max = SAFE_INTEGER_LIMIT))]
    pub connection: ConnectionId,
    /// The page, which the connection has selected.
    pub page: PageNumber,
    /// Its sub-page.
    pub sub: SubCode,
    /// The program's own number for the question.
    pub request: U64In<1, { u32::MAX as u64 }>,
}

/// `object_page` (ANIM-17; TODO-6569): a teletext object page whose
/// objects a page's sprites show, sent to the connections named that have
/// that page selected, before the page. It is no selection's answer and
/// the server keeps none of it (RULE-40): a program sends a page's object
/// pages at each selection of it.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ObjectPage {
    /// Who it is for: of them, those that have the page selected.
    pub connections: Connections,
    /// The page whose sprites use it.
    pub page: PageNumber,
    /// Its sub-page.
    pub sub: SubCode,
    /// The object page: a Public or Global Public Object Page (EN 300 706
    /// clause 10.5), encoded as a `page` message's sub-page is.
    pub object: Page,
}

// ------------------------------------------------ server to program

/// What a `rule_event` reports.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RuleEventBody {
    /// A run fired, with no input event.
    Fired,
    /// A `report` of a value: what was read, as the table names it, and
    /// its reading.
    Value {
        /// What was read.
        read: RuleValue,
        /// Its reading.
        value: i32,
    },
}

/// `rule_event`: a `report` of a firing with no input event, or of a value
/// (RULE-37).
#[derive(Clone, PartialEq, Eq, Debug, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RuleEvent {
    /// The connection.
    #[schemars(range(min = 1, max = SAFE_INTEGER_LIMIT))]
    pub connection: ConnectionId,
    /// The page on screen.
    pub page: PageNumber,
    /// The sub-code on screen.
    pub sub: SubCode,
    /// The rule's name, or null when the run was not made by a rule (a
    /// command the server sent, or a sequence it started).
    pub rule: Option<RuleName>,
    /// The place of the firing row within the rule's rows, or null.
    #[schemars(range(max = MAX_ROWS))]
    pub row: Option<u8>,
    /// The page tick at which it fired.
    pub tick: u32,
    /// What it reports.
    pub event: RuleEventBody,
}

/// A timer's state in `page_state`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TimerState {
    /// Whether it runs.
    pub running: bool,
    /// The ticks since it started.
    pub elapsed: u32,
}

/// A sequence's state in `page_state`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SequenceState {
    /// Whether it runs.
    pub running: bool,
    /// The passes completed since its start.
    pub passes: u32,
    /// Its position within the pass; 0 while it is not running.
    pub position: u32,
    /// Whether it waits for the tick it starts at (RULE-103).
    pub waiting: bool,
}

/// A control's or an object's state in `page_state`.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TargetState {
    /// The control or the object.
    pub target: RuleTarget,
    /// Whether it exists on the page.
    pub exists: bool,
    /// Whether it is shown.
    pub shown: bool,
    /// The look in force.
    pub look: LookName,
    /// Its frame in force.
    #[schemars(range(max = limits::MAX_FRAME))]
    pub frame: u8,
    /// Its position across, in its renderer's units.
    pub x: i16,
    /// Its position down.
    pub y: i16,
    /// Its layer.
    #[schemars(range(max = MAX_LAYER))]
    pub layer: u8,
    /// Its order within the layer.
    pub order: i8,
    /// Whether its frames cycle.
    pub cycling: bool,
}

/// A value's reading in `page_state`.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ValueReading {
    /// What was read, as the table names it.
    pub read: RuleValue,
    /// Its reading.
    pub value: i32,
}

/// `page_state`: the answer to `query_state` (RULE-38), a page's local
/// state in one view. `table` is false, the lists empty and the variables
/// zero, when the page has no table in that view.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PageState {
    /// The connection.
    #[schemars(range(min = 1, max = SAFE_INTEGER_LIMIT))]
    pub connection: ConnectionId,
    /// The page.
    pub page: PageNumber,
    /// The sub-code.
    pub sub: SubCode,
    /// The `request` of the `query_state` it answers.
    #[schemars(range(min = 1))]
    pub request: u32,
    /// The page tick the state was read at.
    pub tick: u32,
    /// Whether the page has a table in that view.
    pub table: bool,
    /// The sixteen variables, `v0` first.
    #[schemars(length(min = VARIABLES, max = VARIABLES))]
    pub variables: Vec<i32>,
    /// The timers that run, by name.
    #[schemars(length(max = TIMERS))]
    pub timers: BTreeMap<String, TimerState>,
    /// Every sequence of the table, by name.
    #[schemars(length(max = MAX_SEQUENCES))]
    pub sequences: BTreeMap<RuleName, SequenceState>,
    /// Every control and object the table names by a constant, in the
    /// order first named.
    #[schemars(length(max = MAX_TABLE_TARGETS))]
    pub targets: Vec<TargetState>,
    /// Every distinct value the table reads, in the order first read.
    #[schemars(length(max = MAX_TABLE_VALUES))]
    pub values: Vec<ValueReading>,
}

/// A fault a receiver reports of a page's table (RULE-48).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RulesFaultName {
    /// The receiver could not assemble or decode the table and refused it
    /// whole: the page has no table in that view.
    Undecodable,
    /// Reports dropped by the receiver's budgets.
    ReportsDropped,
    /// Input events dropped from a full queue.
    EventsDropped,
    /// Commands that did nothing for a missing target or sound.
    CommandsSkipped,
    /// A state query dropped.
    QueryDropped,
    /// A local sound stopped for overrunning its time.
    LocalOverrun,
    /// Animation slowed by its budget.
    AnimationSlowed,
    /// The page's evaluation took longer than its ticks' own time, so page
    /// time slowed.
    EvaluationLate,
    /// A command, a cue or a sound command dropped at a full queue or
    /// budget.
    QueueFull,
}

/// `rules_fault`: a receiver's report of a fault of a page's table
/// (RULE-48), at most one a second for each fault and page.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RulesFault {
    /// The connection.
    #[schemars(range(min = 1, max = SAFE_INTEGER_LIMIT))]
    pub connection: ConnectionId,
    /// The page.
    pub page: PageNumber,
    /// The sub-code.
    pub sub: SubCode,
    /// The generation of the table the receiver holds.
    pub generation: u16,
    /// The fault.
    pub fault: RulesFaultName,
    /// How many times since the receiver's last report of it.
    pub count: u16,
}

// RULE-46: a row's place is carried in one byte, 255 meaning none, so a
// table's rows fit below it.
const _: () = assert!(MAX_ROWS < limits::NO_RULE as usize);
