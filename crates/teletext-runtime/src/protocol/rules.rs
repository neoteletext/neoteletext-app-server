//! The page rule table in the program protocol (neoteletext-common
//! `docs/page-rules.md` section 7, RULE-34 to RULE-40; REQ-6532,
//! DEC-6522): the `rules` and `query_state` messages a program sends, and
//! the `rule_event`, `page_state` and `rules_fault` messages the server
//! gives it. A table is written with names, which exist only between a
//! program and the server (RULE-36): the server turns each into its number
//! when it encodes the record ([`crate::rules`]).
//!
//! The shapes that a tag chooses (`event`, `of`, `action`) are read by
//! following the tag, as a sound's are, so that a fault inside one keeps
//! its path; the shapes that their one key chooses (a target, a key, a
//! condition) are read from the JSON value by hand for the same reason.

use alloc::borrow::Cow;
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};

use super::read::{Fault, join, typed, typed_value};
use super::sound::{carried, not_listed, tagged, tagged_choice};
use super::{ConnectionId, Connections, Data, Data14, I32In, KeyName, MidiChannel, PageNumber, Rule, SAFE_INTEGER_LIMIT, SubCode, U8In, U16In, U64In};

// The limits are the engine's, each defined once in `neoteletext-rules`
// (RULE-57); the figures here are what the schema states, and
// `crate::rules` asserts that each equals the engine's constant.

/// The most rules a table holds (RULE-57, `MAX_RULES`).
pub const MOST_RULES: usize = 64;
/// The most actions a rule holds (`MAX_ACTIONS`).
pub const MOST_ACTIONS: usize = 16;
/// The most variables a table declares (`MAX_VARIABLES`).
pub const MOST_VARIABLES: usize = 32;
/// The most timers a table declares (`MAX_TIMERS`).
pub const MOST_TIMERS: usize = 8;
/// The most rectangles a table declares (`MAX_RECTS`).
pub const MOST_RECTS: usize = 16;
/// The most sub-pages with rules switched off (`MAX_SUBPAGE_MASKS`).
pub const MOST_SUBPAGE_MASKS: usize = 16;
/// The most keys in a key set (`MAX_KEYS_PER_TRIGGER`).
pub const MOST_KEYS: usize = 16;
/// The most children of an `and` or an `or` (`MAX_AND_OR_CHILDREN`).
pub const MOST_CHILDREN: usize = 8;
/// The fewest children of an `and` or an `or` (`MIN_AND_OR_CHILDREN`).
pub const FEWEST_CHILDREN: usize = 2;
/// The most constants of an `in_set` (`MAX_SET_MEMBERS`).
pub const MOST_SET_MEMBERS: usize = 8;
/// The most controls and objects a state answer reports
/// (`MAX_TABLE_TARGETS`).
pub const MOST_TABLE_TARGETS: usize = 64;
/// The most values a state answer reports (`MAX_TABLE_VALUES`).
pub const MOST_TABLE_VALUES: usize = 64;
/// The most characters of a rule's, a variable's, a timer's or a
/// rectangle's name (RULE-36).
pub const NAME_CHARACTERS: usize = 32;
/// What a name matches (RULE-36).
const NAME_PATTERN: &str = "^[a-z][a-z0-9_]{0,31}$";
/// The name a timer and a rectangle may not have (RULE-36): it names the
/// page timer and the page target.
pub const PAGE_NAME: &str = "page";

/// An audio channel, 0 to 3: one of the four voices of the audio profile,
/// not a MIDI channel.
pub type AudioChannel = U8In<0, 3>;
/// A gain, 0 to 100.
pub type GainPercent = U8In<0, 100>;
/// A fade, 0 to 10,000 milliseconds.
pub type FadeMs = U16In<0, 10_000>;
/// Any byte, 0 to 255.
pub type Byte = U8In<0, 255>;
/// Any 16-bit number, 0 to 65,535.
pub type Word = U16In<0, 65_535>;
/// A signed 32-bit constant.
pub type Constant = I32In<{ i32::MIN }, { i32::MAX }>;
/// A position or an amount, −32,768 to 32,767.
pub type Offset = I32In<-32_768, 32_767>;
/// An order within a layer, −128 to 127.
pub type OrderInLayer = I32In<-128, 127>;
/// A layer, 0 to 3.
pub type Layer = U8In<0, 3>;
/// A frame number, 0 to 15.
pub type FrameNumber = U8In<0, 15>;
/// A cycle's period, 1 to 3,600 ticks.
pub type CyclePeriod = U16In<1, 3_600>;
/// A count of ticks, 1 to 216,000: one hour at sixty a second.
pub type Ticks = U64In<1, 216_000>;
/// A pointer button: 0 left, 1 middle, 2 right.
pub type Button = U8In<0, 2>;
/// A part of a synthesizer sound: its MIDI channel, 1 to 16.
pub type Part = U8In<1, 16>;

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

