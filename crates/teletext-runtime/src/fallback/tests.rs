//! The mosaic fallback's drawing (neoteletext-common
//! `docs/renderer-parity.md` PAR-25; REQ-6769): a ReGIS and a NAPLPS
//! picture of a red rectangle on black give red mosaics where it lies.

extern crate std;

use super::{MosaicRows, ROWS, rows};
use crate::kept::PictureLanguage;

/// Teletext's red graphics attribute (EN 300 706 Table 26): 0x11.
const RED_GRAPHICS: u8 = 0x11;
/// A mosaic cell of all six sixels.
const FULL_BLOCK: u8 = 0x7F;

/// For each row, whether it holds red graphics and how many full blocks
/// lie in the left and right halves.
fn shape(rows: &MosaicRows) -> std::vec::Vec<(bool, usize, usize)> {
    rows.iter()
        .map(|row| {
            let (left, right) = row.split_at(row.len() / 2);
            let full = |half: &[u8]| half.iter().copied().filter(|code| *code == FULL_BLOCK).map(usize::from).count();
            (row.contains(&RED_GRAPHICS), full(left), full(right))
        })
        .collect()
}

#[test]
fn a_regis_red_rectangle_on_black_gives_red_mosaics_where_it_lies() {
    // Colour map entry 1 red, hue 120 of the HLS system (manual chapter 2), the left
    // half filled with it (chapter 11, a filled polygon).
    let stream = b"S(M1(H120L50S100))W(I1)P[0,0]F(V[399,0][399,479][0,479][0,0])";
    let drawn = rows(PictureLanguage::Regis, stream, None).unwrap().0;
    let shape = shape(&drawn);
    assert_eq!(shape.len(), ROWS);
    let middle = shape[ROWS / 2];
    assert!(middle.0 && middle.1 > 10 && middle.2 == 0, "{shape:?}\n{drawn:?}");
}

/// Teletext's white graphics attribute (EN 300 706 Table 26): 0x17.
const WHITE_GRAPHICS: u8 = 0x17;

#[test]
fn a_naplps_rectangle_on_black_gives_mosaics_where_it_lies() {
    // SO; SELECT COLOR entry 7, nominal white (ANSI X3.110-1983 sections
    // 5.3.2.5.2, 5.3.2.6); POINT SET absolute at the origin (5.3.3.1);
    // RECTANGLE (FILLED) half the unit screen wide and its whole 3/4 high
    // (5.3.3.4), each coordinate a three-byte pair of 256ths (5.3.2.2.3).
    let pair = |x: u16, y: u16| {
        let byte = |shift: u16| 0x40 | ((((x >> shift) & 7) as u8) << 3) | (((y >> shift) & 7) as u8);
        [byte(6), byte(3), byte(0)]
    };
    let mut stream = std::vec![0x0E, 0x3E, 0x40 | (7 << 2), 0x24];
    stream.extend(pair(0, 0));
    stream.push(0x31);
    stream.extend(pair(128, 191));
    let drawn = rows(PictureLanguage::Naplps, &stream, None).unwrap().0;
    let middle = drawn[ROWS / 2];
    let (left, right) = middle.split_at(middle.len() / 2);
    assert!(middle.contains(&WHITE_GRAPHICS), "{drawn:?}");
    assert!(left.iter().copied().filter(|code| *code == FULL_BLOCK).map(usize::from).count() > 5, "{drawn:?}");
    assert!(!right.contains(&FULL_BLOCK), "{drawn:?}");
}

/// TODO-6813: the same red half drawn inside numbered drawing 1 of ReGIS v2
/// (`docs/regis-v2.md`, `D(B n)` … `D(E)` and the commit `D(C)`), the colour
/// map set on the base, which the drawings share (its rule 4).
pub(crate) const RED_LEFT_IN_A_DRAWING: &[u8] = b"S(M1(H120L50S100))D(B1)W(I1)P[0,0]F(V[399,0][399,479][0,479][0,0])D(E)D(C)";
/// The base alone of [`RED_LEFT_IN_A_DRAWING`]'s picture, drawn on the base.
const RED_LEFT: &[u8] = b"S(M1(H120L50S100))W(I1)P[0,0]F(V[399,0][399,479][0,479][0,0])";

