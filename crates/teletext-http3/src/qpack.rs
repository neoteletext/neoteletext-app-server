//! QPACK field sections (RFC 9204) with the static table alone.
//!
//! This endpoint tells its peer the dynamic table has no capacity and no
//! stream may wait on it ([`crate::settings::ours`]), so a field section
//! that refers to the dynamic table cannot be decoded: it is the
//! connection error `QPACK_DECOMPRESSION_FAILED` (RFC 9204 section 2.2).
//! The decoder therefore holds nothing between field sections, and needs
//! no encoder or decoder stream of its own. The encoder writes static
//! entries and literals without the Huffman code, which any decoder reads.

use alloc::vec::Vec;

use crate::error::{Code, Error};
use crate::limits::{FIELD_OVERHEAD_BYTES, MAX_PREFIXED_INTEGER};

mod huffman;
mod huffman_table;
mod static_table;

pub use static_table::ENTRIES as STATIC_TABLE;

/// A field line: a name and a value.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Field {
    /// The name.
    pub name: Vec<u8>,
    /// The value.
    pub value: Vec<u8>,
}

impl Field {
    /// A field of `name` and `value`.
    #[must_use]
    pub fn new(name: &[u8], value: &[u8]) -> Self {
        Self { name: name.to_vec(), value: value.to_vec() }
    }
}

// RFC 9204 section 4.5's field line representations, by their first
// byte's leading bits.
/// An indexed field line (section 4.5.2): `1 T index(6+)`.
const INDEXED: u8 = 0x80;
/// Its T bit: the static table.
const INDEXED_STATIC: u8 = 0x40;
const INDEXED_PREFIX_BITS: u32 = 6;
/// A literal field line with a name reference (section 4.5.4):
/// `0 1 N T index(4+)`.
const NAME_REFERENCE: u8 = 0x40;
/// Its T bit.
const NAME_REFERENCE_STATIC: u8 = 0x10;
const NAME_REFERENCE_PREFIX_BITS: u32 = 4;
/// A literal field line with a literal name (section 4.5.6):
/// `0 0 1 N H length(3+)`.
const LITERAL_NAME: u8 = 0x20;
const LITERAL_NAME_PREFIX_BITS: u32 = 3;
/// A string literal's length prefix where it starts a byte: `H length(7+)`.
const VALUE_PREFIX_BITS: u32 = 7;
/// The encoded field section prefix (section 4.5.1): the Required Insert
/// Count, of an 8-bit prefix, then a sign bit and the Delta Base.
const INSERT_COUNT_PREFIX_BITS: u32 = 8;
const BASE_SIGN: u8 = 0x80;
const BASE_PREFIX_BITS: u32 = 7;
/// The bits of a prefixed integer's continuation byte that carry value
/// (RFC 7541 section 5.1), the bit that says another follows, and the
/// furthest a byte's bits are shifted before the value would pass 2^63.
const CONTINUATION_VALUE: u8 = 0x7F;
const CONTINUATION_MORE: u8 = 0x80;
const CONTINUATION_BITS: u32 = 7;
const MAX_CONTINUATION_SHIFT: u32 = 56;

const FAILED: Error = Error::connection(Code::QpackDecompressionFailed);
const EXCESSIVE: Error = Error::connection(Code::ExcessiveLoad);

/// The bytes of an encoded field section, read from the front.
struct Cursor<'section> {
    rest: &'section [u8],
}

impl<'section> Cursor<'section> {
    fn peek(&self) -> Result<u8, Error> {
        self.rest.first().copied().ok_or(FAILED)
    }

    fn byte(&mut self) -> Result<u8, Error> {
        let (&first, rest) = self.rest.split_first().ok_or(FAILED)?;
        self.rest = rest;
        Ok(first)
    }