// ------------------------------------------------------------ names

/// A rule's, a variable's, a timer's or a rectangle's name (RULE-36): a
/// lower case letter, then lower case letters, digits and `_`, at most 32
/// characters. Names are distinct within their kind in a table, and no
/// name is sent to a receiver.
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
            "description": "A rule's, a variable's, a timer's or a rectangle's name (RULE-36): distinct within its kind in a table, and never sent to a receiver.",
            "type": "string",
            "pattern": NAME_PATTERN
        })
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

/// Nothing: a tagged shape with no field beside its tag.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NoFields {}

// ---------------------------------------------------------- targets

/// A control's number, 1 to 32 in the order the page declares its
/// controls, or `"any"` in a pointer trigger (RULE-12).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum ControlRef {
    /// Any control: the rule fires once for each control concerned.
    Any(AnyWord),
    /// One control.
    Numbered(U8In<1, 32>),
}

/// An animation object's number, 1 to 65,535, or `"any"` in a pointer
/// trigger (RULE-12): a teletext sprite, a ReGIS numbered drawing or a
/// NAPLPS object of the sub-page shown.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum ObjectRef {
    /// Any object: the rule fires once for each object concerned.
    Any(AnyWord),
    /// One object.
    Numbered(U16In<1, 65_535>),
}

/// `"any"`, or a number read as `T` at `at`.
fn any_or<T: serde::de::DeserializeOwned, R>(value: Value, at: &str, any: R, numbered: impl FnOnce(T) -> R) -> Result<R, Fault> {
    match value {
        Value::String(word) if word == "any" => Ok(any),
        Value::String(word) => Err(Fault::new(at, Rule::NotListed, format!("`{word}`: a number, or \"any\""))),
        number => typed_value(number, at).map(numbered),
    }
}

