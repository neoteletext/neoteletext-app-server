//! A picture drawn as the browser draws it, and as Level 1 mosaics
//! (neoteletext-common `docs/renderer-parity.md` PAR-25, PAR-26; REQ-6769,
//! REQ-6768): for a receiver that did not agree the picture's language, and
//! for a public page's picture.
//!
//! ReGIS is drawn by `regis::Terminal` on its 800 by 480 screen with the
//! VT340's alphabet 0 (the ReGIS repository's font) and the 55 provided
//! sets from the engine's compiled Noto Teletext glyphs (PAR-12), sets 44
//! to 51 with their joining data (PAR-3, PAR-13); NAPLPS by
//! `naplps::Terminal` on the 480 by 462 display area of the teletext
//! raster (REQ-5819, DEC-6667) with the NAPLPS repository's stroke font,
//! the same 55 sets and joining data (REQ-6668, PAR-14), and the shown
//! objects composed over it (ANIM-11). Each is turned into RGB by its
//! colour map, and converted by the engine's
//! `neoteletext_picture::convert_raster` (PAR-11) into mosaics on rows 1
//! to 23.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use neoteletext_core::charset::CharRef;
use neoteletext_core::consts::{CELL_HEIGHT, CELL_WIDTH, COLUMNS, PAGE_HEIGHT, PAGE_WIDTH};
use neoteletext_core::glyphs::conformance::{FONT_SET_COUNT, FONT_SETS};
use neoteletext_core::glyphs::{GlyphSet, JoiningClass, joining};
use neoteletext_core::packet::{Decoded, Packet, decode};
use neoteletext_picture::{Fit, Level, Options, Raster};

use crate::kept::PictureLanguage;
use crate::protocol::{NaplpsObject, NaplpsObjectKind};

/// The rows the fallback draws: 1 to 23, rows 0 and 24 being the
/// program's (PAR-25).
pub(crate) const FIRST_ROW: u8 = 1;
/// See [`FIRST_ROW`].
pub(crate) const LAST_ROW: u8 = 23;
/// How many.
pub(crate) const ROWS: usize = (LAST_ROW - FIRST_ROW + 1) as usize;
/// The fallback's rows: 23 rows of 40 seven-bit codes.
pub(crate) type MosaicRows = [[u8; COLUMNS]; ROWS];

/// The colour map of a NAPLPS picture: sixteen entries, 2 to the 4, as the
/// browser's (`animation.rs`).
const NAPLPS_PALETTE_BITS: u32 = 4;
/// The display area of the teletext raster a NAPLPS picture takes: 480 by
/// 462 (REQ-5819, DEC-6667 item 1).
pub(crate) const NAPLPS_DISPLAY_LINES: usize = 462;
/// The first and last codes of a provided set (REQ-6668).
const FIRST_SET_CODE: u8 = 0x20;
/// See [`FIRST_SET_CODE`].
const LAST_SET_CODE: u8 = 0x7F;
/// The sets the renderers join (group 1001, PAR-3).
const JOINED_SETS: core::ops::RangeInclusive<u8> = neoteletext_core::glyphs::JOINED_SETS;
/// A packet's parity bit, which a row's codes are stripped of.
const PARITY_BIT: u8 = 0x80;

// PAR-12, REQ-6668: the engine's sets are each renderer's, one for one.
const _: () = assert!(FONT_SET_COUNT == regis::limits::PROVIDED_SETS as usize);
const _: () = assert!(FONT_SET_COUNT == naplps::TELETEXT_SET_COUNT);
const _: () = assert!((LAST_SET_CODE - FIRST_SET_CODE + 1) as usize == regis::limits::PROVIDED_SET_POSITIONS);

/// Noto Teletext, the engine's one compiled font (its REQ-FNT-001).
static GLYPHS: GlyphSet<'static> = GlyphSet::noto_teletext();

/// A side as the renderers take it.
fn side(value: usize) -> Result<u8, String> {
    u8::try_from(value).map_err(|_| String::from("the engine's cell passes a renderer's glyph"))
}

