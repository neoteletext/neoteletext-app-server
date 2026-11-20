//! Decoding the Huffman code of RFC 7541 appendix B (section 5.2 there),
//! a bit at a time down a tree made from the table when the crate is
//! compiled.

use alloc::vec::Vec;

use super::huffman_table::{CODES, EOS, SYMBOLS};

/// A complete binary code of [`SYMBOLS`] leaves has one node fewer that
/// branches.
const BRANCHES: usize = SYMBOLS - 1;
/// Marks a tree entry that is a symbol and not the next node.
const LEAF: u16 = 0x8000;
/// The longest padding RFC 7541 section 5.2 allows: under one byte.
const MAX_PADDING_BITS: u32 = 7;

/// The tree: for each branching node, what a 0 bit and a 1 bit lead to,
/// the number of another node or a symbol marked [`LEAF`]. Node 0 is the
/// root, which nothing leads to, so 0 marks an entry not yet made; the
/// tests check that none is left.
const TREE: [[u16; 2]; BRANCHES] = tree();

#[allow(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    reason = "evaluated when the crate is compiled, where a fault fails the build; every number here is under 2^15"
)]
const fn tree() -> [[u16; 2]; BRANCHES] {
    let mut nodes = [[0_u16; 2]; BRANCHES];
    let mut made = 1_usize;
    let mut symbol = 0_usize;
    while symbol < SYMBOLS {
        let (code, bits) = CODES[symbol];
        let mut node = 0_usize;
        let mut bit = bits;
        while bit > 0 {
            bit -= 1;
            let branch = ((code >> bit) & 1) as usize;
            if bit == 0 {
                nodes[node][branch] = LEAF | symbol as u16;
            } else {
                if nodes[node][branch] == 0 {
                    nodes[node][branch] = made as u16;
                    made += 1;
                }
                node = nodes[node][branch] as usize;
            }
        }
        symbol += 1;
    }
    nodes
}

/// Why a string could not be decoded.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Fault {
    /// Not a string of the code: it holds EOS, or its padding is a byte or
    /// more, or is not the start of EOS (RFC 7541 section 5.2).
    Malformed,
    /// It decodes to more than the limit given.
    TooLong,
}

/// Appends what `encoded` decodes to, at most `limit` bytes of it.
///
/// # Errors
/// See [`Fault`].
pub fn decode(encoded: &[u8], limit: usize, out: &mut Vec<u8>) -> Result<(), Fault> {
    let start = out.len();
    let mut node = 0_u16;
    // The bits read since the last symbol, and whether all were 1.
    let mut pending = 0_u32;
    let mut ones = true;
    for &byte in encoded {
        for shift in (0..u8::BITS).rev() {
            let bit = (byte >> shift) & 1;
            pending = pending.saturating_add(1);
            ones &= bit == 1;
            let next = TREE.get(usize::from(node)).and_then(|branches| branches.get(usize::from(bit))).copied().ok_or(Fault::Malformed)?;
            if next & LEAF == 0 {
                node = next;
                continue;
            }
            let symbol = next & !LEAF;
            if symbol == EOS {
                return Err(Fault::Malformed);
            }
            if out.len().saturating_sub(start) >= limit {
                return Err(Fault::TooLong);
            }
            out.push(u8::try_from(symbol).map_err(|_| Fault::Malformed)?);
            node = 0;
            pending = 0;
            ones = true;
        }
    }
    if pending > MAX_PADDING_BITS || !ones { Err(Fault::Malformed) } else { Ok(()) }
}

#[cfg(test)]
#[allow(clippy::arithmetic_side_effects, reason = "a test")]
mod tests {
    use super::*;

    fn decoded(encoded: &[u8]) -> Result<Vec<u8>, Fault> {
        let mut out = Vec::new();
        decode(encoded, usize::MAX, &mut out).map(|()| out)
    }

    /// `text` in the code, padded with the start of EOS, by the table
    /// alone and not the tree.
    fn encoded(text: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let (mut bits, mut have) = (0_u64, 0_u32);
        for &byte in text {
            let (code, length) = CODES[usize::from(byte)];
            bits = (bits << length) | u64::from(code);
            have += u32::from(length);
            while have >= 8 {
                have -= 8;
                out.push(u8::try_from((bits >> have) & 0xFF).unwrap());
            }
        }
        if have > 0 {
            let padding = 8 - have;
            out.push(u8::try_from(((bits << padding) | ((1 << padding) - 1)) & 0xFF).unwrap());
        }
        out
    }