/// Something on the page (RULE-12): `"page"`, `{"control": n}`,
/// `{"object": n}` or `{"rect": "name"}`. A control or an object may be
/// `"any"` in the six pointer triggers and nowhere else.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, JsonSchema)]
#[serde(untagged, deny_unknown_fields)]
pub enum RuleTarget {
    /// The whole page's picture.
    Page(PageWord),
    /// A teletext control of the page.
    Control {
        /// Which.
        control: ControlRef,
    },
    /// An animation object of the sub-page shown.
    Object {
        /// Which.
        object: ObjectRef,
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
                    "control" => Self::Control { control: any_or(value, "control", ControlRef::Any(AnyWord::Any), ControlRef::Numbered)? },
                    "object" => Self::Object { object: any_or(value, "object", ObjectRef::Any(AnyWord::Any), ObjectRef::Numbered)? },
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

/// A key set (RULE-10): `"any"`, or 1 to 16 keys, any of which matches.
#[derive(Clone, PartialEq, Eq, Debug, JsonSchema)]
#[serde(untagged)]
pub enum KeySet {
    /// Any key.
    Any(AnyWord),
    /// The keys named.
    Keys(#[schemars(length(min = 1, max = MOST_KEYS))] Vec<RuleKey>),
}

impl<'de> Deserialize<'de> for KeySet {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        read_with(deserializer, |value| match value {
            Value::String(word) if word == "any" => Ok(Self::Any(AnyWord::Any)),
            Value::String(word) => Err(Fault::new("", Rule::NotListed, format!("`{word}`: \"any\", or a list of keys"))),
            Value::Array(keys) if keys.is_empty() => Err(Fault::new("", Rule::TooShort, "no key; \"any\" names every one".to_string())),
            Value::Array(keys) if keys.len() > MOST_KEYS => Err(Fault::new("", Rule::TooLong, format!("{} keys, at most {MOST_KEYS}", keys.len()))),
            Value::Array(keys) => {
                keys.into_iter().enumerate().map(|(index, key)| typed_value(key, &format!("[{index}]"))).collect::<Result<_, _>>().map(Self::Keys)
            }
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

/// The button of a `press` or a `release` trigger: 0 left (or a touch
/// contact), 1 middle, 2 right, or `"any"`.
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

/// `becomes`: the condition holds now and did not at the rule's previous
/// evaluation (RULE-11). The rule's `when` is null.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BecomesTrigger {
    /// The condition whose becoming true fires the rule.
    pub condition: RuleCondition,
}

/// `every` and `after`: each `ticks` ticks after the timer started while
/// it runs, or once, `ticks` ticks after it started (RULE-15).
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TimerTrigger {
    /// How many ticks, sixty a second.
    pub ticks: Ticks,
    /// The timer's name, or null for the page timer, which runs from the
    /// page's opening.
    #[serde(default)]
    pub timer: Option<RuleName>,
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
    /// The page stops being shown.
    Close(NoFields),
    /// Text arrives that is not a key: a paste, an input method's
    /// composition or its committed text.
    Text(NoFields),
    /// The page gains the keyboard.
    Focus(NoFields),
    /// The page loses the keyboard.
    Blur(NoFields),
    /// A condition becomes true.
    Becomes(BecomesTrigger),
    /// Each so many ticks after a timer started.
    Every(TimerTrigger),
    /// Once, so many ticks after a timer started.
    After(TimerTrigger),
}

tagged_choice!(RuleTrigger, "event", {
    "key" => Key,
    "key_up" => KeyUp,
    "press" => Press,
    "release" => Release,
    "enter" => Enter,
    "leave" => Leave,
    "wheel" => Wheel,
    "move" => Move,
    "open" => Open,
    "close" => Close,
    "text" => Text,
    "focus" => Focus,
    "blur" => Blur,
    "becomes" => Becomes,
    "every" => Every,
    "after" => After,
});

// ------------------------------------------------------------ values

/// `variable`: a page variable's value (RULE-23).
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VariableValue {
    /// The variable's name.
    pub name: RuleName,
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

/// What `timer` reads.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TimerProperty {
    /// 1 while it runs, else 0.
    Running,
    /// The ticks since it started, 0 while it is stopped.
    Elapsed,
}

/// `timer`: a timer's state (RULE-25).
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TimerValue {
    /// The timer's name, or null for the page timer.
    #[serde(default)]
    pub name: Option<RuleName>,
    /// What is read.
    pub property: TimerProperty,
}

/// What `target` reads (RULE-26).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TargetProperty {
    /// 1 when it exists on the sub-page shown, else 0.
    Exists,
    /// 1 while it is shown.
    Shown,
    /// Its frame in force, 0 to 15.
    Frame,
    /// Its position across, in its renderer's units; a control's first
    /// column.
    X,
    /// Its position down; a control's row.
    Y,
    /// The look in force: 0 normal, 1 hover, 2 pressed, 3 alternative.
    Look,
    /// Its layer, 0 to 3.
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
    /// The channel's gain as last commanded, 0 to 100.
    Gain,
    /// 1 while the sound playing is a local sound, else 0.
    Local,
}

/// `sound`: the sound on an audio channel.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SoundValue {
    /// The audio channel, 0 to 3.
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

/// `synth`: a synthesizer's state (RULE-28).
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SynthValue {
    /// The audio channel, 0 to 3.
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
    #[schemars(length(min = 1, max = 64))]
    pub field: Option<String>,
    /// The controller, with `controller` only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub controller: Option<Data>,
}

/// Something a condition or a `send` reads (RULE-23 to RULE-28), by `of`.
/// Every value is a signed 32-bit integer.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, JsonSchema)]
#[serde(tag = "of", rename_all = "snake_case", deny_unknown_fields)]
pub enum RuleValue {
    /// A page variable.
    Variable(VariableValue),
    /// Whether a key is down.
    Key(KeyValue),
    /// Whether a pointer button is down.
    Button(ButtonValue),
    /// Where the pointer is.
    Pointer(PointerValue),
    /// How many keys are down, 0 to 16.
    KeysDown(NoFields),
    /// A timer's state.
    Timer(TimerValue),
    /// The state of a control or an object.
    Target(TargetValue),
    /// The sound on an audio channel.
    Sound(SoundValue),
    /// A synthesizer's state.
    Synth(SynthValue),
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

/// A condition (RULE-18): a tree of at most 16 nodes, of which at most 8
/// are terms, with `and`, `or` and `not` nested at most 3 deep. Every
/// comparison is of a value with a constant.
#[derive(Clone, PartialEq, Eq, Debug, JsonSchema)]
#[serde(untagged, deny_unknown_fields)]
pub enum RuleCondition {
    /// True when all of its 2 to 8 children are.
    And {
        /// The children.
        #[schemars(length(min = FEWEST_CHILDREN, max = MOST_CHILDREN))]
        and: Vec<RuleCondition>,
    },
    /// True when any of its 2 to 8 children is.
    Or {
        /// The children.
        #[schemars(length(min = FEWEST_CHILDREN, max = MOST_CHILDREN))]
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
        value: Constant,
    },
    /// True when `low ≤ value ≤ high`.
    InRange {
        /// What is read.
        read: RuleValue,
        /// `[low, high]`, `low` at most `high`.
        in_range: [Constant; 2],
    },
    /// True when the value equals one of 1 to 8 constants.
    InSet {
        /// What is read.
        read: RuleValue,
        /// The constants.
        #[schemars(length(min = 1, max = MOST_SET_MEMBERS))]
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

    /// A list of `least` to `most` items, each read by `item` at its place
    /// under `at`.
    fn list<T>(value: Value, at: &str, least: usize, most: usize, item: impl Fn(Value, &str) -> Result<T, Fault>) -> Result<Vec<T>, Fault> {
        let Value::Array(items) = value else { return Err(Fault::new(at, Rule::WrongType, format!("`{at}` is a list"))) };
        if items.len() < least {
            return Err(Fault::new(at, Rule::TooShort, format!("{} items, at least {least}", items.len())));
        }
        if items.len() > most {
            return Err(Fault::new(at, Rule::TooLong, format!("{} items, at most {most}", items.len())));
        }
        items.into_iter().enumerate().map(|(index, value)| item(value, &format!("{at}[{index}]"))).collect()
    }

    /// Exactly two of what `item` reads.
    fn pair<T>(value: Value, at: &str, item: impl Fn(Value, &str) -> Result<T, Fault>) -> Result<[T; 2], Fault> {
        let mut items = Self::list(value, at, 2, 2, item)?.into_iter();
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
        // What may stand beside the choosing key: nothing, but for a
        // term that reads a value.
        let mut take = |name: &str| object.remove(name);
        let condition = match key {
            "and" => Self::And { and: Self::list(value, "and", FEWEST_CHILDREN, MOST_CHILDREN, Self::at)? },
            "or" => Self::Or { or: Self::list(value, "or", FEWEST_CHILDREN, MOST_CHILDREN, Self::at)? },
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
                    Self::InSet { read, in_set: Self::list(set, "in_set", 1, MOST_SET_MEMBERS, typed_value)? }
                } else {
                    return Err(Fault::new("", Rule::MissingField, "a term that reads a value has `op` and `value`, `in_range` or `in_set`"));
                }
            }
        };
        match object.keys().next() {
            Some(extra) => Err(Fault::new(extra, Rule::UnknownField, format!("unknown field `{extra}`"))),
            None => Ok(condition),
        }
    }
}

impl<'de> Deserialize<'de> for RuleCondition {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        read_with(deserializer, Self::from_value)
    }
}

// ----------------------------------------------------------- actions

/// A note on or a note off of a `midi` action.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RuleNote {
    /// The MIDI channel, 1 to 16: the part.
    pub channel: MidiChannel,
    /// The key, 0 to 127.
    pub key: Data,
    /// The velocity, 0 to 127.
    pub velocity: Data,
}

/// A control change of a `midi` action.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RuleControlChange {
    /// The MIDI channel, 1 to 16.
    pub channel: MidiChannel,
    /// The controller, 0 to 127.
    pub controller: Data,
    /// Its value, 0 to 127.
    pub value: Data,
}

/// A program change of a `midi` action.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RuleProgramChange {
    /// The MIDI channel, 1 to 16.
    pub channel: MidiChannel,
    /// The program, 0 to 127.
    pub program: Data,
}

