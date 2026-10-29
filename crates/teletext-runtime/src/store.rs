//! What a channel keeps through a restart (requirement 5.2): each shared
//! page's sub-codes and cursor, each of its sub-pages' packets, and each
//! application's declared pages, as
//! records the host keeps under a [`StoreKey`] and gives back with
//! [`crate::Channel::load`]. The runtime only makes and reads the bytes;
//! where they are kept is the host's: files natively, a Durable Object's
//! storage on Cloudflare.
//!
//! A record is read whole or refused: what the host gives back may have
//! been cut short, altered, or written by another version.
//!
//! A sub-page is a record of its own, so that a record stays small at the
//! standard's 8,190 sub-pages a page (requirement 3.1) and a change to one
//! sub-page writes one sub-page.

use alloc::string::String;
use alloc::vec::Vec;

use neoteletext_core::consts::PACKET_BYTES;
use neoteletext_core::limits::{MAX_SUBPAGES, USABLE_PAGE_NUMBERS, is_sub_code};
use neoteletext_formats::page::CursorLine;

use crate::application::PageEntry;
use crate::held::Held;
use crate::remote::{TITLE_LIMIT, is_page_number, is_title};

/// The records' format. A record of another version is refused, never
/// read in part, so a change of layout raises it. Version 1 held a page's
/// sub-pages in its record.
pub const FORMAT_VERSION: u8 = 2;
/// A shared page's record: its sub-codes, in order, and its cursor.
const KIND_PAGE: u8 = 1;
/// An application's declared pages.
const KIND_PAGES: u8 = 2;
/// One sub-page of a shared page: its packets.
const KIND_SUBPAGE: u8 = 3;
/// The most packets one sub-page encodes to: X/0 to X/25 once each, and
/// X/26, X/27, X/28 and M/29 with each of their 16 designation codes
/// (EN 300 706 clause 7.1 and table 1).
const MOST_PACKETS_PER_SUBPAGE: usize = 26 + 4 * 16;
/// The most bytes one sub-page encodes to.
pub const MOST_SUBPAGE_BYTES: usize = MOST_PACKETS_PER_SUBPAGE * PACKET_BYTES;
/// A page's record: version, kind, page number, whether it has a cursor,
/// the cursor's row, column, a reserved byte (once the echo's width,
/// withdrawn by the engine's E.134) and flags, and the count of
/// sub-codes, so a record cut short at an entry's end is not taken for a
/// shorter page.
const PAGE_HEADER_BYTES: usize = 11;
/// A sub-code's bytes in a page's record.
const SUBCODE_BYTES: usize = 2;
/// A sub-page's record before its packets: version, kind, page number,
/// sub-code and the packets' length.
const SUBPAGE_HEADER_BYTES: usize = 10;
/// A name's length and its UTF-8, of at most four bytes a character.
const MOST_NAME_BYTES: usize = 2 + 4 * TITLE_LIMIT;
/// The declared pages' record: version, kind, count, then a number, a
/// title and a section for each page.
const MOST_PAGES_RECORD_BYTES: usize = 4 + USABLE_PAGE_NUMBERS * (2 + 2 * MOST_NAME_BYTES);
/// A record's largest size, of any kind: the declared pages' record,
/// about 2 MB at the standard's 2,022 pages, is the largest.
pub const MOST_RECORD_BYTES: usize =
    max(MOST_PAGES_RECORD_BYTES, max(PAGE_HEADER_BYTES + MAX_SUBPAGES * SUBCODE_BYTES, SUBPAGE_HEADER_BYTES + MOST_SUBPAGE_BYTES));
/// No section after a page's title.
const NO_SECTION: u16 = u16::MAX;
/// The most applications a key names: more than a channel ever runs.
const MOST_APPLICATIONS: usize = 1_000;

const fn max(first: usize, second: usize) -> usize {
    if first > second { first } else { second }
}

/// Where a record is kept.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum StoreKey {
    /// A shared page.
    Page(u16),
    /// One sub-page of a shared page.
    SubPage {
        /// The page.
        page: u16,
        /// The sub-page's sub-code.
        subcode: u16,
    },
    /// The pages an application declared, by its place in the channel.
    Pages(usize),
}

impl StoreKey {
    /// The key as a name: `page-1AB`, `page-1AB-0001` or `pages-0`,
    /// letters, digits and `-` alone, so a host may use it as a file's
    /// name.
    #[must_use]
    pub fn name(&self) -> String {
        match self {
            Self::Page(page) => alloc::format!("page-{page:03X}"),
            Self::SubPage { page, subcode } => alloc::format!("page-{page:03X}-{subcode:04X}"),
            Self::Pages(application) => alloc::format!("pages-{application}"),
        }
    }

