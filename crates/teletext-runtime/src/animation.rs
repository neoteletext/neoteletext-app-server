//! Animation as the server sends it (neoteletext-common
//! `docs/animation.md` ANIM-2, ANIM-6, ANIM-7, ANIM-11, ANIM-17, ANIM-21;
//! REQ-6533): the protocol's messages turned into the engine's records,
//! each written by the engine's encoder, which checks what it writes with
//! its reader; and a sprite sheet read by the renderer a receiver has. The
//! server keeps no animation state for a later viewer (ANIM-21,
//! page-rules.md RULE-111): a page's program sends it afresh at each
//! selection.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use neoteletext_core::charset::CharRef;
use neoteletext_core::consts::{CELL_HEIGHT, CELL_WIDTH, PAGE_HEIGHT, PAGE_WIDTH};
use neoteletext_core::glyphs::GlyphSet;
use neoteletext_core::glyphs::conformance::{FONT_SET_COUNT, FONT_SETS};
use neoteletext_core::packet::Packet;
use neoteletext_core::types::SubCode as EngineSubCode;
use neoteletext_rules::animate::Operation;
use neoteletext_rules::limits::{MAX_ANIMATE_OPERATIONS, TICK_SPAN_MICROSECONDS, TICKS_PER_TICK_SPAN};
use neoteletext_web::records::frame::{FrameMark, FrameRecord};
use neoteletext_web::records::naplps_objects::{LabelledObject, NaplpsObject as EngineObject, ObjectKind};
use neoteletext_web::records::sheet::{MAX_DATA as SHEET_DATA, SheetKind, SheetRecord};
use neoteletext_web::records::sprites::Sprite as EngineSprite;
use neoteletext_web::records::{animate, frame, naplps_objects, sheet, sprites};

use crate::page::address;
use crate::protocol::{FrameMarkName, NaplpsObject, NaplpsObjectKind, NaplpsObjects, NaplpsObjectsCommand, ObjectSource, ObjectType, SheetKindName, Sprite};

/// The audio and input service the records go on (the reference channel's,
/// DOC-1951).
const SERVICE: u8 = 0;

/// Why the engine's encoder, or a renderer, refused what the protocol's
/// checks passed.
pub(crate) type Refused = String;

/// The colour map a receiver gives a NAPLPS page: 2 to the power of this
/// many entries, sixteen, as its ReGIS pictures have.
const NAPLPS_PALETTE_BITS: u32 = 4;

/// The lines of the teletext raster a NAPLPS page's display area takes
/// (REQ-5819, DEC-6667 item 1): 480 by 462 of the 480 by 500 raster has
/// the NAPLPS page's shape of 1.28 to 1 at teletext's 16/13 pixel shape
/// (480 × 16/13 ÷ 1.28 = 461.5, rounded), with 19 border lines above and
/// below, which the receiver draws itself.
const NAPLPS_DISPLAY_LINES: usize = 462;
const _: () =
    assert!(NAPLPS_DISPLAY_LINES < PAGE_HEIGHT && (PAGE_HEIGHT - NAPLPS_DISPLAY_LINES) / 2 == NAPLPS_BORDER_LINES, "REQ-5819: 19 border lines above and below");
/// See [`NAPLPS_DISPLAY_LINES`].
const NAPLPS_BORDER_LINES: usize = 19;

/// The first and last codes of a character set the NAPLPS renderer takes,
/// 2/0 and 7/15 (REQ-6668: each of the 55 sets has 96 positions).
const FIRST_SET_CODE: u8 = 0x20;
/// See [`FIRST_SET_CODE`].
const LAST_SET_CODE: u8 = 0x7F;
const _: () = assert!((LAST_SET_CODE - FIRST_SET_CODE + 1) as usize == naplps::bitmap::BITMAP_SET_CODES);
// REQ-6668: the engine's sets are the renderer's, one for one, in order.
const _: () = assert!(FONT_SET_COUNT == naplps::TELETEXT_SET_COUNT);

/// Noto Teletext, the engine's one compiled font (its REQ-FNT-001), which
/// a public page's picture is drawn in too.
static GLYPHS: GlyphSet<'static> = GlyphSet::noto_teletext();

