//! The rules compiler of program protocol 15 (page-rules.md RULE-35,
//! RULE-36, RULE-39, RULE-59, RULE-114; REQ-6532, REQ-6608; DEC-6587,
//! DEC-6599, DEC-6635): a `rules` message's table, and the commands sent
//! directly, turned from the names a program writes into the engine's
//! table and records — every name into its number, every key's name into
//! its code, every synthesizer's field into its parameter number, `do` and
//! `echo` into unnamed groups, row sets laid out one after another in the
//! order of their names, every amount into a binding — and the whole
//! encoded and checked with the engine's `neoteletext-rules` and the body
//! check of `neoteletext-web`, the code every receiver decodes it with.
//! The first fault refuses the table whole, with its path; a limit the
//! engine finds is refused naming its constant and its value (REQ-6608).

use alloc::boxed::Box;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use neoteletext_audio::encode::Command as Control;
use neoteletext_audio::record::{FLAG_LOOP, split};
use neoteletext_input::keys;
use neoteletext_rules::animate::{self, Operation, OperationKind};
use neoteletext_rules::command::{self, Binding, Command, Operand, Source, StartMode, VariableOperation, field, midi_status};
use neoteletext_rules::table::{
    self, Fault as EngineFault, Framed, Key, KeyKind, KeyMatch, Node, Operator, Rect, RowRun, Rule as EngineRule, Sequence, Step, Table, Target, TargetKind,
    Trigger, TriggerTarget, Value,
};
use neoteletext_rules::{limits, names};

use super::{Checked, Fault, Path, fault};
use crate::animation;
use crate::protocol::{
    Amount, AmountSource, AnimationOp, ButtonChoice, CommandMessage, CompareOp, CompareValue, CueChoice, DataAmount, FiredName, GroupSound, KeySet,
    LocalMidiEvent, LookChoice, Mods, PAGE_NAME, PageRule, Rule, RuleCommand, RuleCondition, RuleKey, RuleName, RuleRow, RuleTable, RuleTarget, RuleTrigger,
    RuleValue, SequenceAt, SequenceCommand, StartStop, SynthProperty, TargetNumber, TargetSource, TimerProperty, VariableCommand, VariableOp, VariableOperand,
    VariableOrAll,
};
use crate::rules::{Compiled, Names, code_of, look_number, synthesizer_code, word_of};

/// The place of an operation's target number in its bytes: after its code
/// byte and its target's kind byte (animation.md ANIM-6).
const TARGET_NUMBER_AT: usize = 1 + 1;
/// The voices of the synth6581 family, which a `synth` value names as its
/// part (RULE-28).
const SYNTH6581_VOICES: u8 = 3;
/// The seven bits of a MIDI data byte (MIDI 1.0 Detailed Specification,
/// "Channel Voice Messages").
const SEVEN_BITS: u16 = 0x7F;

/// One command as a group frames it (RULE-45), or as the server sends it
/// directly: its record type, its body and its bindings.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct Built {
    /// The record type.
    pub(crate) kind: u8,
    /// The record's body.
    pub(crate) body: Vec<u8>,
    /// Its bindings (RULE-106); none for a command sent directly.
    pub(crate) bindings: Vec<Binding>,
}

/// Where a command is compiled: in a group of a table, or for a record the
/// server sends directly (RULE-114), whose fields are constants alone.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Place {
    /// In a group.
    Group,
    /// Sent directly.
    Direct,
}

/// A control's or an object's number, whatever its range.
#[derive(Clone, Copy)]
enum Number {
    Any,
    Fixed(u16),
    Bound { plus: i32, source: TargetSource },
}

impl<const MOST: u16> From<TargetNumber<MOST>> for Number {
    fn from(number: TargetNumber<MOST>) -> Self {
        match number {
            TargetNumber::Any => Self::Any,
            TargetNumber::Numbered(number) => Self::Fixed(number.get()),
            TargetNumber::Bound { plus, source } => Self::Bound { plus, source },
        }
    }
}

/// The names of a table, as it declares them, and its page.
struct Scope<'a> {
    page: u16,
    sub: u16,
    /// The names, by number: the named groups', the sequences' and the
    /// rectangles'.
    names: &'a Names,
    /// Each row set's name, first row and count.
    rows: &'a [(String, u8, u8)],
    /// Whether names are resolved: false for a command checked before the
    /// table it names is known, whose names resolve to 0.
    resolve: bool,
    /// The generation a `naplps_objects` command is written for: that of
    /// the NAPLPS stream the connection has of the page when the table is
    /// sent (ANIM-21; DEC-6654 item 11).
    naplps_generation: u16,
}

/// The place of `name` in `list`, or its refusal: the table has no such
/// name.
fn place_in(path: &Path<'_>, list: &[String], name: &RuleName, kind: &str) -> Result<u8, Fault> {
    list.iter()
        .position(|held| *held == name.0)
        .and_then(|place| u8::try_from(place).ok())
        .ok_or_else(|| fault(path, Rule::UnknownName, format!("the table has no {kind} named `{}`", name.0)))
}

/// A fault the engine's crate found, as a refusal: a limit of RULE-57 is
/// `rules_over_limit`, naming the limit's constant and its value
/// (REQ-6608); a fault of a group is `invalid_group`; anything else is what
/// a receiver's decoder would refuse, `invalid_rules` (RULE-39).
pub(crate) fn engine_fault(path: &Path<'_>, refused: EngineFault) -> Fault {
    let limit = |name: &str| match limits::value_of(name) {
        Some(value) => fault(path, Rule::RulesOverLimit, format!("{name}: at most {value}")),
        None => fault(path, Rule::RulesOverLimit, format!("{name}: over a limit of the table")),
    };
    match refused {
        EngineFault::Limit(name) => limit(name),
        // The image was encoded into `MAX_TABLE_BYTES`: what does not fit
        // it is over that limit.
        EngineFault::Full => limit("MAX_TABLE_BYTES"),
        EngineFault::Ring | EngineFault::Command(_) | EngineFault::Binding(_) => {
            fault(path, Rule::InvalidGroup, format!("the receivers' decoder refuses the groups: {refused:?}"))
        }
        other => fault(path, Rule::InvalidRules, format!("the receivers' decoder refuses it: {other:?}")),
    }
}

/// A number the record holds in a byte.
fn byte(path: &Path<'_>, number: usize) -> Result<u8, Fault> {
    u8::try_from(number).map_err(|_| fault(path, Rule::OutOfRange, format!("{number} is past 255")))
}

/// A value of the firing's number (RULE-116).
fn fired_number(name: FiredName) -> u8 {
    code_of(&names::FIRED_PROPERTIES, &name).and_then(|code| u8::try_from(code).ok()).unwrap_or(0)
}

/// A property's number in the engine's table of its words.
fn property<T: serde::Serialize>(pairs: &[(&str, u16)], item: &T) -> u16 {
    names::code(pairs, &word_of(item)).unwrap_or(0)
}

/// A constant narrowed to a field the schema bounded.
fn narrow<T: TryFrom<i32> + Default>(value: i32) -> T {
    T::try_from(value).unwrap_or_default()
}

