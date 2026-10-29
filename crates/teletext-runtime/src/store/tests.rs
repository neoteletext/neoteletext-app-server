use super::*;

fn held() -> Held {
    let packets: Vec<u8> = (0..2 * PACKET_BYTES).map(|byte| u8::try_from(byte % 256).unwrap()).collect();
    let (first, second) = packets.split_at(PACKET_BYTES);
    Held::of_encoded([(0x0001, first), (0x3F7E, second)].into_iter(), Some(CursorLine { row: 5, column: 6, flags: 8, acked: 0, line: 0 }))
}

/// A page's record reads back as the page's sub-codes, in order, and its
/// cursor; each sub-page's record as its packets.
#[test]
fn a_page_record_and_its_subpages_read_back() {
    let record = page_record(0x1AB, &held());
    let read = read_page(0x1AB, &record).unwrap();
    assert_eq!(read.subcodes, [0x0001, 0x3F7E]);
    assert_eq!(read.cursor.map(|cursor| (cursor.row, cursor.column, cursor.flags)), Some((5, 6, 8)));
    let without = Held { cursor: None, ..held() };
    assert!(read_page(0x1AB, &page_record(0x1AB, &without)).unwrap().cursor.is_none());
    for (subcode, packets) in held().subpages() {
        assert_eq!(read_subpage(0x1AB, subcode, &subpage_record(0x1AB, subcode, packets)).unwrap(), packets);
    }
}

/// A page of the standard's 8,190 sub-pages (requirement 3.1) is kept in
/// a record within the bound a host reads.
#[test]
fn a_page_of_the_most_subpages_is_kept() {
    let packet = [0_u8; PACKET_BYTES];
    let subcodes: Vec<u16> = (0..=neoteletext_core::limits::LAST_SUBPAGE_CODE).filter(|&subcode| is_sub_code(subcode) && subcode != 0).collect();
    assert_eq!(subcodes.len(), MAX_SUBPAGES);
    let held = Held::of_encoded(subcodes.iter().map(|&subcode| (subcode, &packet[..])), None);
    let record = page_record(0x100, &held);
    assert!(record.len() <= MOST_RECORD_BYTES);
    assert_eq!(read_page(0x100, &record).unwrap().subcodes, subcodes);
}

/// A record of another version, of another kind, for another page or
/// sub-page, cut short at any length, naming a sub-code twice or one
/// outside the thirteen bits, or with packets that are not whole, is
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
    let mut trailing = record.clone();
    trailing.push(0);
    assert_eq!(read_page(0x1AB, &trailing).err(), Some(StoreError::Invalid));
    let mut bad_cursor = record.clone();
    bad_cursor[4] = 2;
    assert_eq!(read_page(0x1AB, &bad_cursor).err(), Some(StoreError::Invalid));
    let mut twice = record.clone();
    let last = twice.len() - 2;
    twice[last..].copy_from_slice(&0x0001_u16.to_be_bytes());
    assert_eq!(read_page(0x1AB, &twice).err(), Some(StoreError::Invalid));
    let mut outside = record;
    outside[last..].copy_from_slice(&0x4000_u16.to_be_bytes());
    assert_eq!(read_page(0x1AB, &outside).err(), Some(StoreError::Invalid));

    let packets = [7_u8; PACKET_BYTES];
    let subpage = subpage_record(0x1AB, 0x0001, &packets);
    assert_eq!(read_subpage(0x1AB, 0x0002, &subpage).err(), Some(StoreError::Invalid));
    assert_eq!(read_subpage(0x1AC, 0x0001, &subpage).err(), Some(StoreError::Invalid));
    assert_eq!(read_subpage(0x1AB, 0x0001, &record_of_kind(&subpage, KIND_PAGE)).err(), Some(StoreError::Kind));
    for length in 0..subpage.len() {
        assert!(read_subpage(0x1AB, 0x0001, &subpage[..length]).is_err(), "cut at {length}");
    }
    assert_eq!(read_subpage(0x1AB, 0x0001, &subpage_record(0x1AB, 0x0001, &packets[1..])).err(), Some(StoreError::Invalid));
}

fn record_of_kind(record: &[u8], kind: u8) -> Vec<u8> {
    let mut other = record.to_vec();
    other[1] = kind;
    other
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
    let sub_page = |page, subcode| StoreKey::SubPage { page, subcode };
    for key in [StoreKey::Page(0x100), StoreKey::Page(0x8FF), sub_page(0x100, 0x0000), sub_page(0x8FF, 0x3F7F), StoreKey::Pages(0), StoreKey::Pages(12)] {
        assert_eq!(StoreKey::parse(&key.name()), Some(key));
    }
    assert_eq!(StoreKey::Page(0x1AB).name(), "page-1AB");
    assert_eq!(sub_page(0x1AB, 0x0A01).name(), "page-1AB-0A01");
    for name in [
        "page-1ab",
        "page-0FF",
        "page-900",
        "page-10",
        "page-1AB-",
        "page-1AB-001",
        "page-1AB-00a1",
        "page-1AB-4000",
        "page-1AB-0001-0001",
        "pages-",
        "pages-01",
        "pages-1000",
        "pages--1",
        "page-1AB.tmp",
        "figures.json",
    ] {
        assert_eq!(StoreKey::parse(name), None, "{name}");
    }
}
