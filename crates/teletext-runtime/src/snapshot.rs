//! What bounds a channel's snapshots (REQ-6882; neoteletext-common
//! `docs/tab-thumbnails.md` section 5.3; DEC-6875 version 2, DEC-6878
//! version 2 items 5 and 6): the snapshot connections one address and the
//! channel may hold and open, the `snapshot` messages its program is sent,
//! and the answers kept for their places. Every value is provisional and
//! unmeasured (DEC-6875 item 7).
//!
//! These counters are apart from viewer state's both ways: a snapshot never
//! counts against `MOST_NEW_VIEWERS_PER_ADDRESS_PER_HOUR` or
//! `MOST_VIEWERS_PER_CHANNEL`, and a viewer never counts against these.
//! However many snapshot connections arrive, from however many addresses,
//! one program is asked to draw at most [`SNAPSHOTS_PER_SECOND`] a second
//! ([`SNAPSHOTS_BURST`] at once), [`MOST_SNAPSHOTS_AWAITED`] at a time, and
//! each place at most once in [`SNAPSHOT_CACHE_MS`].
//!
//! Nothing here holds anything of a viewer: an address is counted, never
//! kept with what it asked, and an answer is kept by its place alone.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::net::IpAddr;

use crate::kept::PictureLanguage;
use crate::protocol::{self, NaplpsObject};

/// The snapshot connections one network address (as the host sees it, or
/// as its trusted proxy states it, exactly as VS-304 takes it) may open on
/// one channel in [`SNAPSHOT_WINDOW_MS`]: two a second sustained, far above
/// what one browser asks (at most two at a time and only for tabs in view,
/// the browser's REQ-6883) and enough for many viewers behind one
/// carrier-grade address.
pub const MOST_SNAPSHOTS_PER_ADDRESS_PER_MINUTE: u32 = 120;
/// The window of [`MOST_SNAPSHOTS_PER_ADDRESS_PER_MINUTE`]: a minute.
pub const SNAPSHOT_WINDOW_MS: u64 = 60_000;
/// The snapshot connections one address may hold open at once on one
/// channel: four times the browser's two at a time.
pub const MOST_OPEN_SNAPSHOTS_PER_ADDRESS: usize = 8;
/// The snapshot connections open at once on one channel, from every
/// address.
pub const MOST_OPEN_SNAPSHOTS: usize = 1_024;
/// The addresses whose snapshots a channel counts at once; at the bound the
/// address counted longest ago with none open is forgotten, as
/// `MOST_COUNTED_ADDRESSES` does for new viewers.
pub const MOST_COUNTED_SNAPSHOT_ADDRESSES: usize = 65_536;
/// The longest a snapshot connection stays open (REQ-6881 item 6): the
/// program's [`SNAPSHOT_ANSWER_MS`] and the description's
/// `DESCRIPTION_WAIT_MS` with room for a slow link, and below the engine's
/// `SNAPSHOT_TIME_LIMIT_US` of 15 seconds, so the server ends first.
pub const SNAPSHOT_CONNECTION_MS: u64 = 10_000;
/// How long the server awaits `snapshot_page` or `snapshot_declined`: the
/// description's wait, and `start.budgets.snapshot_limit_ms`.
pub const SNAPSHOT_ANSWER_MS: u64 = crate::descriptions::DESCRIPTION_WAIT_MS;
/// The `snapshot` messages one program is sent a second, sustained: half
/// the description rate, because drawing a page for a stranger runs the
/// page's selection in the program, which costs more than describing it.
pub const SNAPSHOTS_PER_SECOND: u64 = crate::descriptions::DESCRIPTIONS_PER_SECOND / 2;
/// The `snapshot` messages sent at once after a quiet time: two seconds'
/// worth.
pub const SNAPSHOTS_BURST: u64 = 2 * SNAPSHOTS_PER_SECOND;
/// The `snapshot` messages sent and not yet answered at once on one channel.
pub const MOST_SNAPSHOTS_AWAITED: usize = 16;
/// How long an answer, a decline included, is kept for its place and given
/// to every snapshot connection that selects it: the description cache's
/// minute, which the protocol's documentation tells programs.
pub const SNAPSHOT_CACHE_MS: u64 = crate::descriptions::DESCRIPTION_CACHE_MS;
/// The places whose answers one channel keeps at once.
pub const MOST_CACHED_SNAPSHOTS: usize = 256;
/// The bytes of encoded packets, picture bytes and objects one channel's
/// kept answers hold at once: 16 MiB, room for 64 answers whose pictures
/// are at the 262,144-byte bound.
pub const MOST_CACHED_SNAPSHOT_BYTES: usize = 16 * 1024 * 1024;

