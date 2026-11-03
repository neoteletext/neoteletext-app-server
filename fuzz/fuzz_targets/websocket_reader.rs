//! The server's WebSocket frame reader with any bytes, in pieces the
//! input chooses, at a viewer's limit and at a program's (REQ-1044): it
//! must never panic, and it must end.
#![no_main]

use libfuzzer_sys::fuzz_target;
use teletext_native::apps::LONGEST_PROGRAM_MESSAGE;
use teletext_native::websocket::Reader;

fuzz_target!(|data: &[u8]| {
    let Some((&split, rest)) = data.split_first() else { return };
    for mut reader in [Reader::new(), Reader::with_limit(LONGEST_PROGRAM_MESSAGE)] {
        for piece in rest.chunks(usize::from(split).max(1)) {
            reader.feed(piece);
            while let Ok(Some(_)) = reader.next_message() {}
        }
    }
});
