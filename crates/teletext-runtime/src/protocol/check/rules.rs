//! A `rules` message's table, turned from the names a program writes into
//! the engine's table (page-rules.md RULE-35, RULE-36, RULE-39; REQ-6532):
//! every name into its number, every key's name into its code, every
//! synthesizer's field into its parameter number, and the whole encoded
//! and checked with the engine's `neoteletext-rules` (RULE-59). The first
//! fault refuses the table whole, with its path.

use alloc::boxed::Box;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use neoteletext_input::keys;
use neoteletext_rules::animate::{Operation, OperationKind, Place};
use neoteletext_rules::limits;
use neoteletext_rules::table::{
    self, Action, Condition, Index, Key, KeyKind, KeySet as EngineKeySet, Node, Operator, Rect, Rule as EngineRule, SubpageMask, Table, Target, TargetKind,
    Trigger, Value, Variable, any_key, midi_status, no_condition,
};

use super::{Checked, Fault, Path, fault};
use crate::protocol::{
    ANY_SUBPAGE, AnimationOp, ButtonChoice, CompareOp, ControlRef, CycleOp, FrameOp, KeySet, LookOp, Mods, MoveOp, ObjectRef, OrderOp, PAGE_NAME, PageRule,
    Rule, RuleAction, RuleCondition, RuleKey, RuleMidi, RuleName, RuleTable, RuleTarget, RuleTrigger, RuleValue, SynthProperty, TargetOp,
};
use crate::rules::{self, Compiled, Names};

/// Where a target stands, which decides what it may be (RULE-12).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Use {
    /// A pointer trigger: anything, "any" too.
    Trigger,
    /// The first operand of `overlaps` or `inside`: one control, object or
    /// rectangle (RULE-19).
    First,
    /// The second operand: the page too.
    Second,
    /// A value's target, and `show`, `hide`, `look` and `reset`: one
    /// control or one object.
    Thing,
    /// `move`, `order`, `frame`, `cycle` and `stop_cycle`: one object.
    Object,
}

/// The names of a table, as it declares them, and the faults of names.
struct Scope<'a> {
    names: &'a Names,
    /// Which rule shape faults are refused with: a table's are
    /// `invalid_rules`, an `animate` message's `invalid_animation`.
    shape: Rule,
}

/// The place of `name` in `list`, or its refusal: the table declares no
/// such name.
fn place(path: &Path<'_>, list: &[String], name: &RuleName, kind: &str) -> Result<usize, Fault> {
    list.iter().position(|held| *held == name.0).ok_or_else(|| fault(path, Rule::UnknownName, format!("the table declares no {kind} named `{}`", name.0)))
}

/// A number as the byte the record holds it in.
fn byte(path: &Path<'_>, number: usize) -> Result<u8, Fault> {
    u8::try_from(number).map_err(|_| fault(path, Rule::OutOfRange, format!("{number} is past 255")))
}

/// A list no longer than `most`.
fn at_most(path: &Path<'_>, length: usize, most: usize, what: &str) -> Checked {
    if length > most { Err(fault(path, Rule::TooLong, format!("{length} {what}, at most {most}"))) } else { Ok(()) }
}

/// A position or an amount as the record's signed 16 bits; its range was
/// read.
fn offset(value: i32) -> i16 {
    i16::try_from(value).unwrap_or(0)
}