/// A NAPLPS terminal and canvas as a receiver has them (the browser's
/// decoder): the display area of the teletext raster, 480 by 462 pixels
/// (REQ-5819, DEC-6667), and the engine's 55 teletext character sets in
/// the order of its `FONT_SETS`, which is the renderer's `TELETEXT_SETS`,
/// each code's glyph the teletext renderer's (REQ-6668; DEC-6669: the
/// provided sets only). What the server reads with it is read as the
/// receiver will read it.
fn naplps_receiver() -> Result<(naplps::Terminal, naplps::Canvas), Refused> {
    let size = |side: usize| u32::try_from(side).map_err(|_| String::from("a side of the raster passes 32 bits"));
    let canvas = naplps::Canvas::new(size(PAGE_WIDTH)?, size(NAPLPS_DISPLAY_LINES)?).map_err(|error| format!("{error:?}"))?;
    let mut terminal = naplps::Terminal::new(NAPLPS_PALETTE_BITS);
    let (Ok(width), Ok(height)) = (u8::try_from(CELL_WIDTH), u8::try_from(CELL_HEIGHT)) else {
        return Err(String::from("the engine's cell passes the renderer's bitmap"));
    };
    for (number, (_, set)) in (0_u8..).zip(FONT_SETS.iter()) {
        let mut bitmap = naplps::BitmapSet::new(width, height).map_err(|error| format!("{error:?}"))?;
        for code in FIRST_SET_CODE..=LAST_SET_CODE {
            bitmap.set_glyph(code, &GLYPHS.glyph(CharRef::new(*set, code))).map_err(|error| format!("{error:?}"))?;
        }
        terminal.set_teletext_set(number, bitmap).map_err(|error| format!("{error:?}"))?;
    }
    Ok((terminal, canvas))
}

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

/// ANIMATE records (ANIM-6) of `operations` for one page and sub-code
/// (DEC-6599: never 3F7F, which the engine's encoder refuses): one record
/// for each `MAX_ANIMATE_OPERATIONS` operations.
pub(crate) fn animate_records(page: u16, sub: u16, operations: &[Operation], record_id: &mut u8) -> Result<Vec<u8>, Refused> {
    let mut bytes = Vec::new();
    for chunk in operations.chunks(MAX_ANIMATE_OPERATIONS) {
        bytes.extend(record(record_id, |id, out| animate::encode(address(page), EngineSubCode::new(sub), chunk, SERVICE, id, out))?);
    }
    Ok(bytes)
}

/// `given` as the engine's sprites, handed to `use_list`: the one
/// conversion of the protocol's sprites, for a record sent directly and for
/// a group's command alike (RULE-114).
fn with_sprites<R>(given: &[Sprite], use_list: impl FnOnce(&[EngineSprite<'_>]) -> R) -> R {
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
    use_list(&list)
}

/// A SPRITES record (ANIM-7): all of a sub-page's teletext sprites.
pub(crate) fn sprites_record(page: u16, sub: u16, given: &[Sprite], record_id: &mut u8) -> Result<Vec<u8>, Refused> {
    with_sprites(given, |list| record(record_id, |id, out| sprites::encode(address(page), EngineSubCode::new(sub), list, SERVICE, id, out)))
}

/// The body of a SPRITES record alone, as a group's command holds it
/// (RULE-35, RULE-119): the bytes [`sprites_record`] frames.
pub(crate) fn sprites_body(page: u16, sub: u16, given: &[Sprite]) -> Result<Vec<u8>, Refused> {
    with_sprites(given, |list| {
        let mut body = alloc::vec![0_u8; sprites::MAX_BYTES];
        let length = sprites::write_body(address(page), EngineSubCode::new(sub), list, &mut body).map_err(|fault| format!("{fault:?}"))?;
        body.truncate(length);
        Ok(body)
    })
}

/// What a `naplps_objects` message or a group's command asks of a
/// sub-page's NAPLPS objects (ANIM-11): the objects, the picture's title,
/// and whether the one object given is redefined alone.
#[derive(Clone, Copy)]
pub(crate) struct NaplpsSet<'a> {
    /// The objects: exactly one when `one`.
    pub objects: &'a [NaplpsObject],
    /// The picture's title, never with `one`.
    pub title: Option<&'a str>,
    /// The one object redefines its number's, keeping the rest and the
    /// title (DEC-6638 item 3).
    pub one: bool,
}

impl<'a> From<&'a NaplpsObjects> for NaplpsSet<'a> {
    fn from(message: &'a NaplpsObjects) -> Self {
        Self { objects: &message.objects, title: message.title.as_ref().map(|title| title.0.as_str()), one: message.one }
    }
}