    /// The key a name names, or none.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        if let Some(digits) = name.strip_prefix("page-") {
            let (page, subcode) = match digits.split_once('-') {
                Some((page, subcode)) => (page, Some(subcode)),
                None => (digits, None),
            };
            let page = hexadecimal(page, 3).filter(|&page| is_page_number(page))?;
            return match subcode {
                None => Some(Self::Page(page)),
                Some(digits) => hexadecimal(digits, 4).filter(|&subcode| is_sub_code(subcode)).map(|subcode| Self::SubPage { page, subcode }),
            };
        }
        let digits = name.strip_prefix("pages-")?;
        let decimal = !digits.is_empty() && digits.bytes().all(|digit| digit.is_ascii_digit()) && (digits == "0" || !digits.starts_with('0'));
        digits.parse().ok().filter(|&application: &usize| decimal && application < MOST_APPLICATIONS).map(Self::Pages)
    }
}

/// `digits`, exactly `count` capital hexadecimal digits, as a number.
fn hexadecimal(digits: &str, count: usize) -> Option<u16> {
    let capital = digits.len() == count && digits.bytes().all(|digit| digit.is_ascii_digit() || (b'A'..=b'F').contains(&digit));
    if capital { u16::from_str_radix(digits, 16).ok() } else { None }
}

/// Why a record was refused.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StoreError {
    /// Written by another format version.
    Version,
    /// Not the kind of record its key names.
    Kind,
    /// Shorter than its contents say.
    Truncated,
    /// Its contents break a rule a publication or a declaration keeps.
    Invalid,
    /// It names an application the channel does not have.
    NoApplication,
}

/// A shared page's record: its sub-codes, in order, and its cursor. Its
/// sub-pages are records of their own ([`subpage_record`]).
pub(crate) fn page_record(page: u16, held: &Held) -> Vec<u8> {
    let mut record = Vec::with_capacity(PAGE_HEADER_BYTES.saturating_add(held.parts.len().saturating_mul(SUBCODE_BYTES)));
    record.extend_from_slice(&[FORMAT_VERSION, KIND_PAGE]);
    record.extend_from_slice(&page.to_be_bytes());
    match held.cursor {
        Some(cursor) => record.extend_from_slice(&[1, cursor.row, cursor.column, 0, cursor.flags]),
        None => record.extend_from_slice(&[0; 5]),
    }
    // A page holds at most MAX_SUBPAGES, which fits.
    record.extend_from_slice(&u16::try_from(held.parts.len()).unwrap_or(u16::MAX).to_be_bytes());
    for subcode in held.subcodes() {
        record.extend_from_slice(&subcode.to_be_bytes());
    }
    record
}

/// A shared page as its record gives it: its sub-codes in order and its
/// cursor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct KeptPage {
    pub subcodes: Vec<u16>,
    pub cursor: Option<CursorLine>,
}

/// Reads a shared page's record for `page`.
pub(crate) fn read_page(page: u16, record: &[u8]) -> Result<KeptPage, StoreError> {
    let mut reader = Reader::of(record, KIND_PAGE)?;
    if reader.u16()? != page {
        return Err(StoreError::Invalid);
    }
    let [has_cursor, row, column, _reserved, flags] = reader.array()?;
    let cursor = match has_cursor {
        0 => None,
        1 => Some(CursorLine { row, column, flags, acked: 0, line: 0 }),
        _ => return Err(StoreError::Invalid),
    };
    let count = usize::from(reader.u16()?);
    if count == 0 || count > MAX_SUBPAGES {
        return Err(StoreError::Invalid);
    }
    let mut subcodes: Vec<u16> = Vec::with_capacity(count);
    for _ in 0..count {
        let subcode = reader.u16()?;
        if !is_sub_code(subcode) || subcodes.contains(&subcode) {
            return Err(StoreError::Invalid);
        }
        subcodes.push(subcode);
    }
    if !reader.rest().is_empty() {
        return Err(StoreError::Invalid);
    }
    Ok(KeptPage { subcodes, cursor })
}

/// One sub-page's record: its packets.
pub(crate) fn subpage_record(page: u16, subcode: u16, packets: &[u8]) -> Vec<u8> {
    let mut record = Vec::with_capacity(SUBPAGE_HEADER_BYTES.saturating_add(packets.len()));
    record.extend_from_slice(&[FORMAT_VERSION, KIND_SUBPAGE]);
    record.extend_from_slice(&page.to_be_bytes());
    record.extend_from_slice(&subcode.to_be_bytes());
    record.extend_from_slice(&u32::try_from(packets.len()).unwrap_or(u32::MAX).to_be_bytes());
    record.extend_from_slice(packets);
    record
}

