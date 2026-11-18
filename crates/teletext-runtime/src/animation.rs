//! Animation as the server sends it (neoteletext-common
//! `docs/animation.md` ANIM-2, ANIM-6, ANIM-7, ANIM-11, ANIM-17, ANIM-21;
//! REQ-6533): the protocol's messages turned into the engine's records,
//! each written by the engine's encoder, which checks what it writes with
//! its reader; a sprite sheet read by the renderer a receiver has; and the
//! animation state that the operations sent so far amount to, which a
//! viewer who comes later is sent.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use neoteletext_core::packet::Packet;
use neoteletext_core::types::SubCode as EngineSubCode;
use neoteletext_rules::animate::{AnimationState, AnimationStore, Applied, Operation, OperationKind};
use neoteletext_rules::limits::{KEEP_LAYER, MAX_ANIMATE_OPERATIONS, TICK_SPAN_MICROSECONDS, TICKS_PER_TICK_SPAN};
use neoteletext_rules::table::Target;
use neoteletext_web::records::frame::{FrameMark, FrameRecord};
use neoteletext_web::records::naplps_objects::{NaplpsObject as EngineObject, ObjectKind};
use neoteletext_web::records::sheet::{MAX_DATA as SHEET_DATA, SheetKind, SheetRecord};
use neoteletext_web::records::sprites::Sprite as EngineSprite;
use neoteletext_web::records::{animate, frame, naplps_objects, sheet, sprites};

use crate::page::address;
use crate::protocol::{FrameMarkName, NaplpsObject, NaplpsObjectKind, ObjectSource, ObjectType, SheetKindName, Sprite};

/// The audio and input service the records go on (the reference channel's,
/// DOC-1951).
const SERVICE: u8 = 0;

/// Why the engine's encoder, or a renderer, refused what the protocol's
/// checks passed.
pub(crate) type Refused = String;

/// The most objects and controls of a sub-page whose animation state the
/// server keeps for a viewer who comes later (ANIM-21): what a receiver
/// keeps (ANIM-6, `MAX_ANIMATED_OBJECTS`).
pub const MAX_ANIMATED_OBJECTS: usize = 64;
/// The most objects of a page that cycle at once (ANIM-14).
pub const MAX_CYCLING_OBJECTS: usize = 16;
// What the server keeps is what one renderer keeps at most.
const _: () = assert!(MAX_ANIMATED_OBJECTS == regis::limits::MAX_ANIMATED_DRAWINGS);
const _: () = assert!(MAX_CYCLING_OBJECTS == regis::limits::MAX_CYCLING_OBJECTS);
const _: () = assert!(MAX_ANIMATED_OBJECTS == naplps::limits::MAX_ANIMATED_OBJECTS && MAX_CYCLING_OBJECTS == naplps::limits::MAX_CYCLING_OBJECTS);

/// The colour map a receiver gives a NAPLPS page: 2 to the power of this
/// many entries, sixteen, as its ReGIS pictures have.
const NAPLPS_PALETTE_BITS: u32 = 4;

/// The bits of an Object Invocation triplet's fields in a sprite's object
/// reference (ANIM-7; EN 300 706 Table 28): the source in bits 12 and 11,
/// the type in bits 10 and 9, the number in bits 8 to 0.
const SOURCE_SHIFT: u16 = 11;
const TYPE_SHIFT: u16 = 9;

/// The packets `write` gives, back to back, with the record id moved on.
fn record(
    record_id: &mut u8,
    write: impl FnOnce(u8, &mut dyn FnMut(Packet)) -> Result<usize, neoteletext_web::records::RecordFault>,
) -> Result<Vec<u8>, Refused> {
    let mut bytes = Vec::new();
    write(*record_id, &mut |packet| bytes.extend_from_slice(&packet.bytes)).map_err(|fault| format!("{fault:?}"))?;
    *record_id = record_id.wrapping_add(1);
    Ok(bytes)
}

/// A value the reader bounded, as the record's narrower field.
fn narrow<T: TryFrom<i32> + Default>(value: i32) -> T {
    T::try_from(value).unwrap_or_default()
}