// The values the specification states (tab-thumbnails.md section 5.3),
// held to their derivations.
const _: () = assert!(SNAPSHOT_ANSWER_MS == 2_000 && SNAPSHOTS_PER_SECOND == 10 && SNAPSHOTS_BURST == 20 && SNAPSHOT_CACHE_MS == 60_000);
// Every open snapshot connection's address is among those counted, so one
// with none open can always be forgotten.
const _: () = assert!(MOST_OPEN_SNAPSHOTS < MOST_COUNTED_SNAPSHOT_ADDRESSES);
// A connection lives through the program's wait and the description's.
const _: () = assert!(SNAPSHOT_ANSWER_MS + crate::descriptions::DESCRIPTION_WAIT_MS < SNAPSHOT_CONNECTION_MS);

/// A place: a page and a sub-code together, 3F7F for the page as a whole
/// (DEC-6599).
pub type Place = (u16, u16);

/// The picture a snapshot's page is shown with.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ShownPicture {
    /// The picture kept for this sub-code of the page (PAR-16), as its kept
    /// stream stands when it is sent.
    Kept(u16),
    /// These bytes, read already by the receiver's own reader.
    Bytes {
        /// Their language.
        language: PictureLanguage,
        /// The bytes.
        data: Arc<[u8]>,
        /// For NAPLPS, the objects the shown picture is composed of.
        objects: Option<Vec<NaplpsObject>>,
    },
}

/// What a snapshot connection is sent for a page (REQ-6881 item 5): one
/// sub-page, and the picture shown in its place.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SnapshotContent {
    /// The sub-page.
    pub page: protocol::Page,
    /// Its picture, if it has one.
    pub picture: Option<ShownPicture>,
}

impl SnapshotContent {
    /// What it holds, for [`MOST_CACHED_SNAPSHOT_BYTES`]: the sub-page as a
    /// whole frame of packets, its picture's bytes, and its objects as they
    /// are held.
    fn bytes(&self) -> usize {
        let packets = crate::page::encode(&self.page).packets.len().saturating_mul(neoteletext_core::consts::PACKET_BYTES);
        let picture = match &self.picture {
            Some(ShownPicture::Bytes { data, objects, .. }) => {
                let objects = objects.as_ref().map_or(0, |objects| {
                    objects.iter().fold(0_usize, |sum, object| {
                        sum.saturating_add(size_of::<NaplpsObject>()).saturating_add(object.frames.len().saturating_mul(size_of::<protocol::MacroCode>()))
                    })
                });
                data.len().saturating_add(objects)
            }
            Some(ShownPicture::Kept(_)) | None => 0,
        };
        packets.saturating_add(picture).saturating_add(size_of::<Self>())
    }
}

/// How one address's snapshots stand.
struct Counted {
    /// When its window began.
    since: u64,
    /// Connections it opened in the window.
    opened: u32,
    /// Its connections open now.
    open: usize,
}

/// Why a snapshot connection was not admitted: each is DENIED 429.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Over {
    /// [`MOST_SNAPSHOTS_PER_ADDRESS_PER_MINUTE`].
    PerMinute,
    /// [`MOST_OPEN_SNAPSHOTS_PER_ADDRESS`].
    OpenFromAddress,
    /// [`MOST_OPEN_SNAPSHOTS`].
    Open,
}

/// The snapshot connections a channel counts, by address.
#[derive(Default)]
pub(crate) struct Counter {
    counted: BTreeMap<IpAddr, Counted>,
    /// Each counted address by when its window began.
    order: BTreeSet<(u64, IpAddr)>,
    /// Snapshot connections open now, from every address.
    open: usize,
}

impl Counter {
    /// Whether a snapshot connection from `address` may be held at `now`,
    /// counted open if so. A connection whose address the host does not know
    /// is counted only among the channel's.
    pub(crate) fn admit(&mut self, address: Option<IpAddr>, now: u64) -> Result<(), Over> {
        if self.open >= MOST_OPEN_SNAPSHOTS {
            return Err(Over::Open);
        }
        if let Some(address) = address {
            self.admit_address(address, now)?;
        }
        self.open = self.open.saturating_add(1);
        Ok(())
    }

    fn admit_address(&mut self, address: IpAddr, now: u64) -> Result<(), Over> {
        if let Some(counted) = self.counted.get_mut(&address) {
            if counted.since.saturating_add(SNAPSHOT_WINDOW_MS) <= now {
                self.order.remove(&(counted.since, address));
                counted.since = now;
                counted.opened = 0;
                self.order.insert((now, address));
            }
            if counted.opened >= MOST_SNAPSHOTS_PER_ADDRESS_PER_MINUTE {
                return Err(Over::PerMinute);
            }
            if counted.open >= MOST_OPEN_SNAPSHOTS_PER_ADDRESS {
                return Err(Over::OpenFromAddress);
            }
            counted.opened = counted.opened.saturating_add(1);
            counted.open = counted.open.saturating_add(1);
            return Ok(());
        }
        if self.counted.len() >= MOST_COUNTED_SNAPSHOT_ADDRESSES {
            // The oldest with nothing open: one exists, since fewer are open
            // than are counted (asserted above).
            let oldest = self.order.iter().find(|(_, counted)| self.counted.get(counted).is_some_and(|counted| counted.open == 0)).copied();
            if let Some((since, oldest)) = oldest {
                self.order.remove(&(since, oldest));
                self.counted.remove(&oldest);
            }
        }
        self.counted.insert(address, Counted { since: now, opened: 1, open: 1 });
        self.order.insert((now, address));
        Ok(())
    }

