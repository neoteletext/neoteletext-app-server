//! What the server keeps of a page's description (REQ-1728): the short cache
//! one for all visitors, and the limit on what reaches the program. The server
//! keeps no page but this (REQ-1474), and each entry lives for
//! [`DESCRIPTION_CACHE_MS`] at most. Nothing here depends on who asked,
//! so nothing here can tell one visitor from another.
//!
//! Every bound is named, so that a crawler or an attacker walking a channel's
//! addresses reaches the program at a rate this file states:
//! - only page numbers that are not reserved are asked for (`Channel`);
//! - one page is asked for once in [`DESCRIPTION_CACHE_MS`], however many ask;
//! - at most [`DESCRIPTIONS_PER_SECOND`] different pages a second, and a burst
//!   of [`DESCRIPTIONS_BURST`];
//! - at most [`MOST_WAITING_VISITORS`] visitors wait, each at most
//!   [`DESCRIPTION_WAIT_MS`];
//! - at most [`MOST_CACHED_DESCRIPTIONS`] entries are kept.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

/// How long a description is kept, not-public answers included: a minute, so
/// that a crawler repeating itself costs the program nothing and a page
/// a program stops making public leaves the web within that time. Provisional,
/// not measured.
pub const DESCRIPTION_CACHE_MS: u64 = 60_000;
/// How long a visitor waits for a description, in milliseconds: then the page
/// is not public (REQ-1728 item 2). Shorter than a selection's deadline
/// because a web visitor is looking at nothing meanwhile. Provisional.
pub const DESCRIPTION_WAIT_MS: u64 = 2_000;
/// The pages a second a program is asked to describe, sustained. A program
/// serves its viewers first, and a description costs a render on each side.
/// Provisional, not measured.
pub const DESCRIPTIONS_PER_SECOND: u64 = 20;
/// The pages asked for at once after a quiet time: two seconds' worth.
pub const DESCRIPTIONS_BURST: u64 = 2 * DESCRIPTIONS_PER_SECOND;
/// The descriptions kept: an eighth of a channel's 2,022 pages, more than a
/// crawler reaches at [`DESCRIPTIONS_PER_SECOND`] within a bursty
/// [`DESCRIPTION_CACHE_MS`] of public pages, and at most about 32 MB at
/// [`MOST_PICTURE_BYTES`] and 16,000 four-byte characters each.
pub const MOST_CACHED_DESCRIPTIONS: usize = 256;
/// The visitors that may wait for descriptions at once, on all pages.
pub const MOST_WAITING_VISITORS: usize = 1_024;
/// The largest picture carried, in bytes. Measured with `neottx render` on 1
/// November 2026: the engine's test card is 11,503 bytes, a Level 2.5 page
/// 8,602, and a page of 23 rows of random characters, which leaves the PNG
/// writer's matches little to find, 39,863. A picture over this is left out,
/// so that one answer stays small.
pub const MOST_PICTURE_BYTES: usize = 64 * 1024;

/// A web request a host is holding, numbered by the host.
pub type VisitorId = u64;

/// A public page as a web address shows it. Its text is the program's,
/// checked, and not yet escaped for anywhere.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct PublicPage {
    /// Its title.
    pub title: String,
    /// Its text.
    pub text: String,
    /// Its picture as a PNG, when one could be drawn.
    pub picture: Option<Arc<[u8]>>,
}

/// What a page's address answers: the page, when it is public; none for a
/// page that is not public, does not exist or was not described.
pub type Answer = Option<Arc<PublicPage>>;

/// An answer kept.
struct Cached {
    answer: Answer,
    /// When it goes.
    until: u64,
}

/// Those waiting for one page's description.
struct Waiting {
    visitors: Vec<VisitorId>,
    /// When the page counts as not public.
    due: u64,
}

/// What became of a visitor's asking.
pub(crate) enum Asked {
    /// The answer is known now.
    Answered(Answer),
    /// The visitor waits for an answer already asked for.
    Waiting,
    /// The visitor waits, and the program is to be asked.
    AskProgram,
}

