//! A program's Opus packets (requirement 10.1): any bytes as a live sound's
//! packets, of any length the protocol allows, through the server's check
//! and the engine's carriage. No panic; a line is refused whole or sent.
#![no_main]

use libfuzzer_sys::fuzz_target;
use teletext_runtime::remote::declared_pages;
use teletext_runtime::{Channel, ChannelConfig, Effect, Remote};

/// The longest packet the protocol takes, and one more, so the refusal is reached.
const LENGTHS: u8 = 42;

fuzz_target!(|data: &[u8]| {
    let Some((&length, packets)) = data.split_first() else { return };
    let Ok(pages) = declared_pages(br#"{"type":"pages","pages":[{"page":"100","title":"Index"}]}"#) else { return };
    let config = ChannelConfig { name: "Fuzz".into(), push_address: "ws://127.0.0.1/fuzz/push".into(), initial_page: 0x100, epoch_at_start_ms: 0, session_grace_ms: 0 };
    let mut channel = Channel::new(config, vec![Box::new(Remote::new(pages))], 1);
    channel.opened(1, 0);
    let _ = channel.take_effects();
    let mut encoded = vec![0_u8; neoteletext_web::base64::encoded_len(packets.len())];
    let Ok(written) = neoteletext_web::base64::encode(packets, &mut encoded) else { return };
    let text = String::from_utf8_lossy(&encoded[..written]);
    let line = format!(r#"{{"type":"sound","to":{{"session":1}},"opus_packets":{{"handle":1,"packet_bytes":{},"first":0,"packets":"{text}"}}}}"#, length % LENGTHS);
    channel.from_application(0, line.as_bytes(), 0);
    let effects = channel.take_effects();
    let refused = effects.iter().any(|effect| matches!(effect, Effect::Log { .. }));
    let sent = effects.iter().any(|effect| matches!(effect, Effect::Send { .. }));
    assert!(refused != sent, "refused whole or sent: {effects:?}");
});
