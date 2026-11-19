//! The bounds of what is kept (neoteletext-common `docs/renderer-parity.md`
//! PAR-17; REQ-6766; TODO-6836, TODO-6837): each reached exactly is kept,
//! and one byte or one picture more is refused naming its constant; a
//! stream costs its bytes and its reader's memory; readers are let go for
//! room and after a refusal, and a kept stream is never read again.

extern crate std;

use alloc::vec;
use alloc::vec::Vec;

use super::{
    Companion, KeptPictures, MOST_KEPT_COST, MOST_KEPT_PICTURES, MOST_KEPT_STREAM_BYTES, NAPLPS_READER_BYTES, PictureLanguage, REGIS_READER_BYTES, StreamChange,
};
use crate::protocol::Rule;

/// ReGIS that draws nothing: spaces between commands.
fn blank(length: usize) -> Vec<u8> {
    vec![b' '; length]
}

/// The NAPLPS stream that holds the most memory for its bytes that the
/// NAPLPS renderer's memory test knows: a square text field and mask
/// (TEXT and TEXTURE, ANSI X3.110-1983 sections 5.3.2.6 and 5.3.2.4), then
/// every DRCS character (section 6.2.3) and texture (section 6.2.4) defined
/// with a filled rectangle over the largest mask, each bitmap
/// `MASK_SIDE` squared, 16,384 bytes, from seven bytes of stream.
pub(crate) fn heaviest_naplps() -> Vec<u8> {
    const SO: u8 = 0x0E;
    const SI: u8 = 0x0F;
    const DEF_DRCS: u8 = 0x83;
    const DEF_TEXTURE: u8 = 0x84;
    const END: u8 = 0x85;
    /// DRCS characters a stream can define, 2/0 to 7/15.
    const DRCS_CHARACTERS: usize = 96;
    let square = [0x40, 0x52, 0x40];
    let mut bytes = vec![SO, 0x22, 0x40, 0x40];
    bytes.extend_from_slice(&square);
    bytes.extend_from_slice(&[0x21, 0x40]);
    bytes.extend_from_slice(&square);
    bytes.push(SI);
    let fill = [SO, 0x31, 0x5B, 0x7F, 0x7F, SI];
    bytes.extend_from_slice(&[DEF_DRCS, 0x20]);
    bytes.extend_from_slice(&fill);
    for _ in 1..DRCS_CHARACTERS {
        bytes.push(DEF_DRCS);
        bytes.extend_from_slice(&fill);
    }
    for pattern in 0x41..=0x44_u8 {
        bytes.extend_from_slice(&[DEF_TEXTURE, pattern]);
        bytes.extend_from_slice(&fill);
    }
    bytes.push(END);
    bytes
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
    let last = u16::try_from(MOST_KEPT_PICTURES - 1).unwrap();
    for sub in 0..=last {
        kept.stream(0x100, sub, PictureLanguage::Regis, true, b"P[1,1]").unwrap();
    }
    assert!(kept.cost() <= MOST_KEPT_COST, "readers were let go for room");
    let refused = kept.stream(0x101, 0, PictureLanguage::Regis, true, b"P[1,1]").unwrap_err();
    assert_eq!((refused.rule, refused.detail.as_str()), (Rule::KeptOverLimit, "MOST_KEPT_PICTURES (512) would be passed"));
    // A kept one may still change.
    kept.stream(0x100, last, PictureLanguage::Regis, false, b"V[2,2]").unwrap();
    let refused = kept.companion(0x101, 0, PictureLanguage::Regis, Companion::Cursor(1), 10).unwrap_err();
    assert_eq!(refused.rule, Rule::KeptOverLimit, "a companion of a new page counts as a picture");
}