impl<'a> From<&'a NaplpsObjectsCommand> for NaplpsSet<'a> {
    fn from(command: &'a NaplpsObjectsCommand) -> Self {
        Self { objects: &command.objects, title: command.title.as_ref().map(|title| title.0.as_str()), one: command.one }
    }
}

/// Why `set` cannot be written, of what the engine's reader refuses that
/// the protocol's types do not: `one` with other than one object, or with
/// a title (ANIM-11: ONE has a count of exactly 1 and never TITLED).
pub(crate) fn naplps_set_fault(set: NaplpsSet<'_>) -> Option<(&'static str, String)> {
    if set.one && set.objects.len() != 1 {
        return Some(("objects", format!("one redefines exactly one object, not {}", set.objects.len())));
    }
    if set.one && set.title.is_some() {
        return Some(("title", "one keeps the picture's title: no title with one".into()));
    }
    None
}

/// `set` as the engine's contents, handed to `use_contents`: the one
/// conversion, for a record sent directly and for a group's command alike.
fn with_objects<R>(set: NaplpsSet<'_>, use_contents: impl FnOnce(&naplps_objects::Contents<'_>) -> Result<R, Refused>) -> Result<R, Refused> {
    if let Some((_, detail)) = naplps_set_fault(set) {
        return Err(detail);
    }
    let frames: Vec<Vec<u8>> = set.objects.iter().map(|object| object.frames.iter().map(|code| code.get()).collect()).collect();
    let code = |look: Option<crate::protocol::MacroCode>| look.map_or(0, crate::protocol::MacroCode::get);
    let list: Vec<LabelledObject<'_>> = set
        .objects
        .iter()
        .zip(&frames)
        .map(|(object, frames)| LabelledObject {
            object: EngineObject {
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
            },
            label: object.label.as_ref().map_or("", |label| label.0.as_str()),
        })
        .collect();
    let contents = match (set.one, list.as_slice()) {
        (true, [object]) => naplps_objects::Contents::One(*object),
        _ => naplps_objects::Contents::Whole { objects: &list, title: set.title },
    };
    use_contents(&contents)
}

/// A `NAPLPS_OBJECTS` record (ANIM-11): a sub-page's objects and title,
/// or one object redefined, for the NAPLPS stream of `generation`.
pub(crate) fn naplps_objects_record(page: u16, sub: u16, generation: u16, set: NaplpsSet<'_>, record_id: &mut u8) -> Result<Vec<u8>, Refused> {
    with_objects(set, |contents| {
        record(record_id, |id, out| naplps_objects::encode_contents((address(page), EngineSubCode::new(sub), generation), contents, SERVICE, id, out))
    })
}

/// The body of a `NAPLPS_OBJECTS` record alone, as a group's command holds
/// it (RULE-35, RULE-119): the bytes [`naplps_objects_record`] frames.
pub(crate) fn naplps_objects_body(page: u16, sub: u16, generation: u16, set: NaplpsSet<'_>) -> Result<Vec<u8>, Refused> {
    with_objects(set, |contents| {
        let mut body = alloc::vec![0_u8; naplps_objects::MAX_BYTES];
        let length =
            naplps_objects::write_contents((address(page), EngineSubCode::new(sub), generation), contents, &mut body).map_err(|fault| format!("{fault:?}"))?;
        body.truncate(length);
        Ok(body)
    })
}

