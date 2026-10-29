//! A page as the channel keeps it (`SERVER.md` section 6.2): encoded once,
//! sub-page by sub-page, each named by its sub-code, in the page's order,
//! with what the cursor record needs, not the page model. The sub-pages
//! lie end to end in one frame, which is what a receiver that tunes no
//! sub-page is sent, so a page costs its bytes once however it is read.

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::ops::Range;

use neoteletext_formats::page::{CursorLine, PageFile};

use crate::frame::append_page;

/// One sub-page's place in its page's frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Part {
    /// Its sub-code.
    pub subcode: u16,
    /// Its packets' bytes in the frame.
    pub bytes: Range<usize>,
}

/// A page, encoded.
#[derive(Clone, Debug)]
pub(crate) struct Held {
    /// Every sub-page's packets, in the page's order.
    pub frame: Arc<[u8]>,
    /// Where each sub-page is in the frame, in the same order.
    pub parts: Vec<Part>,
    /// The first sub-page's cursor.
    pub cursor: Option<CursorLine>,
}

impl Held {
    /// The page of `subpages`, in their order, each named by its sub-code.
    pub(crate) fn of(subpages: &[PageFile]) -> Self {
        let mut frame = Vec::new();
        let mut parts = Vec::with_capacity(subpages.len());
        for subpage in subpages {
            let start = frame.len();
            append_page(&mut frame, subpage);
            parts.push(Part { subcode: subpage.subcode.value(), bytes: start..frame.len() });
        }
        Self { frame: Arc::from(frame), parts, cursor: subpages.first().and_then(|page| page.cursor) }
    }

    /// The page of sub-pages already encoded, in the order given, with
    /// `cursor`.
    pub(crate) fn of_encoded<'a>(subpages: impl Iterator<Item = (u16, &'a [u8])>, cursor: Option<CursorLine>) -> Self {
        let mut frame = Vec::new();
        let mut parts = Vec::new();
        for (subcode, bytes) in subpages {
            let start = frame.len();
            frame.extend_from_slice(bytes);
            parts.push(Part { subcode, bytes: start..frame.len() });
        }
        Self { frame: Arc::from(frame), parts, cursor }
    }

    /// The sub-codes, in the page's order.
    pub(crate) fn subcodes(&self) -> impl Iterator<Item = u16> + '_ {
        self.parts.iter().map(|part| part.subcode)
    }

    /// Each sub-page's sub-code and packets, in the page's order.
    pub(crate) fn subpages(&self) -> impl Iterator<Item = (u16, &[u8])> {
        self.parts.iter().map(|part| (part.subcode, self.frame.get(part.bytes.clone()).unwrap_or_default()))
    }

    /// Sub-page `subcode`'s packets, if the page has it.
    pub(crate) fn subpage(&self, subcode: u16) -> Option<&[u8]> {
        self.subpages().find(|&(each, _)| each == subcode).map(|(_, bytes)| bytes)
    }

    /// The first sub-page's packets: the page itself when it has one.
    pub(crate) fn first(&self) -> &[u8] {
        self.subpages().next().map_or(&[][..], |(_, bytes)| bytes)
    }

    /// This page with `subpages` put in it (`docs/applications.md`
    /// section 3.2): each replaces the sub-page of its sub-code where it
    /// stands, or, when the page has none of that sub-code, goes before
    /// the first with a higher one; the others are kept as they were.
    pub(crate) fn with(&self, subpages: &[PageFile]) -> Self {
        let put = Self::of(subpages);
        let mut order: Vec<(u16, &[u8])> = self.subpages().collect();
        for (subcode, bytes) in put.subpages() {
            if let Some(slot) = order.iter_mut().find(|(each, _)| *each == subcode) {
                slot.1 = bytes;
            } else {
                let at = order.iter().position(|&(each, _)| each > subcode).unwrap_or(order.len());
                order.insert(at, (subcode, bytes));
            }
        }
        // The cursor is the first sub-page's: the one put, if it was.
        let first = order.first().map(|&(subcode, _)| subcode);
        let cursor = match subpages.iter().find(|page| Some(page.subcode.value()) == first) {
            Some(page) => page.cursor,
            None => self.cursor,
        };
        Self::of_encoded(order.into_iter(), cursor)
    }

    /// This page without the sub-pages of `subcodes`, or none when none
    /// is left.
    pub(crate) fn without(&self, subcodes: &[u16]) -> Option<Self> {
        let kept = self.subpages().filter(|(subcode, _)| !subcodes.contains(subcode));
        let held = Self::of_encoded(kept, self.cursor);
        (!held.parts.is_empty()).then_some(held)
    }
}

#[cfg(test)]
mod tests;
