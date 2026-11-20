//! The version 2 header reader on headers written here as section 2.2 of
//! the specification lays them out (REQ-6974).
#![allow(clippy::arithmetic_side_effects, clippy::indexing_slicing, reason = "a test")]

use std::net::SocketAddr;

use super::{FIXED_BYTES, Header, Read, Refused, SIGNATURE, read};
use crate::limits::MAX_PROXY_HEADER_BYTES;

/// Version 2 with the PROXY command, and with LOCAL.
const PROXY: u8 = 0x21;
const LOCAL: u8 = 0x20;
/// The family and protocol bytes section 2.2 lists.
const UNSPEC: u8 = 0x00;
const TCP4: u8 = 0x11;
const UDP4: u8 = 0x12;
const TCP6: u8 = 0x21;
const UDP6: u8 = 0x22;
const UNIX_STREAM: u8 = 0x31;
const UNIX_DATAGRAM: u8 = 0x32;
/// A Unix address block: 2 * 108 bytes.
const UNIX_BLOCK_BYTES: usize = 216;
/// `PP2_TYPE_ALPN` and `PP2_TYPE_NOOP` (section 2.2).
const TLV_ALPN: u8 = 0x01;
const TLV_NOOP: u8 = 0x04;

/// A header: the signature, the two bytes, the length of `following` and
/// `following`.
fn header(version_command: u8, family_protocol: u8, following: &[u8]) -> Vec<u8> {
    let mut bytes = SIGNATURE.to_vec();
    bytes.extend([version_command, family_protocol]);
    bytes.extend(u16::try_from(following.len()).unwrap().to_be_bytes());
    bytes.extend(following);
    bytes
}

/// The fixed part alone, stating `length` to follow.
fn fixed(version_command: u8, family_protocol: u8, length: u16) -> Vec<u8> {
    let mut bytes = SIGNATURE.to_vec();
    bytes.extend([version_command, family_protocol]);
    bytes.extend(length.to_be_bytes());
    bytes
}

/// The address block of a connection from `source` to `destination`.
fn block(source: &str, destination: &str) -> Vec<u8> {
    let (source, destination): (SocketAddr, SocketAddr) = (source.parse().unwrap(), destination.parse().unwrap());
    let mut bytes = Vec::new();
    for address in [source, destination] {
        match address {
            SocketAddr::V4(address) => bytes.extend(address.ip().octets()),
            SocketAddr::V6(address) => bytes.extend(address.ip().octets()),
        }
    }
    bytes.extend(source.port().to_be_bytes());
    bytes.extend(destination.port().to_be_bytes());
    bytes
}

/// A TLV vector: its type, its value's length in two bytes, its value.
fn tlv(kind: u8, value: &[u8]) -> Vec<u8> {
    let mut bytes = vec![kind];
    bytes.extend(u16::try_from(value.len()).unwrap().to_be_bytes());
    bytes.extend(value);
    bytes
}

fn proxied(source: &str, length: usize) -> Read {
    Read::Whole { header: Header::Proxied(source.parse().unwrap()), length }
}

/// REQ-6974 item 5: a PROXY command over TCP on IPv4 gives the source
/// address and port, and the header is 16 + 12 bytes whatever follows it.
#[test]
fn a_proxy_header_over_ipv4_gives_its_source() {
    let whole = header(PROXY, TCP4, &block("203.0.113.7:51234", "192.0.2.1:443"));
    assert_eq!(whole.len(), 28);
    assert_eq!(read(&whole), Ok(proxied("203.0.113.7:51234", 28)));
    // A TLS ClientHello's first bytes after it are not the header's.
    let mut followed = whole.clone();
    followed.extend([0x16, 0x03, 0x01, 0x02, 0x00]);
    assert_eq!(read(&followed), Ok(proxied("203.0.113.7:51234", 28)));
}

/// REQ-6974 item 5: over IPv6 the header is 16 + 36 bytes, and an
/// IPv4-mapped source is given as the IPv4 address.
#[test]
fn a_proxy_header_over_ipv6_gives_its_source() {
    let whole = header(PROXY, TCP6, &block("[2001:db8::7]:51234", "[2001:db8::1]:443"));
    assert_eq!(whole.len(), 52);
    assert_eq!(read(&whole), Ok(proxied("[2001:db8::7]:51234", 52)));
    let mapped = header(PROXY, TCP6, &block("[::ffff:203.0.113.7]:51234", "[::ffff:192.0.2.1]:443"));
    assert_eq!(read(&mapped), Ok(proxied("203.0.113.7:51234", 52)));
}

