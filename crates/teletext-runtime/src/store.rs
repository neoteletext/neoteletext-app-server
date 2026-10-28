//! What a channel keeps through a restart (requirement 5.2): each shared
//! page's frame and cursor, and each application's declared pages, as
//! records the host keeps under a [`StoreKey`] and gives back with
//! [`crate::Channel::load`]. The runtime only makes and reads the bytes;
//! where they are kept is the host's: files natively, a Durable Object's
//! storage on Cloudflare.
//!
//! A record is read whole or refused: what the host gives back may have
//! been cut short, altered, or written by another version.

use alloc::string::String;
use alloc::vec::Vec;

use neoteletext_core::consts::PACKET_BYTES;
use neoteletext_formats::page::CursorLine;

use crate::application::PageEntry;
use crate::channel::Held;
use crate::remote::{MOST_PAGES, MOST_SUBPAGES, is_title};

/// The records' format. A record of another version is refused, never
/// read in part, so a change of layout raises it.
pub const FORMAT_VERSION: u8 = 1;
/// A shared page's record.
const KIND_PAGE: u8 = 1;
/// An application's declared pages.
const KIND_PAGES: u8 = 2;
/// The most packets one sub-page encodes to: X/0 to X/25 once each, and
/// X/26, X/27, X/28 and M/29 with each of their 16 designation codes
/// (EN 300 706 clause 7.1 and table 1).
const MOST_PACKETS_PER_SUBPAGE: usize = 26 + 4 * 16;
/// The largest frame a publication can make, so the largest a page's
/// record may hold.
pub const MOST_FRAME_BYTES: usize = MOST_SUBPAGES * MOST_PACKETS_PER_SUBPAGE * PACKET_BYTES;
/// A record's largest size: a page's header and its largest frame. The
/// declared pages' record is far smaller: [`MOST_PAGES`] entries of a
/// number and two names of at most 480 bytes each.
pub const MOST_RECORD_BYTES: usize = PAGE_HEADER_BYTES + MOST_FRAME_BYTES;
/// A page's record before its frame: version, kind, page number, whether
/// it has a cursor, the cursor's row, column, width and flags, and the
/// frame's length, so a record cut short at a packet's end is not taken
/// for a shorter page.
const PAGE_HEADER_BYTES: usize = 13;
/// No section after a page's title.
const NO_SECTION: u16 = u16::MAX;
/// The page numbers the protocol allows: magazines 1 to 8 (EN 300 706
/// clause 3.1).
const PAGES: core::ops::RangeInclusive<u16> = 0x100..=0x8FF;
/// The most applications a key names: more than a channel ever runs.
const MOST_APPLICATIONS: usize = 1_000;

/// Where a record is kept.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum StoreKey {
    /// A shared page.
    Page(u16),
    /// The pages an application declared, by its place in the channel.
    Pages(usize),
}

impl StoreKey {
    /// The key as a name: `page-1AB` or `pages-0`, letters, digits and `-`
    /// alone, so a host may use it as a file's name.
    #[must_use]
    pub fn name(&self) -> String {
        match self {
            Self::Page(page) => alloc::format!("page-{page:03X}"),
            Self::Pages(application) => alloc::format!("pages-{application}"),
        }
    }

    /// The key a name names, or none.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        if let Some(digits) = name.strip_prefix("page-") {
            let hexadecimal = digits.len() == 3 && digits.bytes().all(|digit| digit.is_ascii_digit() || (b'A'..=b'F').contains(&digit));
            return u16::from_str_radix(digits, 16).ok().filter(|page| hexadecimal && PAGES.contains(page)).map(Self::Page);
        }
        let digits = name.strip_prefix("pages-")?;
        let decimal = !digits.is_empty() && digits.bytes().all(|digit| digit.is_ascii_digit()) && (digits == "0" || !digits.starts_with('0'));
        digits.parse().ok().filter(|&application: &usize| decimal && application < MOST_APPLICATIONS).map(Self::Pages)
    }
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

/// A shared page's record.
pub(crate) fn page_record(page: u16, held: &Held) -> Vec<u8> {
    let mut record = Vec::with_capacity(PAGE_HEADER_BYTES.saturating_add(held.frame.len()));
    record.extend_from_slice(&[FORMAT_VERSION, KIND_PAGE]);
    record.extend_from_slice(&page.to_be_bytes());
    match held.cursor {
        Some(cursor) => record.extend_from_slice(&[1, cursor.row, cursor.column, cursor.width, cursor.flags]),
        None => record.extend_from_slice(&[0; 5]),
    }
    record.extend_from_slice(&u32::try_from(held.frame.len()).unwrap_or(u32::MAX).to_be_bytes());
    record.extend_from_slice(&held.frame);
    record
}

/// Reads a shared page's record for `page`.
pub(crate) fn read_page(page: u16, record: &[u8]) -> Result<Held, StoreError> {
    let mut reader = Reader::of(record, KIND_PAGE)?;
    if reader.u16()? != page {
        return Err(StoreError::Invalid);
    }
    let [has_cursor, row, column, width, flags] = reader.array()?;
    let cursor = match has_cursor {
        0 => None,
        1 => Some(CursorLine { row, column, width, flags, acked: 0, line: 0 }),
        _ => return Err(StoreError::Invalid),
    };
    let length = usize::try_from(u32::from_be_bytes(reader.array()?)).map_err(|_| StoreError::Invalid)?;
    if length == 0 || !length.is_multiple_of(PACKET_BYTES) || length > MOST_FRAME_BYTES {
        return Err(StoreError::Invalid);
    }
    let frame = reader.take(length)?;
    if !reader.rest().is_empty() {
        return Err(StoreError::Invalid);
    }
    Ok(Held { frame: alloc::sync::Arc::from(frame), cursor })
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
    if count > MOST_PAGES {
        return Err(StoreError::Invalid);
    }
    let mut pages: Vec<PageEntry> = Vec::with_capacity(count);
    for _ in 0..count {
        let number = reader.u16()?;
        let title = reader.text()?.ok_or(StoreError::Invalid)?;
        let section = reader.text()?;
        if !PAGES.contains(&number) || pages.iter().any(|entry| entry.number == number) {
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
