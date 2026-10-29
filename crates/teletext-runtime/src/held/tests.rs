use super::*;
use neoteletext_core::types::SubCode;

/// A sub-page with sub-code `subcode` and `text` on row 1.
fn subpage(subcode: u16, text: &str) -> PageFile {
    let mut page = PageFile { subcode: SubCode::new(subcode), ..PageFile::default() };
    for (cell, byte) in page.rows[1].iter_mut().zip(text.bytes()) {
        *cell = byte;
    }
    page.rows_present |= 1 << 1;
    page
}

fn order(held: &Held) -> Vec<u16> {
    held.subcodes().collect()
}

/// Each sub-page lies in the frame where its part says, in the page's
/// order, the frame being the sub-pages end to end.
#[test]
fn a_page_is_its_subpages_end_to_end() {
    let held = Held::of(&alloc::vec![subpage(1, "one"), subpage(2, "two"), subpage(3, "three")]);
    assert_eq!(order(&held), [1, 2, 3]);
    let joined: Vec<u8> = held.subpages().flat_map(|(_, bytes)| bytes.to_vec()).collect();
    assert_eq!(joined, held.frame.to_vec());
    assert_eq!(held.first(), held.subpage(1).unwrap());
    assert_eq!(held.subpage(2).unwrap(), Held::of(&alloc::vec![subpage(2, "two")]).frame.to_vec());
    assert!(held.subpage(4).is_none());
}

/// A put replaces a sub-page where it stands and places a new one before
/// the first with a higher sub-code, keeping the rest unchanged.
#[test]
fn a_put_replaces_and_places_and_keeps_the_rest() {
    let held = Held::of(&alloc::vec![subpage(1, "one"), subpage(3, "three"), subpage(5, "five")]);
    let put = held.with(&alloc::vec![subpage(3, "THREE"), subpage(4, "four"), subpage(0x0100, "last")]);
    assert_eq!(order(&put), [1, 3, 4, 5, 0x0100]);
    assert_eq!(put.subpage(1), held.subpage(1), "kept as it was");
    assert_eq!(put.subpage(3).unwrap(), Held::of(&alloc::vec![subpage(3, "THREE")]).frame.to_vec());
    assert_eq!(order(&held.with(&alloc::vec![subpage(0, "first")])), [0, 1, 3, 5]);
}

/// A remove ends the sub-pages it names; ending every one leaves no page.
#[test]
fn a_remove_ends_what_it_names() {
    let held = Held::of(&alloc::vec![subpage(1, "one"), subpage(2, "two"), subpage(3, "three")]);
    let left = held.without(&[2, 9]).unwrap();
    assert_eq!(order(&left), [1, 3]);
    assert_eq!(left.subpage(3), held.subpage(3));
    assert!(held.without(&[1, 2, 3]).is_none());
}

/// The cursor is the first sub-page's, whichever publication gave it.
#[test]
fn the_cursor_is_the_first_subpages() {
    let mut with_cursor = subpage(1, "one");
    with_cursor.cursor = Some(CursorLine { row: 4, column: 5, flags: 0, acked: 0, line: 0 });
    let held = Held::of(&alloc::vec![with_cursor, subpage(2, "two")]);
    assert_eq!(held.cursor.map(|cursor| cursor.row), Some(4));
    assert_eq!(held.with(&alloc::vec![subpage(2, "TWO")]).cursor.map(|cursor| cursor.row), Some(4), "the first was not put");
    assert!(held.with(&alloc::vec![subpage(1, "ONE")]).cursor.is_none(), "the first was put, without one");
}
