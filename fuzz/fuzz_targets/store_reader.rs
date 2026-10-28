//! What the server kept, given back to a channel (requirement 5.2): any
//! bytes under any key, and any name as a key. No panic, and a record is
//! taken whole or not at all.
#![no_main]

use libfuzzer_sys::fuzz_target;
use teletext_runtime::{Channel, ChannelConfig, Remote, StoreKey};

fuzz_target!(|data: &[u8]| {
    let Some((&kind, record)) = data.split_first() else { return };
    if let Ok(name) = std::str::from_utf8(record) {
        let _ = StoreKey::parse(name);
    }
    let config = ChannelConfig { name: "Fuzz".into(), push_address: "ws://127.0.0.1/fuzz/push".into(), initial_page: 0x100, epoch_at_start_ms: 0, session_grace_ms: 0 };
    let mut channel = Channel::new(config, vec![Box::new(Remote::new(Vec::new()))], 1);
    // The first byte chooses the key: a page of any number, or an application.
    let key = if kind & 0x80 == 0 { StoreKey::Page(0x100 | u16::from(kind)) } else { StoreKey::Pages(usize::from(kind & 0x03)) };
    if channel.load(key, record).is_err() {
        assert!(channel.take_effects().is_empty(), "a refused record changes nothing");
    }
    let _ = channel.resource("contents.txt");
    let _ = channel.resource("index.t42");
});