/// Renderer parity PAR-25 and TODO-6813: the fallback composes a ReGIS v2
/// picture's committed numbered drawings over its base, as the browser
/// does, so a red half drawn in a drawing gives the mosaics the same red
/// half drawn on the base gives.
#[test]
fn the_fallback_composes_a_regis_pictures_numbered_drawings() {
    let in_drawing = rows(PictureLanguage::Regis, RED_LEFT_IN_A_DRAWING, None).unwrap().0;
    let middle = shape(&in_drawing)[ROWS / 2];
    assert!(middle.0 && middle.1 > 10 && middle.2 == 0, "{in_drawing:?}");
    assert_eq!(in_drawing, rows(PictureLanguage::Regis, RED_LEFT, None).unwrap().0);
}

/// TODO-6813: a drawing given a paint rate (`docs/regis-v2.md`, "Paint
/// rate") is drawn finished, the picture painted at once.
#[test]
fn the_fallback_draws_a_gradually_painted_drawing_finished() {
    let slow = b"S(M1(H120L50S100))D(B1)W(I1)P[0,0]F(V[399,0][399,479][0,479][0,0])D(E)D(S1(T1),C)";
    assert_eq!(rows(PictureLanguage::Regis, slow, None).unwrap().0, rows(PictureLanguage::Regis, RED_LEFT, None).unwrap().0);
}

/// Renderer parity PAR-25 with ANIM-11 (TODO-6813's check of NAPLPS): the
/// fallback composes a NAPLPS picture's shown objects over its background,
/// as the browser does, the object's rectangle in raster dots: the display
/// area, 480 by 460 from line 20 (RAS-4, RAS-11). The stream only defines macro 0x40, a white
/// rectangle over the left half (DEF MACRO, ANSI X3.110-1983 section
/// 5.3.4.1, which does not draw it), so the background stays black and the
/// white half comes from a macro object showing it. The macro's body
/// starts with SO, since an object's macro runs from the state a terminal
/// powers up in (ANIM-11), with the text set in.
#[test]
fn the_fallback_composes_a_naplps_pictures_shown_objects() {
    /// DEF MACRO and END, the C1 codes 0x80 and 0x85 (section 5.3.4.1).
    const DEF_MACRO: u8 = 0x80;
    /// See [`DEF_MACRO`].
    const END: u8 = 0x85;
    /// The macro's code.
    const MACRO: u8 = 0x40;
    let pair = |x: u16, y: u16| {
        let byte = |shift: u16| 0x40 | ((((x >> shift) & 7) as u8) << 3) | (((y >> shift) & 7) as u8);
        [byte(6), byte(3), byte(0)]
    };
    let mut stream = std::vec![DEF_MACRO, MACRO, 0x0E, 0x3E, 0x40 | (7 << 2), 0x24];
    stream.extend(pair(0, 0));
    stream.push(0x31);
    stream.extend(pair(128, 191));
    stream.push(END);
    let object: crate::protocol::NaplpsObject = serde_json::from_value(serde_json::json!({
        "number": 1, "kind": "macro", "shown": true, "detectable": false, "layer": 1, "order": 0,
        "x": 0, "y": 20, "width": 480, "height": 460, "origin_x": 0, "origin_y": 20, "frames": [MACRO]}))
    .unwrap();
    let bare = rows(PictureLanguage::Naplps, &stream, None).unwrap().0;
    assert!(!bare.iter().any(|row| row.contains(&FULL_BLOCK)), "the macro is only defined: {bare:?}");
    let drawn = rows(PictureLanguage::Naplps, &stream, Some(&[object])).unwrap().0;
    let middle = drawn[ROWS / 2];
    let (left, right) = middle.split_at(middle.len() / 2);
    assert!(middle.contains(&WHITE_GRAPHICS), "{drawn:?}");
    assert!(left.iter().copied().filter(|code| *code == FULL_BLOCK).map(usize::from).count() > 5, "{drawn:?}");
    assert!(!right.contains(&FULL_BLOCK), "{drawn:?}");
}

