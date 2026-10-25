//! The server's WebSocket frame reader with any bytes, in pieces the
//! input chooses: it must never panic, and it must end.
#![no_main]

use libfuzzer_sys::fuzz_target;
use teletext_native::websocket::Reader;

fuzz_target!(|data: &[u8]| {
    let Some((&split, rest)) = data.split_first() else { return };
    let mut reader = Reader::new();
    for piece in rest.chunks(usize::from(split).max(1)) {
        reader.feed(piece);
        while let Ok(Some(_)) = reader.next_message() {}
    }
});
