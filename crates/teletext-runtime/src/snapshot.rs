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