/// A pitch bend of a `midi` action.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RulePitchBend {
    /// The MIDI channel, 1 to 16.
    pub channel: MidiChannel,
    /// The bend, 0 to 16,383; 8,192 is none.
    pub value: Data14,
}

/// The one MIDI channel voice message of a `midi` action (RULE-21), by
/// `event`: a family's event shapes, without a time.
#[derive(Clone, Copy, PartialEq, Eq, Debug, JsonSchema)]
#[serde(tag = "event", rename_all = "snake_case", deny_unknown_fields)]
pub enum RuleMidi {
    /// `9n key velocity`.
    NoteOn(RuleNote),
    /// `8n key velocity`.
    NoteOff(RuleNote),
    /// `Bn controller value`.
    ControlChange(RuleControlChange),
    /// `Cn program`.
    ProgramChange(RuleProgramChange),
    /// `En lsb msb`.
    PitchBend(RulePitchBend),
}

tagged_choice!(RuleMidi, "event", {
    "note_on" => NoteOn,
    "note_off" => NoteOff,
    "control_change" => ControlChange,
    "program_change" => ProgramChange,
    "pitch_bend" => PitchBend,
});

/// `midi`: gives one MIDI message to the local sound playing on an audio
/// channel (RULE-21).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MidiAction {
    /// The audio channel, 0 to 3.
    pub channel: AudioChannel,
    /// The message.
    pub message: RuleMidi,
}

