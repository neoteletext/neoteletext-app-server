//! A program's instrument sounds (REQ-6413, DEC-6429): any bytes as a SID
//! sound's images, the slots of a DX7 sound or of an analogue
//! synthesizer's, or a bank's patches, stored or live, with any images or
//! slots to a block, through the server's check and the records it writes.
//! No panic; a line is refused whole or sent.
#![no_main]

use libfuzzer_sys::fuzz_target;
use teletext_runtime::{Channel, ChannelConfig, Effect, InitialPage};

/// The kinds that carry data: the SID's and the DX7's five, then three for
/// each analogue synthesizer.
const KINDS: u8 = 5 + 3 * 5;
/// The analogue synthesizers' kinds, each with the fields its stored sound
/// needs beyond the DX7's.
const SYNTHS: [(&str, &str); 5] = [
    ("juno", r#","model":"60""#),
    ("minimoog", ""),
    ("prophet5", r#","model":"revision_3""#),
    ("jupiter8", r#","key_mode":"split","split_key":60"#),
    ("cs80", ""),
];
/// The most slots to a block, and two more, so the refusals at both ends are
/// reached.
const SIZES: u8 = 18;

fuzz_target!(|data: &[u8]| {
    let [kind, size, blocks @ ..] = data else { return };
    let config = ChannelConfig { name: "Fuzz".into(), address: "http://127.0.0.1/fuzz/".into(), initial_page: InitialPage::new(0x100).unwrap(), epoch_at_start_ms: 0, session_grace_ms: 0 };
    let mut channel = Channel::new(config);
    channel.program_started(0);
    channel.opened(1, 1, Vec::new(), 0);
    let _ = channel.take_effects();
    let mut encoded = vec![0_u8; neoteletext_web::base64::encoded_len(blocks.len())];
    let Ok(written) = neoteletext_web::base64::encode(blocks, &mut encoded) else { return };
    let text = String::from_utf8_lossy(&encoded[..written]);
    let size = size % SIZES;
    let kind = kind % KINDS;
    let (synth, more) = SYNTHS[usize::from(kind.saturating_sub(5) / 3) % SYNTHS.len()];
    let fields = match kind {
        5.. => match (kind - 5) % 3 {
            0 => format!(r#""kind":"{synth}","slots":{size},"bank":0{more},"blocks":"{text}""#),
            1 => format!(r#""kind":"{synth}_blocks","slots":{size},"first":65535,"blocks":"{text}""#),
            _ => format!(r#""kind":"{synth}_bank","patches":"{text}""#),
        },
        0 => format!(r#""kind":"sid","images_per_block":{size},"chip":"6581","clock":"pal","images":"{text}""#),
        1 => format!(r#""kind":"sid_blocks","images_per_block":{size},"first":65535,"images":"{text}""#),
        2 => format!(r#""kind":"dx7","slots":{size},"bank":0,"blocks":"{text}""#),
        3 => format!(r#""kind":"dx7_blocks","slots":{size},"first":65535,"blocks":"{text}""#),
        _ => format!(r#""kind":"dx7_bank","patches":"{text}""#),
    };
    let line = format!(r#"{{"type":"sound","connections":[1],"handle":1,"generation":0,{fields}}}"#);
    channel.from_program(line.as_bytes(), 0);
    let effects = channel.take_effects();
    let refused = effects.iter().any(|effect| matches!(effect, Effect::ToProgram { line, .. } if line.starts_with(br#"{"type":"refused""#)));
    let sent = effects.iter().any(|effect| matches!(effect, Effect::Send { .. }));
    assert!(refused != sent, "refused whole or sent: {effects:?}");
});