/// The ReGIS terminal the browser draws with (PAR-12, PAR-13).
///
/// # Errors
/// A font or set the renderer refuses: a fault of this build.
pub(crate) fn regis_terminal() -> Result<regis::Terminal, String> {
    let mut terminal = regis::Terminal::new();
    terminal.set_font(regis::Font::parse(regis_fonts::VT340).map_err(|error| format!("the VT340 font: {error:?}"))?);
    for (number, (_, set)) in (0_u8..).zip(FONT_SETS.iter()) {
        let mut provided = regis::ProvidedSet::new(side(CELL_WIDTH)?, side(CELL_HEIGHT)?).map_err(|error| format!("{error:?}"))?;
        for code in FIRST_SET_CODE..=LAST_SET_CODE {
            provided.set_glyph(code, &GLYPHS.glyph(CharRef::new(*set, code))).map_err(|error| format!("{error:?}"))?;
            if !JOINED_SETS.contains(&number) {
                continue;
            }
            let Some(joined) = joining(number, code) else { continue };
            let kind = match joined.class {
                JoiningClass::None => regis::JoiningType::None,
                JoiningClass::Right => regis::JoiningType::Right,
                JoiningClass::Dual => regis::JoiningType::Dual,
                JoiningClass::Transparent => regis::JoiningType::Transparent,
            };
            provided.set_joining(code, kind).map_err(|error| format!("{error:?}"))?;
            for (form, rows) in
                [(regis::JoinedForm::Final, joined.final_form), (regis::JoinedForm::Initial, joined.initial), (regis::JoinedForm::Medial, joined.medial)]
            {
                provided.set_form(code, form, &rows).map_err(|error| format!("{error:?}"))?;
            }
        }
        terminal.set_provided_set(number, provided).map_err(|error| format!("{error:?}"))?;
    }
    Ok(terminal)
}

/// The NAPLPS terminal and canvas the browser draws with (REQ-5819,
/// REQ-6668, PAR-14).
///
/// # Errors
/// A font, set or canvas the renderer refuses: a fault of this build.
pub(crate) fn naplps_terminal() -> Result<(naplps::Terminal, naplps::Canvas), String> {
    let size = |value: usize| u32::try_from(value).map_err(|_| String::from("a side of the raster passes 32 bits"));
    let canvas = naplps::Canvas::new(size(PAGE_WIDTH)?, size(NAPLPS_DISPLAY_LINES)?).map_err(|error| format!("{error:?}"))?;
    let mut terminal = naplps::Terminal::new(NAPLPS_PALETTE_BITS);
    terminal.set_font(naplps::StrokeFont::parse(naplps_fonts::NEOTELETEXT).map_err(|error| format!("the stroke font: {error:?}"))?);
    for (number, (_, set)) in (0_u8..).zip(FONT_SETS.iter()) {
        let mut bitmap = naplps::BitmapSet::new(side(CELL_WIDTH)?, side(CELL_HEIGHT)?).map_err(|error| format!("{error:?}"))?;
        for code in FIRST_SET_CODE..=LAST_SET_CODE {
            bitmap.set_glyph(code, &GLYPHS.glyph(CharRef::new(*set, code))).map_err(|error| format!("{error:?}"))?;
            if !JOINED_SETS.contains(&number) {
                continue;
            }
            let Some(joined) = joining(number, code) else { continue };
            let kind = match joined.class {
                JoiningClass::None => naplps::JoiningType::None,
                JoiningClass::Right => naplps::JoiningType::Right,
                JoiningClass::Dual => naplps::JoiningType::Dual,
                JoiningClass::Transparent => naplps::JoiningType::Transparent,
            };
            bitmap.set_joining(code, kind).map_err(|error| format!("{error:?}"))?;
            for (form, rows) in
                [(naplps::JoinForm::Final, joined.final_form), (naplps::JoinForm::Initial, joined.initial), (naplps::JoinForm::Medial, joined.medial)]
            {
                bitmap.set_form_glyph(code, form, &rows).map_err(|error| format!("{error:?}"))?;
            }
        }
        terminal.set_teletext_set(number, bitmap).map_err(|error| format!("{error:?}"))?;
    }
    Ok((terminal, canvas))
}