/// A FRAME record (ANIM-2): a frame of the sub-page begins or ends.
pub(crate) fn frame_record(page: u16, sub: u16, mark: FrameMarkName, identifier: u16, record_id: &mut u8) -> Result<Vec<u8>, Refused> {
    let mark = match mark {
        FrameMarkName::Begin => FrameMark::Begin,
        FrameMarkName::End => FrameMark::End,
    };
    let frame = FrameRecord { page: address(page), subcode: EngineSubCode::new(sub), mark, identifier };
    record(record_id, |id, out| frame::encode(frame, SERVICE, id, out))
}

/// ANIMATE records (ANIM-6) of `operations` for a sub-page, or with the
/// sub-code 3F7F for whichever sub-page of the page is shown: one record
/// for each 64 operations.
pub(crate) fn animate_records(page: u16, sub: u16, operations: &[Operation], record_id: &mut u8) -> Result<Vec<u8>, Refused> {
    let subcode = (sub != neoteletext_core::limits::SUBCODE_ANY).then(|| EngineSubCode::new(sub));
    let mut bytes = Vec::new();
    for chunk in operations.chunks(MAX_ANIMATE_OPERATIONS) {
        bytes.extend(record(record_id, |id, out| animate::encode(address(page), subcode, chunk, SERVICE, id, out))?);
    }
    Ok(bytes)
}

/// A SPRITES record (ANIM-7): all of a sub-page's teletext sprites.
pub(crate) fn sprites_record(page: u16, sub: u16, given: &[Sprite], record_id: &mut u8) -> Result<Vec<u8>, Refused> {
    let references: Vec<Vec<u8>> = given
        .iter()
        .map(|sprite| {
            sprite
                .frames
                .iter()
                .flat_map(|frame| {
                    let source: u16 = match frame.source {
                        ObjectSource::Local => 1,
                        ObjectSource::Public => 2,
                        ObjectSource::Global => 3,
                    };
                    let kind: u16 = match frame.kind {
                        ObjectType::Active => 1,
                        ObjectType::Adaptive => 2,
                        ObjectType::Passive => 3,
                    };
                    ((source << SOURCE_SHIFT) | (kind << TYPE_SHIFT) | frame.number.get()).to_be_bytes()
                })
                .collect()
        })
        .collect();
    let list: Vec<EngineSprite<'_>> = given
        .iter()
        .zip(&references)
        .map(|(sprite, frames)| {
            EngineSprite::new(
                sprite.number.get(),
                sprite.shown,
                sprite.detectable,
                sprite.layer.get(),
                narrow(sprite.order.get()),
                (narrow(sprite.column.get()), narrow(sprite.row.get())),
                (sprite.width.get(), sprite.height.get()),
                frames,
            )
        })
        .collect();
    record(record_id, |id, out| sprites::encode(address(page), EngineSubCode::new(sub), &list, SERVICE, id, out))
}

/// A `NAPLPS_OBJECTS` record (ANIM-11): all of a sub-page's objects, for
/// the NAPLPS stream of `generation`.
pub(crate) fn naplps_objects_record(page: u16, sub: u16, generation: u16, given: &[NaplpsObject], record_id: &mut u8) -> Result<Vec<u8>, Refused> {
    let frames: Vec<Vec<u8>> = given.iter().map(|object| object.frames.iter().map(|code| code.get()).collect()).collect();
    let code = |look: Option<crate::protocol::MacroCode>| look.map_or(0, crate::protocol::MacroCode::get);
    let list: Vec<EngineObject<'_>> = given
        .iter()
        .zip(&frames)
        .map(|(object, frames)| EngineObject {
            number: object.number.get(),
            kind: match object.kind {
                NaplpsObjectKind::Macro => ObjectKind::Macro,
                NaplpsObjectKind::Region => ObjectKind::Region,
            },
            shown: object.shown,
            detectable: object.detectable,
            layer: object.layer.get(),
            order: narrow(object.order.get()),
            x: narrow(object.x.get()),
            y: narrow(object.y.get()),
            width: object.width.get(),
            height: object.height.get(),
            origin: (narrow(object.origin_x.get()), narrow(object.origin_y.get())),
            cover: object.cover.map_or(0, crate::protocol::U8In::get),
            looks: [code(object.hover), code(object.pressed), code(object.alternative)],
            frames,
        })
        .collect();
    record(record_id, |id, out| naplps_objects::encode((address(page), EngineSubCode::new(sub), generation), &list, SERVICE, id, out))
}

