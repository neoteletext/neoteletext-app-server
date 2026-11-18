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