/// `play`: starts a sound the page already has on an audio channel.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlayAction {
    /// The audio channel, 0 to 3.
    pub channel: AudioChannel,
    /// The sound's handle.
    pub handle: Byte,
    /// Its generation.
    pub generation: Word,
    /// The gain, 0 to 100.
    pub gain: GainPercent,
    /// The fade in, 0 to 10,000 milliseconds.
    pub fade_ms: FadeMs,
    /// It loops.
    pub r#loop: bool,
}

/// `stop`: ramps an audio channel to silence and stops it.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StopAction {
    /// The audio channel, 0 to 3.
    pub channel: AudioChannel,
    /// The fade out, 0 to 10,000 milliseconds.
    pub fade_ms: FadeMs,
}

/// `gain`: ramps an audio channel to a gain.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GainAction {
    /// The audio channel, 0 to 3.
    pub channel: AudioChannel,
    /// The gain, 0 to 100.
    pub gain: GainPercent,
    /// The ramp, 0 to 10,000 milliseconds.
    pub fade_ms: FadeMs,
}

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

/// `look`: forces a look on a control or an object.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LookOp {
    /// One control or one object.
    pub target: RuleTarget,
    /// The look.
    pub look: LookName,
}

/// `set`: sets a variable to a constant, held to its range (RULE-14).
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetAction {
    /// The variable's name.
    pub variable: RuleName,
    /// The constant.
    pub value: Constant,
}

/// `add`: adds a constant to a variable, saturating at its range.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AddAction {
    /// The variable's name.
    pub variable: RuleName,
    /// The constant.
    pub amount: Constant,
}

/// `start_timer` and `stop_timer` (RULE-15).
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TimerAction {
    /// The timer's name; the page timer cannot be started or stopped.
    pub timer: RuleName,
}

/// `enable` and `disable`: switches rules of the same table on or off,
/// from the next tick (RULE-16).
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SwitchAction {
    /// The rules' names.
    #[schemars(length(min = 1, max = MOST_RULES))]
    pub rules: Vec<RuleName>,
}

/// `send`: reports to the page's program (RULE-22).
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SendAction {
    /// The value reported, or null for the trigger's own event.
    #[serde(default)]
    pub value: Option<RuleValue>,
}

/// `move`: puts an object at a position, or moves it by an amount, in its
/// renderer's units (ANIM-6).
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MoveOp {
    /// One object.
    pub target: RuleTarget,
    /// Across.
    pub x: Offset,
    /// Down.
    pub y: Offset,
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
    /// The layer, 0 to 3, or null to keep it.
    #[serde(default)]
    pub layer: Option<Layer>,
    /// The order within the layer, lowest drawn first.
    pub order: OrderInLayer,
}

/// `frame`: shows one frame of an object, and stops any cycle (ANIM-14).
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FrameOp {
    /// One object.
    pub target: RuleTarget,
    /// The frame, 0 to 15.
    pub frame: FrameNumber,
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
    pub period: CyclePeriod,
    /// Stop at `last` rather than begin again.
    #[serde(default)]
    pub once: bool,
}