/// SO, then POINT SET at the origin and RECTANGLE (FILLED) over the whole
/// unit screen (ANSI X3.110-1983 sections 5.3.3.1 and 5.3.3.4), again and
/// again while they fit `length` bytes.
pub(crate) fn whole_screen_fills(length: usize) -> std::vec::Vec<u8> {
    /// POINT SET (ABSOLUTE) and RECTANGLE (FILLED), section 5.3.3.
    const POINT_SET: u8 = 0x24;
    const RECTANGLE_FILLED: u8 = 0x31;
    /// One fill: two opcodes and two three-byte coordinate pairs.
    const FILL_BYTES: usize = 8;
    let mut naplps = std::vec![0x0E];
    while naplps.len().saturating_add(FILL_BYTES) <= length {
        naplps.push(POINT_SET);
        naplps.extend(naplps_pair(0, 0));
        naplps.push(RECTANGLE_FILLED);
        naplps.extend(naplps_pair(255, 191));
    }
    naplps
}

/// A NAPLPS operand pair of 256ths, three bytes (ANSI X3.110-1983 section
/// 5.3.2.2.3), as the test above writes it.
fn naplps_pair(x: u16, y: u16) -> [u8; 3] {
    let byte = |shift: u16| 0x40 | ((((x >> shift) & 7) as u8) << 3) | (((y >> shift) & 7) as u8);
    [byte(6), byte(3), byte(0)]
}

/// TODO-6839: the time of one fallback drawing of the costliest streams
/// this test knows, against `MOST_FALLBACK_RENDERS_PER_SECOND` and
/// `MOST_FALLBACK_BYTES_PER_SECOND`. Ignored, since it times: run it in a
/// release build, `cargo test --release -p teletext-runtime --lib
/// fallback::tests::the_costliest_fallback_drawings_are_timed -- --ignored
/// --nocapture`.
#[test]
#[ignore = "a benchmark: run by hand in a release build, with --ignored --nocapture"]
fn the_costliest_fallback_drawings_are_timed() {
    use crate::kept::MOST_KEPT_STREAM_BYTES;
    use std::time::Instant;
    /// A whole-screen filled polygon (manual chapter 11).
    const REGIS_FILL: &[u8] = b"P[0,0]F(V[799,0][799,479][0,479][0,0])";
    /// Numbered drawings of whole-screen fills whose composition the
    /// renderer's `MAX_REBUILD_WORK` still allows (measured: 150 fail).
    const REGIS_DRAWINGS: u16 = 100;
    let mut regis = std::vec::Vec::new();
    while regis.len() + REGIS_FILL.len() < MOST_KEPT_STREAM_BYTES / 2 {
        regis.extend_from_slice(REGIS_FILL);
    }
    for number in 1..=REGIS_DRAWINGS {
        regis.extend_from_slice(std::format!("D(B{number})").as_bytes());
        regis.extend_from_slice(REGIS_FILL);
        regis.extend_from_slice(b"D(E)");
    }
    regis.extend_from_slice(b"D(C)");
    while regis.len() + REGIS_FILL.len() <= MOST_KEPT_STREAM_BYTES {
        regis.extend_from_slice(REGIS_FILL);
    }
    let start = Instant::now();
    rows(PictureLanguage::Regis, &regis, None).unwrap();
    std::println!("ReGIS: {} bytes of whole-screen fills and {REGIS_DRAWINGS} numbered drawings: {:?} a drawing", regis.len(), start.elapsed());
    // SO, then POINT SET at the origin and RECTANGLE (FILLED) over the
    // whole unit screen (sections 5.3.3.1 and 5.3.3.4), again and again to
    // the longest kept stream: the renderer's work allowance (DEC-6851)
    // stops it, and the budget counts it by the work it spent (TODO-6845).
    let naplps = whole_screen_fills(MOST_KEPT_STREAM_BYTES);
    let start = Instant::now();
    let (_, work) = rows(PictureLanguage::Naplps, &naplps, None).unwrap();
    let elapsed = start.elapsed();
    let work = work.unwrap();
    std::println!(
        "NAPLPS: {} bytes of whole-screen fills: {elapsed:?} a drawing, {work} work units, counted as {} bytes of MOST_FALLBACK_BYTES_PER_SECOND",
        naplps.len(),
        work.div_ceil(crate::kept::NAPLPS_WORK_PER_FALLBACK_BYTE)
    );
}