/// A protocol object as the renderer's.
fn object(object: &NaplpsObject) -> naplps::Object {
    let narrow = |value: i32| i16::try_from(value).unwrap_or_default();
    naplps::Object {
        number: object.number.get(),
        kind: match object.kind {
            NaplpsObjectKind::Macro => naplps::ObjectKind::Macro,
            NaplpsObjectKind::Region => naplps::ObjectKind::Region,
        },
        shown: object.shown,
        detectable: object.detectable,
        layer: object.layer.get(),
        order: i8::try_from(object.order.get()).unwrap_or_default(),
        x: narrow(object.x.get()),
        y: narrow(object.y.get()),
        width: object.width.get(),
        height: object.height.get(),
        origin_x: narrow(object.origin_x.get()),
        origin_y: narrow(object.origin_y.get()),
        cover: object.cover.map_or(0, crate::protocol::U8In::get),
        hover: object.hover.map_or(0, crate::protocol::U8In::get),
        pressed: object.pressed.map_or(0, crate::protocol::U8In::get),
        alternative: object.alternative.map_or(0, crate::protocol::U8In::get),
        frames: object.frames.iter().map(|code| code.get()).collect(),
        label: String::new(),
    }
}

/// A drawn picture: its pixels by colour-map address, its width and its
/// colours.
pub(crate) struct Drawn {
    pub width: usize,
    pub height: usize,
    pub addresses: Vec<u8>,
    pub colours: Vec<[u8; 3]>,
    /// NAPLPS: the screen colour its border lines take (REQ-5819).
    pub border: Option<u8>,
    /// ReGIS: the picture's own title, `D(T'...')`, as the last commit left
    /// it, for a public page's `alt` (PAR-26). A NAPLPS picture's title
    /// comes with its objects, outside the stream, so it is none here.
    pub title: Option<String>,
    /// NAPLPS: the pixel work its drawing spent, the stream's and the
    /// objects' composing together (the renderer's `work_spent` and
    /// `compose_work`, DEC-6851), what the fallback's budget counts it by
    /// (TODO-6845); none for ReGIS, counted by its bytes.
    pub work: Option<u64>,
}

impl Drawn {
    /// As RGB, row by row.
    pub(crate) fn raster(&self) -> Option<Raster> {
        let mut data = Vec::with_capacity(self.addresses.len().saturating_mul(3));
        for &address in &self.addresses {
            data.extend_from_slice(&self.colours.get(usize::from(address)).copied().unwrap_or_default());
        }
        Raster::new(self.width, self.height, data)
    }
}

/// `stream` drawn as the browser draws it, with the committed numbered
/// drawings composed for ReGIS and `objects` for NAPLPS: no animation and
/// no pointer look (PAR-25, PAR-26).
///
/// # Errors
/// A renderer's refusal to be built or to compose.
pub(crate) fn draw(language: PictureLanguage, stream: &[u8], objects: Option<&[NaplpsObject]>) -> Result<Drawn, String> {
    match language {
        PictureLanguage::Regis => {
            let mut terminal = regis_terminal()?;
            let mut screen = regis::Screen::new();
            let _ = terminal.feed_regis(&mut screen, stream);
            let _ = terminal.finish(&mut screen);
            // The committed numbered drawings over the base, as the
            // browser's decoder composes them (`docs/regis-v2.md`, "Commit
            // and repaint"), with no viewer state; a drawing given a paint
            // rate is shown finished, which is exactly the picture painted
            // at once (`docs/regis-v2.md`, "Paint rate").
            let _ = terminal.advance_ticks(u32::MAX);
            let mut picture = regis::Picture::new();
            terminal.compose(&screen, &mut picture);
            let canvas = picture.canvas();
            let colours = terminal.palette().colours().to_vec();
            let title = terminal.title();
            Ok(Drawn {
                width: canvas.width() as usize,
                height: canvas.height() as usize,
                addresses: canvas.pixels().to_vec(),
                colours,
                border: None,
                title,
                work: None,
            })
        }
        PictureLanguage::Naplps => {
            let (mut terminal, mut canvas) = naplps_terminal()?;
            let _ = terminal.feed(&mut canvas, stream);
            let _ = terminal.finish(&mut canvas);
            let shown: Vec<naplps::Object> = objects.unwrap_or_default().iter().map(object).collect();
            let (presented, composing) = if shown.is_empty() {
                (canvas, 0)
            } else {
                terminal.set_objects(&shown).map_err(|error| format!("{error:?}"))?;
                let mut presented = naplps::Canvas::new(canvas.width(), canvas.height()).map_err(|error| format!("{error:?}"))?;
                terminal.compose(&canvas, &mut presented).map_err(|error| format!("{error:?}"))?;
                // A composing that stopped at MAX_COMPOSE_WORK shows only
                // the objects it paid for, as the browser's does (DEC-6851).
                (presented, terminal.compose_work().0)
            };
            let colours = terminal.palette().colours().to_vec();
            let (width, height) = (presented.width() as usize, presented.height() as usize);
            let work = Some(terminal.work_spent().saturating_add(composing));
            Ok(Drawn { width, height, addresses: presented.pixels().to_vec(), colours, border: Some(terminal.screen_colour()), title: None, work })
        }
    }
}

