//! Page frames (`SERVER.md` section 5.2): one page, all its sub-pages, as
//! the engine encodes them, with the erase bit set so the page replaces
//! what the receiver held, and in serial magazine mode, so the frame
//! stands alone. The browser's decoder completes the page at the end of
//! the WebSocket message that carries it.

use alloc::vec::Vec;

use neoteletext_core::consts::{C4_ERASE, C11_SERIAL};
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
