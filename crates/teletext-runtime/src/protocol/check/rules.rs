//! A `rules` message's table, and the commands sent directly, turned from
//! the names a program writes into the engine's table and records
//! (page-rules.md RULE-35, RULE-36, RULE-39, RULE-114; REQ-6532).
//!
//! This first commit of program protocol 15 carries its schema alone: the
//! compiler of the second form's tables follows it, built on the engine's
//! revision for DEC-6635, and until then a table or a command is refused
//! whole, never half sent.

use alloc::format;
use alloc::string::ToString;
use alloc::vec::Vec;

use neoteletext_rules::animate::{Operation, OperationKind};
use neoteletext_rules::limits;
use neoteletext_rules::table::Target;

use super::{Checked, Fault, Path, fault};
use crate::protocol::{AnimationOp, CommandMessage, LookChoice, Rule, RuleTable, RuleTarget, TargetNumber};
use crate::rules::{Compiled, look_number};

/// The refusal of what the next commit builds.
fn not_yet(path: &Path<'_>) -> Fault {
    fault(path, Rule::InvalidRules, "the second form's compiler is not in this build of the server".to_string())
}

/// A table, compiled for `page` and `sub` (RULE-59).
///
/// # Errors
/// Every table, in this commit.
pub(super) fn compile(path: &Path<'_>, _table: &RuleTable, _page: (u16, u16)) -> Result<Compiled, Fault> {
    Err(not_yet(path))
}

/// A command sent directly (RULE-114).
///
/// # Errors
/// Every command, in this commit.
pub(super) fn check_direct(root: &Path<'_>, _message: &CommandMessage) -> Checked {
    Err(not_yet(root))
}

/// One constant of an operation sent directly: an amount is a group's
/// alone (RULE-106, RULE-39 `invalid_command`).
fn constant<const LEAST: i32, const MOST: i32>(path: &Path<'_>, amount: crate::protocol::Amount<LEAST, MOST>) -> Result<i32, Fault> {
    match amount.source() {
        None => Ok(amount.constant()),
        Some(_) => Err(fault(path, Rule::InvalidCommand, "an amount is a group's: a message sent directly gives a constant".to_string())),
    }
}

/// The target of an operation sent directly: one control or one object by
/// its number (ANIM-6).
fn thing(path: &Path<'_>, target: &RuleTarget, object_only: bool) -> Result<Target, Fault> {
    let at = path.field("target");
    match target {
        RuleTarget::Control { control: TargetNumber::Numbered(number) } if !object_only => Ok(Target::control(number.get())),
        RuleTarget::Object { object: TargetNumber::Numbered(number) } => Ok(Target::object(number.get())),
        RuleTarget::Control { control: TargetNumber::Bound { .. } } | RuleTarget::Object { object: TargetNumber::Bound { .. } } => {
            Err(fault(&at, Rule::InvalidCommand, "a number from the row or the firing is a group's".to_string()))
        }
        _ if object_only => Err(fault(&at, Rule::InvalidAnimation, "the target is one object".to_string())),
        _ => Err(fault(&at, Rule::InvalidAnimation, "the target is one control or one object".to_string())),
    }
}

/// The operations of an `animate` message or of a timeline's step, as the
/// engine holds them (ANIM-6, ANIM-21): constants alone.
///
/// # Errors
/// `invalid_animation`, for an operation on "any", on the page or on a
/// rectangle, or one the engine's check refuses; `invalid_command` for an
/// amount.
pub(super) fn operations(path: &Path<'_>, ops: &[AnimationOp]) -> Result<Vec<Operation>, Fault> {
    if ops.is_empty() {
        return Err(fault(path, Rule::TooShort, "no operation".to_string()));
    }
    if ops.len() > limits::MAX_ANIMATE_OPERATIONS {
        return Err(fault(path, Rule::TooLong, format!("{} operations, at most {}", ops.len(), limits::MAX_ANIMATE_OPERATIONS)));
    }
    ops.iter()
        .enumerate()
        .map(|(index, op)| {
            let at = path.item(index);
            let narrow = |value: i32| i16::try_from(value).unwrap_or(0);
            let (target, kind) = match op {
                AnimationOp::Show(op) => (thing(&at, &op.target, false)?, OperationKind::Show),
                AnimationOp::Hide(op) => (thing(&at, &op.target, false)?, OperationKind::Hide),
                AnimationOp::Reset(op) => (thing(&at, &op.target, false)?, OperationKind::Reset),
                AnimationOp::StopCycle(op) => (thing(&at, &op.target, true)?, OperationKind::StopCycle),
                AnimationOp::Look(op) => {
                    let look = match op.look {
                        LookChoice::Named(name) => look_number(name),
                        LookChoice::Amount(amount) => u8::try_from(constant(&at.field("look"), amount)?).unwrap_or(0),
                    };
                    (thing(&at, &op.target, false)?, OperationKind::Look(look))
                }
                AnimationOp::Move(op) => {
                    let (x, y) = (constant(&at.field("x"), op.x)?, constant(&at.field("y"), op.y)?);
                    (thing(&at, &op.target, true)?, OperationKind::Move { relative: op.relative, x: narrow(x), y: narrow(y) })
                }
                AnimationOp::Order(op) => {
                    let layer = op.layer.map_or(limits::KEEP_LAYER, crate::protocol::Layer::get);
                    let order = i8::try_from(constant(&at.field("order"), op.order)?).unwrap_or(0);
                    (thing(&at, &op.target, true)?, OperationKind::Order { layer, order })
                }
                AnimationOp::Frame(op) => {
                    let frame = u8::try_from(constant(&at.field("frame"), op.frame)?).unwrap_or(0);
                    (thing(&at, &op.target, true)?, OperationKind::Frame(frame))
                }
                AnimationOp::Cycle(op) => {
                    if op.first.get() > op.last.get() {
                        return Err(fault(&at.field("first"), Rule::InvalidAnimation, format!("first {} is above last {}", op.first.get(), op.last.get())));
                    }
                    let period = u16::try_from(constant(&at.field("period"), op.period)?).unwrap_or(1);
                    (thing(&at, &op.target, true)?, OperationKind::Cycle { first: op.first.get(), last: op.last.get(), period, once: op.once })
                }
            };
            let operation = Operation { target, kind };
            operation.check().map_err(|refused| fault(&at, Rule::InvalidAnimation, format!("{refused:?}")))?;
            Ok(operation)
        })
        .collect()
}
