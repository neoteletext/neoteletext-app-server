//! REQ-1475: a refused line applies nothing: the program is told, the
//! operator's log says so, and no viewer is sent anything.

use alloc::vec::Vec;

use serde_json::json;

use super::{line, spaces, subpage, with};
use crate::{Channel, ChannelConfig, Effect, InitialPage};

#[test]
fn a_refused_line_applies_nothing() {
    let config = ChannelConfig {
        name: "Test".into(),
        address: "http://127.0.0.1/test/".into(),
        initial_page: InitialPage::new(0x100).unwrap(),
        epoch_at_start_ms: 0,
        session_grace_ms: 0,
    };
    let mut channel = Channel::new(config);
    channel.program_started(0);
    channel.opened(1, 1, Vec::new(), 0);
    let _ = channel.take_effects();
    let page = json!({"type": "page", "connections": "all", "page": subpage()});
    let bad: Vec<Vec<u8>> = [
        with(&page, "/extra", json!(1)),
        with(&page, "/page/rows/2", json!(spaces().trim_end_matches('='))),
        with(&page, "/page/link_extension/keys/0/label", json!("cafe\u{301}")),
        with(&page, "/page/enhancements/0/triplets/0/data", json!(128)),
        json!({"type": "sound", "kind": "play", "connections": "all", "channel": 4, "handle": 1, "generation": 0, "gain": 1, "fade_ms": 0, "loop": false, "after": false, "duck": 0, "page": null}),
    ]
    .iter()
    .map(line)
    .chain([b"{\"type\":\"page\"".to_vec(), b"[]".to_vec(), b"{\"type\":\"log\",\"type\":\"log\"}".to_vec()])
    .collect();
    for line in bad {
        channel.from_program(&line, 0);
        let effects = channel.take_effects();
        assert!(!effects.iter().any(|effect| matches!(effect, Effect::Send { .. })), "{}: {effects:?}", alloc::string::String::from_utf8_lossy(&line));
        let told = effects.iter().filter(|effect| matches!(effect, Effect::ToProgram { line, .. } if line.starts_with(br#"{"type":"refused""#))).count();
        assert_eq!(told, 1, "{effects:?}");
    }
}
