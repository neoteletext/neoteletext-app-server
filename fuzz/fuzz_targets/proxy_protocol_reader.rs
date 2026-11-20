//! The PROXY protocol version 2 header reader with any bytes (REQ-6974):
//! it must never panic, and what it answers must hold together. A header
//! it takes ends within the bytes given and within the bound, reads the
//! same alone as with anything after it, and at every shorter length
//! asks for no more than itself; when it asks for more, it asks for more
//! than was given and never past the bound; and bytes it refuses stay
//! refused whatever follows them.
#![no_main]

use libfuzzer_sys::fuzz_target;
use teletext_native::limits::MAX_PROXY_HEADER_BYTES;
use teletext_native::proxy_protocol::{FIXED_BYTES, Read, read};

fuzz_target!(|data: &[u8]| {
    match read(data) {
        Ok(Read::Whole { header, length }) => {
            assert!((FIXED_BYTES..=MAX_PROXY_HEADER_BYTES).contains(&length) && length <= data.len());
            assert_eq!(read(&data[..length]), Ok(Read::Whole { header, length }), "the header alone reads the same");
            for shorter in 0..length {
                let needed = if shorter < FIXED_BYTES { FIXED_BYTES } else { length };
                assert_eq!(read(&data[..shorter]), Ok(Read::More { needed }), "{shorter} of {length} bytes");
            }
        }
        Ok(Read::More { needed }) => assert!(needed > data.len() && needed <= MAX_PROXY_HEADER_BYTES),
        Err(refused) => {
            let mut followed = data.to_vec();
            followed.extend_from_slice(&[0; FIXED_BYTES]);
            assert_eq!(read(&followed), Err(refused), "what is refused stays refused");
        }
    }
});