/// Collects the bindings of one command (RULE-106).
struct Binder {
    place: Place,
    bindings: Vec<Binding>,
    /// Each bound target of an `ANIMATE` record: its operation's place and
    /// its constant, written into the body once it is encoded.
    targets: Vec<(u8, u16)>,
}

impl Binder {
    const fn new(place: Place) -> Self {
        Self { place, bindings: Vec::new(), targets: Vec::new() }
    }

    /// The engine's source of an amount's source.
    fn source(source: AmountSource) -> Source {
        match source {
            AmountSource::Variable(variable) => Source::Variable(variable.0),
            AmountSource::Row | AmountSource::Fired(FiredName::P) => Source::Parameter,
            AmountSource::Fired(name) => Source::Fired(fired_number(name)),
        }
    }

    /// An amount's constant, binding its source to `field` of operation
    /// `item` (0 for a record that is not an `ANIMATE`).
    fn amount<const LEAST: i32, const MOST: i32>(&mut self, path: &Path<'_>, amount: Amount<LEAST, MOST>, item: u8, field: u8) -> Result<i32, Fault> {
        if let Some(source) = amount.source() {
            if self.place == Place::Direct {
                return Err(fault(path, Rule::InvalidCommand, "an amount is a group's: a message sent directly gives a constant".to_string()));
            }
            self.bindings.push(Binding::of_operation(item, field, Self::source(source)));
        }
        Ok(amount.constant())
    }
}

