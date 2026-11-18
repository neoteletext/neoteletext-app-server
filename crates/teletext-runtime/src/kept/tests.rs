//! The bounds of what is kept (neoteletext-common `docs/renderer-parity.md`
//! PAR-17; REQ-6766): each reached exactly is kept, and one byte or one
//! picture more is refused naming its constant; a stream costs twice its
//! bytes.

use alloc::vec;
use alloc::vec::Vec;

use super::{Companion, KeptPictures, MOST_KEPT_COST, MOST_KEPT_PICTURES, MOST_KEPT_STREAM_BYTES, PictureLanguage, StreamChange};
use crate::protocol::Rule;

/// ReGIS that draws nothing: spaces between commands.
fn blank(length: usize) -> Vec<u8> {
    vec![b' '; length]
}

#[test]
fn a_stream_of_most_kept_stream_bytes_is_kept_and_one_byte_more_refused() {
    let mut kept = KeptPictures::default();
    assert_eq!(kept.stream(0x100, 1, PictureLanguage::Regis, true, &blank(MOST_KEPT_STREAM_BYTES)).map(|(change, _)| change), Ok(StreamChange::Restarted));
    let refused = kept.stream(0x100, 1, PictureLanguage::Regis, false, b" ").unwrap_err();
    assert_eq!((refused.rule, refused.detail.as_str()), (Rule::KeptOverLimit, "MOST_KEPT_STREAM_BYTES (1048576) would be passed"));
    assert_eq!(kept.get(0x100, 1).map(|picture| picture.stream.len()), Some(MOST_KEPT_STREAM_BYTES), "nothing of it kept");
}

#[test]
fn most_kept_pictures_are_kept_and_one_more_refused() {
    let mut kept = KeptPictures::default();
    for sub in 0..u16::try_from(MOST_KEPT_PICTURES).unwrap() {
        kept.stream(0x100, sub, PictureLanguage::Regis, true, b"P[1,1]").unwrap();
    }
    let refused = kept.stream(0x101, 0, PictureLanguage::Regis, true, b"P[1,1]").unwrap_err();
    assert_eq!((refused.rule, refused.detail.as_str()), (Rule::KeptOverLimit, "MOST_KEPT_PICTURES (512) would be passed"));
    // A kept one may still change.
    kept.stream(0x100, 0, PictureLanguage::Regis, false, b"V[2,2]").unwrap();
    let refused = kept.companion(0x101, 0, PictureLanguage::Regis, Companion::Cursor(1), 10).unwrap_err();
    assert_eq!(refused.rule, Rule::KeptOverLimit, "a companion of a new page counts as a picture");
}

#[test]
fn the_cost_counts_each_stream_twice_and_most_kept_cost_is_kept_and_one_byte_more_refused() {
    let mut kept = KeptPictures::default();
    kept.stream(0x100, 0, PictureLanguage::Regis, true, &blank(100)).unwrap();
    assert_eq!(kept.cost(), 200, "the bytes and its validator's copy");
    kept.companion(0x100, 0, PictureLanguage::Regis, Companion::Cursor(1), 40).unwrap();
    assert_eq!(kept.cost(), 240, "and the cursor message's line");
    let mut kept = KeptPictures::default();
    let pictures = MOST_KEPT_COST / (2 * MOST_KEPT_STREAM_BYTES);
    assert_eq!(pictures * 2 * MOST_KEPT_STREAM_BYTES, MOST_KEPT_COST, "whole pictures reach it exactly");
    for sub in 0..u16::try_from(pictures).unwrap() {
        kept.stream(0x100, sub, PictureLanguage::Regis, true, &blank(MOST_KEPT_STREAM_BYTES)).unwrap();
    }
    assert_eq!(kept.cost(), MOST_KEPT_COST);
    let refused = kept.stream(0x200, 0, PictureLanguage::Regis, true, b" ").unwrap_err();
    assert_eq!((refused.rule, refused.detail.as_str()), (Rule::KeptOverLimit, "MOST_KEPT_COST (33554432) would be passed"));
    let refused = kept.companion(0x100, 0, PictureLanguage::Regis, Companion::Cursor(1), 1).unwrap_err();
    assert_eq!(refused.path, "cursor");
}

/// PAR-16: a refused message leaves what was kept, and its checker reads
/// on as before.
#[test]
fn bytes_the_check_refuses_keep_nothing() {
    let mut kept = KeptPictures::default();
    kept.stream(0x100, 0, PictureLanguage::Regis, true, b"P[1,1]").unwrap();
    // Drawing 2 names a parent that does not exist: the commit fails with code 5.
    let refused = kept.stream(0x100, 0, PictureLanguage::Regis, false, b"D(S2(P9),C)").map(|_| ()).unwrap_err();
    assert_eq!(refused.rule, Rule::InvalidRegis, "{}", refused.detail);
    assert_eq!(kept.get(0x100, 0).unwrap().stream, b"P[1,1]");
    kept.stream(0x100, 0, PictureLanguage::Regis, false, b"V[2,2]").unwrap();
    let refused = kept.stream(0x100, 0, PictureLanguage::Regis, true, b"D(S2(P9),C)").map(|_| ()).unwrap_err();
    assert_eq!(refused.rule, Rule::InvalidRegis);
    assert_eq!(kept.get(0x100, 0).unwrap().stream, b"P[1,1]V[2,2]", "a refused restart keeps what was kept");
}
