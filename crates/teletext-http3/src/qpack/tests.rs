#![allow(clippy::arithmetic_side_effects, reason = "a test")]

use alloc::vec;

use super::*;
use crate::limits::MAX_FIELD_SECTION_BYTES;

const MOST: u64 = MAX_FIELD_SECTION_BYTES as u64;

fn fields(pairs: &[(&[u8], &[u8])]) -> Vec<Field> {
    pairs.iter().map(|(name, value)| Field::new(name, value)).collect()
}

/// RFC 9204 appendix A, at the entries a WebSocket's request and answer
/// use, the table's ends, and the entries the RFC's table breaks over
/// lines.
#[test]
fn the_static_table_is_appendix_a() {
    assert_eq!(STATIC_TABLE.len(), 99);
    let entries: [(usize, &[u8], &[u8]); 14] = [
        (0, b":authority", b""),
        (1, b":path", b"/"),
        (15, b":method", b"CONNECT"),
        (17, b":method", b"GET"),
        (23, b":scheme", b"https"),
        (25, b":status", b"200"),
        (27, b":status", b"404"),
        (41, b"cache-control", b"public, max-age=31536000"),
        (45, b"content-type", b"application/javascript"),
        (54, b"content-type", b"text/plain;charset=utf-8"),
        (68, b":status", b"403"),
        (85, b"content-security-policy", b"script-src 'none'; object-src 'none'; base-uri 'none'"),
        (90, b"origin", b""),
        (98, b"x-frame-options", b"sameorigin"),
    ];
    for (index, name, value) in entries {
        assert_eq!(STATIC_TABLE[index], (name, value), "entry {index}");
    }
}

/// RFC 9204 appendix B.1's field section: a literal field line with a
/// name reference to the static table.
#[test]
fn the_example_of_rfc_9204_appendix_b1_decodes() {
    let section = [0x00, 0x00, 0x51, 0x0B, 0x2F, 0x69, 0x6E, 0x64, 0x65, 0x78, 0x2E, 0x68, 0x74, 0x6D, 0x6C];
    assert_eq!(decode(&section, MOST), Ok(fields(&[(b":path", b"/index.html")])));
    let mut encoded = Vec::new();
    encode([(&b":path"[..], &b"/index.html"[..])], &mut encoded);
    assert_eq!(encoded, section, "the encoder writes the same bytes");
}

/// REQ-6967: each form of field line that needs no dynamic table (RFC
/// 9204 sections 4.5.2, 4.5.4 and 4.5.6), with and without the Huffman
/// code, and with the N bit, which changes nothing here.
#[test]
fn every_static_and_literal_form_decodes() {
    let section = [
        0x00,
        0x00,      // Required Insert Count 0, Base 0
        0xC0 + 17, // indexed, static: :method GET
        0xFF,
        0x00, // indexed, static, index 63 in a continuation byte: :status 100
        0x50 + 1,
        0x01,
        b'/', // name reference, static: :path, value "/"
        0x70 | 0x0F,
        0x00,
        0x82,
        0x64,
        0x02, // the same with N, index 15 continued: :method, Huffman "302"
        0x20 + 3,
        b'f',
        b'o',
        b'o',
        0x03,
        b'b',
        b'a',
        b'r', // literal name
        0x30 + 0x08 + 7,
        0x01,
        0x25,
        0xA8,
        0x49,
        0xE9,
        0x5B,
        0xA9,
        0x7D,
        0x7F, // literal name, N, Huffman "custom-key" of RFC 7541 C.4.3, its length continued ...
        0x00, // ... and an empty value
    ];
    let decoded = decode(&section, MOST).unwrap();
    assert_eq!(decoded[..5], fields(&[(b":method", b"GET"), (b":status", b"100"), (b":path", b"/"), (b":method", b"302"), (b"foo", b"bar")]));
    assert_eq!(decoded[5], Field::new(b"custom-key", b""));
    assert_eq!(decoded.len(), 6);
}

/// REQ-6967: any reference to the dynamic table is the connection error
/// `QPACK_DECOMPRESSION_FAILED`: a Required Insert Count, a Base below
/// it, and each of the four forms that name a dynamic entry (RFC 9204
/// sections 4.5.2 to 4.5.5).
#[test]
fn any_dynamic_table_reference_fails() {
    let failed = Err(Error::connection(Code::QpackDecompressionFailed));
    assert_eq!(decode(&[0x01, 0x00], MOST), failed, "a Required Insert Count");
    assert_eq!(decode(&[0x00, 0x80], MOST), failed, "a Base below the count");
    assert_eq!(decode(&[0x00, 0x00, 0x80], MOST), failed, "indexed, dynamic");
    assert_eq!(decode(&[0x00, 0x00, 0x10], MOST), failed, "indexed, past the Base");
    assert_eq!(decode(&[0x00, 0x00, 0x40, 0x00], MOST), failed, "name reference, dynamic");
    assert_eq!(decode(&[0x00, 0x00, 0x00, 0x00], MOST), failed, "name reference, past the Base");
}

