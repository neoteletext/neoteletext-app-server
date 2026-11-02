//! `packets`, raw packets (REQ-1462).

use alloc::vec::Vec;

use neoteletext_core::packet::encode;
use neoteletext_core::types::Magazine;
use serde_json::{Value, json};

use super::{accepted, base64, base64_field, refused, with, without};
use crate::protocol::Rule;

/// A row packet, 1/1, a kind the stream may carry.
fn row() -> Vec<u8> {
    encode::row(Magazine::from_bits(1), 1, &[0x20; 40]).bytes.to_vec()
}

fn packets() -> Value {
    json!({"type": "packets", "connections": "all", "packets": base64(&row())})
}

/// REQ-1476: `packets`.
#[test]
fn raw_packets_are_read_whole() {
    accepted(&packets());
    accepted(&with(&packets(), "/packets", json!(base64(&[row(), row()].concat()))));
}

/// REQ-1462 item 2: a program's packet that fails is refused when it is sent.
#[test]
fn raw_packets_name_the_fault() {
    refused(&with(&packets(), "/extra", json!(1)), "extra", Rule::UnknownField);
    refused(&without(&packets(), "/packets"), "packets", Rule::MissingField);
    base64_field(&packets(), "/packets", "packets", &row(), &row()[..41]);
    // An address byte Hamming 8/4 cannot correct (EN 300 706 clause 8.2).
    let broken = (0..=u8::MAX).find(|&byte| neoteletext_core::ecc::hamming84(byte).is_none()).unwrap();
    let mut bad = row();
    bad[0] = broken;
    refused(&with(&packets(), "/packets", json!(base64(&[row(), bad].concat()))), "packets[1]", Rule::InvalidPacket);
    // Packet 30 of magazine 1: a kind the stream may not carry.
    let thirty = encode::address(Magazine::from_bits(1), 30).bytes.to_vec();
    refused(&with(&packets(), "/packets", json!(base64(&thirty))), "packets[0]", Rule::InvalidPacket);
}
