//! A program's typed sub-page (REQ-1478) as the engine's page model,
//! `PageFile`, encoded by the engine's encoder into one page frame of one
//! sub-page (`SERVER.md` section 5.2, REQ-1501), and what a view is sent of
//! a new version of the sub-page it shows (DEC-1499). The sub-page has
//! been checked whole ([`crate::protocol::check`]) before it gets here.

use alloc::format;
use alloc::sync::Arc;
use alloc::vec::Vec;

use neoteletext_core::consts::{
    C4_ERASE, C5_NEWSFLASH, C6_SUBTITLE, C7_SUPPRESS_HEADER, C8_UPDATE, C9_INTERRUPTED, C10_INHIBIT_DISPLAY, C11_SERIAL, C12, C13, C14, PACKET_BYTES,
    STATUS_COLUMNS,
};
use neoteletext_core::packet::{Packet, Triplet, decode_address};
use neoteletext_core::types::{ControlBits, Magazine, PageAddress, PageNumber, SubCode};
use neoteletext_formats::encode::encode_page;
use neoteletext_formats::page::{CursorLine, EnhancementPacket, FastextLink, PageFile, RawBody, RawPacket};
use neoteletext_image::raster::{grid_of_packets, picture as draw};
use neoteletext_input::cursor::BLOCK;
use neoteletext_web::base64;

use crate::descriptions::MOST_PICTURE_BYTES;
use crate::frame::append_page;
use crate::protocol::{self, M29Line, PackedLine, Page, X27Line, X27NibblesLine, X28Line};

/// Packet numbers of the packed lines (EN 300 706 clause 7.1, table 1).
const PACKET_LINKS: u8 = 27;
/// X/28.
const PACKET_PAGE_ENHANCEMENT: u8 = 28;
/// M/29.
const PACKET_MAGAZINE_ENHANCEMENT: u8 = 29;

/// A sub-page as encoded: its packets whole, with the erase bit, and its
/// header without it, for a view sent only its changes (DEC-1499).
pub(crate) struct Encoded {
    /// Every packet, in the encoder's order, the header with C4 and C11 set.
    pub packets: Vec<[u8; PACKET_BYTES]>,
    /// The header with C11 set and C4 clear.
    pub update_header: Option<[u8; PACKET_BYTES]>,
    /// The sub-page's cursor.
    pub cursor: Option<CursorLine>,
}

/// What a view is sent of a sub-page it was sent before.
pub(crate) enum Change {
    /// Nothing differs.
    Nothing,
    /// The whole sub-page.
    Whole,
    /// The header and what differs.
    Rows(Vec<u8>),
}

/// The engine's address of a page number already checked.
pub(crate) fn address(number: u16) -> PageAddress {
    PageAddress::from_mpp(number).unwrap_or(PageAddress { magazine: Magazine::from_bits(neoteletext_core::limits::FIRST_MAGAZINE), page: PageNumber(0) })
}

/// Encodes one sub-page (REQ-1501: a frame carries one sub-page).
pub(crate) fn encode(page: &Page) -> Encoded {
    let mut model = PageFile::new();
    fill(&mut model, page);
    let mut packets = Vec::new();
    append_page(&mut packets, &model);
    let control = model.control();
    model.set_control(ControlBits((control.0 | C11_SERIAL) & !C4_ERASE));
    let mut update_header = None;
    encode_page(&model, &mut |packet| {
        if update_header.is_none() && number(&packet.bytes) == Some(HEADER) {
            update_header = Some(packet.bytes);
        }
    });
    Encoded { packets: packets.as_chunks::<PACKET_BYTES>().0.to_vec(), update_header, cursor: model.cursor }
}

/// The picture of a sub-page, as a PNG (REQ-1728 item 4): the engine's own
/// drawing of it, through the decoder a receiver uses. None when it cannot be
/// drawn or is over [`MOST_PICTURE_BYTES`]: a page's answer is whole without
/// one.
pub(crate) fn picture(page: &Page) -> Option<Arc<[u8]>> {
    let number = page.page.0;
    let mut status = [b' '; STATUS_COLUMNS];
    let shown = format!("P{number:03X}");
    for (slot, byte) in status.iter_mut().zip(shown.bytes()) {
        *slot = byte;
    }
    let grid = grid_of_packets(&encode(page).packets, address(number), status)?;
    draw(&grid).ok().filter(|png| png.len() <= MOST_PICTURE_BYTES).map(Arc::from)
}

/// The sub-page's header alone, without the erase bit: a frame that
/// changes nothing of the page, for a sub-page whose links alone changed,
/// which its LINKS record, or its absence, then carries (DEC-1517).
pub(crate) fn header_only(new: &Encoded) -> Change {
    new.update_header.map_or(Change::Whole, |header| Change::Rows(header.to_vec()))
}

/// Packet X/0, the header.
const HEADER: u8 = 0;
/// The rows' packets, X/1 to X/25.
const ROWS: core::ops::RangeInclusive<u8> = 1..=25;

/// A packet's number, 0 to 31 (EN 300 706 clause 7.1.2).
fn number(packet: &[u8; PACKET_BYTES]) -> Option<u8> {
    decode_address(&Packet { bytes: *packet }).ok().map(|address| address.number)
}