    /// The table is a complete prefix code (RFC 7541 appendix B): every
    /// entry of the tree was made, by exactly the symbols there are, and
    /// the code lengths fill the code space exactly.
    #[test]
    fn the_table_is_a_complete_prefix_code() {
        assert!(TREE.iter().flatten().all(|&entry| entry != 0));
        let mut leaves: Vec<u16> = TREE.iter().flatten().filter(|&&entry| entry & LEAF != 0).map(|&entry| entry & !LEAF).collect();
        leaves.sort_unstable();
        assert_eq!(leaves, (0..=EOS).collect::<Vec<_>>());
        let filled: u64 = CODES.iter().map(|&(_, bits)| 1_u64 << (32 - u32::from(bits))).sum();
        assert_eq!(filled, 1 << 32);
        assert_eq!(CODES[usize::from(EOS)], (0x3FFF_FFFF, 30));
    }

    /// REQ-6967: the Huffman-coded strings of RFC 7541 appendix C
    /// (sections C.4.1 to C.4.3 and C.6.1 to C.6.3) decode to what the RFC
    /// says.
    #[test]
    fn the_examples_of_rfc_7541_appendix_c_decode() {
        let examples: [(&[u8], &[u8]); 11] = [
            (&[0xF1, 0xE3, 0xC2, 0xE5, 0xF2, 0x3A, 0x6B, 0xA0, 0xAB, 0x90, 0xF4, 0xFF], b"www.example.com"),
            (&[0xA8, 0xEB, 0x10, 0x64, 0x9C, 0xBF], b"no-cache"),
            (&[0x25, 0xA8, 0x49, 0xE9, 0x5B, 0xA9, 0x7D, 0x7F], b"custom-key"),
            (&[0x25, 0xA8, 0x49, 0xE9, 0x5B, 0xB8, 0xE8, 0xB4, 0xBF], b"custom-value"),
            (&[0x64, 0x02], b"302"),
            (&[0xAE, 0xC3, 0x77, 0x1A, 0x4B], b"private"),
            (
                &[0xD0, 0x7A, 0xBE, 0x94, 0x10, 0x54, 0xD4, 0x44, 0xA8, 0x20, 0x05, 0x95, 0x04, 0x0B, 0x81, 0x66, 0xE0, 0x82, 0xA6, 0x2D, 0x1B, 0xFF],
                b"Mon, 21 Oct 2013 20:13:21 GMT",
            ),
            (&[0x9D, 0x29, 0xAD, 0x17, 0x18, 0x63, 0xC7, 0x8F, 0x0B, 0x97, 0xC8, 0xE9, 0xAE, 0x82, 0xAE, 0x43, 0xD3], b"https://www.example.com"),
            (&[0x64, 0x0E, 0xFF], b"307"),
            (&[0x9B, 0xD9, 0xAB], b"gzip"),
            (
                &[
                    0x94, 0xE7, 0x82, 0x1D, 0xD7, 0xF2, 0xE6, 0xC7, 0xB3, 0x35, 0xDF, 0xDF, 0xCD, 0x5B, 0x39, 0x60, 0xD5, 0xAF, 0x27, 0x08, 0x7F, 0x36, 0x72,
                    0xC1, 0xAB, 0x27, 0x0F, 0xB5, 0x29, 0x1F, 0x95, 0x87, 0x31, 0x60, 0x65, 0xC0, 0x03, 0xED, 0x4E, 0xE5, 0xB1, 0x06, 0x3D, 0x50, 0x07,
                ],
                b"foo=ASDJKHQKBZXOQWEOPIUAXQWEOIU; max-age=3600; version=1",
            ),
        ];
        for (code, text) in examples {
            assert_eq!(decoded(code).as_deref(), Ok(text));
            assert_eq!(encoded(text), code, "the table writes the RFC's bytes");
        }
    }

    /// Every octet, coded by the table, decodes to itself.
    #[test]
    fn every_octet_round_trips() {
        let all: Vec<u8> = (0..=u8::MAX).collect();
        assert_eq!(decoded(&encoded(&all)), Ok(all));
    }

    /// RFC 7541 section 5.2: padding of a byte or more, padding that is
    /// not the start of EOS, and EOS itself are each a decoding error.
    #[test]
    fn bad_padding_and_eos_are_malformed() {
        // 'a' is 00011 in 5 bits: the three bits of padding must be 1s.
        assert_eq!(decoded(&[0b0001_1111]), Ok(b"a".to_vec()));
        assert_eq!(decoded(&[0b0001_1000]), Err(Fault::Malformed));
        assert_eq!(decoded(&[0b0001_1111, 0xFF]), Err(Fault::Malformed), "a whole byte of padding");
        assert_eq!(decoded(&[0xFF, 0xFF, 0xFF, 0xFF]), Err(Fault::Malformed), "EOS");
        assert_eq!(decoded(&[]), Ok(Vec::new()));
    }

    /// A string is decoded up to the limit given and no further.
    #[test]
    fn decoding_stops_at_the_limit() {
        let code = encoded(b"abcd");
        let mut out = Vec::new();
        assert_eq!(decode(&code, 4, &mut out), Ok(()));
        out.clear();
        assert_eq!(decode(&code, 3, &mut out), Err(Fault::TooLong));
    }
}