/// REQ-6974 item 6: LOCAL is taken whatever its family byte says, and the
/// length it states is skipped, not assumed zero.
#[test]
fn a_local_header_is_taken_and_its_length_skipped() {
    assert_eq!(read(&header(LOCAL, UNSPEC, &[])), Ok(Read::Whole { header: Header::Local, length: FIXED_BYTES }));
    let with_addresses = header(LOCAL, TCP4, &block("203.0.113.7:51234", "192.0.2.1:443"));
    assert_eq!(read(&with_addresses), Ok(Read::Whole { header: Header::Local, length: 28 }));
    for family_protocol in [UDP4, UNIX_STREAM, 0xFF] {
        let odd = header(LOCAL, family_protocol, &[0xAA; 5]);
        assert_eq!(read(&odd), Ok(Read::Whole { header: Header::Local, length: FIXED_BYTES + 5 }), "{family_protocol:#04x}");
        assert_eq!(read(&odd[..FIXED_BYTES + 4]), Ok(Read::More { needed: FIXED_BYTES + 5 }), "{family_protocol:#04x}");
    }
}

/// REQ-6974 item 7: TLV vectors after the address block are within the
/// header's length and change nothing of what it says, whatever they
/// hold, a vector whose own length runs past the header's end included.
#[test]
fn tlv_vectors_are_skipped_within_the_stated_length() {
    let addresses = block("203.0.113.7:51234", "192.0.2.1:443");
    let vectors = [tlv(TLV_NOOP, &[0; 3]), tlv(TLV_ALPN, b"h2")].concat();
    let whole = header(PROXY, TCP4, &[addresses.clone(), vectors.clone()].concat());
    assert_eq!(read(&whole), Ok(proxied("203.0.113.7:51234", 28 + vectors.len())));
    assert_eq!(read(&whole[..whole.len() - 1]), Ok(Read::More { needed: whole.len() }), "the last TLV byte is the header's");
    let overrunning = [TLV_ALPN, 0xFF, 0xFF, b'h'];
    let whole = header(PROXY, TCP6, &[block("[2001:db8::7]:51234", "[2001:db8::1]:443"), overrunning.to_vec()].concat());
    assert_eq!(read(&whole), Ok(proxied("[2001:db8::7]:51234", 52 + overrunning.len())));
}

/// REQ-6974 items 2 and 4: every truncation of a valid header asks for
/// the fixed part and then for exactly the whole header, never more.
#[test]
fn a_truncated_header_says_how_many_bytes_it_needs() {
    let whole = header(PROXY, TCP6, &[block("[2001:db8::7]:51234", "[2001:db8::1]:443"), tlv(TLV_NOOP, &[0; 9])].concat());
    for length in 0..whole.len() {
        let needed = if length < FIXED_BYTES { FIXED_BYTES } else { whole.len() };
        assert_eq!(read(&whole[..length]), Ok(Read::More { needed }), "{length} bytes");
    }
}

/// REQ-6974 item 4: bytes that are not the signature are refused at the
/// first that differs, with nothing more waited for: TLS, HTTP, and the
/// version 1 header among them.
#[test]
fn a_wrong_signature_is_refused_at_its_first_wrong_byte() {
    let whole = header(PROXY, TCP4, &block("203.0.113.7:51234", "192.0.2.1:443"));
    for at in 0..SIGNATURE.len() {
        let mut wrong = whole.clone();
        wrong[at] ^= 0x01;
        assert_eq!(read(&wrong), Err(Refused::Signature), "byte {at}");
        assert_eq!(read(&wrong[..=at]), Err(Refused::Signature), "byte {at}, the last to have come");
        assert_eq!(read(&wrong[..at]), Ok(Read::More { needed: FIXED_BYTES }), "byte {at}, not yet come");
    }
    let tls_client_hello = [0x16, 0x03, 0x01];
    let version_1 = b"PROXY TCP4 203.0.113.7 192.0.2.1 51234 443\r\n";
    for other in [&tls_client_hello[..], b"GET / HTTP/1.1\r\n", version_1, &tls_client_hello[..1]] {
        assert_eq!(read(other), Err(Refused::Signature), "{other:?}");
    }
}

