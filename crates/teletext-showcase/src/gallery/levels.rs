//! A picture's three levels as the sub-pages of one page: each TTI file
//! read with the engine's reader, then numbered as the gallery's page.

use alloc::vec::Vec;

use neoteletext_core::types::SubCode;
use neoteletext_formats::page::{FastextLink, PageFile};
use neoteletext_formats::tti::TtiReader;
use neoteletext_formats::{Item, Reader};
use teletext_runtime::screen::address;

/// The levels a picture is converted at, by the name the picture folder
/// gives each, and the sub-page each is shown as: 1 Level 1.5, 2 Level
/// 2.5, 3 Level 3.5 (`teletext-native`'s `pictures.rs`).
pub const LEVELS: [(&str, u8); 3] = [("l15", 1), ("l25", 2), ("l35", 3)];

/// The sub-page a level's name is shown as.
#[must_use]
pub fn subpage_of(level: &str) -> Option<u8> {
    LEVELS.iter().find(|(name, _)| *name == level).map(|&(_, subpage)| subpage)
}

/// The sub-code of sub-page `subpage`: its number in decimal digits, as
/// TTI files and broadcasts number sub-pages (EN 300 706 Annex A.1).
#[must_use]
pub fn subcode(subpage: u8) -> SubCode {
    SubCode::new((u16::from(subpage / 10) << 4) | u16::from(subpage % 10))
}

/// A Fastext link to page `number` (such as `0x601`), at `subpage` or any.
#[must_use]
pub fn link(number: u16, subpage: Option<u8>) -> FastextLink {
    FastextLink { address: address(number), subcode: subpage.map_or(SubCode::ANY, subcode) }
}

/// The first page of a TTI file, or none when it holds none.
#[must_use]
pub fn read(bytes: &[u8]) -> Option<PageFile> {
    let mut reader = TtiReader::new();
    let mut first: Option<PageFile> = None;
    let mut keep = |item: Item<'_>| {
        if let (Item::Page(page), None) = (item, &first) {
            first = Some(*page);
        }
    };
    reader.feed(bytes, &mut keep);
    reader.finish(&mut keep);
    first
}

/// `levels`, each a sub-page and its TTI, as the sub-pages of page
/// `number` with `fastext` (red, green, yellow, cyan, index): a level
/// that does not read is left out. The picture fills rows 0 to 24, so
/// its row 24 stays, under links of the gallery's own.
#[must_use]
pub fn subpages(levels: &[(u8, Vec<u8>)], number: u16, fastext: [Option<FastextLink>; 5]) -> Vec<PageFile> {
    let mut pages = Vec::new();
    for (subpage, bytes) in levels {
        let Some(mut page) = read(bytes) else { continue };
        page.address = address(number);
        page.subpage = *subpage;
        page.subcode = subcode(*subpage);
        page.fastext.iter_mut().for_each(|slot| *slot = None);
        for (slot, target) in page.fastext.iter_mut().zip(fastext) {
            *slot = target;
        }
        page.fastext_row_24 = true;
        pages.push(page);
    }
    pages
}