/// SHEET records (ANIM-17) of `data`, bytes of one page's (number and
/// sub-code, DEC-6599) sheet of `kind` at `generation` from `offset`: as
/// many as they need. With no bytes, one record of none, which starts the
/// generation.
pub(crate) fn sheet_records(
    (page, sub): (u16, u16),
    kind: SheetKindName,
    generation: u16,
    offset: u32,
    data: &[u8],
    record_id: &mut u8,
) -> Result<Vec<u8>, Refused> {
    let kind = match kind {
        SheetKindName::Regis => SheetKind::Regis,
        SheetKindName::Naplps => SheetKind::Naplps,
    };
    let empty: &[u8] = &[];
    let chunks: Vec<&[u8]> = if data.is_empty() { alloc::vec![empty] } else { data.chunks(SHEET_DATA).collect() };
    let mut bytes = Vec::new();
    let mut at = offset;
    for chunk in chunks {
        let sheet = SheetRecord { page: address(page), subcode: EngineSubCode::new(sub), kind, generation, offset: at, data: chunk };
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
            // ANIM-17: a sheet defines hidden masters and draws nothing
            // itself, neither a drawing left visible nor anything outside
            // numbered drawings; the validator is the receiver's own
            // renderer, which says both.
            if let Some(drawing) = validator.visible_drawing() {
                return Err(format!("drawing {drawing} is left visible: a sheet's drawings are hidden masters, and the sheet draws nothing"));
            }
            if validator.base_work() > 0 {
                return Err(String::from("it draws on the picture outside numbered drawings: a sheet defines hidden masters and draws nothing"));
            }
            Ok(())
        }
        SheetKindName::Naplps => {
            let (mut terminal, mut canvas) = naplps_receiver()?;
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
        // TODO-6570: a drawing left visible, and drawing outside any
        // numbered drawing, are refused with the renderer's own validator.
        assert_eq!(read_sheet(SheetKindName::Regis, b"D(B41)P[10,10]V[20,20]D(E)D(S41(V0),C)"), Ok(()));
        assert!(read_sheet(SheetKindName::Regis, b"D(B41)P[10,10]V[20,20]D(E)D(C)").is_err());
        assert!(read_sheet(SheetKindName::Regis, b"P[10,10]V[20,20]").is_err());
    }

    /// REQ-5819, DEC-6667, REQ-6668: the server reads NAPLPS on the canvas
    /// a receiver draws it on, the 480 by 462 display area of the teletext
    /// raster, with the engine's 55 sets: a page designating Greek (set 17,
    /// ESC 2/8 2/1 3/1 into G0) draws the engine's Noto Teletext capital
    /// delta for its 4/4, exactly as a terminal given that glyph alone
    /// draws it, and not nothing.
    #[test]
    fn naplps_is_read_on_the_receivers_raster_with_its_character_sets() {
        const GREEK_D: &[u8] = b"\x1b\x28\x21\x31D";
        let (mut terminal, mut canvas) = naplps_receiver().unwrap();
        assert_eq!((canvas.width(), canvas.height()), (480, 462));
        let _ = terminal.feed(&mut canvas, GREEK_D);
        let _ = terminal.finish(&mut canvas);
        let delta = *GLYPHS.lookup('\u{0394}').unwrap();
        let mut alone = naplps::Terminal::new(NAPLPS_PALETTE_BITS);
        let mut set = naplps::BitmapSet::new(12, 20).unwrap();
        set.set_glyph(0x44, &delta).unwrap();
        alone.set_teletext_set(17, set).unwrap();
        let mut expected = naplps::Canvas::new(480, 462).unwrap();
        let _ = alone.feed(&mut expected, GREEK_D);
        let _ = alone.finish(&mut expected);
        assert!(expected.pixels().iter().any(|&pixel| pixel != 0), "the delta draws");
        assert_eq!(canvas.pixels(), expected.pixels(), "the engine's Greek glyph, as the receiver draws it");
    }

    /// REQ-6533, ANIM-21: a timeline's tick is a sixtieth of a second on
    /// the server's clock, due at the millisecond at or after it.
    #[test]
    fn a_tick_is_a_sixtieth_of_a_second() {
        assert_eq!([tick_ms(0), tick_ms(1), tick_ms(3), tick_ms(30), tick_ms(60), tick_ms(216_000)], [0, 17, 50, 500, 1_000, 3_600_000]);
    }
}