impl Scope<'_> {
    /// A group's number by its name.
    fn group(&self, path: &Path<'_>, name: &RuleName) -> Result<u8, Fault> {
        if !self.resolve {
            return Ok(0);
        }
        place_in(path, &self.names.groups, name, "group")
    }

    /// A sequence's number by its name.
    fn sequence(&self, path: &Path<'_>, name: &RuleName) -> Result<u8, Fault> {
        if !self.resolve {
            return Ok(0);
        }
        place_in(path, &self.names.sequences, name, "sequence")
    }

    /// One control or one object a value or a condition names (RULE-19,
    /// RULE-26); with `page_and_rect` the page and a rectangle too, with
    /// `rect` a rectangle.
    fn named_target(&self, path: &Path<'_>, target: &RuleTarget, page_and_rect: bool, rect: bool) -> Result<Target, Fault> {
        let invalid = |detail: &str| fault(path, Rule::InvalidRules, detail.to_string());
        let number = |kind: TargetKind, number: Number| match number {
            Number::Fixed(number) => Ok(Target { kind, number }),
            Number::Any => Err(invalid("\"any\" is for the pointer triggers: here one control or object is named")),
            Number::Bound { .. } => Err(invalid("a number from the row or the firing is for a trigger or an operation: here one number is named")),
        };
        match target {
            RuleTarget::Page(_) if page_and_rect => Ok(Target::PAGE),
            RuleTarget::Control { control } => number(TargetKind::Control, Number::from(*control)),
            RuleTarget::Object { object } => number(TargetKind::Object, Number::from(*object)),
            RuleTarget::Rect { rect: name } if rect || page_and_rect => {
                Ok(Target::rect(u16::from(place_in(&path.field("rect"), &self.names.rects, name, "rectangle")?)))
            }
            _ => Err(invalid("the target is one control or one object")),
        }
    }

    /// A pointer trigger's target (RULE-12): anything, "any" too, and in a
    /// rule with rows a constant plus the row's parameter (RULE-105).
    fn trigger_target(&self, path: &Path<'_>, target: &RuleTarget, rows: bool) -> Result<TriggerTarget, Fault> {
        let numbered = |kind: TargetKind, number: Number| match number {
            Number::Any if kind == TargetKind::Control => Ok(TriggerTarget::of(Target::ANY_CONTROL)),
            Number::Any => Ok(TriggerTarget::of(Target::ANY_OBJECT)),
            Number::Fixed(number) => Ok(TriggerTarget::of(Target { kind, number })),
            Number::Bound { source, .. } if source != TargetSource::Row => {
                Err(fault(path, Rule::InvalidRules, "a trigger's target number is from the row, `{\"row\":\"p\"}`, never the firing".to_string()))
            }
            Number::Bound { .. } if !rows => Err(fault(path, Rule::InvalidRules, "a `{\"row\":…}` is in a rule with `rows` alone".to_string())),
            Number::Bound { plus, .. } => Ok(TriggerTarget::row(kind, narrow(plus))),
        };
        match target {
            RuleTarget::Page(_) => Ok(TriggerTarget::PAGE),
            RuleTarget::Control { control } => numbered(TargetKind::Control, Number::from(*control)),
            RuleTarget::Object { object } => numbered(TargetKind::Object, Number::from(*object)),
            rect @ RuleTarget::Rect { .. } => Ok(TriggerTarget::of(self.named_target(path, rect, false, true)?)),
        }
    }

    /// One key (RULE-10): its kind and its code.
    fn key(path: &Path<'_>, key: &RuleKey) -> Result<Key, Fault> {
        let character = |path: &Path<'_>, text: &str| {
            let character = text.chars().next().ok_or_else(|| fault(path, Rule::TooShort, "one character".to_string()))?;
            if character.is_uppercase() {
                return Err(fault(path, Rule::InvalidRules, format!("a letter is given in lower case: `{}`", character.to_lowercase())));
            }
            Ok(u32::from(character))
        };
        Ok(match key {
            RuleKey::Char { char } => Key::of(KeyKind::Character, character(&path.field("char"), char)?),
            RuleKey::Physical { physical } => Key::of(KeyKind::Physical, character(&path.field("physical"), physical)?),
            RuleKey::Name { name } => {
                let code = keys::code(&name.0)
                    .filter(|_| name.0.chars().count() > 1)
                    .ok_or_else(|| fault(&path.field("name"), Rule::NotListed, format!("`{}` names no key of the engine's table (DOC-1936)", name.0)))?;
                Key::of(KeyKind::Named, code)
            }
        })
    }

    /// A key set and its modifiers (RULE-10), one key a row with `by_row`
    /// (RULE-105): the set then has as many keys as the rule has rows.
    fn keys(path: &Path<'_>, set: &KeySet, mods: Mods, by_row: bool, rows: Option<usize>) -> Result<KeyMatch, Fault> {
        let listed: Vec<Key> = match set {
            KeySet::Any(_) if by_row => return Err(fault(&path.field("keys"), Rule::InvalidRules, "`by_row` gives one key a row: not \"any\"".to_string())),
            KeySet::Any(_) => Vec::new(),
            KeySet::Keys(listed) => listed.iter().enumerate().map(|(index, key)| Self::key(&path.field("keys").item(index), key)).collect::<Result<_, _>>()?,
        };
        if by_row {
            let Some(rows) = rows else {
                return Err(fault(&path.field("by_row"), Rule::InvalidRules, "`by_row` is for a rule with `rows` alone".to_string()));
            };
            if listed.len() != rows {
                return Err(fault(&path.field("keys"), Rule::InvalidRules, format!("{} keys for {rows} rows: `by_row` gives one key a row", listed.len())));
            }
        }
        let mut matched = if by_row {
            KeyMatch::by_row(&listed)
        } else if listed.is_empty() {
            KeyMatch::any()
        } else {
            KeyMatch::of(&listed)
        };
        matched.mods_mask = mods.mask.get();
        matched.mods_value = mods.value.get();
        Ok(matched)
    }

    /// A trigger (RULE-9, RULE-43).
    fn trigger(&self, path: &Path<'_>, trigger: &RuleTrigger, rows: Option<usize>) -> Result<Trigger, Fault> {
        let button = |choice: ButtonChoice| match choice {
            ButtonChoice::Any(_) => limits::ANY_BUTTON,
            ButtonChoice::Numbered(button) => button.get(),
        };
        let target = |target: &RuleTarget| self.trigger_target(&path.field("target"), target, rows.is_some());
        let ticks = |ticks: u64| u32::try_from(ticks).unwrap_or(u32::MAX);
        Ok(match trigger {
            RuleTrigger::Key(key) => Trigger::Key { keys: Self::keys(path, &key.keys, key.mods, key.by_row, rows)?, repeat: key.repeat },
            RuleTrigger::KeyUp(key) => Trigger::KeyUp { keys: Self::keys(path, &key.keys, key.mods, key.by_row, rows)? },
            RuleTrigger::Press(press) => Trigger::Press { target: target(&press.target)?, button: button(press.button) },
            RuleTrigger::Release(release) => Trigger::Release { target: target(&release.target)?, button: button(release.button) },
            RuleTrigger::Enter(enter) => Trigger::Enter { target: target(&enter.target)? },
            RuleTrigger::Leave(leave) => Trigger::Leave { target: target(&leave.target)? },
            RuleTrigger::Wheel(wheel) => Trigger::Wheel { target: target(&wheel.target)? },
            RuleTrigger::Move(moved) => Trigger::Move { target: target(&moved.target)?, held: moved.held },
            RuleTrigger::Open(_) => Trigger::Open,
            RuleTrigger::Close(_) => Trigger::Close,
            RuleTrigger::Text(_) => Trigger::Text,
            RuleTrigger::Focus(_) => Trigger::Focus,
            RuleTrigger::Blur(_) => Trigger::Blur,
            RuleTrigger::Becomes(_) => Trigger::Becomes,
            RuleTrigger::Every(every) => Trigger::Every { timer: every.timer.number(), ticks: ticks(every.ticks.get()) },
            RuleTrigger::After(after) => Trigger::After { timer: after.timer.number(), ticks: ticks(after.ticks.get()) },
            RuleTrigger::Cue(cue) => Trigger::Cue {
                channel: cue.channel.get(),
                number: match cue.number {
                    CueChoice::Any(_) => limits::ANY_CUE,
                    CueChoice::Numbered(number) => number.get(),
                },
            },
        })
    }

    /// A value (section 6).
    fn value(&self, path: &Path<'_>, value: &RuleValue) -> Result<Value, Fault> {
        Ok(match value {
            RuleValue::Variable(variable) => Value::variable(variable.variable.0),
            RuleValue::Key(key) => {
                let key = Self::key(&path.field("key"), &key.key)?;
                Value::key(key.kind.code(), key.code)
            }
            RuleValue::Button(button) => Value::button(button.button.get()),
            RuleValue::Pointer(pointer) => Value::pointer(property(&names::POINTER_PROPERTIES, &pointer.property)),
            RuleValue::KeysDown(_) => Value::keys_down(),
            RuleValue::Timer(timer) => match (timer.property, timer.period) {
                (TimerProperty::Phase, Some(period)) => Value::timer_phase(timer.timer.number(), u32::try_from(period.get()).unwrap_or(u32::MAX)),
                (TimerProperty::Phase, None) => {
                    return Err(fault(&path.field("period"), Rule::InvalidRules, "a timer's `phase` is read in a `period`".to_string()));
                }
                (_, Some(_)) => return Err(fault(&path.field("period"), Rule::InvalidRules, "`period` is given with `phase` alone".to_string())),
                (other, None) => Value::timer(timer.timer.number(), property(&names::TIMER_PROPERTIES, &other)),
            },
            RuleValue::Target(target) => {
                let named = self.named_target(&path.field("target"), &target.target, false, false)?;
                Value::target(named.kind, named.number, property(&names::TARGET_PROPERTIES, &target.property))
            }
            RuleValue::Sound(sound) => Value::sound(sound.channel.get(), property(&names::SOUND_PROPERTIES, &sound.property)),
            RuleValue::Synth(synth) => {
                let (code, variant) = synthesizer_code(synth.family);
                let needs = |given: bool, wanted: bool, name: &'static str, with: &str| match (given, wanted) {
                    (false, true) => Err(fault(&path.field(name), Rule::MissingField, format!("missing field `{name}`, which `{with}` reads"))),
                    (true, false) => Err(fault(&path.field(name), Rule::InvalidRules, format!("`{name}` is given only with {with}"))),
                    _ => Ok(()),
                };
                let synth_property = synth.property;
                needs(synth.key.is_some(), matches!(synth_property, SynthProperty::Note | SynthProperty::Velocity), "key", "note and velocity")?;
                needs(synth.field.is_some(), synth_property == SynthProperty::Field, "field", "field")?;
                needs(synth.controller.is_some(), synth_property == SynthProperty::Controller, "controller", "controller")?;
                let index = match (&synth.field, synth.key, synth.controller) {
                    (Some(name), _, _) => {
                        let fields = crate::rules::fields_of(synth.family);
                        let number = fields.iter().position(|field| field.name == name).ok_or_else(|| {
                            fault(
                                &path.field("field"),
                                Rule::UnknownParameter,
                                format!("`{name}` is no readable field of {:?} (page-rules.md appendix A)", synth.family),
                            )
                        })?;
                        u16::try_from(number).unwrap_or(u16::MAX)
                    }
                    (None, Some(key), _) => u16::from(key.get()),
                    (None, None, Some(controller)) => u16::from(controller.get()),
                    (None, None, None) => 0,
                };
                // The synth6581 family has three voices and no parts
                // (RULE-28).
                if code == neoteletext_audio_instruments::midi::family::SYNTH6581 && synth.part.get() > SYNTH6581_VOICES {
                    return Err(fault(
                        &path.field("part"),
                        Rule::OutOfRange,
                        format!("{}: a voice of this synthesizer is 1 to {SYNTH6581_VOICES}", synth.part.get()),
                    ));
                }
                let class = code_of(&names::SYNTH_PROPERTIES, &synth_property).unwrap_or(0);
                Value::synth(synth.channel.get(), code, variant, synth.part.get().saturating_sub(1), class, index)
            }
            RuleValue::Fired(fired) => Value::fired(u16::from(fired_number(fired.property))),
            RuleValue::Sequence(sequence) => {
                Value::sequence(self.sequence(&path.field("sequence"), &sequence.sequence)?, property(&names::SEQUENCE_PROPERTIES, &sequence.property))
            }
        })
    }

    /// A condition's nodes in prefix order, added to `nodes` (RULE-44).
    fn nodes(&self, path: &Path<'_>, condition: &RuleCondition, rows: bool, nodes: &mut Vec<Node>) -> Checked {
        let children = |count: usize| u8::try_from(count).unwrap_or(u8::MAX);
        match condition {
            RuleCondition::And { and } => {
                nodes.push(Node::And(children(and.len())));
                and.iter().enumerate().try_for_each(|(index, child)| self.nodes(&path.field("and").item(index), child, rows, nodes))
            }
            RuleCondition::Or { or } => {
                nodes.push(Node::Or(children(or.len())));
                or.iter().enumerate().try_for_each(|(index, child)| self.nodes(&path.field("or").item(index), child, rows, nodes))
            }
            RuleCondition::Not { not } => {
                nodes.push(Node::Not);
                self.nodes(&path.field("not"), not, rows, nodes)
            }
            RuleCondition::Compare { read, op, value } => {
                let operator = match op {
                    CompareOp::Equal => Operator::Equal,
                    CompareOp::NotEqual => Operator::NotEqual,
                    CompareOp::Less => Operator::Less,
                    CompareOp::LessOrEqual => Operator::LessOrEqual,
                    CompareOp::Greater => Operator::Greater,
                    CompareOp::GreaterOrEqual => Operator::GreaterOrEqual,
                };
                let (constant, plus_p) = match *value {
                    CompareValue::Constant(constant) => (constant, false),
                    CompareValue::PlusRow(_) if !rows => {
                        return Err(fault(&path.field("value"), Rule::InvalidRules, "a `{\"row\":…}` is in a rule with `rows` alone".to_string()));
                    }
                    CompareValue::PlusRow(constant) => (constant, true),
                };
                nodes.push(Node::Compare { value: self.value(&path.field("read"), read)?, operator, constant, plus_p });
                Ok(())
            }
            RuleCondition::InRange { read, in_range: [low, high] } => {
                if low.get() > high.get() {
                    return Err(fault(&path.field("in_range"), Rule::InvalidRules, format!("low {} is above high {}", low.get(), high.get())));
                }
                nodes.push(Node::InRange { value: self.value(&path.field("read"), read)?, low: low.get(), high: high.get() });
                Ok(())
            }
            RuleCondition::InSet { read, in_set } => {
                let mut members = table::SetMembers::new(0);
                for member in in_set {
                    members.push(member.get()).map_err(|_| engine_fault(&path.field("in_set"), EngineFault::Limit("MAX_SET_MEMBERS")))?;
                }
                nodes.push(Node::InSet { value: self.value(&path.field("read"), read)?, members });
                Ok(())
            }
            RuleCondition::Overlaps { overlaps: [first, second] } => {
                let at = path.field("overlaps");
                nodes.push(Node::Overlaps(self.named_target(&at.item(0), first, false, true)?, self.named_target(&at.item(1), second, true, true)?));
                Ok(())
            }
            RuleCondition::Inside { inside: [first, second] } => {
                let at = path.field("inside");
                nodes.push(Node::Inside(self.named_target(&at.item(0), first, false, true)?, self.named_target(&at.item(1), second, true, true)?));
                Ok(())
            }
        }
    }

    /// A rule's condition, or none: one tree within RULE-18's limits.
    fn condition(&self, path: &Path<'_>, condition: Option<&RuleCondition>, rows: bool) -> Result<table::Condition, Fault> {
        let Some(condition) = condition else { return Ok(table::no_condition()) };
        let mut nodes = Vec::new();
        self.nodes(path, condition, rows, &mut nodes)?;
        if nodes.len() > limits::MAX_CONDITION_NODES {
            return Err(engine_fault(path, EngineFault::Limit("MAX_CONDITION_NODES")));
        }
        table::check_condition(&nodes).map_err(|refused| engine_fault(path, refused))?;
        Ok(table::condition(&nodes))
    }

    /// An operation's target (ANIM-6): one control or one object, or in a
    /// group a constant plus the row's parameter or the firing's target or
    /// row, which is bound (RULE-106).
    fn operation_target(path: &Path<'_>, target: &RuleTarget, object_only: bool, item: u8, binder: &mut Binder) -> Result<Target, Fault> {
        let at = path.field("target");
        let (kind, number) = match target {
            RuleTarget::Control { control } if !object_only => (TargetKind::Control, Number::from(*control)),
            RuleTarget::Object { object } => (TargetKind::Object, Number::from(*object)),
            _ if object_only => return Err(fault(&at, Rule::InvalidAnimation, "the target is one object".to_string())),
            _ => return Err(fault(&at, Rule::InvalidAnimation, "the target is one control or one object".to_string())),
        };
        match number {
            Number::Fixed(number) => Ok(Target { kind, number }),
            Number::Any => Err(fault(&at, Rule::InvalidAnimation, "\"any\" is for the pointer triggers: an operation names one".to_string())),
            Number::Bound { plus, source } => {
                if binder.place == Place::Direct {
                    return Err(fault(&at, Rule::InvalidCommand, "a number from the row or the firing is a group's".to_string()));
                }
                let constant = u16::try_from(plus)
                    .map_err(|_| fault(&at.field("plus"), Rule::InvalidGroup, format!("{plus}: an operation's target constant is 0 to {}", u16::MAX)))?;
                let source = match source {
                    TargetSource::Row => Source::Parameter,
                    TargetSource::FiredTarget => Source::Fired(fired_number(FiredName::Target)),
                    TargetSource::FiredRow => Source::Fired(fired_number(FiredName::Row)),
                };
                binder.bindings.push(Binding::of_operation(item, field::TARGET, source));
                binder.targets.push((item, constant));
                // The record's writer refuses the number 0, "any": the
                // constant is written once the record is (RULE-106).
                Ok(Target { kind, number: constant.max(1) })
            }
        }
    }

    /// The operations of an `animate` command or message, and the record's
    /// body (ANIM-6).
    fn animate(&self, path: &Path<'_>, ops: &[AnimationOp], binder: &mut Binder) -> Result<(Vec<Operation>, Vec<u8>), Fault> {
        if ops.is_empty() {
            return Err(fault(path, Rule::TooShort, "no operation".to_string()));
        }
        if ops.len() > limits::MAX_ANIMATE_OPERATIONS {
            return Err(fault(path, Rule::TooLong, format!("{} operations, at most {}", ops.len(), limits::MAX_ANIMATE_OPERATIONS)));
        }
        let mut operations = Vec::with_capacity(ops.len());
        for (index, op) in ops.iter().enumerate() {
            let at = path.item(index);
            let item = byte(&at, index)?;
            let (target, kind) = match op {
                AnimationOp::Show(op) => (Self::operation_target(&at, &op.target, false, item, binder)?, OperationKind::Show),
                AnimationOp::Hide(op) => (Self::operation_target(&at, &op.target, false, item, binder)?, OperationKind::Hide),
                AnimationOp::Reset(op) => (Self::operation_target(&at, &op.target, false, item, binder)?, OperationKind::Reset),
                AnimationOp::StopCycle(op) => (Self::operation_target(&at, &op.target, true, item, binder)?, OperationKind::StopCycle),
                AnimationOp::Look(op) => {
                    let target = Self::operation_target(&at, &op.target, false, item, binder)?;
                    let look = match op.look {
                        LookChoice::Named(name) => look_number(name),
                        LookChoice::Amount(amount) => narrow(binder.amount(&at.field("look"), amount, item, field::LOOK)?),
                    };
                    (target, OperationKind::Look(look))
                }
                AnimationOp::Move(op) => {
                    let target = Self::operation_target(&at, &op.target, true, item, binder)?;
                    let x = binder.amount(&at.field("x"), op.x, item, field::X)?;
                    let y = binder.amount(&at.field("y"), op.y, item, field::Y)?;
                    (target, OperationKind::Move { relative: op.relative, x: narrow(x), y: narrow(y) })
                }
                AnimationOp::Order(op) => {
                    let target = Self::operation_target(&at, &op.target, true, item, binder)?;
                    let layer = op.layer.map_or(limits::KEEP_LAYER, crate::protocol::Layer::get);
                    let order = binder.amount(&at.field("order"), op.order, item, field::ORDER)?;
                    (target, OperationKind::Order { layer, order: narrow(order) })
                }
                AnimationOp::Frame(op) => {
                    let target = Self::operation_target(&at, &op.target, true, item, binder)?;
                    (target, OperationKind::Frame(narrow(binder.amount(&at.field("frame"), op.frame, item, field::FRAME)?)))
                }
                AnimationOp::Cycle(op) => {
                    if op.first.get() > op.last.get() {
                        return Err(fault(&at.field("first"), Rule::InvalidAnimation, format!("first {} is above last {}", op.first.get(), op.last.get())));
                    }
                    let target = Self::operation_target(&at, &op.target, true, item, binder)?;
                    let period = binder.amount(&at.field("period"), op.period, item, field::PERIOD)?;
                    (target, OperationKind::Cycle { first: op.first.get(), last: op.last.get(), period: narrow(period), once: op.once })
                }
            };
            let operation = Operation { target, kind };
            operation.check().map_err(|refused| fault(&at, Rule::InvalidAnimation, format!("{refused:?}")))?;
            operations.push(operation);
        }
        let mut body = alloc::vec![0_u8; animate::MAX_RECORD_BYTES];
        let length = animate::write_record(self.page, self.sub, &operations, &mut body)
            .map_err(|refused| fault(path, Rule::InvalidAnimation, format!("{refused:?}")))?;
        body.truncate(length);
        for (item, constant) in &binder.targets {
            let at = operations
                .iter()
                .take(usize::from(*item))
                .fold(animate::RECORD_FIXED, |at, operation| at.saturating_add(animate::length_of(operation.code()).unwrap_or(0)));
            let number_at = at.saturating_add(TARGET_NUMBER_AT);
            if let Some(slot) = body.get_mut(number_at..number_at.saturating_add(2)) {
                slot.copy_from_slice(&constant.to_be_bytes());
            }
        }
        Ok((operations, body))
    }

    /// The body of one command of the engine's `neoteletext_rules::command`
    /// records (types 28 to 33).
    fn encoded(&self, path: &Path<'_>, command: &Command) -> Result<Vec<u8>, Fault> {
        let mut body = alloc::vec![0_u8; command::MAX_BODY_BYTES];
        let length = command::encode(command, self.page, self.sub, &mut body).map_err(|refused| engine_fault(path, refused))?;
        body.truncate(length);
        Ok(body)
    }

    /// A group's `sound` command: the audio profile's `CONTROL` record, of
    /// the table's page, never for the whole service or after the current
    /// sound (RULE-119).
    fn control(&self, path: &Path<'_>, sound: &GroupSound, binder: &mut Binder) -> Result<Vec<u8>, Fault> {
        let mut control =
            Control { channel: 0, sequence: 0, operation: 0, handle: 0, generation: 0, gain: 0, fade_ms: 0, flags: 0, duck: 0, page: Some(self.page) };
        match sound {
            GroupSound::Play(play) => {
                if play.page.is_some() {
                    return Err(fault(&path.field("page"), Rule::InvalidGroup, "a group's `play` takes its page from the table".to_string()));
                }
                if play.after {
                    return Err(fault(&path.field("after"), Rule::InvalidGroup, "a group's `play` never waits for the current sound".to_string()));
                }
                control.operation = crate::sound::PLAY;
                control.channel = play.channel.get();
                control.handle = play.handle.get();
                control.generation = play.generation.get();
                control.gain = narrow(binder.amount(&path.field("gain"), play.gain, 0, field::CONTROL_GAIN)?);
                control.fade_ms = narrow(binder.amount(&path.field("fade_ms"), play.fade_ms, 0, field::CONTROL_FADE)?);
                control.duck = play.duck.get();
                if play.r#loop {
                    control.flags = FLAG_LOOP;
                }
            }
            GroupSound::Stop(stop) => {
                control.operation = crate::sound::STOP;
                control.channel = stop.channel.get();
                control.fade_ms = narrow(binder.amount(&path.field("fade_ms"), stop.fade_ms, 0, field::CONTROL_FADE)?);
            }
            GroupSound::Gain(gain) => {
                control.operation = crate::sound::GAIN;
                control.channel = gain.channel.get();
                control.gain = narrow(binder.amount(&path.field("gain"), gain.gain, 0, field::CONTROL_GAIN)?);
                control.fade_ms = narrow(binder.amount(&path.field("fade_ms"), gain.fade_ms, 0, field::CONTROL_FADE)?);
            }
        }
        let record = control.record();
        let (_, body) = split(&record).map_err(|refused| fault(path, Rule::InvalidGroup, format!("{refused:?}")))?;
        Ok(body.to_vec())
    }

    /// A `local_midi` command's one message (RULE-21).
    fn local_midi(path: &Path<'_>, channel: u8, event: &LocalMidiEvent, binder: &mut Binder) -> Result<Command, Fault> {
        let at = path.field("message");
        let data = |binder: &mut Binder, name: &'static str, amount: DataAmount, field: u8| -> Result<u8, Fault> {
            Ok(narrow(binder.amount(&at.field(name), amount, 0, field)?))
        };
        let (status, part, data1, data2) = match event {
            LocalMidiEvent::NoteOn(note) => (
                midi_status::NOTE_ON,
                note.channel,
                data(binder, "key", note.key, field::MIDI_DATA1)?,
                data(binder, "velocity", note.velocity, field::MIDI_DATA2)?,
            ),
            LocalMidiEvent::NoteOff(note) => (
                midi_status::NOTE_OFF,
                note.channel,
                data(binder, "key", note.key, field::MIDI_DATA1)?,
                data(binder, "velocity", note.velocity, field::MIDI_DATA2)?,
            ),
            LocalMidiEvent::ControlChange(change) => (
                midi_status::CONTROL_CHANGE,
                change.channel,
                data(binder, "controller", change.controller, field::MIDI_DATA1)?,
                data(binder, "value", change.value, field::MIDI_DATA2)?,
            ),
            LocalMidiEvent::ProgramChange(change) => {
                (midi_status::PROGRAM_CHANGE, change.channel, data(binder, "program", change.program, field::MIDI_DATA1)?, 0)
            }
            LocalMidiEvent::PitchBend(bend) => {
                // `En lsb msb`: seven bits each (RULE-21).
                let value: u16 = narrow(binder.amount(&at.field("value"), bend.value, 0, field::MIDI_BEND)?);
                let [low, high] = [value & SEVEN_BITS, value >> limits::MIDI_DATA_BITS].map(|bits| u8::try_from(bits).unwrap_or(0));
                (midi_status::PITCH_BEND, bend.channel, low, high)
            }
        };
        Ok(Command::LocalMidi { channel, status: status | part.get().saturating_sub(1), data1, data2 })
    }

    /// A `variable` command (RULE-99): its operation, variable and operand,
    /// paired as the record allows.
    fn variable(&self, path: &Path<'_>, command: &VariableCommand, place: Place) -> Result<Command, Fault> {
        let shape = if place == Place::Group { Rule::InvalidGroup } else { Rule::InvalidCommand };
        let invalid = |at: &'static str, detail: &str| fault(&path.field(at), shape, detail.to_string());
        let operation = match command.op {
            VariableOp::Set => VariableOperation::Set,
            VariableOp::Inc => VariableOperation::Inc,
            VariableOp::Dec => VariableOperation::Dec,
            VariableOp::Add => VariableOperation::Add,
            VariableOp::Sub => VariableOperation::Sub,
            VariableOp::Toggle => VariableOperation::Toggle,
            VariableOp::Min => VariableOperation::Min,
            VariableOp::Max => VariableOperation::Max,
            VariableOp::Wrap => VariableOperation::Wrap,
            VariableOp::Clear => VariableOperation::Clear,
        };
        let variable = match command.variable {
            VariableOrAll::One(variable) => variable.0,
            VariableOrAll::All if command.op == VariableOp::Clear => limits::ALL_VARIABLES,
            VariableOrAll::All => return Err(invalid("variable", "\"all\" is for `clear` alone")),
        };
        let takes_value = matches!(command.op, VariableOp::Set | VariableOp::Add | VariableOp::Sub | VariableOp::Min | VariableOp::Max);
        let range = command.op == VariableOp::Wrap;
        if command.value.is_some() != takes_value {
            return Err(invalid("value", if takes_value { "this operation takes a `value`" } else { "this operation takes no `value`" }));
        }
        let bounds = (command.low, command.high);
        if range != matches!(bounds, (Some(_), Some(_))) || (!range && bounds != (None, None)) {
            return Err(invalid("low", if range { "`wrap` takes `low` and `high`" } else { "`low` and `high` are for `wrap` alone" }));
        }
        let operand = match (&command.value, bounds) {
            (Some(VariableOperand::Read(read)), _) if command.op == VariableOp::Set => Operand::Value(self.value(&path.field("value").field("read"), read)?),
            (Some(VariableOperand::Read(_)), _) => return Err(invalid("value", "`{\"read\":…}` is for `set` alone")),
            (Some(VariableOperand::Amount(amount)), _) => match amount.source() {
                None => Operand::Constant(amount.constant()),
                Some(AmountSource::Variable(variable)) => Operand::Variable { constant: amount.constant(), variable: variable.0 },
                Some(_) if place == Place::Direct => {
                    return Err(fault(
                        &path.field("value"),
                        Rule::InvalidCommand,
                        "a value of the firing is a group's: a message sent directly has none".to_string(),
                    ));
                }
                Some(AmountSource::Row) => Operand::Fired { constant: amount.constant(), property: fired_number(FiredName::P) },
                Some(AmountSource::Fired(name)) => Operand::Fired { constant: amount.constant(), property: fired_number(name) },
            },
            (None, (Some(low), Some(high))) => {
                if low.get() > high.get() {
                    return Err(invalid("low", "`low` is at most `high`"));
                }
                Operand::Range { low: low.get(), high: high.get() }
            }
            _ => Operand::None,
        };
        Ok(Command::Variable { operation, variable, operand })
    }

    /// A `sequence` command (RULE-103): start, with when it begins, or stop.
    fn sequence_command(&self, path: &Path<'_>, command: &SequenceCommand) -> Result<Command, Fault> {
        match command.op {
            StartStop::Start => {
                let name =
                    command.sequence.as_ref().ok_or_else(|| fault(&path.field("sequence"), Rule::InvalidRules, "`start` names its sequence".to_string()))?;
                let mode = match command.at {
                    None => StartMode::Next,
                    Some(SequenceAt::Tick { tick }) => StartMode::At(u32::try_from(tick.get()).unwrap_or(u32::MAX)),
                    Some(SequenceAt::Beat { beat }) => {
                        StartMode::Beat { timer: beat.timer.number(), ticks: u32::try_from(beat.ticks.get()).unwrap_or(u32::MAX) }
                    }
                };
                Ok(Command::Sequence { start: true, sequence: self.sequence(&path.field("sequence"), name)?, mode })
            }
            StartStop::Stop => {
                if command.at.is_some() {
                    return Err(fault(&path.field("at"), Rule::InvalidRules, "a sequence is stopped at once: `stop` takes no `at`".to_string()));
                }
                let sequence = match &command.sequence {
                    Some(name) => self.sequence(&path.field("sequence"), name)?,
                    None => limits::ALL_SEQUENCES,
                };
                Ok(Command::Sequence { start: false, sequence, mode: StartMode::Next })
            }
        }
    }

    /// One command (RULE-114, RULE-118): its record type, body and
    /// bindings, the same bytes in a group as sent directly.
    fn command(&self, path: &Path<'_>, command: &RuleCommand, place: Place) -> Result<Built, Fault> {
        let mut binder = Binder::new(place);
        let (kind, body) = match command {
            RuleCommand::Sound(sound) => (command::TYPE_CONTROL, self.control(path, sound, &mut binder)?),
            RuleCommand::LocalMidi(midi) => {
                (command::TYPE_LOCAL_MIDI, self.encoded(path, &Self::local_midi(path, midi.channel.get(), &midi.message, &mut binder)?)?)
            }
            RuleCommand::Animate(animate) => (command::TYPE_ANIMATE, self.animate(&path.field("ops"), &animate.ops, &mut binder)?.1),
            // The body a SPRITES or NAPLPS_OBJECTS record sent directly
            // carries, written by the engine's writer (RULE-114, RULE-119);
            // the table's check reads it with the record's own reader.
            RuleCommand::Sprites(sprites) => (
                command::TYPE_SPRITES,
                animation::sprites_body(self.page, self.sub, &sprites.sprites).map_err(|detail| fault(&path.field("sprites"), Rule::InvalidGroup, detail))?,
            ),
            RuleCommand::NaplpsObjects(objects) => (
                command::TYPE_NAPLPS_OBJECTS,
                animation::naplps_objects_body(self.page, self.sub, self.naplps_generation, &objects.objects)
                    .map_err(|detail| fault(&path.field("objects"), Rule::InvalidGroup, detail))?,
            ),
            RuleCommand::Variable(variable) => (command::TYPE_VARIABLE, self.encoded(path, &self.variable(path, variable, place)?)?),
            RuleCommand::Timer(timer) => {
                (command::TYPE_TIMER, self.encoded(path, &Command::Timer { start: timer.op == StartStop::Start, timer: timer.timer.0 })?)
            }
            RuleCommand::Report(report) => {
                if place == Place::Direct && report.value.is_none() {
                    return Err(fault(
                        &path.field("value"),
                        Rule::InvalidCommand,
                        "a report of the firing is a group's: one sent directly reports a value".to_string(),
                    ));
                }
                let value = report.value.as_ref().map(|value| self.value(&path.field("value"), value)).transpose()?;
                (command::TYPE_REPORT, self.encoded(path, &Command::Report { value })?)
            }
            RuleCommand::Run(run) => (command::TYPE_RUN, self.encoded(path, &Command::Run { group: self.group(&path.field("group"), &run.group)? })?),
            RuleCommand::Sequence(sequence) => (command::TYPE_SEQUENCE, self.encoded(path, &self.sequence_command(path, sequence)?)?),
        };
        command::check_bindings(kind, &body, &binder.bindings).map_err(|refused| engine_fault(path, refused))?;
        Ok(Built { kind, body, bindings: binder.bindings })
    }

    /// A list of a group's commands.
    fn commands(&self, path: &Path<'_>, commands: &[RuleCommand]) -> Result<Vec<Built>, Fault> {
        if commands.is_empty() {
            return Err(fault(path, Rule::TooShort, "a group holds at least one command".to_string()));
        }
        if commands.len() > limits::MAX_GROUP_COMMANDS {
            return Err(engine_fault(path, EngineFault::Limit("MAX_GROUP_COMMANDS")));
        }
        commands.iter().enumerate().map(|(index, command)| self.command(&path.item(index), command, Place::Group)).collect()
    }

    /// The `report` of the firing that `echo` adds (RULE-35).
    fn echo(&self, path: &Path<'_>) -> Result<Built, Fault> {
        Ok(Built { kind: command::TYPE_REPORT, body: self.encoded(path, &Command::Report { value: None })?, bindings: Vec::new() })
    }

    /// The commands of the unnamed group a rule's `do` or `echo` makes, or
    /// with neither the named group it runs (RULE-35).
    fn wanted(&self, path: &Path<'_>, rule: &PageRule) -> Result<Result<Vec<Built>, u8>, Fault> {
        match (&rule.run, &rule.r#do) {
            (Some(run), None) => {
                let group = place_in(&path.field("run"), &self.names.groups, run, "group")?;
                if !rule.echo {
                    return Ok(Err(group));
                }
                let run = Built { kind: command::TYPE_RUN, body: self.encoded(path, &Command::Run { group })?, bindings: Vec::new() };
                Ok(Ok(alloc::vec![run, self.echo(path)?]))
            }
            (None, Some(commands)) => {
                let mut wanted = self.commands(&path.field("do"), commands)?;
                if rule.echo {
                    wanted.push(self.echo(path)?);
                }
                Ok(Ok(wanted))
            }
            _ => Err(fault(path, Rule::InvalidRules, "a rule has exactly one of `run` and `do`".to_string())),
        }
    }

    /// A rule (RULE-3, RULE-43), running `group`.
    fn rule(&self, path: &Path<'_>, rule: &PageRule, group: u8) -> Result<EngineRule, Fault> {
        let run = match &rule.rows {
            None => None,
            Some(name) => {
                let (_, first, count) = self
                    .rows
                    .iter()
                    .find(|(held, _, _)| *held == name.0)
                    .ok_or_else(|| fault(&path.field("rows"), Rule::UnknownName, format!("the table has no row set named `{}`", name.0)))?;
                Some(RowRun { first: *first, count: *count })
            }
        };
        let rows = run.map(|run| usize::from(run.count));
        let triggers = &rule.on.0;
        let becomes = triggers.iter().any(|trigger| matches!(trigger, RuleTrigger::Becomes(_)));
        if becomes && (triggers.len() != 1 || rule.when.is_none()) {
            return Err(fault(&path.field("on"), Rule::InvalidRules, "a `becomes` rule has that one trigger and a `when`".to_string()));
        }
        let on = path.field("on");
        let mut compiled = triggers
            .iter()
            .enumerate()
            .map(|(index, trigger)| if triggers.len() == 1 { self.trigger(&on, trigger, rows) } else { self.trigger(&on.item(index), trigger, rows) });
        let first = compiled.next().ok_or_else(|| fault(&path.field("on"), Rule::TooShort, "a rule has a trigger".to_string()))??;
        let mut engine = EngineRule::new(first, group);
        for trigger in compiled {
            engine.triggers.push(trigger?).map_err(|_| engine_fault(&path.field("on"), EngineFault::Limit("MAX_TRIGGERS_PER_RULE")))?;
        }
        engine.rows = run;
        engine.condition = self.condition(&path.field("when"), rule.when.as_ref(), rows.is_some())?;
        Ok(engine)
    }
}

