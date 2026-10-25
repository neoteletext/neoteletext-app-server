use super::*;

/// Sub-page 10 is sent as 0010, as receivers number it; before, as 000A.
#[test]
fn sub_pages_are_numbered_in_decimal_digits() {
    assert_eq!(decimal_subcode(1), 0x0001);
    assert_eq!(decimal_subcode(10), 0x0010);
    assert_eq!(decimal_subcode(79), 0x0079);
    assert_eq!(Screen::new().page(0x101, 12).subcode.value(), 0x0012);
}

/// A page with Fastext links shows row 24, where their prompts are.
#[test]
fn fastext_links_show_row_24() {
    let mut screen = Screen::new();
    assert!(!screen.page(0x100, 0).fastext_row_24);
    screen.fastext([Some(0x101), None, None, None]);
    assert!(screen.page(0x100, 0).fastext_row_24);
}

/// The Index link goes where receivers read it: X/27/0's sixth link, after
/// the four coloured keys and link 4, which is not a viewer link (EN 300 706
/// Annex H; the engine's `X27_INDEX_LINK`).
#[test]
fn the_index_link_is_the_sixth() {
    let mut screen = Screen::new();
    screen.fastext([Some(0x101), Some(0x102), Some(0x103), Some(0x104)]);
    screen.index_link(0x100);
    let page = screen.page(0x200, 0);
    let numbers: alloc::vec::Vec<Option<u16>> =
        page.fastext.iter().map(|link| link.map(|link| (u16::from(link.address.magazine.number()) << 8) | u16::from(link.address.page.0))).collect();
    assert_eq!(numbers, [Some(0x101), Some(0x102), Some(0x103), Some(0x104), None, Some(0x100)]);
}
