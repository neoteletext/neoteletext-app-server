//! Page frames (`SERVER.md` section 5.2): one page, all its sub-pages, as
//! the engine encodes them, with the erase bit set so the page replaces
//! what the receiver held, and in serial magazine mode, so the frame
//! stands alone. The browser's decoder completes the page at the end of
//! the WebSocket message that carries it; a page larger than one message
//! may hold goes in several, cut between its sub-pages ([`messages`]).

use alloc::vec::Vec;

use neoteletext_core::consts::{C4_ERASE, C11_SERIAL, PACKET_BYTES};
use neoteletext_core::packet::{Decoded, Packet, decode};
use neoteletext_core::types::ControlBits;
use neoteletext_formats::encode::encode_page;
use neoteletext_formats::page::PageFile;

/// The packets of `subpages`, one after another, as one frame.
#[must_use]
pub fn page_frame(subpages: &[PageFile]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for subpage in subpages {
        append_page(&mut bytes, subpage);
    }
    bytes
}

/// Appends one sub-page's packets to `bytes`.
pub fn append_page(bytes: &mut Vec<u8>, page: &PageFile) {
    let mut whole = *page;
    let control = whole.control();
    whole.set_control(ControlBits(control.0 | C4_ERASE | C11_SERIAL));
    encode_page(&whole, &mut |packet| bytes.extend_from_slice(&packet.bytes));
}

/// The most bytes one push message may hold: the engine's web agent refuses
/// a longer one (`neoteletext_web::consts::DEFAULT_PUSH_LIMIT`, its
/// REQ-WEB-008) and drops the connection.
pub const MOST_MESSAGE_BYTES: usize = neoteletext_web::consts::DEFAULT_PUSH_LIMIT;

/// `bytes` as the WebSocket messages that carry it, each at most `limit`
/// bytes of whole packets, cut only before a sub-page's header where it
/// can be, so that each message stands alone (`SERVER.md` section 5.2):
/// a page of many sub-pages, larger than the limit, becomes several.
/// Bytes that are not whole packets, or fit, are one message.
#[must_use]
pub fn messages(bytes: &[u8], limit: usize) -> Vec<&[u8]> {
    let most = limit.saturating_sub(limit % PACKET_BYTES).max(PACKET_BYTES);
    if bytes.len() <= limit || !bytes.len().is_multiple_of(PACKET_BYTES) {
        return alloc::vec![bytes];
    }
    // Where each sub-page starts: every header packet but the first.
    let starts: Vec<usize> = bytes
        .as_chunks::<PACKET_BYTES>()
        .0
        .iter()
        .enumerate()
        .filter(|&(index, &bytes)| index > 0 && matches!(decode(&Packet { bytes }), Ok(Decoded::Header(_))))
        .map(|(index, _)| index.saturating_mul(PACKET_BYTES))
        .chain(core::iter::once(bytes.len()))
        .collect();
    let mut parts = Vec::new();
    let mut from = 0;
    let mut last = 0;
    for &start in &starts {
        if start.saturating_sub(from) > most && last > from {
            parts.push(bytes.get(from..last).unwrap_or_default());
            from = last;
        }
        // A sub-page longer than the limit by itself is cut at packets.
        while start.saturating_sub(from) > most {
            let to = from.saturating_add(most);
            parts.push(bytes.get(from..to).unwrap_or_default());
            from = to;
        }
        last = start;
    }
    if from < bytes.len() {
        parts.push(bytes.get(from..).unwrap_or_default());
    }
    parts
}

#[cfg(test)]
mod tests;