/// A section that ends inside its prefix or a field line, an index past
/// the static table's end, a string that is not of the Huffman code, and
/// an integer over the bound are each `QPACK_DECOMPRESSION_FAILED`.
#[test]
fn malformed_sections_fail() {
    let failed = Err(Error::connection(Code::QpackDecompressionFailed));
    assert_eq!(decode(&[], MOST), failed);
    assert_eq!(decode(&[0x00], MOST), failed);
    assert_eq!(decode(&[0x00, 0x00, 0xC0 + 62], MOST), Ok(fields(&[(b"x-xss-protection", b"1; mode=block")])));
    assert_eq!(decode(&[0x00, 0x00, 0xFF, 98 - 63], MOST), Ok(fields(&[(b"x-frame-options", b"sameorigin")])), "the last entry");
    assert_eq!(decode(&[0x00, 0x00, 0xFF, 99 - 63], MOST), failed, "one past the last entry");
    assert_eq!(decode(&[0x00, 0x00, 0x51, 0x05, b'a'], MOST), failed, "a value cut short");
    assert_eq!(decode(&[0x00, 0x00, 0x51], MOST), failed, "no value");
    assert_eq!(decode(&[0x00, 0x00, 0x51, 0x81, 0x18], MOST), failed, "Huffman padding of 0s");
    assert_eq!(decode(&[0x00, 0x00, 0x21, b'a'], MOST), failed, "a literal name and no value");
    // An integer of ten continuation bytes, and one that passes 2^62.
    assert_eq!(decode(&[0x00, 0x00, 0xFF, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x01], MOST), failed);
    assert_eq!(decode(&[0x00, 0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x7F], MOST), failed);
}

/// A section whose fields measure `size` by RFC 9114 section 4.2.2.
fn section_of(size: u64) -> Vec<u8> {
    let value = vec![b'v'; usize::try_from(size - FIELD_OVERHEAD_BYTES - 1).unwrap()];
    let mut section = Vec::new();
    encode([(&b"n"[..], value.as_slice())], &mut section);
    section
}

/// REQ-6967: a field section of exactly the bound decodes, and one byte
/// over is `H3_EXCESSIVE_LOAD`; so is a section of many small fields,
/// whose 32 bytes each count (RFC 9114 section 4.2.2), and a Huffman
/// string that would decode past the bound.
#[test]
fn a_field_section_is_bounded_by_its_measure() {
    let excessive = Err(Error::connection(Code::ExcessiveLoad));
    assert_eq!(decode(&section_of(MOST), MOST).map(|fields| fields.len()), Ok(1));
    assert_eq!(decode(&section_of(MOST + 1), MOST), excessive);
    // :path "/" by index measures 6 + 32, in one byte.
    let small = 6 + FIELD_OVERHEAD_BYTES;
    let fit = usize::try_from(MOST / small).unwrap();
    let mut section = vec![0x00, 0x00];
    section.extend(vec![0xC1; fit]);
    assert_eq!(decode(&section, MOST).map(|fields| fields.len()), Ok(fit));
    section.push(0xC1);
    assert_eq!(decode(&section, MOST), excessive);
    // '0' is five bits of 0: seven of them are 35 bits, padded with five 1s.
    let mut huffman = vec![0x00, 0x00, 0x51, 0x80 + 5, 0x00, 0x00, 0x00, 0x00, 0x1F];
    assert_eq!(decode(&huffman, FIELD_OVERHEAD_BYTES + 5 + 7), Ok(fields(&[(b":path", b"0000000")])));
    assert_eq!(decode(&huffman, FIELD_OVERHEAD_BYTES + 5 + 6), excessive);
    // Cut to four bytes that the length still says are five.
    huffman.truncate(8);
    assert_eq!(decode(&huffman, MOST), Err(Error::connection(Code::QpackDecompressionFailed)));
}

/// What the encoder writes, the decoder reads: static entries, static
/// names, literal names, and lengths past each integer prefix.
#[test]
fn what_is_encoded_decodes() {
    let long = vec![b'x'; 300];
    let pairs: [(&[u8], &[u8]); 7] = [
        (b":status", b"200"),
        (b":status", b"426"),
        (b"sec-websocket-protocol", b"neoteletext"),
        (b"content-type", b"application/json"),
        (b"x-long", &long),
        (&long, b""),
        (b"alt-svc", b"h3=\":443\""),
    ];
    let mut section = Vec::new();
    encode(pairs, &mut section);
    assert_eq!(section[..3], [0x00, 0x00, 0xC0 + 25], ":status 200 is one byte");
    assert_eq!(decode(&section, MOST), Ok(fields(&pairs)));
}