    /// A snapshot connection admitted from `address` closed.
    pub(crate) fn release(&mut self, address: Option<IpAddr>) {
        self.open = self.open.saturating_sub(1);
        if let Some(counted) = address.and_then(|address| self.counted.get_mut(&address)) {
            counted.open = counted.open.saturating_sub(1);
        }
    }

    /// The addresses counted now.
    #[cfg(test)]
    pub(crate) fn addresses(&self) -> usize {
        self.counted.len()
    }

    /// Whether `address` is counted now.
    #[cfg(test)]
    pub(crate) fn counts(&self, address: IpAddr) -> bool {
        self.counted.contains_key(&address)
    }
}

/// Thousandths of a snapshot a millisecond at the sustained rate, which is
/// the rate a second as thousandths a millisecond.
const SNAPSHOT: u64 = 1_000;

/// The program's snapshot rate: a token bucket, as the description rate is
/// kept.
pub(crate) struct Bucket {
    /// Snapshots the program may be sent now, in thousandths of one.
    allowance: u64,
    /// When [`Self::allowance`] was last worked out.
    refilled: u64,
}

impl Default for Bucket {
    fn default() -> Self {
        Self { allowance: SNAPSHOTS_BURST.saturating_mul(SNAPSHOT), refilled: 0 }
    }
}

impl Bucket {
    /// One snapshot of the allowance, if there is one at `now`.
    pub(crate) fn take(&mut self, now: u64) -> bool {
        let gained = now.saturating_sub(self.refilled).saturating_mul(SNAPSHOTS_PER_SECOND);
        self.refilled = self.refilled.max(now);
        self.allowance = self.allowance.saturating_add(gained).min(SNAPSHOTS_BURST.saturating_mul(SNAPSHOT));
        let Some(left) = self.allowance.checked_sub(SNAPSHOT) else { return false };
        self.allowance = left;
        true
    }
}

/// The answer kept for a place.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) enum Kept {
    /// What the program drew.
    Drawn(Arc<SnapshotContent>),
    /// The program declined, answered with something refused, or did not
    /// answer in time.
    Declined,
}

/// An answer kept for a place.
struct Cached {
    answer: Kept,
    until: u64,
    bytes: usize,
}

/// The answers kept, by place.
#[derive(Default)]
pub(crate) struct Cache {
    cached: BTreeMap<Place, Cached>,
    /// What the answers hold together.
    bytes: usize,
}

impl Cache {
    /// The answer kept for `place` at `now`, if one is.
    pub(crate) fn get(&self, place: Place, now: u64) -> Option<Kept> {
        self.cached.get(&place).filter(|kept| kept.until > now).map(|kept| kept.answer.clone())
    }

    /// Keeps `answer` for `place` from `now`, forgetting the answers kept
    /// longest ago while [`MOST_CACHED_SNAPSHOTS`] or
    /// [`MOST_CACHED_SNAPSHOT_BYTES`] would be passed.
    pub(crate) fn keep(&mut self, place: Place, answer: Kept, now: u64) {
        let drawn = if let Kept::Drawn(content) = &answer { content.bytes() } else { 0 };
        let bytes = drawn.saturating_add(size_of::<Cached>());
        self.remove(place);
        let stale: Vec<Place> = self.cached.iter().filter(|(_, kept)| kept.until <= now).map(|(&place, _)| place).collect();
        for place in stale {
            self.remove(place);
        }
        while self.cached.len() >= MOST_CACHED_SNAPSHOTS || self.bytes.saturating_add(bytes) > MOST_CACHED_SNAPSHOT_BYTES {
            let Some(oldest) = self.cached.iter().min_by_key(|(_, kept)| kept.until).map(|(&place, _)| place) else { break };
            self.remove(oldest);
        }
        self.bytes = self.bytes.saturating_add(bytes);
        self.cached.insert(place, Cached { answer, until: now.saturating_add(SNAPSHOT_CACHE_MS), bytes });
    }

    fn remove(&mut self, place: Place) {
        if let Some(kept) = self.cached.remove(&place) {
            self.bytes = self.bytes.saturating_sub(kept.bytes);
        }
    }

    /// The program went: what it said goes with it.
    pub(crate) fn clear(&mut self) {
        self.cached.clear();
        self.bytes = 0;
    }

    /// The places kept and what they hold.
    #[cfg(test)]
    pub(crate) fn held(&self) -> (usize, usize) {
        (self.cached.len(), self.bytes)
    }
}

#[cfg(test)]
mod tests;