impl Scope<'_> {
    /// A fault of shape, of this scope's rule.
    fn invalid(&self, path: &Path<'_>, detail: impl Into<String>) -> Fault {
        fault(path, self.shape, detail.into())
    }

    /// A target, where `usage` allows it.
    fn target(&self, path: &Path<'_>, target: &RuleTarget, usage: Use) -> Result<Target, Fault> {
        let any = || self.invalid(path, "\"any\" is for the six pointer triggers: here one control or object is named");
        let found = match target {
            RuleTarget::Page(_) => Target::PAGE,
            RuleTarget::Control { control: ControlRef::Any(_) } if usage == Use::Trigger => Target::ANY_CONTROL,
            RuleTarget::Object { object: ObjectRef::Any(_) } if usage == Use::Trigger => Target::ANY_OBJECT,
            RuleTarget::Control { control: ControlRef::Any(_) } | RuleTarget::Object { object: ObjectRef::Any(_) } => return Err(any()),
            RuleTarget::Control { control: ControlRef::Numbered(number) } => Target::control(u16::from(number.get())),
            RuleTarget::Object { object: ObjectRef::Numbered(number) } => Target::object(number.get()),
            RuleTarget::Rect { rect } => Target::rect(u16::from(byte(path, place(&path.field("rect"), &self.names.rects, rect, "rectangle")?)?)),
        };
        let allowed = matches!(
            (usage, found.kind),
            (Use::Trigger | Use::Second, _)
                | (Use::First | Use::Thing, TargetKind::Control)
                | (Use::First | Use::Thing | Use::Object, TargetKind::Object)
                | (Use::First, TargetKind::Rect)
        );
        if allowed {
            return Ok(found);
        }
        Err(self.invalid(
            path,
            match usage {
                Use::First => "the first target is one control, object or rectangle",
                Use::Object => "the target is one object",
                _ => "the target is one control or one object",
            },
        ))
    }

    /// One key (RULE-10): its kind and its code.
    fn key(&self, path: &Path<'_>, key: &RuleKey) -> Result<Key, Fault> {
        let character = |path: &Path<'_>, text: &str| {
            let character = text.chars().next().ok_or_else(|| fault(path, Rule::TooShort, "one character".to_string()))?;
            if character.is_uppercase() {
                return Err(self.invalid(path, format!("a letter is given in lower case: `{}`", character.to_lowercase())));
            }
            Ok(u32::from(character))
        };
        Ok(match key {
            RuleKey::Char { char } => Key { kind: KeyKind::Character, code: character(&path.field("char"), char)? },
            RuleKey::Physical { physical } => Key { kind: KeyKind::Physical, code: character(&path.field("physical"), physical)? },
            RuleKey::Name { name } => {
                let code = keys::code(&name.0)
                    .filter(|_| name.0.chars().count() > 1)
                    .ok_or_else(|| fault(&path.field("name"), Rule::NotListed, format!("`{}` names no key of the engine's table (DOC-1936)", name.0)))?;
                Key { kind: KeyKind::Named, code }
            }
        })
    }

    /// A key set: no key for any.
    fn keys(&self, path: &Path<'_>, set: &KeySet) -> Result<EngineKeySet, Fault> {
        let mut keys = any_key();
        let KeySet::Keys(listed) = set else { return Ok(keys) };
        at_most(path, listed.len(), limits::MAX_KEYS_PER_TRIGGER, "keys")?;
        for (index, key) in listed.iter().enumerate() {
            // The list's length was held to the set's room.
            let _ = keys.push(self.key(&path.item(index), key)?);
        }
        Ok(keys)
    }

    /// A timer by its name, or the page timer for none.
    fn timer(&self, path: &Path<'_>, name: Option<&RuleName>) -> Result<u8, Fault> {
        name.map_or(Ok(limits::PAGE_TIMER), |name| byte(path, place(path, &self.names.timers, name, "timer")?))
    }

    /// A variable by its name.
    fn variable(&self, path: &Path<'_>, name: &RuleName) -> Result<u8, Fault> {
        byte(path, place(path, &self.names.variables, name, "variable")?)
    }

    /// The mask of the rules named: bit `n` for rule `n` (RULE-16).
    fn mask(&self, path: &Path<'_>, listed: &[RuleName]) -> Result<u64, Fault> {
        at_most(path, listed.len(), limits::MAX_RULES, "rules")?;
        listed.iter().enumerate().try_fold(0_u64, |mask, (index, name)| {
            let rule = place(&path.item(index), &self.names.rules, name, "rule")?;
            Ok(mask | u32::try_from(rule).ok().and_then(|bit| 1_u64.checked_shl(bit)).unwrap_or(0))
        })
    }

    /// A value (RULE-23 to RULE-28).
    fn value(&self, path: &Path<'_>, value: &RuleValue) -> Result<Value, Fault> {
        Ok(match value {
            RuleValue::Variable(variable) => Value::variable(self.variable(&path.field("name"), &variable.name)?),
            RuleValue::Key(key) => {
                let key = self.key(&path.field("key"), &key.key)?;
                Value::key(key.kind.code(), key.code)
            }
            RuleValue::Button(button) => Value::button(button.button.get()),
            RuleValue::Pointer(pointer) => Value::pointer(rules::pointer_property(pointer.property)),
            RuleValue::KeysDown(_) => Value::keys_down(),
            RuleValue::Timer(timer) => Value::timer(self.timer(&path.field("name"), timer.name.as_ref())?, rules::timer_property(timer.property)),
            RuleValue::Target(target) => {
                let named = self.target(&path.field("target"), &target.target, Use::Thing)?;
                Value::target(named.kind, named.number, rules::target_property(target.property))
            }
            RuleValue::Sound(sound) => Value::sound(sound.channel.get(), rules::sound_property(sound.property)),
            RuleValue::Synth(synth) => {
                let (code, variant) = rules::synthesizer_code(synth.family);
                let needs = |given: bool, wanted: bool, name: &'static str, with: &str| match (given, wanted) {
                    (false, true) => Err(fault(&path.field(name), Rule::MissingField, format!("missing field `{name}`, which `{with}` reads"))),
                    (true, false) => Err(self.invalid(&path.field(name), format!("`{name}` is given only with {with}"))),
                    _ => Ok(()),
                };
                let property = synth.property;
                needs(synth.key.is_some(), matches!(property, SynthProperty::Note | SynthProperty::Velocity), "key", "note and velocity")?;
                needs(synth.field.is_some(), property == SynthProperty::Field, "field", "field")?;
                needs(synth.controller.is_some(), property == SynthProperty::Controller, "controller", "controller")?;
                let index = match (&synth.field, synth.key, synth.controller) {
                    (Some(name), _, _) => {
                        let fields = rules::fields_of(synth.family);
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
                Value::synth(synth.channel.get(), code, variant, synth.part.get().saturating_sub(1), rules::synth_class(property), index)
            }
        })
    }

    /// A condition's nodes in prefix order, added to `nodes` (RULE-44).
    fn nodes(&self, path: &Path<'_>, condition: &RuleCondition, nodes: &mut Condition) -> Checked {
        let mut push = |node: Node| nodes.push(node).map(|_| ()).map_err(|_| fault(path, Rule::RulesOverLimit, rules::limit_in_words("MAX_CONDITION_NODES")));
        let children = |count: usize| u8::try_from(count).unwrap_or(u8::MAX);
        match condition {
            RuleCondition::And { and } => {
                push(Node::And(children(and.len())))?;
                and.iter().enumerate().try_for_each(|(index, child)| self.nodes(&path.field("and").item(index), child, nodes))
            }
            RuleCondition::Or { or } => {
                push(Node::Or(children(or.len())))?;
                or.iter().enumerate().try_for_each(|(index, child)| self.nodes(&path.field("or").item(index), child, nodes))
            }
            RuleCondition::Not { not } => {
                push(Node::Not)?;
                self.nodes(&path.field("not"), not, nodes)
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
                push(Node::Compare { value: self.value(&path.field("read"), read)?, operator, constant: value.get() })
            }
            RuleCondition::InRange { read, in_range: [low, high] } => {
                if low.get() > high.get() {
                    return Err(self.invalid(&path.field("in_range"), format!("low {} is above high {}", low.get(), high.get())));
                }
                push(Node::InRange { value: self.value(&path.field("read"), read)?, low: low.get(), high: high.get() })
            }
            RuleCondition::InSet { read, in_set } => {
                let mut members = table::SetMembers::new(0);
                for member in in_set {
                    members
                        .push(member.get())
                        .map_err(|_| fault(&path.field("in_set"), Rule::TooLong, format!("at most {} constants", limits::MAX_SET_MEMBERS)))?;
                }
                push(Node::InSet { value: self.value(&path.field("read"), read)?, members })
            }
            RuleCondition::Overlaps { overlaps: [first, second] } => {
                let at = path.field("overlaps");
                push(Node::Overlaps(self.target(&at.item(0), first, Use::First)?, self.target(&at.item(1), second, Use::Second)?))
            }
            RuleCondition::Inside { inside: [first, second] } => {
                let at = path.field("inside");
                push(Node::Inside(self.target(&at.item(0), first, Use::First)?, self.target(&at.item(1), second, Use::Second)?))
            }
        }
    }

    /// A condition, or none: one tree within RULE-18's limits.
    fn condition(&self, path: &Path<'_>, condition: Option<&RuleCondition>) -> Result<Condition, Fault> {
        let mut nodes = no_condition();
        let Some(condition) = condition else { return Ok(nodes) };
        self.nodes(path, condition, &mut nodes)?;
        table::check_condition(&nodes).map_err(|refused| engine_fault(path, refused))?;
        Ok(nodes)
    }

    /// The operation on `target` of `kind`, checked as the engine checks it
    /// in `place`.
    fn operation(&self, path: &Path<'_>, target: &RuleTarget, usage: Use, kind: OperationKind, place: Place) -> Result<Operation, Fault> {
        let operation = Operation { target: self.target(&path.field("target"), target, usage)?, kind };
        operation.check(place).map_err(|refused| self.invalid(path, format!("{refused:?}")))?;
        Ok(operation)
    }

    fn look(&self, path: &Path<'_>, op: &LookOp, place: Place) -> Result<Operation, Fault> {
        self.operation(path, &op.target, Use::Thing, OperationKind::Look(rules::look_number(op.look)), place)
    }

    fn moved(&self, path: &Path<'_>, op: &MoveOp, place: Place) -> Result<Operation, Fault> {
        self.operation(path, &op.target, Use::Object, OperationKind::Move { relative: op.relative, x: offset(op.x.get()), y: offset(op.y.get()) }, place)
    }

    fn order(&self, path: &Path<'_>, op: &OrderOp, place: Place) -> Result<Operation, Fault> {
        let layer = op.layer.map_or(limits::KEEP_LAYER, crate::protocol::Layer::get);
        self.operation(path, &op.target, Use::Object, OperationKind::Order { layer, order: i8::try_from(op.order.get()).unwrap_or(0) }, place)
    }

    fn frame(&self, path: &Path<'_>, op: &FrameOp, place: Place) -> Result<Operation, Fault> {
        self.operation(path, &op.target, Use::Object, OperationKind::Frame(op.frame.get()), place)
    }

    fn cycle(&self, path: &Path<'_>, op: &CycleOp, place: Place) -> Result<Operation, Fault> {
        if op.first.get() > op.last.get() {
            return Err(self.invalid(&path.field("first"), format!("first {} is above last {}", op.first.get(), op.last.get())));
        }
        let kind = OperationKind::Cycle { first: op.first.get(), last: op.last.get(), period: op.period.get(), once: op.once };
        self.operation(path, &op.target, Use::Object, kind, place)
    }

    fn bare(&self, path: &Path<'_>, op: &TargetOp, usage: Use, kind: OperationKind, place: Place) -> Result<Operation, Fault> {
        self.operation(path, &op.target, usage, kind, place)
    }

    /// An operation of an `animate` message or of a timeline's step
    /// (ANIM-6).
    fn animation(&self, path: &Path<'_>, op: &AnimationOp) -> Result<Operation, Fault> {
        let place = Place::Record;
        match op {
            AnimationOp::Show(op) => self.bare(path, op, Use::Thing, OperationKind::Show, place),
            AnimationOp::Hide(op) => self.bare(path, op, Use::Thing, OperationKind::Hide, place),
            AnimationOp::Reset(op) => self.bare(path, op, Use::Thing, OperationKind::Reset, place),
            AnimationOp::StopCycle(op) => self.bare(path, op, Use::Object, OperationKind::StopCycle, place),
            AnimationOp::Look(op) => self.look(path, op, place),
            AnimationOp::Move(op) => self.moved(path, op, place),
            AnimationOp::Order(op) => self.order(path, op, place),
            AnimationOp::Frame(op) => self.frame(path, op, place),
            AnimationOp::Cycle(op) => self.cycle(path, op, place),
        }
    }

    /// An action (RULE-20, RULE-45).
    fn action(&self, path: &Path<'_>, action: &RuleAction) -> Result<Action, Fault> {
        let place = Place::Rule;
        Ok(match action {
            RuleAction::Midi(midi) => {
                let (status, channel, data1, data2) = match midi.message {
                    RuleMidi::NoteOn(note) => (midi_status::NOTE_ON, note.channel, note.key.get(), note.velocity.get()),
                    RuleMidi::NoteOff(note) => (midi_status::NOTE_OFF, note.channel, note.key.get(), note.velocity.get()),
                    RuleMidi::ControlChange(change) => (midi_status::CONTROL_CHANGE, change.channel, change.controller.get(), change.value.get()),
                    RuleMidi::ProgramChange(change) => (midi_status::PROGRAM_CHANGE, change.channel, change.program.get(), 0),
                    // `En lsb msb` (MIDI 1.0 4.2, Table II): seven bits each.
                    RuleMidi::PitchBend(bend) => {
                        let [low, high] = [bend.value.get() & SEVEN_BITS, bend.value.get() >> SEVEN_BIT_SHIFT].map(|bits| u8::try_from(bits).unwrap_or(0));
                        (midi_status::PITCH_BEND, bend.channel, low, high)
                    }
                };
                Action::Midi { channel: midi.channel.get(), status: status | channel.get().saturating_sub(1), data1, data2 }
            }
            RuleAction::Play(play) => Action::Play {
                channel: play.channel.get(),
                handle: play.handle.get(),
                generation: play.generation.get(),
                gain: play.gain.get(),
                fade_ms: play.fade_ms.get(),
                looped: play.r#loop,
            },
            RuleAction::Stop(stop) => Action::Stop { channel: stop.channel.get(), fade_ms: stop.fade_ms.get() },
            RuleAction::Gain(gain) => Action::Gain { channel: gain.channel.get(), gain: gain.gain.get(), fade_ms: gain.fade_ms.get() },
            RuleAction::Show(op) => Action::Animate(self.bare(path, op, Use::Thing, OperationKind::Show, place)?),
            RuleAction::Hide(op) => Action::Animate(self.bare(path, op, Use::Thing, OperationKind::Hide, place)?),
            RuleAction::StopCycle(op) => Action::Animate(self.bare(path, op, Use::Object, OperationKind::StopCycle, place)?),
            RuleAction::Look(op) => Action::Animate(self.look(path, op, place)?),
            RuleAction::Move(op) => Action::Animate(self.moved(path, op, place)?),
            RuleAction::Order(op) => Action::Animate(self.order(path, op, place)?),
            RuleAction::Frame(op) => Action::Animate(self.frame(path, op, place)?),
            RuleAction::Cycle(op) => Action::Animate(self.cycle(path, op, place)?),
            RuleAction::Set(set) => Action::Set { variable: self.variable(&path.field("variable"), &set.variable)?, value: set.value.get() },
            RuleAction::Add(add) => Action::Add { variable: self.variable(&path.field("variable"), &add.variable)?, amount: add.amount.get() },
            RuleAction::StartTimer(timer) => Action::StartTimer(self.timer(&path.field("timer"), Some(&timer.timer))?),
            RuleAction::StopTimer(timer) => Action::StopTimer(self.timer(&path.field("timer"), Some(&timer.timer))?),
            RuleAction::Enable(switch) => Action::Enable(self.mask(&path.field("rules"), &switch.rules)?),
            RuleAction::Disable(switch) => Action::Disable(self.mask(&path.field("rules"), &switch.rules)?),
            RuleAction::Send(send) => Action::Send(send.value.as_ref().map(|value| self.value(&path.field("value"), value)).transpose()?),
        })
    }

    /// A trigger (RULE-9, RULE-43).
    fn trigger(&self, path: &Path<'_>, trigger: &RuleTrigger) -> Result<Trigger, Fault> {
        let mods = |mods: Mods| (mods.mask.get(), mods.value.get());
        let button = |choice: ButtonChoice| match choice {
            ButtonChoice::Any(_) => limits::ANY_BUTTON,
            ButtonChoice::Numbered(button) => button.get(),
        };
        let target = |target: &RuleTarget| self.target(&path.field("target"), target, Use::Trigger);
        let ticks = |ticks: u64| u32::try_from(ticks).unwrap_or(u32::MAX);
        Ok(match trigger {
            RuleTrigger::Key(key) => {
                let (mods_mask, mods_value) = mods(key.mods);
                Trigger::Key { keys: self.keys(&path.field("keys"), &key.keys)?, repeat: key.repeat, mods_mask, mods_value }
            }
            RuleTrigger::KeyUp(key) => {
                let (mods_mask, mods_value) = mods(key.mods);
                Trigger::KeyUp { keys: self.keys(&path.field("keys"), &key.keys)?, mods_mask, mods_value }
            }
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
            RuleTrigger::Every(every) => Trigger::Every { timer: self.timer(&path.field("timer"), every.timer.as_ref())?, ticks: ticks(every.ticks.get()) },
            RuleTrigger::After(after) => Trigger::After { timer: self.timer(&path.field("timer"), after.timer.as_ref())?, ticks: ticks(after.ticks.get()) },
        })
    }

    /// A rule (RULE-3).
    fn rule(&self, path: &Path<'_>, rule: &PageRule) -> Result<EngineRule, Fault> {
        let trigger = self.trigger(&path.field("on"), &rule.on)?;
        let condition = match (&rule.on, &rule.when) {
            (RuleTrigger::Becomes(_), Some(_)) => return Err(self.invalid(&path.field("when"), "a `becomes` rule carries its own condition: `when` is null")),
            (RuleTrigger::Becomes(becomes), None) => self.condition(&path.field("on").field("condition"), Some(&becomes.condition))?,
            (_, when) => self.condition(&path.field("when"), when.as_ref())?,
        };
        let listed = path.field("do");
        at_most(&listed, rule.r#do.len(), limits::MAX_ACTIONS, "actions")?;
        if rule.r#do.is_empty() && !rule.echo {
            return Err(self.invalid(&listed, "a rule with no actions has `echo`: it would do nothing"));
        }
        let mut actions = table::no_actions();
        for (index, action) in rule.r#do.iter().enumerate() {
            // The list's length was held to the rule's room.
            let _ = actions.push(self.action(&listed.item(index), action)?);
        }
        Ok(EngineRule { enabled: rule.enabled, echo: rule.echo, trigger, condition, actions })
    }
}

