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
    let drawn = rows(PictureLanguage::Regis, stream, None).unwrap();
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
    let drawn = rows(PictureLanguage::Naplps, &stream, None).unwrap();
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
    let in_drawing = rows(PictureLanguage::Regis, RED_LEFT_IN_A_DRAWING, None).unwrap();
    let middle = shape(&in_drawing)[ROWS / 2];
    assert!(middle.0 && middle.1 > 10 && middle.2 == 0, "{in_drawing:?}");
    assert_eq!(in_drawing, rows(PictureLanguage::Regis, RED_LEFT, None).unwrap());
}

/// TODO-6813: a drawing given a paint rate (`docs/regis-v2.md`, "Paint
/// rate") is drawn finished, the picture painted at once.
#[test]
fn the_fallback_draws_a_gradually_painted_drawing_finished() {
    let slow = b"S(M1(H120L50S100))D(B1)W(I1)P[0,0]F(V[399,0][399,479][0,479][0,0])D(E)D(S1(T1),C)";
    assert_eq!(rows(PictureLanguage::Regis, slow, None).unwrap(), rows(PictureLanguage::Regis, RED_LEFT, None).unwrap());
}

/// Renderer parity PAR-25 with ANIM-11 (TODO-6813's check of NAPLPS): the
/// fallback composes a NAPLPS picture's shown objects over its background,
/// as the browser does. The stream only defines macro 0x40, a white
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
        "x": 0, "y": 0, "width": 480, "height": 462, "origin_x": 0, "origin_y": 0, "frames": [MACRO]}))
    .unwrap();
    let bare = rows(PictureLanguage::Naplps, &stream, None).unwrap();
    assert!(!bare.iter().any(|row| row.contains(&FULL_BLOCK)), "the macro is only defined: {bare:?}");
    let drawn = rows(PictureLanguage::Naplps, &stream, Some(&[object])).unwrap();
    let middle = drawn[ROWS / 2];
    let (left, right) = middle.split_at(middle.len() / 2);
    assert!(middle.contains(&WHITE_GRAPHICS), "{drawn:?}");
    assert!(left.iter().copied().filter(|code| *code == FULL_BLOCK).map(usize::from).count() > 5, "{drawn:?}");
    assert!(!right.contains(&FULL_BLOCK), "{drawn:?}");
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
    /// NAPLPS whole-screen fills drawn to time one, the stream's cost
    /// being linear in them.
    const NAPLPS_FILLS: usize = 1_000;
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
    // whole unit screen (sections 5.3.3.1 and 5.3.3.4), again and again.
    let mut naplps = std::vec![0x0E];
    for _ in 0..NAPLPS_FILLS {
        naplps.push(0x24);
        naplps.extend(naplps_pair(0, 0));
        naplps.push(0x31);
        naplps.extend(naplps_pair(255, 191));
    }
    let start = Instant::now();
    rows(PictureLanguage::Naplps, &naplps, None).unwrap();
    let elapsed = start.elapsed();
    let per_byte = elapsed / u32::try_from(naplps.len()).unwrap();
    std::println!(
        "NAPLPS: {NAPLPS_FILLS} whole-screen fills, {} bytes: {elapsed:?}, so {per_byte:?} a byte and about {:?} for {MOST_KEPT_STREAM_BYTES} bytes",
        naplps.len(),
        per_byte * u32::try_from(MOST_KEPT_STREAM_BYTES).unwrap()
    );
}