#[test]
fn the_cost_counts_each_stream_and_its_reader_and_most_kept_cost_is_kept_and_one_byte_more_refused() {
    let mut kept = KeptPictures::default();
    kept.stream(0x100, 0, PictureLanguage::Regis, true, &blank(100)).unwrap();
    assert_eq!(kept.cost(), 100 + REGIS_READER_BYTES, "the bytes and its reader");
    kept.companion(0x100, 0, PictureLanguage::Regis, Companion::Cursor(1), 40).unwrap();
    assert_eq!(kept.cost(), 140 + REGIS_READER_BYTES, "and the cursor message's line");
    // Whole streams fill the cost but for the last one's reader, which the
    // last stream leaves room for.
    let mut kept = KeptPictures::default();
    let pictures = MOST_KEPT_COST / MOST_KEPT_STREAM_BYTES;
    assert_eq!(pictures * MOST_KEPT_STREAM_BYTES, MOST_KEPT_COST, "whole streams reach it exactly");
    let last = u16::try_from(pictures - 1).unwrap();
    for sub in 0..last {
        kept.stream(0x100, sub, PictureLanguage::Regis, true, &blank(MOST_KEPT_STREAM_BYTES)).unwrap();
    }
    kept.stream(0x100, last, PictureLanguage::Regis, true, &blank(MOST_KEPT_STREAM_BYTES - REGIS_READER_BYTES)).unwrap();
    assert_eq!(kept.cost(), MOST_KEPT_COST);
    assert_eq!(kept.readers_held(), 1, "only the last written keeps its reader");
    let refused = kept.stream(0x200, 0, PictureLanguage::Regis, true, b" ").unwrap_err();
    assert_eq!((refused.rule, refused.detail.as_str()), (Rule::KeptOverLimit, "MOST_KEPT_COST (33554432) would be passed"));
    let refused = kept.companion(0x100, last, PictureLanguage::Regis, Companion::Cursor(1), 1).unwrap_err();
    assert_eq!(refused.path, "cursor");
}

/// TODO-6836: a NAPLPS reader is counted at what it can hold, which seven
/// bytes of a definition can make 16,384, so the heaviest NAPLPS pictures
/// hold no more than `MOST_KEPT_COST` together, however many are kept (the
/// ignored test below keeps `MOST_KEPT_PICTURES` of them).
#[test]
fn a_naplps_pictures_reader_is_counted_at_what_it_can_hold() {
    let heaviest = heaviest_naplps();
    let mut kept = KeptPictures::default();
    kept.stream(0x100, 0, PictureLanguage::Naplps, true, &heaviest).unwrap();
    assert!(kept.cost() >= NAPLPS_READER_BYTES, "its reader is counted: {}", kept.cost());
    assert_eq!(NAPLPS_READER_BYTES, 2_113_536, "the renderer's documented 1,851,392 bytes and the fixed allowance");
    // Twice as many as the cost holds readers for.
    let pictures = 2 * MOST_KEPT_COST / NAPLPS_READER_BYTES;
    for sub in 1..u16::try_from(pictures).unwrap() {
        kept.stream(0x100, sub, PictureLanguage::Naplps, true, &heaviest).unwrap();
    }
    let streams = pictures * heaviest.len();
    assert!(kept.cost() <= MOST_KEPT_COST);
    assert_eq!(kept.cost(), streams + kept.readers_held() * NAPLPS_READER_BYTES, "each reader held counted whole");
    assert_eq!(kept.readers_held(), (MOST_KEPT_COST - streams) / NAPLPS_READER_BYTES, "as many readers as the room left holds");
}

