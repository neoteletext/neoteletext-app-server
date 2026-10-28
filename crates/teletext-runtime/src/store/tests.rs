use super::*;
use alloc::sync::Arc;

fn held() -> Held {
    let frame: Vec<u8> = (0..2 * PACKET_BYTES).map(|byte| u8::try_from(byte % 256).unwrap()).collect();
    Held { frame: Arc::from(frame), cursor: Some(CursorLine { row: 5, column: 6, flags: 8, acked: 0, line: 0 }) }
}

/// A page's record reads back as the page it was made from, cursor and
/// frame.
#[test]
fn a_page_record_reads_back() {
    let record = page_record(0x1AB, &held());
    let read = read_page(0x1AB, &record).unwrap();
    assert_eq!(read.frame, held().frame);
    assert_eq!(read.cursor.map(|cursor| (cursor.row, cursor.column, cursor.flags)), Some((5, 6, 8)));
    let without = Held { cursor: None, ..held() };
    assert!(read_page(0x1AB, &page_record(0x1AB, &without)).unwrap().cursor.is_none());
}

/// A record of another version, of another kind, for another page, cut
/// short at any length, or with a frame that is not whole packets, is
/// refused whole.
#[test]
fn a_page_record_not_whole_is_refused() {
    let record = page_record(0x1AB, &held());
    let mut other_version = record.clone();
    other_version[0] = FORMAT_VERSION + 1;
    assert_eq!(read_page(0x1AB, &other_version).err(), Some(StoreError::Version));
    assert_eq!(read_page(0x1AB, &pages_record(&[])).err(), Some(StoreError::Kind));
    assert_eq!(read_page(0x1AC, &record).err(), Some(StoreError::Invalid));
    for length in 0..record.len() {
        assert!(read_page(0x1AB, &record[..length]).is_err(), "cut at {length}");
    }
    let mut odd = record.clone();
    odd.push(0);
    assert_eq!(read_page(0x1AB, &odd).err(), Some(StoreError::Invalid));
    let mut bad_cursor = record;
    bad_cursor[4] = 2;
    assert_eq!(read_page(0x1AB, &bad_cursor).err(), Some(StoreError::Invalid));
}

/// Declared pages read back with their titles and sections; a record whose
/// entries a declaration would refuse is refused.
#[test]
fn a_pages_record_reads_back_and_keeps_the_declarations_rules() {
    let pages = alloc::vec![PageEntry::new(0x100, "Front"), PageEntry::new(0x200, "Retro").starting("Retrocomputing"), PageEntry::new(0x8FF, "")];
    let record = pages_record(&pages);
    assert_eq!(read_pages(&record).unwrap(), pages);
    for length in 0..record.len() {
        assert!(read_pages(&record[..length]).is_err(), "cut at {length}");
    }
    let mut trailing = record;
    trailing.push(0);
    assert_eq!(read_pages(&trailing).err(), Some(StoreError::Invalid));
    for refused in [
        alloc::vec![PageEntry::new(0x100, "Twice"), PageEntry::new(0x100, "Twice")],
        alloc::vec![PageEntry::new(0x0FF, "Below magazine 1")],
        alloc::vec![PageEntry::new(0x100, "A\ncontrol")],
        alloc::vec![PageEntry::new(0x100, &"x".repeat(121))],
    ] {
        assert_eq!(read_pages(&pages_record(&refused)).err(), Some(StoreError::Invalid), "{refused:?}");
    }
}

/// A key's name is letters, digits and `-`, and names that key alone.
#[test]
fn a_key_is_named_and_read_back() {
    for key in [StoreKey::Page(0x100), StoreKey::Page(0x8FF), StoreKey::Pages(0), StoreKey::Pages(12)] {
        assert_eq!(StoreKey::parse(&key.name()), Some(key));
    }
    assert_eq!(StoreKey::Page(0x1AB).name(), "page-1AB");
    for name in ["page-1ab", "page-0FF", "page-900", "page-10", "pages-", "pages-01", "pages-1000", "pages--1", "page-1AB.tmp", "figures.json"] {
        assert_eq!(StoreKey::parse(name), None, "{name}");
    }
}