/// SHEET records (ANIM-17) of `data`, bytes of the page's sheet of `kind`
/// at `generation` from `offset`: as many as they need. With no bytes, one
/// record of none, which starts the generation.
pub(crate) fn sheet_records(page: u16, kind: SheetKindName, generation: u16, offset: u32, data: &[u8], record_id: &mut u8) -> Result<Vec<u8>, Refused> {
    let kind = match kind {
        SheetKindName::Regis => SheetKind::Regis,
        SheetKindName::Naplps => SheetKind::Naplps,
    };
    let empty: &[u8] = &[];
    let chunks: Vec<&[u8]> = if data.is_empty() { alloc::vec![empty] } else { data.chunks(SHEET_DATA).collect() };
    let mut bytes = Vec::new();
    let mut at = offset;
    for chunk in chunks {
        let sheet = SheetRecord { page: address(page), kind, generation, offset: at, data: chunk };
        bytes.extend(record(record_id, |id, out| sheet::encode(sheet, SERVICE, id, out))?);
        at = at.saturating_add(u32::try_from(chunk.len()).unwrap_or(u32::MAX));
    }
    Ok(bytes)
}

/// Reads a whole sprite sheet as the renderer of a receiver will, before
/// any sub-page's stream (ANIM-17, ANIM-21).
///
/// # Errors
/// What makes it no sheet: a ReGIS commit that fails, with its code, or
/// NAPLPS that draws on the picture, since a sheet draws nothing itself.
pub(crate) fn read_sheet(kind: SheetKindName, sheet: &[u8]) -> Result<(), Refused> {
    match kind {
        SheetKindName::Regis => {
            let mut validator = regis::Validator::new();
            for event in validator.feed_regis(sheet) {
                if let regis::Event::CommitFailed { error, drawing } = event {
                    return Err(format!("a commit fails with code {} ({error:?}), drawing {drawing}", error.code()));
                }
            }
            Ok(())
        }
        SheetKindName::Naplps => {
            let (width, height) = (naplps::limits::OBJECT_PICTURE_WIDTH, naplps::limits::OBJECT_PICTURE_HEIGHT);
            let mut canvas = naplps::Canvas::new(width, height).map_err(|error| format!("{error:?}"))?;
            let mut terminal = naplps::Terminal::new(NAPLPS_PALETTE_BITS);
            // A canvas begins all of one entry, so any other entry is
            // something the sheet drew.
            let blank = canvas.pixels().first().copied();
            let _ = terminal.feed(&mut canvas, sheet);
            // The last command draws when the next begins, or at the end.
            let _ = terminal.finish(&mut canvas);
            if terminal.take_damage().is_some() || canvas.pixels().iter().any(|entry| Some(*entry) != blank) {
                return Err(String::from("it draws on the picture: a sheet defines macros, DRCS characters and textures, and draws nothing"));
            }
            Ok(())
        }
    }
}

/// The animation state of a sub-page's controls and objects that the
/// operations sent so far amount to (ANIM-21), folded with the engine's
/// `neoteletext-rules` as a receiver folds them.
#[derive(Clone, Debug, Default)]
pub(crate) struct Folded {
    states: AnimationStore<MAX_ANIMATED_OBJECTS>,
    /// The targets whose position is the sum of relative moves alone: the
    /// server does not know where an object of its own is, so what is
    /// restored for these is the move by that sum.
    relative: Vec<Target>,
}

impl Folded {
    /// Folds `operations` in. Whether each found room: one for a 65th
    /// object, or a 17th cycle, is not kept, as a receiver does not keep
    /// it.
    pub(crate) fn apply(&mut self, operations: &[Operation]) -> bool {
        let mut kept = true;
        for operation in operations {
            match operation.kind {
                OperationKind::Move { relative: true, .. } if self.states.get(operation.target).is_none_or(|state| state.position.is_none()) => {
                    self.relative.push(operation.target);
                }
                OperationKind::Move { relative: false, .. } | OperationKind::Reset => self.relative.retain(|target| *target != operation.target),
                _ => {}
            }
            // Relative moves add to a position of (0, 0), so that what is
            // held of them is their sum; a cycle is held as it was asked
            // for, and starts again at the viewer's tick.
            kept &= self.states.apply(operation, (0, 0), 0, MAX_CYCLING_OBJECTS) == Applied::Done;
        }
        self.relative.retain(|target| self.states.get(*target).is_some_and(|state| state.position.is_some()));
        kept
    }

