//! A program's NAPLPS as a viewer receives it (the NAPLPS renderer's
//! REQ-6408 item 3): carried in NAPLPS records, one stream a sub-page,
//! begun by `restart` and added to after.

use alloc::vec::Vec;

use neoteletext_core::consts::PACKET_BYTES;
use neoteletext_core::packet::Packet;
use serde_json::{Value, json};

use super::{CONNECTION, answered, line, record_types, sent_to, told};
use crate::protocol::check::TYPE_NAPLPS;
use crate::protocol::tests::base64;

/// A `naplps` message for the test's connection and page 100, sub-code 0000.
fn naplps(data: &[u8], restart: bool) -> Vec<u8> {
    line(&json!({"type": "naplps", "connections": [CONNECTION], "page": "100", "sub": "0000", "restart": restart, "data": base64(data)}))
}

/// The NAPLPS records in `bytes`, each as (generation, offset, data).
fn records(bytes: &[u8]) -> Vec<(u16, u32, Vec<u8>)> {
    let mut found = Vec::new();
    let mut record: Vec<u8> = Vec::new();
    for bytes in bytes.as_chunks::<PACKET_BYTES>().0 {
        let fragment = neoteletext_audio::idl::fragment(&Packet { bytes: *bytes }).unwrap().unwrap();
        if fragment.index == 0 {
            record.clear();
        }
        record.extend_from_slice(&fragment.data);
        if fragment.last {
            let (kind, body) = neoteletext_audio::record::split(&record[..neoteletext_audio::record::framed_length(&record).unwrap()]).unwrap();
            assert_eq!(kind, TYPE_NAPLPS);
            assert_eq!(&body[..4], &[0x01, 0x00, 0x00, 0x00], "page 100, sub-code 0000");
            found.push((u16::from_be_bytes([body[4], body[5]]), u32::from_be_bytes([body[6], body[7], body[8], body[9]]), body[10..].to_vec()));
        }
    }
    found
}

/// A sub-page's NAPLPS reaches the viewer who shows it as NAPLPS records:
/// a stream started by `restart`, added to after, each record beginning
/// where the last ended; started again, a new generation from offset 0;
/// and nothing is sent of a stream that was never started.
#[test]
fn naplps_reaches_the_viewer_as_naplps_records_in_one_stream() {
    let mut channel = answered();
    channel.from_program(&naplps(b"abc", false), 1);
    let effects = channel.take_effects();
    let refused: Vec<Value> = told(&effects).into_iter().filter(|line| line["type"] == "refused").collect();
    assert_eq!(refused, [] as [Value; 0]);
    assert_eq!(sent_to(&effects, CONNECTION), [] as [u8; 0], "no stream was begun for it to add to");

    // RESET, then text: bytes as a NAPLPS terminal would receive them.
    let first = b"\x1b\x22\x46Hello";
    channel.from_program(&naplps(first, true), 2);
    let sent = sent_to(&channel.take_effects(), CONNECTION);
    assert_eq!(record_types(&sent), [TYPE_NAPLPS]);
    assert_eq!(records(&sent), [(1, 0, first.to_vec())]);

    channel.from_program(&naplps(b", world", false), 3);
    assert_eq!(records(&sent_to(&channel.take_effects(), CONNECTION)), [(1, u32::try_from(first.len()).unwrap(), b", world".to_vec())]);

    channel.from_program(&naplps(b"Again", true), 4);
    assert_eq!(records(&sent_to(&channel.take_effects(), CONNECTION)), [(2, 0, b"Again".to_vec())]);
}