/// The names declared at `path`, each distinct within its kind and, for a
/// rectangle, never `page` (RULE-36).
fn declared<'a>(path: &Path<'_>, names: impl Iterator<Item = &'a RuleName>, what: &str, page_reserved: bool) -> Result<Vec<String>, Fault> {
    let mut held: Vec<String> = Vec::new();
    for (index, name) in names.enumerate() {
        let item = path.item(index);
        let at = item.field("name");
        if page_reserved && name.0 == PAGE_NAME {
            return Err(fault(&at, Rule::Reserved, format!("`{PAGE_NAME}` is not a {what}'s name: it names the page")));
        }
        if held.contains(&name.0) {
            return Err(fault(&at, Rule::DuplicateName, format!("a {what} named `{}` is declared twice", name.0)));
        }
        held.push(name.0.clone());
    }
    Ok(held)
}

/// The row sets, one after another in the order of their names
/// (RULE-105): each name, first row and count.
fn rows_of(path: &Path<'_>, given: &RuleTable, table: &mut Table) -> Result<Vec<(String, u8, u8)>, Fault> {
    let mut rows = Vec::new();
    for (name, set) in &given.rows {
        // A row set's name is the program's: its faults are the set's at
        // `rows`, named in their detail.
        let at = path;
        if set.is_empty() {
            return Err(fault(at, Rule::TooShort, format!("the row set `{}` holds no row: a set holds at least one", name.0)));
        }
        let first = byte(at, table.rows.len())?;
        let count = byte(at, set.len())?;
        for row in set {
            let value = match row {
                RuleRow::Integer(integer) => integer.get(),
                RuleRow::Key(key) => i32::try_from(Scope::key(at, key)?.code).unwrap_or(i32::MAX),
            };
            table.rows.push(value).map_err(|_| engine_fault(at, EngineFault::Limit("MAX_ROWS")))?;
        }
        rows.push((name.0.clone(), first, count));
    }
    Ok(rows)
}