/// REQ-6974 item 4: only version 2 is taken, and of its commands only
/// LOCAL and PROXY.
#[test]
fn a_bad_version_or_command_is_refused() {
    let addresses = block("203.0.113.7:51234", "192.0.2.1:443");
    for version_command in 0..=u8::MAX {
        let expected = match (version_command >> 4, version_command & 0x0F) {
            (2, 0) => Ok(Read::Whole { header: Header::Local, length: 28 }),
            (2, 1) => Ok(proxied("203.0.113.7:51234", 28)),
            (2, _) => Err(Refused::Command),
            _ => Err(Refused::Version),
        };
        assert_eq!(read(&header(version_command, TCP4, &addresses)), expected, "{version_command:#04x}");
    }
}

/// REQ-6974 item 4: with a PROXY command every family and protocol byte
/// but TCP over IPv4 and TCP over IPv6 is refused, on the fixed part
/// alone: the unspecified one, UDP, the Unix sockets, and the rest.
#[test]
fn every_family_but_tcp_over_ip_is_refused() {
    let following = [0_u8; UNIX_BLOCK_BYTES];
    for family_protocol in 0..=u8::MAX {
        let refused = !matches!(family_protocol, TCP4 | TCP6);
        assert_eq!(read(&header(PROXY, family_protocol, &following)) == Err(Refused::Family), refused, "{family_protocol:#04x}");
        assert_eq!(read(&fixed(PROXY, family_protocol, 216)) == Err(Refused::Family), refused, "{family_protocol:#04x}, the fixed part alone");
    }
    for named in [UNSPEC, UDP4, UDP6, UNIX_STREAM, UNIX_DATAGRAM] {
        assert_eq!(read(&header(PROXY, named, &following)), Err(Refused::Family), "{named:#04x}");
    }
}

/// REQ-6974 item 4: a PROXY command whose length is shorter than its
/// family's address block is refused, on the fixed part alone.
#[test]
fn a_length_shorter_than_the_address_block_is_refused() {
    for (family_protocol, block) in [(TCP4, 12_u16), (TCP6, 36)] {
        for short in [0, block - 1] {
            assert_eq!(read(&fixed(PROXY, family_protocol, short)), Err(Refused::AddressLength), "{family_protocol:#04x} with {short}");
        }
        assert_eq!(read(&fixed(PROXY, family_protocol, block)), Ok(Read::More { needed: FIXED_BYTES + usize::from(block) }));
    }
}

/// REQ-6974 item 8: a header of exactly `MAX_PROXY_HEADER_BYTES` is
/// taken, and one a byte longer is refused on its fixed part alone,
/// before any of the rest is asked for, whatever its command.
#[test]
fn a_header_over_the_bound_is_refused_before_the_rest_is_read() {
    assert_eq!(MAX_PROXY_HEADER_BYTES, 536, "section 2: 576 - 40");
    let most = u16::try_from(MAX_PROXY_HEADER_BYTES - FIXED_BYTES).unwrap();
    let mut longest = block("203.0.113.7:51234", "192.0.2.1:443");
    longest.resize(usize::from(most), 0);
    let whole = header(PROXY, TCP4, &longest);
    assert_eq!(whole.len(), MAX_PROXY_HEADER_BYTES);
    assert_eq!(read(&whole), Ok(proxied("203.0.113.7:51234", MAX_PROXY_HEADER_BYTES)));
    for (version_command, family_protocol) in [(PROXY, TCP4), (PROXY, TCP6), (LOCAL, UNSPEC)] {
        assert_eq!(read(&fixed(version_command, family_protocol, most)), Ok(Read::More { needed: MAX_PROXY_HEADER_BYTES }));
        for over in [most + 1, u16::MAX] {
            assert_eq!(read(&fixed(version_command, family_protocol, over)), Err(Refused::TooLong), "{version_command:#04x} stating {over}");
        }
    }
}