/// The voices of the synth6581 family, which a `synth` value names as its
/// part (RULE-28).
const SYNTH6581_VOICES: u8 = 3;
/// The seven bits of a MIDI data byte (MIDI 1.0 4.2).
const SEVEN_BITS: u16 = 0x7F;
/// How far a pitch bend's high seven bits are from its low.
const SEVEN_BIT_SHIFT: u32 = 7;

/// A fault the engine's crate found, as a refusal: a limit of RULE-57 is
/// `rules_over_limit`, with the limit and its value; anything else is what
/// a receiver's decoder would refuse, `invalid_rules` (RULE-39).
fn engine_fault(path: &Path<'_>, refused: table::Fault) -> Fault {
    match refused {
        table::Fault::Limit(name) => fault(path, Rule::RulesOverLimit, rules::limit_in_words(name)),
        // The buffer it was encoded into is one record's body: what does
        // not fit it is over `MAX_TABLE_BYTES`.
        table::Fault::Full => fault(path, Rule::RulesOverLimit, rules::limit_in_words("MAX_TABLE_BYTES")),
        other => fault(path, Rule::InvalidRules, format!("the receivers' decoder refuses it: {other:?}")),
    }
}

/// The names declared at `path`, each distinct within its kind and, for a
/// timer and a rectangle, never `page` (RULE-36).
fn declared<'a>(path: &Path<'_>, names: impl Iterator<Item = &'a RuleName>, most: usize, what: &str, page_reserved: bool) -> Result<Vec<String>, Fault> {
    let mut held: Vec<String> = Vec::new();
    for (index, name) in names.enumerate() {
        let at = path.item(index);
        if index >= most {
            return Err(fault(path, Rule::TooLong, format!("more than {most} {what}s")));
        }
        if page_reserved && name.0 == PAGE_NAME {
            return Err(fault(&at.field("name"), Rule::Reserved, format!("`{PAGE_NAME}` is not a {what}'s name: it names the page")));
        }
        if held.contains(&name.0) {
            return Err(fault(&at.field("name"), Rule::DuplicateName, format!("a {what} named `{}` is declared twice", name.0)));
        }
        held.push(name.0.clone());
    }
    Ok(held)
}