/// One local effect of a rule (RULE-20, RULE-35), by `action`.
#[derive(Clone, PartialEq, Eq, Debug, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum RuleAction {
    /// A MIDI message to a local sound.
    Midi(MidiAction),
    /// Starts a sound.
    Play(PlayAction),
    /// Stops an audio channel.
    Stop(StopAction),
    /// Changes an audio channel's gain.
    Gain(GainAction),
    /// Makes a control or an object shown.
    Show(TargetOp),
    /// Makes it hidden.
    Hide(TargetOp),
    /// Forces a look on a control or an object.
    Look(LookOp),
    /// Sets a variable.
    Set(SetAction),
    /// Adds to a variable.
    Add(AddAction),
    /// Starts or restarts a timer.
    StartTimer(TimerAction),
    /// Stops a timer.
    StopTimer(TimerAction),
    /// Switches rules on.
    Enable(SwitchAction),
    /// Switches rules off.
    Disable(SwitchAction),
    /// Reports to the page's program.
    Send(SendAction),
    /// Moves an object.
    Move(MoveOp),
    /// Sets an object's layer and order.
    Order(OrderOp),
    /// Shows one frame of an object.
    Frame(FrameOp),
    /// Cycles an object's frames.
    Cycle(CycleOp),
    /// Stops an object's cycle at the frame shown.
    StopCycle(TargetOp),
}

tagged_choice!(RuleAction, "action", {
    "midi" => Midi,
    "play" => Play,
    "stop" => Stop,
    "gain" => Gain,
    "show" => Show,
    "hide" => Hide,
    "look" => Look,
    "set" => Set,
    "add" => Add,
    "start_timer" => StartTimer,
    "stop_timer" => StopTimer,
    "enable" => Enable,
    "disable" => Disable,
    "send" => Send,
    "move" => Move,
    "order" => Order,
    "frame" => Frame,
    "cycle" => Cycle,
    "stop_cycle" => StopCycle,
});

// ---------------------------------------------------------- the table

/// A page variable's declaration (RULE-14): `min ≤ initial ≤ max`.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VariableDeclaration {
    /// Its name.
    pub name: RuleName,
    /// The least value it holds.
    pub min: Constant,
    /// The greatest.
    pub max: Constant,
    /// Its value when the page opens.
    pub initial: Constant,
}

/// A timer's declaration (RULE-15).
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TimerDeclaration {
    /// Its name; never `page`.
    pub name: RuleName,
    /// Whether it runs from the page's opening.
    pub running: bool,
}

/// A rectangle's declaration (RULE-13), in the pixels of the sub-page's
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
    pub width: U16In<1, 65_535>,
    /// The height, at least 1.
    pub height: U16In<1, 65_535>,
}

/// The rules switched off while one sub-page is shown (RULE-17).
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SubpageRules {
    /// The sub-page: never 3F7F, and named once in a table.
    pub sub: SubCode,
    /// The names of the rules that are off while it is shown.
    #[schemars(length(max = MOST_RULES))]
    pub off: Vec<RuleName>,
}

/// What `enabled` is when a rule does not say.
const fn enabled_by_default() -> bool {
    true
}

/// A rule (RULE-3): one trigger, at most one condition and at most 16
/// actions.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PageRule {
    /// Its name, by which `input` and `rule_event` name it.
    pub name: RuleName,
    /// The trigger.
    pub on: RuleTrigger,
    /// What must hold for it to fire, or null; null for a `becomes`
    /// trigger, which carries its own condition.
    #[serde(default)]
    pub when: Option<RuleCondition>,
    /// Its actions, run in written order. A rule with none has `echo`.
    #[schemars(length(max = MOST_ACTIONS))]
    pub r#do: Vec<RuleAction>,
    /// A `send` of the trigger's own event after its last action.
    #[serde(default)]
    pub echo: bool,
    /// Its state when the page opens.
    #[serde(default = "enabled_by_default")]
    pub enabled: bool,
}

/// A page's rule table (RULE-2, RULE-35): one for a page, for all its
/// sub-pages.
#[derive(Clone, PartialEq, Eq, Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RuleTable {
    /// The page variables.
    #[serde(default)]
    #[schemars(length(max = MOST_VARIABLES))]
    pub variables: Vec<VariableDeclaration>,
    /// The timers, beside the page timer.
    #[serde(default)]
    #[schemars(length(max = MOST_TIMERS))]
    pub timers: Vec<TimerDeclaration>,
    /// The rectangles.
    #[serde(default)]
    #[schemars(length(max = MOST_RECTS))]
    pub rects: Vec<RectDeclaration>,
    /// The rules, in rule order.
    #[schemars(length(max = MOST_RULES))]
    pub rules: Vec<PageRule>,
    /// The rules switched off for a sub-page.
    #[serde(default)]
    #[schemars(length(max = MOST_SUBPAGE_MASKS))]
    pub subpages: Vec<SubpageRules>,
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

