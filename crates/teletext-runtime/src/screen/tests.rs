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
