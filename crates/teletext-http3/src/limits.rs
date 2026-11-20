//! Every bound this crate keeps on what a peer sends (REQ-6967), each
//! defined here and nowhere else.

/// The largest field section accepted, by RFC 9114 section 4.2.2's
/// measure: each field's name and value lengths and 32 bytes more. It is
/// what `SETTINGS_MAX_FIELD_SECTION_SIZE` tells the peer, and also the
/// largest HEADERS frame payload buffered, since no encoding this decoder
/// accepts is longer than the section it stands for. 16 KiB is the header
/// list bound hyper keeps for HTTP/2 by default, far above what a receiver
/// or a web browser sends a channel (a few hundred bytes), and small
/// enough that a connection's streams, each holding one, hold little.
pub const MAX_FIELD_SECTION_BYTES: u32 = 16 * 1024;

/// What RFC 9114 section 4.2.2 adds to each field's name and value lengths
/// when it measures a field section.
pub const FIELD_OVERHEAD_BYTES: u64 = 32;

/// The largest SETTINGS frame payload accepted. A setting is an identifier
/// and a value, each a variable-length integer of at most 8 bytes (RFC
/// 9114 section 7.2.4); web browsers send under ten settings, reserved
/// ones for exercise included, so 512 bytes holds at least 32 of the
/// longest and is never reached by a peer that means well.
pub const MAX_SETTINGS_BYTES: u64 = 512;

/// The longest variable-length integer (RFC 9000 section 16), which is the
/// whole payload of GOAWAY, `MAX_PUSH_ID` and `CANCEL_PUSH` (RFC 9114
/// sections 7.2.3, 7.2.6 and 7.2.7).
pub const MAX_VARINT_BYTES: usize = 8;

/// The largest value a QPACK prefixed integer may have here (RFC 9204
/// section 4.1.1 leaves the bound to the implementation): the largest QUIC
/// variable-length integer, 2^62 - 1, since no index or length of a field
/// section this crate accepts comes near it.
pub const MAX_PREFIXED_INTEGER: u64 = crate::varint::MAX;