/// `rules`: gives a page its rule table for the connections named, or
/// with a null table removes it, which puts the channel's default table
/// back where the channel has one (RULE-35, RULE-53). The server keeps the
/// newest table for each connection and page and sends it when the
/// connection selects the page, before the page (RULE-40).
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RulesMessage {
    /// Who it is for.
    pub connections: Connections,
    /// The page.
    pub page: PageNumber,
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
    /// The program's own number for the question.
    pub request: U64In<1, 4_294_967_295>,
}

// ------------------------------------------------ server to program

/// What a `rule_event` reports.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RuleEventBody {
    /// The rule fired, on `open`, `close`, `becomes`, `every` or `after`.
    Fired,
    /// A `send` of a value: what was read, as the table names it, and its
    /// reading in the tick's snapshot.
    Value {
        /// What was read.
        read: RuleValue,
        /// Its reading.
        value: i32,
    },
}

/// `rule_event`: a rule that has no input event fired and sent, or a rule
/// sent a value (RULE-37).
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
    /// The rule's name.
    pub rule: RuleName,
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
    /// The ticks since it started; 0 while it is stopped.
    pub elapsed: u32,
}

/// A control's or an object's state in `page_state`.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TargetState {
    /// The control or the object.
    pub target: RuleTarget,
    /// Whether it exists on the sub-page shown.
    pub exists: bool,
    /// Whether it is shown.
    pub shown: bool,
    /// The look in force.
    pub look: LookName,
    /// Its frame in force.
    #[schemars(range(max = 15))]
    pub frame: u8,
    /// Its position across, in its renderer's units.
    pub x: i16,
    /// Its position down.
    pub y: i16,
    /// Its layer.
    #[schemars(range(max = 3))]
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
    /// Its reading in the answering tick's snapshot.
    pub value: i32,
}

/// `page_state`: the answer to `query_state` (RULE-38), a page's local
/// state in one view at one tick. `table` is false, and every list empty,
/// when the page has no table in that view.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PageState {
    /// The connection.
    #[schemars(range(min = 1, max = SAFE_INTEGER_LIMIT))]
    pub connection: ConnectionId,
    /// The page.
    pub page: PageNumber,
    /// The sub-code on screen.
    pub sub: SubCode,
    /// The `request` of the `query_state` it answers.
    #[schemars(range(min = 1))]
    pub request: u32,
    /// The page tick of the snapshot reported.
    pub tick: u32,
    /// Whether the page has a table in that view.
    pub table: bool,
    /// Each variable's value, by its name.
    #[schemars(length(max = MOST_VARIABLES))]
    pub variables: BTreeMap<RuleName, i32>,
    /// Each timer's state, by its name.
    #[schemars(length(max = MOST_TIMERS))]
    pub timers: BTreeMap<RuleName, TimerState>,
    /// The names of the rules enabled, in rule order.
    #[schemars(length(max = MOST_RULES))]
    pub enabled: Vec<RuleName>,
    /// Every control and object the table names, in the order first named.
    #[schemars(length(max = MOST_TABLE_TARGETS))]
    pub targets: Vec<TargetState>,
    /// Every distinct value the table's conditions and sends read, in the
    /// order first read.
    #[schemars(length(max = MOST_TABLE_VALUES))]
    pub values: Vec<ValueReading>,
}

/// A fault a receiver reports of a page's table (RULE-48).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RulesFaultName {
    /// The receiver could not decode the table and refused it whole: the
    /// page has no table in that view.
    Undecodable,
    /// A tick passed 256 actions, and further firings in it were dropped.
    TickOverrun,
    /// Sends dropped by the receiver's send budget.
    SendsDropped,
    /// Input events dropped from a full queue.
    EventsDropped,
    /// Actions skipped for a missing target or sound.
    ActionsSkipped,
    /// A state query dropped.
    QueryDropped,
    /// A local sound stopped for overrunning its time.
    LocalOverrun,
    /// Animation slowed by its budget.
    AnimationSlowed,
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
    /// The generation of the table the receiver holds, as the server
    /// numbers each table it sends that connection for the page.
    pub generation: u16,
    /// The fault.
    pub fault: RulesFaultName,
    /// How many times since the receiver's last report of it.
    pub count: u16,
}
