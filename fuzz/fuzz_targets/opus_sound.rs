//! A program's Opus packets (requirement 10.1): any bytes as a live sound's
//! packets, of any length the protocol allows, through the server's check
//! and the engine's carriage. No panic; a line is refused whole or sent.
#![no_main]

use libfuzzer_sys::fuzz_target;
use teletext_runtime::{Channel, ChannelConfig, Effect, HandleSecret, InitialPage, RandomSource, RandomUnavailable};

/// The longest packet the protocol takes, and one more, so the refusal is reached.
const LENGTHS: u8 = 42;

/// Random bytes that count, so that a run repeats.
struct Counting(u8);

impl RandomSource for Counting {
    fn fill(&mut self, bytes: &mut [u8]) -> Result<(), RandomUnavailable> {
        for byte in bytes {
            self.0 = self.0.wrapping_add(1);
            *byte = self.0;
        }
        Ok(())
    }
}

fuzz_target!(|data: &[u8]| {
    let Some((&length, packets)) = data.split_first() else { return };
    let config = ChannelConfig { name: "Fuzz".into(), address: "http://127.0.0.1/fuzz/".into(), initial_page: InitialPage::new(0x100).unwrap(), epoch_at_start_ms: 0, other_addresses: Vec::new(), wheel: false };
    let mut channel = Channel::new(config, &HandleSecret::new([0; 32]), Box::new(Counting(0)));
    channel.program_started(0);
    channel.opened(1, None, 0);
    let _ = channel.take_effects();
    let mut encoded = vec![0_u8; neoteletext_web::base64::encoded_len(packets.len())];
    let Ok(written) = neoteletext_web::base64::encode(packets, &mut encoded) else { return };
    let text = String::from_utf8_lossy(&encoded[..written]);
    let line = format!(
        r#"{{"type":"sound","kind":"opus_packets","connections":[1],"handle":1,"generation":0,"packet_bytes":{},"first":0,"packets":"{text}"}}"#,
        length % LENGTHS
    );
    channel.from_program(line.as_bytes(), 0);
    let effects = channel.take_effects();
    let refused = effects.iter().any(|effect| matches!(effect, Effect::ToProgram { line, .. } if line.starts_with(br#"{"type":"refused""#)));
    let sent = effects.iter().any(|effect| matches!(effect, Effect::Send { .. }));
    assert!(refused != sent, "refused whole or sent: {effects:?}");
});
