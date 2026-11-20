//! The PROXY protocol's binary version 2 header, read without input or
//! output (REQ-6974, DEC-6972): what a layer 4 proxy that relays a
//! viewer's TLS connection still encrypted puts at the start of the TCP
//! connection, so that the server learns the address the viewer connected
//! from. The specification is `HAProxy` Technologies' "The PROXY protocol,
//! Versions 1 & 2" (`proxy-protocol.txt`), section 2.2, cited here as
//! "section".
//!
//! [`read`] takes the bytes that have come so far and answers with the
//! header and its exact length, with how many bytes it needs before it
//! can say more, or with a refusal. It never asks for a byte past the
//! header's end, so whoever carries the bytes can leave what follows, the
//! TLS handshake, unread. Who may send a header, and how long it may
//! take, is the host's to decide (`serving`); this only reads it.
//!
//! The human-readable version 1 header (section 2.1) is not read: its
//! first byte is not the signature's, so it is refused.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use crate::limits::MAX_PROXY_HEADER_BYTES;

/// The 12 bytes every version 2 header starts with (section 2.2, "the
/// protocol signature").
pub const SIGNATURE: [u8; 12] = [0x0D, 0x0A, 0x0D, 0x0A, 0x00, 0x0D, 0x0A, 0x51, 0x55, 0x49, 0x54, 0x0A];

/// The thirteenth byte, the version and command; the fourteenth, the
/// address family and transport protocol; and the fifteenth and
/// sixteenth, the length of what follows (section 2.2's `proxy_hdr_v2`).
const VERSION_COMMAND_BYTES: usize = 1;
const FAMILY_PROTOCOL_BYTES: usize = 1;
const LENGTH_BYTES: usize = 2;

/// The header's fixed part, after which its whole length is known: "the
/// length of the protocol header in bytes is always exactly 16 + this
/// value" (section 2.2).
pub const FIXED_BYTES: usize = SIGNATURE.len() + VERSION_COMMAND_BYTES + FAMILY_PROTOCOL_BYTES + LENGTH_BYTES;

/// The version is the highest four bits of the thirteenth byte and "must
/// always be sent as \x2 and the receiver must only accept this value";
/// the command is the lowest four (section 2.2).
const VERSION_BITS: u8 = 0xF0;
const VERSION_2: u8 = 0x20;
const COMMAND_BITS: u8 = 0x0F;
/// "\x0 : LOCAL : the connection was established on purpose by the proxy
/// without being relayed" (section 2.2).
const COMMAND_LOCAL: u8 = 0x0;
/// "\x1 : PROXY : the connection was established on behalf of another
/// node, and reflects the original connection endpoints" (section 2.2).
const COMMAND_PROXY: u8 = 0x1;

/// The fourteenth byte's two values taken with a PROXY command (section
/// 2.2, "the following protocol bytes are expected"): "\x11 : TCP over
/// IPv4" and "\x21 : TCP over IPv6". Every other value is refused: the
/// unspecified `\x00`, UDP's `\x12` and `\x22`, the Unix sockets' `\x31`
/// and `\x32`, and the values the section leaves unspecified.
const TCP_OVER_IPV4: u8 = 0x11;
const TCP_OVER_IPV6: u8 = 0x21;

/// The sizes of section 2.2's address blocks: two addresses, the source
/// first, then two ports, the source's first, each "in network byte
/// order".
const IPV4_BYTES: usize = 4;
const IPV6_BYTES: usize = 16;
const PORT_BYTES: usize = 2;
/// "Address length is 2*4 + 2*2 = 12 bytes."
const IPV4_BLOCK_BYTES: usize = 2 * IPV4_BYTES + 2 * PORT_BYTES;
/// "Address length is 2*16 + 2*2 = 36 bytes."
const IPV6_BLOCK_BYTES: usize = 2 * IPV6_BYTES + 2 * PORT_BYTES;

const _: () = assert!(FIXED_BYTES + IPV6_BLOCK_BYTES <= MAX_PROXY_HEADER_BYTES, "the bound must hold the longest header with no TLV vectors");

/// What a header says of the connection it starts.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Header {
    /// A LOCAL command: the proxy's own connection, such as a health
    /// check. "The receiver must accept this connection as valid and must
    /// use the real connection endpoints" (section 2.2), so the TCP
    /// peer's address stands.
    Local,
    /// A PROXY command over TCP: the address and port the viewer
    /// connected from, the source of the address block. An IPv4 address
    /// written as an IPv4-mapped IPv6 one is given as the IPv4 address,
    /// so that one viewer is counted under one address.
    Proxied(SocketAddr),
}

