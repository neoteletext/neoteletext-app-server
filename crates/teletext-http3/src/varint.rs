//! QUIC's variable-length integers (RFC 9000 section 16): the first byte's
//! two most significant bits give the length, 1, 2, 4 or 8 bytes, and the
//! rest, with the bytes that follow, the value in network byte order.

use alloc::vec::Vec;

use crate::limits::MAX_VARINT_BYTES;

/// The largest value: 62 bits (RFC 9000 section 16).
pub const MAX: u64 = (1 << 62) - 1;
/// The bits of the first byte that give the length.
const LENGTH_BITS: u8 = 0xC0;
/// How far right they are shifted to give the length's exponent.
const LENGTH_SHIFT: u32 = 6;
/// The largest value of each length but the last (RFC 9000 table 4).
const MAX_1: u64 = (1 << 6) - 1;
const MAX_2: u64 = (1 << 14) - 1;
const MAX_4: u64 = (1 << 30) - 1;
/// The first byte's length bits for 2, 4 and 8 bytes.
const PREFIX_2: u8 = 0x40;
const PREFIX_4: u8 = 0x80;
const PREFIX_8: u8 = 0xC0;

/// The bytes of the integer whose first byte is `first`.
#[must_use]
pub const fn length(first: u8) -> usize {
    1 << ((first & LENGTH_BITS) >> LENGTH_SHIFT)
}

/// The integer at the start of `input` and the bytes it takes, or `None`
/// when `input` ends before it does. Any encoding is accepted, the
/// shortest or not, as RFC 9000 section 16 allows.
#[must_use]
pub fn decode(input: &[u8]) -> Option<(u64, usize)> {
    let (&first, rest) = input.split_first()?;
    let bytes = length(first);
    let mut value = u64::from(first & !LENGTH_BITS);
    for &byte in rest.get(..bytes.checked_sub(1)?)? {
        value = (value << 8) | u64::from(byte);
    }
    Some((value, bytes))
}

/// Appends `value` in its shortest encoding. A value over [`MAX`] cannot
/// be written, and is written as [`MAX`]; nothing here makes one.
pub fn encode(value: u64, out: &mut Vec<u8>) {
    let value = value.min(MAX);
    let bytes = value.to_be_bytes();
    let (prefix, from) = if value <= MAX_1 {
        (0, 7)
    } else if value <= MAX_2 {
        (PREFIX_2, 6)
    } else if value <= MAX_4 {
        (PREFIX_4, 4)
    } else {
        (PREFIX_8, 0)
    };
    let tail = bytes.get(from..).unwrap_or(&[]);
    if let Some((&first, rest)) = tail.split_first() {
        out.push(first | prefix);
        out.extend_from_slice(rest);
    }
}

/// An integer read as its bytes arrive.
#[derive(Clone, Copy, Debug, Default)]
pub struct Reader {
    bytes: [u8; MAX_VARINT_BYTES],
    have: usize,
}

impl Reader {
    /// Takes bytes from the start of `input` until the integer is whole:
    /// how many it took, and the integer once it is.
    pub fn read(&mut self, input: &[u8]) -> (usize, Option<u64>) {
        let mut taken = 0_usize;
        for &byte in input {
            let Some(slot) = self.bytes.get_mut(self.have) else { break };
            *slot = byte;
            self.have = self.have.saturating_add(1);
            taken = taken.saturating_add(1);
            if let Some((value, _)) = decode(self.bytes.get(..self.have).unwrap_or(&[])) {
                *self = Self::default();
                return (taken, Some(value));
            }
        }
        (taken, None)
    }

    /// Whether part of an integer has been read.
    #[must_use]
    pub const fn started(&self) -> bool {
        self.have > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 9000 appendix A.1's examples, and section 16's note that an
    /// integer need not be in its shortest encoding (REQ-6967).
    #[test]
    fn the_examples_of_rfc_9000_decode() {
        assert_eq!(decode(&[0xC2, 0x19, 0x7C, 0x5E, 0xFF, 0x14, 0xE8, 0x8C]), Some((151_288_809_941_952_652, 8)));
        assert_eq!(decode(&[0x9D, 0x7F, 0x3E, 0x7D]), Some((494_878_333, 4)));
        assert_eq!(decode(&[0x7B, 0xBD]), Some((15_293, 2)));
        assert_eq!(decode(&[0x25]), Some((37, 1)));
        assert_eq!(decode(&[0x40, 0x25]), Some((37, 2)));
    }

    /// An integer cut short is not yet an integer.
    #[test]
    fn a_cut_integer_waits() {
        assert_eq!(decode(&[]), None);
        assert_eq!(decode(&[0x40]), None);
        assert_eq!(decode(&[0xC2, 0x19, 0x7C, 0x5E, 0xFF, 0x14, 0xE8]), None);
    }

    /// Each length's largest value and the next (RFC 9000 table 4) are
    /// written in the shortest encoding and read back.
    #[test]
    fn values_at_each_length_round_trip() {
        for (value, bytes) in [(0, 1), (MAX_1, 1), (MAX_1 + 1, 2), (MAX_2, 2), (MAX_2 + 1, 4), (MAX_4, 4), (MAX_4 + 1, 8), (MAX, 8)] {
            let mut out = Vec::new();
            encode(value, &mut out);
            assert_eq!(out.len(), bytes, "{value}");
            assert_eq!(decode(&out), Some((value, bytes)));
            let mut reader = Reader::default();
            let mut read = None;
            for byte in &out {
                assert_eq!(read, None);
                let (taken, value) = reader.read(core::slice::from_ref(byte));
                assert_eq!(taken, 1);
                read = value;
            }
            assert_eq!(read, Some(value));
        }
    }

    /// The reader takes no byte past the integer's end.
    #[test]
    fn the_reader_stops_at_the_integers_end() {
        let mut reader = Reader::default();
        assert_eq!(reader.read(&[0x7B, 0xBD, 0xFF]), (2, Some(15_293)));
        assert!(!reader.started());
    }
}