    /// Forgets the state of every object, as a record that replaces a
    /// sub-page's sprites or objects, and a new generation of its stream,
    /// make a receiver forget it (ANIM-6); controls keep theirs.
    pub(crate) fn forget_objects(&mut self) {
        let objects: Vec<Target> =
            self.states.entries().iter().map(|(target, _)| *target).filter(|target| target.kind == neoteletext_rules::table::TargetKind::Object).collect();
        for target in objects {
            let _ = self.states.apply(&Operation { target, kind: OperationKind::Reset }, (0, 0), 0, MAX_CYCLING_OBJECTS);
        }
        self.relative.retain(|target| self.states.get(*target).is_some());
    }

    /// The operations that give a receiver with no state this one.
    pub(crate) fn operations(&self) -> Vec<Operation> {
        let mut operations = Vec::new();
        for (target, state) in self.states.entries() {
            let AnimationState { shown, position, layer, order, frame, cycle, look } = *state;
            let mut push = |kind: OperationKind| operations.push(Operation { target: *target, kind });
            match shown {
                Some(true) => push(OperationKind::Show),
                Some(false) => push(OperationKind::Hide),
                None => {}
            }
            if look != 0 {
                push(OperationKind::Look(look));
            }
            if let Some((x, y)) = position {
                push(OperationKind::Move { relative: self.relative.contains(target), x, y });
            }
            if let Some(order) = order {
                push(OperationKind::Order { layer: layer.unwrap_or(KEEP_LAYER), order });
            }
            match cycle {
                Some(cycle) => push(OperationKind::Cycle { first: cycle.first, last: cycle.last, period: cycle.period, once: cycle.once }),
                None if frame != 0 => push(OperationKind::Frame(frame)),
                None => {}
            }
        }
        operations
    }

    /// Whether nothing is held.
    pub(crate) fn is_empty(&self) -> bool {
        self.states.entries().is_empty()
    }
}

/// How long after a timeline's start its tick `tick` is due, in the
/// host's milliseconds: a tick is one sixtieth of a second, tick `n` due
/// `n × 50,000 ÷ 3` microseconds after the start (ANIM-1, ANIM-21),
/// rounded up to the millisecond at or after it.
pub(crate) const fn tick_ms(tick: u64) -> u64 {
    /// Microseconds in a millisecond.
    const MICROSECONDS: u64 = 1_000;
    tick.saturating_mul(TICK_SPAN_MICROSECONDS).div_ceil(TICKS_PER_TICK_SPAN * MICROSECONDS)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// NAPLPS (ANSI X3.110-1983): SO, then RECT with two operands, half
    /// the screen across and a quarter up, as the renderer's own tests
    /// draw one.
    const RECTANGLE: [u8; 5] = [0x0E, 0x31, 0x51, 0x40, 0x40];
    /// DEF MACRO and END (section 6.2), in their eight-bit forms.
    const DEF_MACRO: u8 = 0x80;
    const END: u8 = 0x85;

    /// REQ-6533, ANIM-17, ANIM-21: a sheet draws nothing itself. A NAPLPS
    /// sheet that defines a macro is one; the same drawing outside a macro
    /// is refused, and so is a ReGIS sheet whose commit fails.
    #[test]
    fn a_sheet_draws_nothing() {
        let mut defines = alloc::vec![DEF_MACRO, 0x21];
        defines.extend_from_slice(&RECTANGLE);
        defines.push(END);
        assert_eq!(read_sheet(SheetKindName::Naplps, &defines), Ok(()));
        assert_eq!(read_sheet(SheetKindName::Naplps, &[]), Ok(()));
        assert!(read_sheet(SheetKindName::Naplps, &RECTANGLE).is_err());
        assert_eq!(read_sheet(SheetKindName::Regis, b"D(S1(V0),C)"), Ok(()));
        assert!(read_sheet(SheetKindName::Regis, b"D(S1(H2),S2(A1),C)").is_err());
    }

    /// REQ-6533, ANIM-21: a timeline's tick is a sixtieth of a second on
    /// the server's clock, due at the millisecond at or after it.
    #[test]
    fn a_tick_is_a_sixtieth_of_a_second() {
        assert_eq!([tick_ms(0), tick_ms(1), tick_ms(3), tick_ms(30), tick_ms(60), tick_ms(216_000)], [0, 17, 50, 500, 1_000, 3_600_000]);
    }
}