/// The cache, and those waiting.
pub(crate) struct Descriptions {
    cached: BTreeMap<u16, Cached>,
    waiting: BTreeMap<u16, Waiting>,
    /// Requests the program may be sent now, in thousandths of one.
    allowance: u64,
    /// When [`Self::allowance`] was last worked out.
    refilled: u64,
}

/// Thousandths of a request a millisecond at the sustained rate, which is the
/// rate a second as thousandths a millisecond.
const REQUEST: u64 = 1_000;

impl Default for Descriptions {
    fn default() -> Self {
        Self { cached: BTreeMap::new(), waiting: BTreeMap::new(), allowance: DESCRIPTIONS_BURST.saturating_mul(REQUEST), refilled: 0 }
    }
}

impl Descriptions {
    /// One request of the allowance, if there is one at `now`.
    fn take_request(&mut self, now: u64) -> bool {
        let gained = now.saturating_sub(self.refilled).saturating_mul(DESCRIPTIONS_PER_SECOND);
        self.refilled = self.refilled.max(now);
        self.allowance = self.allowance.saturating_add(gained).min(DESCRIPTIONS_BURST.saturating_mul(REQUEST));
        let Some(left) = self.allowance.checked_sub(REQUEST) else { return false };
        self.allowance = left;
        true
    }

    fn visitors(&self) -> usize {
        self.waiting.values().map(|waiting| waiting.visitors.len()).sum()
    }

    /// `visitor` is at `page`'s address at `now`. Past the limits the answer is
    /// not public and is not kept, so that a flood leaves no mark on the page.
    pub(crate) fn ask(&mut self, page: u16, visitor: VisitorId, now: u64) -> Asked {
        if let Some(kept) = self.cached.get(&page).filter(|kept| kept.until > now) {
            return Asked::Answered(kept.answer.clone());
        }
        if self.visitors() >= MOST_WAITING_VISITORS {
            return Asked::Answered(None);
        }
        if let Some(waiting) = self.waiting.get_mut(&page) {
            waiting.visitors.push(visitor);
            return Asked::Waiting;
        }
        if !self.take_request(now) {
            return Asked::Answered(None);
        }
        self.waiting.insert(page, Waiting { visitors: alloc::vec![visitor], due: now.saturating_add(DESCRIPTION_WAIT_MS) });
        Asked::AskProgram
    }

    /// Whether a description of `page` is awaited.
    pub(crate) fn awaited(&self, page: u16) -> bool {
        self.waiting.contains_key(&page)
    }

    /// Keeps `answer` for `page`, and gives those who waited for it.
    pub(crate) fn answered(&mut self, page: u16, answer: &Answer, now: u64) -> Vec<VisitorId> {
        let visitors = self.waiting.remove(&page).map(|waiting| waiting.visitors).unwrap_or_default();
        self.keep(page, answer.clone(), now);
        visitors
    }

    fn keep(&mut self, page: u16, answer: Answer, now: u64) {
        self.cached.retain(|_, kept| kept.until > now);
        if self.cached.len() >= MOST_CACHED_DESCRIPTIONS
            && let Some(oldest) = self.cached.iter().min_by_key(|(_, kept)| kept.until).map(|(&page, _)| page)
        {
            self.cached.remove(&oldest);
        }
        self.cached.insert(page, Cached { answer, until: now.saturating_add(DESCRIPTION_CACHE_MS) });
    }

    /// The visitors whose page's program did not answer by `now`: its page
    /// counts as not public, for them and for those after them (REQ-1728 item 2).
    pub(crate) fn late(&mut self, now: u64) -> Vec<VisitorId> {
        let due: Vec<u16> = self.waiting.iter().filter(|(_, waiting)| waiting.due <= now).map(|(&page, _)| page).collect();
        due.into_iter().flat_map(|page| self.answered(page, &None, now)).collect()
    }

    /// The program went: nothing will answer, and what it said goes with it.
    pub(crate) fn forget(&mut self) -> Vec<VisitorId> {
        self.cached.clear();
        core::mem::take(&mut self.waiting).into_values().flat_map(|waiting| waiting.visitors).collect()
    }

    /// When the next page stops being waited for.
    pub(crate) fn next_due(&self) -> Option<u64> {
        self.waiting.values().map(|waiting| waiting.due).min()
    }
}

#[cfg(test)]
mod tests;