/// Reads sub-page `subcode` of `page` from its record: its packets.
pub(crate) fn read_subpage(page: u16, subcode: u16, record: &[u8]) -> Result<&[u8], StoreError> {
    let mut reader = Reader::of(record, KIND_SUBPAGE)?;
    if reader.u16()? != page || reader.u16()? != subcode {
        return Err(StoreError::Invalid);
    }
    let length = usize::try_from(u32::from_be_bytes(reader.array()?)).map_err(|_| StoreError::Invalid)?;
    if length == 0 || !length.is_multiple_of(PACKET_BYTES) || length > MOST_SUBPAGE_BYTES {
        return Err(StoreError::Invalid);
    }
    let packets = reader.take(length)?;
    if !reader.rest().is_empty() {
        return Err(StoreError::Invalid);
    }
    Ok(packets)
}

/// An application's declared pages' record.
pub(crate) fn pages_record(pages: &[PageEntry]) -> Vec<u8> {
    let mut record = alloc::vec![FORMAT_VERSION, KIND_PAGES];
    record.extend_from_slice(&u16::try_from(pages.len()).unwrap_or(u16::MAX).to_be_bytes());
    for entry in pages.iter().take(usize::from(u16::MAX)) {
        record.extend_from_slice(&entry.number.to_be_bytes());
        put_text(&mut record, &entry.title);
        match &entry.section {
            Some(section) => put_text(&mut record, section),
            None => record.extend_from_slice(&NO_SECTION.to_be_bytes()),
        }
    }
    record
}

/// Reads an application's declared pages' record, with the checks a
/// declaration has (`remote::declared_pages`).
pub(crate) fn read_pages(record: &[u8]) -> Result<Vec<PageEntry>, StoreError> {
    let mut reader = Reader::of(record, KIND_PAGES)?;
    let count = usize::from(reader.u16()?);
    if count > USABLE_PAGE_NUMBERS {
        return Err(StoreError::Invalid);
    }
    let mut pages: Vec<PageEntry> = Vec::with_capacity(count);
    for _ in 0..count {
        let number = reader.u16()?;
        let title = reader.text()?.ok_or(StoreError::Invalid)?;
        let section = reader.text()?;
        if !is_page_number(number) || pages.iter().any(|entry| entry.number == number) {
            return Err(StoreError::Invalid);
        }
        let entry = PageEntry::new(number, title);
        pages.push(match section {
            Some(name) if !name.is_empty() => entry.starting(name),
            _ => entry,
        });
    }
    if !reader.rest().is_empty() {
        return Err(StoreError::Invalid);
    }
    Ok(pages)
}

/// A name, as its length in bytes and its UTF-8.
fn put_text(record: &mut Vec<u8>, text: &str) {
    // A title is at most 120 characters (`remote::TITLE_LIMIT`), so its
    // length always fits below `NO_SECTION`.
    record.extend_from_slice(&u16::try_from(text.len()).unwrap_or(0).to_be_bytes());
    record.extend_from_slice(text.as_bytes());
}

/// Reads a record from its start, refusing what runs short.
struct Reader<'a> {
    rest: &'a [u8],
}

impl<'a> Reader<'a> {
    /// A reader past the record's version and kind, which must be ours.
    fn of(record: &'a [u8], kind: u8) -> Result<Self, StoreError> {
        let mut reader = Self { rest: record };
        let [version, found] = reader.array()?;
        if version != FORMAT_VERSION {
            return Err(StoreError::Version);
        }
        if found != kind {
            return Err(StoreError::Kind);
        }
        Ok(reader)
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8], StoreError> {
        let (taken, rest) = self.rest.split_at_checked(count).ok_or(StoreError::Truncated)?;
        self.rest = rest;
        Ok(taken)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], StoreError> {
        self.take(N)?.try_into().map_err(|_| StoreError::Truncated)
    }

    fn u16(&mut self) -> Result<u16, StoreError> {
        Ok(u16::from_be_bytes(self.array()?))
    }

    /// A name, or none where none was written; a name must be one a
    /// declaration allows.
    fn text(&mut self) -> Result<Option<&'a str>, StoreError> {
        let length = self.u16()?;
        if length == NO_SECTION {
            return Ok(None);
        }
        let text = core::str::from_utf8(self.take(usize::from(length))?).map_err(|_| StoreError::Invalid)?;
        if !is_title(text) {
            return Err(StoreError::Invalid);
        }
        Ok(Some(text))
    }

    const fn rest(&self) -> &'a [u8] {
        self.rest
    }
}

#[cfg(test)]
mod tests;