    fn bytes(&mut self, count: u64) -> Result<&'section [u8], Error> {
        let count = usize::try_from(count).map_err(|_| FAILED)?;
        let (taken, rest) = self.rest.split_at_checked(count).ok_or(FAILED)?;
        self.rest = rest;
        Ok(taken)
    }

    /// A prefixed integer (RFC 9204 section 4.1.1, RFC 7541 section 5.1)
    /// in the low `prefix_bits` of the next byte.
    fn integer(&mut self, prefix_bits: u32) -> Result<u64, Error> {
        let mask = u8::MAX.checked_shr(u8::BITS.saturating_sub(prefix_bits)).unwrap_or(0);
        let mut value = u64::from(self.byte()? & mask);
        if value < u64::from(mask) {
            return Ok(value);
        }
        let mut shift = 0_u32;
        loop {
            let byte = self.byte()?;
            if shift > MAX_CONTINUATION_SHIFT {
                return Err(FAILED);
            }
            let part = u64::from(byte & CONTINUATION_VALUE).checked_shl(shift).ok_or(FAILED)?;
            value = value.checked_add(part).filter(|&value| value <= MAX_PREFIXED_INTEGER).ok_or(FAILED)?;
            if byte & CONTINUATION_MORE == 0 {
                return Ok(value);
            }
            shift = shift.saturating_add(CONTINUATION_BITS);
        }
    }

    /// A string literal (RFC 9204 section 4.1.2) whose length is in the
    /// low `prefix_bits` of the next byte, the bit above them saying
    /// whether it is in the Huffman code; at most `limit` bytes of it.
    fn string(&mut self, prefix_bits: u32, limit: u64) -> Result<Vec<u8>, Error> {
        let huffman = self.peek()? & 1_u8.checked_shl(prefix_bits).unwrap_or(0) != 0;
        let length = self.integer(prefix_bits)?;
        let encoded = self.bytes(length)?;
        let limit = usize::try_from(limit).unwrap_or(usize::MAX);
        if !huffman {
            return if encoded.len() > limit { Err(EXCESSIVE) } else { Ok(encoded.to_vec()) };
        }
        let mut decoded = Vec::new();
        match huffman::decode(encoded, limit, &mut decoded) {
            Ok(()) => Ok(decoded),
            Err(huffman::Fault::Malformed) => Err(FAILED),
            Err(huffman::Fault::TooLong) => Err(EXCESSIVE),
        }
    }
}

/// The static table's entry `index`.
fn entry(index: u64) -> Result<(&'static [u8], &'static [u8]), Error> {
    usize::try_from(index).ok().and_then(|index| STATIC_TABLE.get(index)).copied().ok_or(FAILED)
}