/// What [`read`] made of the bytes so far.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Read {
    /// The whole header is there.
    Whole {
        /// What it says.
        header: Header,
        /// Its length: the bytes that are the header's. What follows them
        /// is the connection's own.
        length: usize,
    },
    /// The header is not all there yet, and nothing so far refuses it.
    More {
        /// How many bytes, counted from the connection's first, must have
        /// come before `read` can say more: more than were given, never
        /// past the header's end, and at most `MAX_PROXY_HEADER_BYTES`.
        needed: usize,
    },
}

/// Why bytes are not a header this server takes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Refused {
    /// They do not start with [`SIGNATURE`]: "the protocol is not covered
    /// by this specification and the connection must be dropped" (section
    /// 2.2). TLS, HTTP and the version 1 header all end here.
    Signature,
    /// The version is not 2.
    Version,
    /// The command is neither LOCAL nor PROXY: "Receivers must drop
    /// connections presenting unexpected values here" (section 2.2).
    Command,
    /// A PROXY command for anything but TCP over IPv4 or IPv6.
    Family,
    /// A PROXY command whose stated length is shorter than its family's
    /// address block.
    AddressLength,
    /// The whole header would be longer than `MAX_PROXY_HEADER_BYTES`.
    TooLong,
}

/// Reads a version 2 header from `bytes`, the first bytes of a connection
/// as far as they have come.
///
/// # Errors
/// Why the bytes are refused, as soon as they show it: a wrong signature
/// at its first wrong byte, everything else once the fixed part is there
/// and before any of the rest is asked for.
pub fn read(bytes: &[u8]) -> Result<Read, Refused> {
    if bytes.iter().zip(SIGNATURE).any(|(&given, signature)| given != signature) {
        return Err(Refused::Signature);
    }
    let Some((fixed, rest)) = bytes.split_first_chunk::<FIXED_BYTES>() else { return Ok(Read::More { needed: FIXED_BYTES }) };
    let [.., version_command, family_protocol, length_high, length_low] = *fixed;
    if version_command & VERSION_BITS != VERSION_2 {
        return Err(Refused::Version);
    }
    let command = version_command & COMMAND_BITS;
    let block = match (command, family_protocol) {
        // "discard the protocol block including the family which is
        // ignored" (section 2.2).
        (COMMAND_LOCAL, _) => 0,
        (COMMAND_PROXY, TCP_OVER_IPV4) => IPV4_BLOCK_BYTES,
        (COMMAND_PROXY, TCP_OVER_IPV6) => IPV6_BLOCK_BYTES,
        (COMMAND_PROXY, _) => return Err(Refused::Family),
        _ => return Err(Refused::Command),
    };
    // "The 15th and 16th bytes is the address length in bytes in network
    // endian order" (section 2.2).
    let stated = usize::from(u16::from_be_bytes([length_high, length_low]));
    if stated < block {
        return Err(Refused::AddressLength);
    }
    let length = FIXED_BYTES.saturating_add(stated);
    if length > MAX_PROXY_HEADER_BYTES {
        return Err(Refused::TooLong);
    }
    // "The receiver must not start to parse an address before the whole
    // address block is received" (section 2.2). What `stated` holds past
    // the address block is TLV vectors, which "a receiver may choose to
    // skip over and ignore", and for LOCAL it is skipped whole:
    // "Receivers MUST always consider this field to skip the appropriate
    // number of bytes and must not assume zero is presented for LOCAL
    // connections".
    let Some(following) = rest.get(..stated) else { return Ok(Read::More { needed: length }) };
    let source = match family_protocol {
        _ if command == COMMAND_LOCAL => return Ok(Read::Whole { header: Header::Local, length }),
        TCP_OVER_IPV4 => source::<IPV4_BYTES>(following).map(|(address, port)| (IpAddr::V4(Ipv4Addr::from(address)), port)),
        _ => source::<IPV6_BYTES>(following).map(|(address, port)| (IpAddr::V6(Ipv6Addr::from(address)).to_canonical(), port)),
    };
    // `stated` was held to the block's length above, so the block is there.
    let (address, port) = source.ok_or(Refused::AddressLength)?;
    Ok(Read::Whole { header: Header::Proxied(SocketAddr::new(address, port)), length })
}

/// The source address and port of an address block whose addresses are
/// `ADDRESS` bytes: "source layer 3 address", "destination layer 3
/// address", "source layer 4 address", in that order (section 2.2). The
/// destination is where the proxy was reached, which the server has no
/// use for.
fn source<const ADDRESS: usize>(block: &[u8]) -> Option<([u8; ADDRESS], u16)> {
    let (source, rest) = block.split_first_chunk::<ADDRESS>()?;
    let (_destination, rest) = rest.split_first_chunk::<ADDRESS>()?;
    let (port, _) = rest.split_first_chunk::<PORT_BYTES>()?;
    Some((*source, u16::from_be_bytes(*port)))
}

#[cfg(test)]
mod tests;