/// The converter's options for the fallback (PAR-11): Level 1, the picture
/// contained in rows 1 to 23, nothing on rows 0 and 24.
fn options() -> Options {
    Options { level: Level::One, fit: Fit::Contain, keep_header: true, keep_footer: true, ..Options::default() }
}

/// `drawn` as Level 1 mosaics on rows 1 to 23 (PAR-25).
///
/// # Errors
/// The converter's refusal.
pub(crate) fn mosaics(drawn: &Drawn) -> Result<MosaicRows, String> {
    let raster = drawn.raster().ok_or_else(|| String::from("a drawn picture with no pixels"))?;
    let page = neoteletext_picture::convert_raster(raster, &options()).map_err(|error| format!("{error:?}"))?;
    let packets = page.t42().map_err(|error| format!("{error:?}"))?;
    let mut rows: MosaicRows = [[b' '; COLUMNS]; ROWS];
    for bytes in packets.as_chunks::<{ neoteletext_core::consts::PACKET_BYTES }>().0 {
        let Ok(Decoded::Row(row)) = decode(&Packet { bytes: *bytes }) else { continue };
        let Some(slot) = row.row.checked_sub(FIRST_ROW).and_then(|index| rows.get_mut(usize::from(index))) else { continue };
        for (code, byte) in slot.iter_mut().zip(row.bytes.iter()) {
            *code = byte & !PARITY_BIT;
        }
    }
    Ok(rows)
}

/// `drawn` as a PNG at its language's own size, as the browser shows it
/// (PAR-26): ReGIS 800 by 480; NAPLPS on the 480 by 500 teletext raster,
/// its 19 border lines above and below in the screen colour (REQ-5819),
/// the same raster and pixel shape as a teletext page's public picture.
/// None when the writer refuses it.
pub(crate) fn png(drawn: &Drawn) -> Option<(Vec<u8>, usize, usize)> {
    let palette = drawn.colours.iter().map(|&[red, green, blue]| neoteletext_image::palette::Rgba::opaque(red, green, blue)).collect();
    let (height, indices) = match drawn.border {
        None => (drawn.height, drawn.addresses.clone()),
        Some(border) => {
            let lines = PAGE_HEIGHT.saturating_sub(drawn.height) / 2;
            let edge = alloc::vec![border; lines.saturating_mul(drawn.width)];
            let mut indices = edge.clone();
            indices.extend_from_slice(&drawn.addresses);
            indices.extend_from_slice(&edge);
            (drawn.height.saturating_add(lines.saturating_mul(2)), indices)
        }
    };
    let raster = neoteletext_image::raster::Raster { width: drawn.width, height, indices, palette };
    neoteletext_image::raster::png(&raster).ok().map(|bytes| (bytes, drawn.width, height))
}

/// `stream` as the fallback's rows, and for NAPLPS the pixel work its
/// drawing spent (TODO-6845).
///
/// # Errors
/// As [`draw`] and [`mosaics`].
pub(crate) fn rows(language: PictureLanguage, stream: &[u8], objects: Option<&[NaplpsObject]>) -> Result<(MosaicRows, Option<u64>), String> {
    let drawn = draw(language, stream, objects)?;
    Ok((mosaics(&drawn)?, drawn.work))
}

#[cfg(test)]
pub(crate) mod tests;
