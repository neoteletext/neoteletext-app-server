//! A viewer's `Cookie` header, read for the session cookie (requirement
//! 5.5): any bytes. No panic, and nothing names a session but a cookie
//! signed with the key.
#![no_main]

use libfuzzer_sys::fuzz_target;
use teletext_runtime::session_cookie::{KEY_BYTES, session_of};

/// A key the fuzzer's inputs cannot know the signatures of.
const KEY: [u8; KEY_BYTES] = [0x5A; KEY_BYTES];
/// A time before every expiry a cookie may name, and after none.
const NOW_S: u64 = 1_800_000_000;

fuzz_target!(|data: &[u8]| {
    assert_eq!(session_of(data, "fuzz", NOW_S, &[&KEY]), None, "an unsigned header names no session");
});