/// A table as the engine holds it, with its names, encoded and checked
/// with the engine's crate: what the server sends for `page` (RULE-59).
///
/// # Errors
/// The first fault of RULE-39, with its path under `path`.
pub(crate) fn compile(path: &Path<'_>, given: &RuleTable, (page, sub): (u16, u16), naplps_generation: u16) -> Result<Compiled, Fault> {
    let mut table = Box::new(Table::new(page, sub));
    let at_field = |field: &'static str| path.field(field);
    let rect_names = declared(&at_field("rects"), given.rects.iter().map(|rect| &rect.name), "rectangle", true)?;
    for rect in &given.rects {
        table
            .rects
            .push(Rect { x: rect.x.get(), y: rect.y.get(), width: rect.width.get(), height: rect.height.get() })
            .map_err(|_| engine_fault(&at_field("rects"), EngineFault::Limit("MAX_RECTS")))?;
    }
    let rows = rows_of(&at_field("rows"), given, &mut table)?;
    let names = Names {
        rules: declared(&at_field("rules"), given.rules.iter().map(|rule| &rule.name), "rule", false)?,
        groups: declared(&at_field("groups"), given.groups.iter().map(|group| &group.name), "group", false)?,
        sequences: declared(&at_field("sequences"), given.sequences.iter().map(|sequence| &sequence.name), "sequence", false)?,
        rects: rect_names,
    };
    if given.rules.len() > limits::MAX_RULES {
        return Err(engine_fault(&at_field("rules"), EngineFault::Limit("MAX_RULES")));
    }
    let scope = Scope { page, sub, names: &names, rows: &rows, resolve: true, naplps_generation };
    // The named groups first, numbered in the order written; then the
    // unnamed groups the rules' `do` and `echo` make, equal ones shared
    // (RULE-35, RULE-36).
    let mut groups: Vec<Vec<Built>> = Vec::new();
    for (index, group) in given.groups.iter().enumerate() {
        let groups_path = at_field("groups");
        let item = groups_path.item(index);
        groups.push(scope.commands(&item.field("commands"), &group.commands)?);
    }
    let mut rule_groups = Vec::with_capacity(given.rules.len());
    for (index, rule) in given.rules.iter().enumerate() {
        let rules_path = at_field("rules");
        let at = rules_path.item(index);
        let group = match scope.wanted(&at, rule)? {
            Err(named_group) => usize::from(named_group),
            Ok(wanted) => {
                if let Some(place) = groups.iter().skip(given.groups.len()).position(|held| *held == wanted) {
                    place.saturating_add(given.groups.len())
                } else {
                    groups.push(wanted);
                    groups.len().saturating_sub(1)
                }
            }
        };
        if group >= limits::MAX_GROUPS {
            return Err(engine_fault(&at, EngineFault::Limit("MAX_GROUPS")));
        }
        rule_groups.push(byte(&at, group)?);
    }
    for (index, group) in groups.iter().enumerate() {
        let framed: Vec<Framed<'_>> = group.iter().map(|built| Framed { kind: built.kind, body: &built.body, bindings: &built.bindings }).collect();
        let (groups_path, rules_path) = (at_field("groups"), at_field("rules"));
        let at = if index < given.groups.len() { groups_path.item(index) } else { rules_path };
        table.push_group(&framed).map_err(|refused| engine_fault(&at, refused))?;
    }
    for (index, (rule, group)) in given.rules.iter().zip(rule_groups).enumerate() {
        let rules_path = at_field("rules");
        let compiled = scope.rule(&rules_path.item(index), rule, group)?;
        table.rules.push(compiled).map_err(|_| engine_fault(&at_field("rules"), EngineFault::Limit("MAX_RULES")))?;
    }
    for (index, sequence) in given.sequences.iter().enumerate() {
        let sequences_path = at_field("sequences");
        let at = sequences_path.item(index);
        let length = u32::try_from(sequence.length.get()).unwrap_or(u32::MAX);
        let mut held = Sequence::new(length, sequence.repeat.get());
        let mut last = 0;
        for (step_index, step) in sequence.steps.iter().enumerate() {
            let steps_path = at.field("steps");
            let step_at = steps_path.item(step_index);
            let offset = u32::try_from(step.tick.get()).unwrap_or(u32::MAX);
            if offset >= length || offset < last {
                return Err(fault(
                    &step_at.field("tick"),
                    Rule::InvalidRules,
                    format!("tick {offset}: steps are in ascending tick, below the length {length}"),
                ));
            }
            last = offset;
            let group = place_in(&step_at.field("run"), &names.groups, &step.run, "group")?;
            held.steps.push(Step { offset, group }).map_err(|_| engine_fault(&at.field("steps"), EngineFault::Limit("MAX_SEQUENCE_STEPS")))?;
        }
        table.sequences.push(held).map_err(|_| engine_fault(&at_field("sequences"), EngineFault::Limit("MAX_SEQUENCES")))?;
    }
    // The whole table, as every receiver checks it (RULE-59, RULE-70).
    table.check(neoteletext_web::records::body_check).map_err(|refused| engine_fault(path, refused))?;
    let compiled = Compiled::of(&table, names).map_err(|refused| engine_fault(path, refused))?;
    // A table whose groups place NAPLPS objects is written again for each
    // connection's stream generation when it is sent.
    Ok(if holds_naplps_objects(given) { compiled.recompiled_from(alloc::sync::Arc::new(given.clone())) } else { compiled })
}