/// A table as the engine holds it, with its names, encoded and checked
/// with the engine's crate: what the server sends (RULE-59).
///
/// # Errors
/// The first fault of RULE-39, with its path under `path`.
pub(super) fn compile(path: &Path<'_>, given: &RuleTable) -> Result<Compiled, Fault> {
    let names = Names {
        variables: declared(&path.field("variables"), given.variables.iter().map(|variable| &variable.name), limits::MAX_VARIABLES, "variable", false)?,
        timers: declared(&path.field("timers"), given.timers.iter().map(|timer| &timer.name), limits::MAX_TIMERS, "timer", true)?,
        rects: declared(&path.field("rects"), given.rects.iter().map(|rect| &rect.name), limits::MAX_RECTS, "rectangle", true)?,
        rules: declared(&path.field("rules"), given.rules.iter().map(|rule| &rule.name), limits::MAX_RULES, "rule", false)?,
    };
    let scope = Scope { names: &names, shape: Rule::InvalidRules };
    let mut table = Box::new(Table::new());
    // Each list's length was held to the table's room by `declared`.
    for (index, variable) in given.variables.iter().enumerate() {
        let (min, max, initial) = (variable.min.get(), variable.max.get(), variable.initial.get());
        if min > max || !(min..=max).contains(&initial) {
            return Err(scope.invalid(&path.field("variables").item(index), format!("min {min}, initial {initial} and max {max} are not in that order")));
        }
        let _ = table.variables.push(Variable { min, max, initial });
    }
    for timer in &given.timers {
        let _ = table.timers.push(timer.running);
    }
    for rect in &given.rects {
        let _ = table.rects.push(Rect { x: rect.x.get(), y: rect.y.get(), width: rect.width.get(), height: rect.height.get() });
    }
    for (index, rule) in given.rules.iter().enumerate() {
        let _ = table.rules.push(scope.rule(&path.field("rules").item(index), rule)?);
    }
    let subpages = path.field("subpages");
    at_most(&subpages, given.subpages.len(), limits::MAX_SUBPAGE_MASKS, "sub-pages")?;
    for (index, subpage) in given.subpages.iter().enumerate() {
        let at = subpages.item(index);
        if subpage.sub == ANY_SUBPAGE {
            return Err(fault(&at.field("sub"), Rule::OutOfRange, "3F7F names no sub-page".to_string()));
        }
        if given.subpages.iter().take(index).any(|earlier| earlier.sub == subpage.sub) {
            return Err(scope.invalid(&at.field("sub"), format!("sub-code {:04X} is given twice", subpage.sub.0)));
        }
        let _ = table.subpages.push(SubpageMask { sub: subpage.sub.0, off: scope.mask(&at.field("off"), &subpage.off)? });
    }
    // The limits over the whole table (RULE-57): its distinct values and
    // targets, and its encoded size.
    Index::of(&table).map_err(|refused| engine_fault(path, refused))?;
    Compiled::of(&table, names).map_err(|refused| engine_fault(path, refused))
}

/// The operations of an `animate` message or of a timeline's step, as the
/// engine holds them (ANIM-6, ANIM-21).
///
/// # Errors
/// `invalid_animation`, for an operation on "any", on the page or on a
/// rectangle, or one the engine's check refuses.
pub(super) fn operations(path: &Path<'_>, ops: &[AnimationOp]) -> Result<Vec<Operation>, Fault> {
    let names = Names::default();
    let scope = Scope { names: &names, shape: Rule::InvalidAnimation };
    if ops.is_empty() {
        return Err(fault(path, Rule::TooShort, "no operation".to_string()));
    }
    at_most(path, ops.len(), limits::MAX_ANIMATE_OPERATIONS, "operations")?;
    ops.iter()
        .enumerate()
        .map(|(index, op)| {
            let at = path.item(index);
            scope.animation(&at, op).map_err(|refused| match refused.rule {
                // A rectangle is a table's: an operation names none.
                Rule::UnknownName => fault(&at.field("target"), Rule::InvalidAnimation, "the target is one control or one object".to_string()),
                _ => refused,
            })
        })
        .collect()
}