/// What a view that was last sent `kept` of this sub-page is sent of
/// `new` (DEC-1499 item 2, REQ-1504): the header without the erase bit,
/// each row that differs, and the whole enhancement group (X/26 to X/28,
/// and M/29) when any of it differs; nothing when nothing does. A row
/// gone, which only the erase bit clears, sends the sub-page whole.
pub(crate) fn changes(kept: &[[u8; PACKET_BYTES]], new: &Encoded) -> Change {
    let rows = |packets: &[[u8; PACKET_BYTES]]| -> Vec<(u8, [u8; PACKET_BYTES])> {
        packets.iter().filter_map(|packet| number(packet).filter(|number| ROWS.contains(number)).map(|number| (number, *packet))).collect()
    };
    let group = |packets: &[[u8; PACKET_BYTES]]| -> Vec<[u8; PACKET_BYTES]> {
        packets.iter().filter(|packet| number(packet).is_some_and(|number| number > *ROWS.end())).copied().collect()
    };
    let header = |packets: &[[u8; PACKET_BYTES]]| packets.iter().find(|packet| number(packet) == Some(HEADER)).copied();
    let (before, after) = (rows(kept), rows(&new.packets));
    if before.iter().any(|(row, _)| !after.iter().any(|(each, _)| each == row)) {
        return Change::Whole;
    }
    let changed: Vec<[u8; PACKET_BYTES]> = after.iter().filter(|entry| !before.contains(entry)).map(|&(_, packet)| packet).collect();
    let group_changed = group(kept) != group(&new.packets);
    if changed.is_empty() && !group_changed && header(kept) == header(&new.packets) {
        return Change::Nothing;
    }
    let Some(update_header) = new.update_header else { return Change::Whole };
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&update_header);
    if group_changed {
        for packet in group(&new.packets) {
            bytes.extend_from_slice(&packet);
        }
    }
    for packet in changed {
        bytes.extend_from_slice(&packet);
    }
    Change::Rows(bytes)
}

fn triplets(from: &[protocol::Triplet; neoteletext_core::consts::TRIPLETS_PER_PACKET]) -> [Triplet; neoteletext_core::consts::TRIPLETS_PER_PACKET] {
    from.map(|triplet| Triplet { address: triplet.address, mode: triplet.mode, data: triplet.data })
}

/// `model` made the sub-page `subpage`.
fn fill(model: &mut PageFile, subpage: &Page) {
    model.address = address(subpage.page.0);
    model.subcode = SubCode::new(subpage.sub.0);
    for (row, codes) in subpage.rows.iter().enumerate() {
        let Some(codes) = codes else { continue };
        let mut bytes = alloc::vec![0_u8; base64::decoded_len_max(codes.len())];
        if let Ok(length) = base64::decode(codes.as_bytes(), &mut bytes) {
            model.set_row(row, bytes.get(..length).unwrap_or_default());
        }
    }
    for (slot, link) in model.fastext.iter_mut().zip(&subpage.links) {
        *slot = link.map(|link| FastextLink { address: address(link.page.0), subcode: SubCode::new(link.sub.0) });
    }
    model.fastext_row_24 = subpage.display_row_24;
    let control = &subpage.control;
    let mut bits = 0_u16;
    for (on, bit) in [
        (control.erase, C4_ERASE),
        (control.newsflash, C5_NEWSFLASH),
        (control.subtitle, C6_SUBTITLE),
        (control.suppress_header, C7_SUPPRESS_HEADER),
        (control.update, C8_UPDATE),
        (control.interrupted_sequence, C9_INTERRUPTED),
        (control.inhibit_display, C10_INHIBIT_DISPLAY),
        (control.magazine_serial, C11_SERIAL),
    ] {
        if on {
            bits |= bit;
        }
    }
    // C12 to C14: the national option index 4×C12 + 2×C13 + C14 (clause
    // 9.3.1.3).
    let national = subpage.character_set.national_option;
    for (bit, weight) in [(C12, 0b100), (C13, 0b010), (C14, 0b001)] {
        if national & weight != 0 {
            bits |= bit;
        }
    }
    model.set_control(ControlBits(bits));
    model.region = subpage.character_set.region;
    for packet in &subpage.enhancements {
        let _ = model.enhancements.push(EnhancementPacket { designation: packet.designation, triplets: triplets(&packet.triplets) });
    }
    for line in &subpage.packets {
        let raw = match line {
            PackedLine::X27Nibbles(X27NibblesLine { designation, nibbles }) => {
                let mut values = [0_u8; protocol::NIBBLES];
                for (slot, &nibble) in values.iter_mut().zip(nibbles) {
                    *slot = nibble;
                }
                RawPacket { number: PACKET_LINKS, designation: *designation, body: RawBody::Nibbles(values) }
            }
            PackedLine::X27(X27Line { designation, triplets: values }) => {
                RawPacket { number: PACKET_LINKS, designation: *designation, body: RawBody::Triplets(triplets(values)) }
            }
            PackedLine::X28(X28Line { designation, triplets: values }) => {
                RawPacket { number: PACKET_PAGE_ENHANCEMENT, designation: *designation, body: RawBody::Triplets(triplets(values)) }
            }
            PackedLine::M29(M29Line { designation, triplets: values }) => {
                RawPacket { number: PACKET_MAGAZINE_ENHANCEMENT, designation: *designation, body: RawBody::Triplets(triplets(values)) }
            }
        };
        let _ = model.raw_packets.push(raw);
    }
    model.cursor =
        subpage.cursor.map(|cursor| CursorLine { row: cursor.row, column: cursor.column, flags: if cursor.block { BLOCK } else { 0 }, acked: 0, line: 0 });
}