/// SO, a six-byte DOMAIN (RAS-6), SELECT COLOR entry 7, then a filled
/// RECTANGLE over exactly the raster dots `x` to `x + width - 1` and lines
/// `y` to `y + height - 1`, written as the kits' `rect` writes it: its near
/// corner at the dot (x, y + height - 1), its displacement to the dot
/// (x + width - 1, y) (ANSI X3.110-1983 sections 5.3.3.1 and 5.3.3.4).
fn naplps_rectangle(x: i64, y: i64, width: i64, height: i64) -> std::vec::Vec<u8> {
    use naplps::{Rounding, dot_steps, multi_value_operand};
    const BYTES: u8 = 6;
    let mut stream = std::vec![0x0E, 0x21, 0x54];
    multi_value_operand(0, 0, BYTES, &mut stream);
    stream.extend_from_slice(&[0x3E, 0x40 | (7 << 2), 0x24]);
    let corner = dot_steps(x, y.saturating_add(height).saturating_sub(1), 1, BYTES, Rounding::Nearest);
    let far = dot_steps(x.saturating_add(width).saturating_sub(1), y, 1, BYTES, Rounding::Nearest);
    multi_value_operand(corner.0, corner.1, BYTES, &mut stream);
    stream.push(0x31);
    multi_value_operand(far.0.saturating_sub(corner.0), far.1.saturating_sub(corner.1), BYTES, &mut stream);
    stream
}

/// RAS-11, PAR-25: the NAPLPS canvas is the receiver's, 480 by 460, and
/// its lines are exactly the fallback's rows 1 to 23: a rectangle over the
/// left half of teletext row r's 20 lines gives full blocks on row r and
/// on no other, for the first row, a middle one and the last, and the
/// mosaic row is the one a rectangle over row r's cells gives in the same
/// place of every row.
#[test]
fn a_naplps_pictures_rows_are_the_fallbacks_rows() {
    let mut shown = std::vec::Vec::new();
    for row in [1_usize, 12, 23] {
        let top = i64::try_from(row * 20).unwrap();
        let drawn = rows(PictureLanguage::Naplps, &naplps_rectangle(0, top, 240, 20), None).unwrap().0;
        let full: std::vec::Vec<usize> =
            drawn.iter().enumerate().filter(|(_, codes)| codes.contains(&FULL_BLOCK)).map(|(index, _)| index + usize::from(super::FIRST_ROW)).collect();
        assert_eq!(full, [row], "row {row}: {drawn:?}");
        shown.push(drawn[row - 1]);
    }
    assert!(shown.iter().all(|codes| *codes == shown[0]), "{shown:?}");
    let (left, right) = shown[0].split_at(shown[0].len() / 2);
    assert!(left.iter().copied().filter(|code| *code == FULL_BLOCK).map(usize::from).count() >= 15, "{shown:?}");
    assert!(!right.contains(&FULL_BLOCK), "{shown:?}");
}

/// RAS-11, PAR-26: a NAPLPS picture's public PNG is the 480 by 500 raster,
/// its canvas on lines 20 to 479, rows 0 and 24 in the screen colour: a
/// picture filled white over its whole display area has its first white
/// line at 20 and its last at 479.
#[test]
fn a_naplps_pictures_public_png_draws_the_canvas_on_lines_20_to_479() {
    let drawn = super::draw(PictureLanguage::Naplps, &naplps_rectangle(0, 20, 480, 460), None).unwrap();
    assert_eq!((drawn.width, drawn.height), (480, 460));
    let (height, indices) = super::placed(&drawn);
    assert_eq!((height, indices.len()), (500, 480 * 500));
    let border = drawn.border.unwrap();
    let white = drawn.addresses[0];
    assert_ne!(white, border);
    let lines: std::vec::Vec<usize> = (0..height).filter(|line| indices[line * 480..(line + 1) * 480].iter().all(|&index| index == white)).collect();
    assert_eq!((lines.first(), lines.last(), lines.len()), (Some(&20), Some(&479), 460));
    assert!(indices[..20 * 480].iter().chain(&indices[480 * 480..]).all(|&index| index == border), "rows 0 and 24 in the screen colour");
    let (png, png_width, png_height) = super::png(&drawn).unwrap();
    assert!(png.starts_with(b"\x89PNG"), "a PNG");
    assert_eq!((png_width, png_height), (480, 500));
}
