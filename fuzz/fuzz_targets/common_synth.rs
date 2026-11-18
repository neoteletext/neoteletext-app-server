//! What the synthesizer fuzz targets share: a channel with one open
//! connection, base64, and a line of each synthesizer family's kind.

use teletext_runtime::{Channel, ChannelConfig, HandleSecret, InitialPage, RandomSource, RandomUnavailable};

/// The note families' stored kinds, and the fields each needs beyond the
/// common ones (synth8's keyboard).
pub const NOTE_KINDS: [(&str, &str); 9] = [
    ("synth7", ""),
    ("synth106", ""),
    ("synth60", ""),
    ("synthd", ""),
    ("synth53", ""),
    ("synth52", ""),
    ("synth51", ""),
    ("synth8", r#","key_mode":"split","split_key":60"#),
    ("synth80", ""),
];

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

/// A channel whose program has started, with connection 1 open.
pub fn channel() -> Channel {
    let config = ChannelConfig {
        name: "Fuzz".into(),
        address: "http://127.0.0.1/fuzz/".into(),
        initial_page: InitialPage::new(0x100).unwrap(),
        epoch_at_start_ms: 0,
        other_addresses: Vec::new(),
        wheel: false,
        // Every page has the default table, so that input is acted on
        // (page-rules.md RULE-53).
        default_rules: teletext_runtime::channel::DefaultRules::SendAll,
    };
    let mut channel = Channel::new(config, &HandleSecret::new([0; 32]), Box::new(Counting(0)));
    channel.program_started(0);
    channel.opened(1, None, 0);
    let _ = channel.take_effects();
    channel
}

/// Exact base64 of `bytes`.
pub fn base64(bytes: &[u8]) -> String {
    let mut encoded = vec![0_u8; neoteletext_web::base64::encoded_len(bytes.len())];
    let written = neoteletext_web::base64::encode(bytes, &mut encoded).unwrap_or(0);
    String::from_utf8_lossy(&encoded[..written]).into_owned()
}