/// Whether a group of `given`, named or a rule's own, holds a
/// `naplps_objects` command.
fn holds_naplps_objects(given: &RuleTable) -> bool {
    let is_objects = |command: &RuleCommand| matches!(command, RuleCommand::NaplpsObjects(_));
    given.groups.iter().any(|group| group.commands.iter().any(is_objects))
        || given.rules.iter().any(|rule| rule.r#do.as_ref().is_some_and(|commands| commands.iter().any(is_objects)))
}

/// A command sent directly, as its record for one connection: its names
/// resolved in `names`, the names of the table in force there, or with
/// none left unresolved (RULE-114).
///
/// # Errors
/// The first fault, with its path; `unknown_name` for a group or a
/// sequence the table does not have.
pub(crate) fn direct_built(message: &CommandMessage, names: Option<&Names>) -> Result<Built, Fault> {
    let none = Names::default();
    let (page, sub) = message.page();
    let scope = Scope { page, sub, names: names.unwrap_or(&none), rows: &[], resolve: names.is_some(), naplps_generation: 0 };
    scope.command(&Path::Root, &message.command(), Place::Direct)
}

/// A command sent directly (RULE-114), checked but for the names it gives,
/// which are the connection's table's when it is sent.
///
/// # Errors
/// The first fault of RULE-39, with its path.
pub(super) fn check_direct(_root: &Path<'_>, message: &CommandMessage) -> Checked {
    direct_built(message, None).map(|_| ())
}

/// The operations of an `animate` message or of a timeline's step, as the
/// engine holds them (ANIM-6, ANIM-21): constants alone.
///
/// # Errors
/// `invalid_animation`, for an operation on "any", on the page or on a
/// rectangle, or one the engine's check refuses; `invalid_command` for an
/// amount.
pub(super) fn operations(path: &Path<'_>, ops: &[AnimationOp]) -> Result<Vec<Operation>, Fault> {
    let none = Names::default();
    // The page does not matter to the operations themselves: the record
    // written here is only their check.
    let scope = Scope { page: limits::FIRST_PAGE, sub: 0, names: &none, rows: &[], resolve: false, naplps_generation: 0 };
    let mut binder = Binder::new(Place::Direct);
    scope.animate(path, ops, &mut binder).map(|(operations, _)| operations)
}