/// Decodes an encoded field section, a HEADERS frame's payload, of at
/// most `max_section_bytes` by RFC 9114 section 4.2.2's measure: each
/// field's name and value lengths and 32 more.
///
/// # Errors
/// A connection error. `QPACK_DECOMPRESSION_FAILED`: a reference to the
/// dynamic table, an index past the static table's end, a string that is
/// not of the Huffman code, an integer over the bound, or a section that
/// ends inside a field line. `H3_EXCESSIVE_LOAD`: a section over
/// `max_section_bytes`.
pub fn decode(section: &[u8], max_section_bytes: u64) -> Result<Vec<Field>, Error> {
    let mut cursor = Cursor { rest: section };
    // RFC 9204 section 4.5.1: with no dynamic table the Required Insert
    // Count is 0, and a Base below it does not exist.
    if cursor.integer(INSERT_COUNT_PREFIX_BITS)? != 0 || cursor.peek()? & BASE_SIGN != 0 {
        return Err(FAILED);
    }
    cursor.integer(BASE_PREFIX_BITS)?;
    let mut fields = Vec::new();
    let mut left = max_section_bytes;
    while !cursor.rest.is_empty() {
        left = left.checked_sub(FIELD_OVERHEAD_BYTES).ok_or(EXCESSIVE)?;
        let first = cursor.peek()?;
        let (name, value) = if first & INDEXED != 0 {
            if first & INDEXED_STATIC == 0 {
                return Err(FAILED);
            }
            let (name, value) = entry(cursor.integer(INDEXED_PREFIX_BITS)?)?;
            (name.to_vec(), value.to_vec())
        } else if first & NAME_REFERENCE != 0 {
            if first & NAME_REFERENCE_STATIC == 0 {
                return Err(FAILED);
            }
            let (name, _) = entry(cursor.integer(NAME_REFERENCE_PREFIX_BITS)?)?;
            (name.to_vec(), cursor.string(VALUE_PREFIX_BITS, left)?)
        } else if first & LITERAL_NAME != 0 {
            let name = cursor.string(LITERAL_NAME_PREFIX_BITS, left)?;
            let value = cursor.string(VALUE_PREFIX_BITS, left)?;
            (name, value)
        } else {
            // Sections 4.5.3 and 4.5.5: an index or a name past the Base,
            // which only the dynamic table has.
            return Err(FAILED);
        };
        let size = u64::try_from(name.len().saturating_add(value.len())).unwrap_or(u64::MAX);
        left = left.checked_sub(size).ok_or(EXCESSIVE)?;
        fields.push(Field { name, value });
    }
    Ok(fields)
}

/// Appends a prefixed integer (RFC 7541 section 5.1) of `prefix_bits`,
/// under the leading bits `pattern`.
fn integer(pattern: u8, prefix_bits: u32, value: u64, out: &mut Vec<u8>) {
    let mask = u8::MAX.checked_shr(u8::BITS.saturating_sub(prefix_bits)).unwrap_or(0);
    if value < u64::from(mask) {
        out.push(pattern | u8::try_from(value).unwrap_or(mask));
        return;
    }
    out.push(pattern | mask);
    let mut rest = value.saturating_sub(u64::from(mask));
    while rest > u64::from(CONTINUATION_VALUE) {
        out.push(CONTINUATION_MORE | u8::try_from(rest & u64::from(CONTINUATION_VALUE)).unwrap_or(0));
        rest >>= CONTINUATION_BITS;
    }
    out.push(u8::try_from(rest).unwrap_or(0));
}

/// Appends a string literal under `pattern`, not in the Huffman code.
fn string(pattern: u8, prefix_bits: u32, text: &[u8], out: &mut Vec<u8>) {
    integer(pattern, prefix_bits, u64::try_from(text.len()).unwrap_or(u64::MAX), out);
    out.extend_from_slice(text);
}

/// Appends the encoded field section of `fields`, each a name and a
/// value, in their order: an entry of the static table where a field is
/// one, its name from there where only that is, and literals otherwise.
pub fn encode<'field>(fields: impl IntoIterator<Item = (&'field [u8], &'field [u8])>, out: &mut Vec<u8>) {
    // RFC 9204 section 4.5.1: Required Insert Count 0, Base 0.
    out.extend_from_slice(&[0, 0]);
    for (name, value) in fields {
        let index = |position: usize| u64::try_from(position).unwrap_or(u64::MAX);
        if let Some(whole) = STATIC_TABLE.iter().position(|&entry| entry == (name, value)) {
            integer(INDEXED | INDEXED_STATIC, INDEXED_PREFIX_BITS, index(whole), out);
        } else if let Some(named) = STATIC_TABLE.iter().position(|&(entry, _)| entry == name) {
            integer(NAME_REFERENCE | NAME_REFERENCE_STATIC, NAME_REFERENCE_PREFIX_BITS, index(named), out);
            string(0, VALUE_PREFIX_BITS, value, out);
        } else {
            string(LITERAL_NAME, LITERAL_NAME_PREFIX_BITS, name, out);
            string(0, VALUE_PREFIX_BITS, value, out);
        }
    }
}

#[cfg(test)]
mod tests;