/// TODO-6837 and PAR-16: a refused message leaves what was kept; the
/// reader that read part of it is let go, so the picture takes no append
/// until it is begun again, and the kept stream is never read again.
#[test]
fn a_refused_append_leaves_the_picture_shown_and_taking_no_append_until_restarted() {
    let mut kept = KeptPictures::default();
    kept.stream(0x100, 0, PictureLanguage::Regis, true, b"P[1,1]").unwrap();
    // Drawing 2 names a parent that does not exist: the commit fails with code 5.
    let refused = kept.stream(0x100, 0, PictureLanguage::Regis, false, b"D(S2(P9),C)").map(|_| ()).unwrap_err();
    assert_eq!(refused.rule, Rule::InvalidRegis, "{}", refused.detail);
    assert_eq!(kept.get(0x100, 0).unwrap().stream, b"P[1,1]");
    let refused = kept.stream(0x100, 0, PictureLanguage::Regis, false, b"V[2,2]").map(|_| ()).unwrap_err();
    assert_eq!((refused.rule, refused.path), (Rule::KeptOverLimit, "restart"));
    assert_eq!(refused.detail, "the kept picture of 100/0000 takes no append, since its reader refused an earlier append; begin it again with restart");
    assert_eq!(kept.get(0x100, 0).unwrap().stream, b"P[1,1]", "still kept, and shown");
    assert_eq!(kept.cost(), 6, "a picture without its reader costs its bytes");
    let refused = kept.stream(0x100, 0, PictureLanguage::Regis, true, b"D(S2(P9),C)").map(|_| ()).unwrap_err();
    assert_eq!(refused.rule, Rule::InvalidRegis);
    assert_eq!(kept.get(0x100, 0).unwrap().stream, b"P[1,1]", "a refused restart keeps what was kept");
    kept.stream(0x100, 0, PictureLanguage::Regis, true, b"P[1,1]").unwrap();
    kept.stream(0x100, 0, PictureLanguage::Regis, false, b"V[2,2]").unwrap();
    assert_eq!(kept.get(0x100, 0).unwrap().stream, b"P[1,1]V[2,2]", "begun again, it takes appends");
}

/// TODO-6836: a picture whose reader was let go for room says so, and is
/// begun again with its reader.
#[test]
fn a_picture_whose_reader_was_let_go_for_room_is_begun_again() {
    let mut kept = KeptPictures::default();
    let pictures = u16::try_from(MOST_KEPT_COST / NAPLPS_READER_BYTES + 1).unwrap();
    for sub in 0..pictures {
        kept.stream(0x100, sub, PictureLanguage::Naplps, true, b"Hello").unwrap();
    }
    let refused = kept.stream(0x100, 0, PictureLanguage::Naplps, false, b" world").unwrap_err();
    assert_eq!(refused.path, "restart");
    assert!(refused.detail.contains("let go to make room under MOST_KEPT_COST"), "{}", refused.detail);
    kept.stream(0x100, pictures - 1, PictureLanguage::Naplps, false, b" world").unwrap();
    kept.stream(0x100, 0, PictureLanguage::Naplps, true, b"Hello world").unwrap();
    kept.stream(0x100, 0, PictureLanguage::Naplps, false, b"!").unwrap();
}

/// The process's resident size in bytes, from `/proc/self/status`.
fn resident() -> usize {
    let status = std::fs::read_to_string("/proc/self/status").unwrap();
    let line = status.lines().find(|line| line.starts_with("VmRSS:")).unwrap();
    let kilobytes: usize = line.split_whitespace().nth(1).unwrap().parse().unwrap();
    kilobytes.saturating_mul(1024)
}

/// TODO-6836 step 4: `MOST_KEPT_PICTURES` of the heaviest NAPLPS pictures
/// grow the process's resident size by no more than `MOST_KEPT_COST`.
/// Ignored, since it measures the whole process: run it alone, `cargo test
/// --release -p teletext-runtime --lib
/// kept::tests::kept_naplps_pictures_hold_no_more_than_their_cost --
/// --ignored --exact`.
#[test]
#[ignore = "measures the process's resident size, so runs alone: name it with --ignored --exact"]
fn kept_naplps_pictures_hold_no_more_than_their_cost() {
    let heaviest = heaviest_naplps();
    let before = resident();
    let mut kept = KeptPictures::default();
    for sub in 0..u16::try_from(MOST_KEPT_PICTURES).unwrap() {
        kept.stream(0x100, sub, PictureLanguage::Naplps, true, &heaviest).unwrap();
    }
    let growth = resident().saturating_sub(before);
    std::println!(
        "{MOST_KEPT_PICTURES} kept NAPLPS pictures of {} bytes: counted {} bytes, {} readers held; the resident size grew by {growth} bytes",
        heaviest.len(),
        kept.cost(),
        kept.readers_held()
    );
    assert!(growth <= MOST_KEPT_COST, "the resident size grew by {growth} bytes, past MOST_KEPT_COST");
}
