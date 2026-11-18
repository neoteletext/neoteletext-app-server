//! What the synthesizer fuzz targets share: a channel with one open
//! connection, base64, and a line of each synthesizer family's kind.

use teletext_runtime::{Channel, ChannelConfig, InitialPage};

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

/// A channel whose program has started, with connection 1 open.
pub fn channel() -> Channel {
    let config = ChannelConfig {
        name: "Fuzz".into(),
        address: "http://127.0.0.1/fuzz/".into(),
        initial_page: InitialPage::new(0x100).unwrap(),
        epoch_at_start_ms: 0,
        session_grace_ms: 0,
        wheel: false,
    };
    let mut channel = Channel::new(config);
    channel.program_started(0);
    channel.opened(1, 1, Vec::new(), 0);
    let _ = channel.take_effects();
    channel
}

/// Exact base64 of `bytes`.
pub fn base64(bytes: &[u8]) -> String {
    let mut encoded = vec![0_u8; neoteletext_web::base64::encoded_len(bytes.len())];
    let written = neoteletext_web::base64::encode(bytes, &mut encoded).unwrap_or(0);
    String::from_utf8_lossy(&encoded[..written]).into_owned()
}
