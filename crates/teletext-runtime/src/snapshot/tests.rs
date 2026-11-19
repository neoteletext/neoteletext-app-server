#![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic, clippy::arithmetic_side_effects, reason = "a test")]
//! REQ-6882's bounds on their own: the counted addresses and the answers'
//! cache.

use alloc::sync::Arc;
use core::net::IpAddr;

use super::{
    Cache, Counter, Kept, MOST_CACHED_SNAPSHOT_BYTES, MOST_CACHED_SNAPSHOTS, MOST_COUNTED_SNAPSHOT_ADDRESSES, Over, SNAPSHOT_CACHE_MS, ShownPicture,
    SnapshotContent,
};
use crate::kept::PictureLanguage;

/// The `index`th address of a test.
fn address(index: usize) -> IpAddr {
    IpAddr::from(u32::try_from(index).unwrap().to_be_bytes())
}

/// REQ-6882 item 4: at `MOST_COUNTED_SNAPSHOT_ADDRESSES` the address counted
/// longest ago with nothing open is forgotten, and one with a connection
/// open is kept.
#[test]
fn the_counted_addresses_are_bounded_forgetting_the_oldest() {
    let mut counter = Counter::default();
    // The first address keeps a connection open; the rest close theirs.
    counter.admit(Some(address(0)), 0).unwrap();
    for index in 1..MOST_COUNTED_SNAPSHOT_ADDRESSES {
        counter.admit(Some(address(index)), u64::try_from(index).unwrap()).unwrap();
        counter.release(Some(address(index)));
    }
    assert_eq!(counter.addresses(), MOST_COUNTED_SNAPSHOT_ADDRESSES);
    counter.admit(Some(address(MOST_COUNTED_SNAPSHOT_ADDRESSES)), 1 << 20).unwrap();
    assert_eq!(counter.addresses(), MOST_COUNTED_SNAPSHOT_ADDRESSES);
    assert!(counter.counts(address(0)), "one with a connection open is kept");
    assert!(!counter.counts(address(1)), "the oldest with none open is forgotten");
    assert!(counter.counts(address(2)) && counter.counts(address(MOST_COUNTED_SNAPSHOT_ADDRESSES)));
}

/// REQ-6882 items 1 and 2, as the counter alone keeps them.
#[test]
fn an_address_is_bounded_by_the_minute_and_by_what_it_holds_open() {
    let mut counter = Counter::default();
    let from = Some(address(7));
    for _ in 0..super::MOST_OPEN_SNAPSHOTS_PER_ADDRESS {
        counter.admit(from, 0).unwrap();
    }
    assert_eq!(counter.admit(from, 0), Err(Over::OpenFromAddress));
    for _ in 0..super::MOST_OPEN_SNAPSHOTS_PER_ADDRESS {
        counter.release(from);
    }
    let mut admitted = super::MOST_OPEN_SNAPSHOTS_PER_ADDRESS;
    while counter.admit(from, 1).is_ok() {
        counter.release(from);
        admitted += 1;
    }
    assert_eq!(admitted, usize::try_from(super::MOST_SNAPSHOTS_PER_ADDRESS_PER_MINUTE).unwrap());
    assert_eq!(counter.admit(from, 1), Err(Over::PerMinute));
    assert_eq!(counter.admit(from, super::SNAPSHOT_WINDOW_MS), Ok(()));
}

/// A content of `data_bytes` of ReGIS.
fn content(data_bytes: usize) -> Arc<SnapshotContent> {
    let page: crate::protocol::Page = serde_json::from_value(crate::protocol::tests::subpage()).unwrap();
    let picture = Some(ShownPicture::Bytes { language: PictureLanguage::Regis, data: Arc::from(alloc::vec![b' '; data_bytes]), objects: None });
    Arc::new(SnapshotContent { page, picture })
}

/// REQ-6882 item 10: the cache holds at most `MOST_CACHED_SNAPSHOTS` places
/// and `MOST_CACHED_SNAPSHOT_BYTES`, the answer kept longest ago forgotten
/// first; item 9: an answer goes after `SNAPSHOT_CACHE_MS`.
#[test]
fn the_snapshot_cache_is_bounded_in_places_and_bytes() {
    let mut cache = Cache::default();
    for index in 0..=MOST_CACHED_SNAPSHOTS {
        cache.keep((0x100 + u16::try_from(index).unwrap(), 0x3F7F), Kept::Declined, u64::try_from(index).unwrap());
    }
    assert_eq!(cache.held().0, MOST_CACHED_SNAPSHOTS);
    assert!(cache.get((0x100, 0x3F7F), 1).is_none(), "the oldest went");
    assert_eq!(cache.get((0x101, 0x3F7F), 1), Some(Kept::Declined));
    assert!(cache.get((0x101, 0x3F7F), 1 + SNAPSHOT_CACHE_MS).is_none(), "a minute later");

    let mut cache = Cache::default();
    let large = crate::protocol::MOST_DESCRIPTION_PICTURE_BYTES;
    let fit = MOST_CACHED_SNAPSHOT_BYTES / large;
    for index in 0..=fit {
        cache.keep((0x200 + u16::try_from(index).unwrap(), 0), Kept::Drawn(content(large)), u64::try_from(index).unwrap());
        assert!(cache.held().1 <= MOST_CACHED_SNAPSHOT_BYTES);
    }
    assert!(cache.held().0 < fit, "past the bytes, the oldest went: {:?}", cache.held());
    assert!(cache.get((0x200, 0), 0).is_none());
    assert!(matches!(cache.get((0x200 + u16::try_from(fit).unwrap(), 0), 0), Some(Kept::Drawn(_))));
}
