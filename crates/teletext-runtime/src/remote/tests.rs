use super::*;
use crate::{Channel, ChannelConfig, Effect, Packet};
use alloc::boxed::Box;
use neoteletext_core::consts::PACKET_BYTES;
use neoteletext_core::packet::{Decoded, decode};
use neoteletext_input::event::{Event, Item as InputItem, Key, KeyCode, write};
use neoteletext_input::modes::{ALL_MODES, CURSOR_CAP, ECHO_CAP, MOUSE_BUTTONS, PAGE_CAP};

/// What the browser offers in HELLO.
const OFFERED: u32 = PAGE_CAP | CURSOR_CAP | ECHO_CAP | ALL_MODES;

const VIEWER: SessionId = 4;

fn channel() -> Channel {
    let pages =
        declared_pages(br#"{"type":"pages","pages":[{"page":"100","title":"Front"},{"page":"1A0","title":"Hidden"},{"page":"300","title":"Game"}]}"#).unwrap();
    let config =
        ChannelConfig { name: "test".to_string(), push_address: "ws://127.0.0.1:1/test/push".to_string(), initial_page: 0x100, epoch_at_start_ms: 1_000 };
    let mut channel = Channel::new(config, alloc::vec![Box::new(Remote::new(pages))], 1);
    channel.start(0);
    channel
}

/// The lines the channel sent its application, as values.
fn lines(effects: &[Effect]) -> Vec<Value> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::ToApplication { application: 0, line } => {
                assert_eq!(line.last(), Some(&b'\n'), "a line ends with its newline");
                Some(json::parse(line.strip_suffix(b"\n").unwrap()).unwrap())
            }
            _ => None,
        })
        .collect()
}

fn logs(effects: &[Effect]) -> Vec<String> {
    effects.iter().filter_map(|effect| if let Effect::Log { text } = effect { Some(text.clone()) } else { None }).collect()
}

fn send(channel: &mut Channel, item: &InputItem<'_>) {
    let mut bytes = [0_u8; 256];
    let length = write(item, &mut bytes).unwrap();
    channel.received(VIEWER, &bytes[..length], 0);
}

/// The rows' text of every page in `bytes`, by page number.
fn texts(bytes: &[u8]) -> Vec<(u16, String)> {
    let mut pages: Vec<(u16, String)> = Vec::new();
    for chunk in bytes.chunks(PACKET_BYTES) {
        let Ok(packet) = chunk.try_into().map(|bytes| Packet { bytes }) else { continue };
        match decode(&packet) {
            Ok(Decoded::Header(header)) => {
                let number = (u16::from(header.magazine.number()) << 8) | header.page.map_or(0, |page| u16::from(page.0));
                pages.push((number, String::new()));
            }
            Ok(Decoded::Row(row)) => {
                if let Some((_, text)) = pages.last_mut() {
                    text.extend(row.bytes.iter().map(|&byte| char::from(byte & 0x7F)));
                }
            }
            _ => {}
        }
    }
    pages
}

fn sent_to(effects: &[Effect], session: SessionId) -> Vec<u8> {
    effects
        .iter()
        .filter_map(|effect| if let Effect::Send { session: to, bytes } = effect { (*to == session).then(|| bytes.to_vec()) } else { None })
        .flatten()
        .collect()
}

#[test]
fn a_declaration_names_the_pages_and_nothing_else_passes() {
    let pages = declared_pages(br#"{"type":"pages","pages":[{"page":"100","title":"Front"},{"page":"8FF"}]}"#).unwrap();
    assert_eq!(pages.iter().map(|entry| entry.number).collect::<Vec<_>>(), [0x100, 0x8FF]);
    let too_long_pages = alloc::format!(r#"{{"type":"pages","pages":[{{"page":"100","title":"{}"}}]}}"#, "x".repeat(TITLE_LIMIT + 1));
    let whole = alloc::format!(r#"{{"type":"pages","pages":[{{"page":"100","title":"{}"}}]}}"#, "x".repeat(TITLE_LIMIT));
    assert!(declared_pages(whole.as_bytes()).is_ok(), "a title of the engine's longest");
    for bad in [
        &br#"{"type":"publish"}"#[..],
        br#"{"type":"pages","pages":[{"page":"099"}]}"#,
        br#"{"type":"pages","pages":[{"page":"900"}]}"#,
        br#"{"type":"pages","pages":[{"page":"100"},{"page":"100"}]}"#,
        too_long_pages.as_bytes(),
        br#"{"type":"pages","pages":[{"page":100}]}"#,
        b"not json",
    ] {
        assert!(declared_pages(bad).is_err(), "{}", String::from_utf8_lossy(bad));
    }
}

/// The application is told to start, with the protocol and the time; a
/// channel page it publishes as TTI is shared, encoded once.
#[test]
fn a_published_tti_page_is_the_channels() {
    let mut channel = channel();
    let started = lines(&channel.take_effects());
    assert_eq!(started.len(), 1);
    assert_eq!(started[0].get("type").and_then(Value::text), Some("start"));
    assert_eq!(started[0].get("protocol").and_then(Value::integer), Some(PROTOCOL));
    assert_eq!(started[0].get("epoch_ms").and_then(Value::integer), Some(1_000));

    let line = br#"{"type":"publish","page":"100","owner":"channel","tti":"PN,10000\nOL,1,\u001bAHACKADAY\nOL,3,Fresh hacks every day\nFL,200,300,400,500,8ff,100\n"}"#;
    channel.from_application(0, line, 0);
    let effects = channel.take_effects();
    assert!(logs(&effects).is_empty(), "{:?}", logs(&effects));
    let frame = effects.iter().find_map(|effect| if let Effect::Shared { page: 0x100, frame } = effect { Some(frame.clone()) } else { None }).unwrap();
    let pages = texts(&frame);
    assert_eq!(pages.len(), 1);
    assert!(pages[0].1.contains("HACKADAY") && pages[0].1.contains("Fresh hacks every day"), "{:?}", pages[0].1);
    let header = frame.chunks(PACKET_BYTES).find_map(|chunk| match decode(&Packet { bytes: chunk.try_into().ok()? }) {
        Ok(Decoded::Header(header)) => Some(header.display.iter().map(|&byte| char::from(byte & 0x7F)).collect::<String>()),
        _ => None,
    });
    assert_eq!(header.as_deref(), Some("TEST     100 Thu 01 Jan 00:00/01"), "the service's header, as an inserter writes it");
}

/// What a viewer does reaches the application as lines; its answer for one
/// session is that session's page alone.
#[test]
fn a_viewers_input_reaches_the_application_and_its_answer_is_theirs() {
    let mut channel = channel();
    channel.take_effects();
    channel.opened(VIEWER, 0);
    send(&mut channel, &InputItem::Event(Event::Hello { version: neoteletext_input::VERSION, caps: OFFERED, agent: None }));
    send(&mut channel, &InputItem::Event(Event::Page(0x300)));
    send(&mut channel, &InputItem::Text("hi"));
    let code = keys::code("ArrowLeft").unwrap();
    send(&mut channel, &InputItem::Event(Event::Key(Key { action: Action::Press, key: KeyCode::Functional(code), mods: 0, text: None, base: None })));
    let told = lines(&channel.take_effects());
    let kinds: Vec<&str> = told.iter().filter_map(|line| line.get("type").and_then(Value::text)).collect();
    assert_eq!(kinds, ["join", "open", "input", "input"]);
    assert_eq!(told[1].get("page").and_then(Value::text), Some("300"));
    assert_eq!(told[2].get("event").and_then(|event| event.get("text")).and_then(Value::text), Some("hi"));
    let key = told[3].get("event").unwrap();
    assert_eq!(
        (key.get("kind").and_then(Value::text), key.get("action").and_then(Value::text), key.get("key").and_then(Value::text)),
        (Some("key"), Some("press"), Some("ArrowLeft"))
    );
    assert_eq!(told[3].get("session").and_then(Value::integer), Some(4));

    let own = alloc::format!(r#"{{"type":"publish","page":"300","owner":{VIEWER},"tti":"PN,30000\nOL,5,Your game\nIM,{MOUSE_BUTTONS}\n"}}"#);
    channel.from_application(0, own.as_bytes(), 0);
    let effects = channel.take_effects();
    assert!(logs(&effects).is_empty(), "{:?}", logs(&effects));
    assert!(effects.contains(&Effect::Own { session: VIEWER, page: 0x300 }));
    let received = texts(&sent_to(&effects, VIEWER));
    assert!(received.iter().any(|(page, text)| *page == 0x300 && text.contains("Your game")));
    assert!(!effects.iter().any(|effect| matches!(effect, Effect::Shared { .. })), "never the channel's");

    channel.closed(VIEWER, 0);
    let ended = lines(&channel.take_effects());
    assert_eq!(ended.last().and_then(|line| line.get("type")).and_then(Value::text), Some("close"));
}

/// A sound is sent as samples and leaves as the profile's records; each
/// command on a channel is numbered after the last.
#[test]
fn sounds_are_encoded_and_commands_numbered() {
    let mut channel = channel();
    channel.opened(VIEWER, 0);
    channel.take_effects();
    let pcm: Vec<u8> = (0..800_i16).flat_map(|sample| (sample * 20).to_le_bytes()).collect();
    let mut encoded = alloc::vec![0_u8; base64::encoded_len(pcm.len())];
    let length = base64::encode(&pcm, &mut encoded).unwrap();
    let pcm = core::str::from_utf8(&encoded[..length]).unwrap();
    let object = alloc::format!(r#"{{"type":"sound","to":{{"session":{VIEWER}}},"object":{{"handle":3,"generation":1,"rate":8000,"pcm":"{pcm}"}}}}"#);
    channel.from_application(0, object.as_bytes(), 0);
    let effects = channel.take_effects();
    assert!(logs(&effects).is_empty(), "{:?}", logs(&effects));
    assert!(!sent_to(&effects, VIEWER).is_empty(), "the object went");
    for _ in 0..2 {
        let play = alloc::format!(r#"{{"type":"sound","to":{{"session":{VIEWER}}},"play":{{"channel":1,"handle":3,"generation":1,"gain":70,"page":"300"}}}}"#);
        channel.from_application(0, play.as_bytes(), 0);
    }
    let effects = channel.take_effects();
    assert!(logs(&effects).is_empty(), "{:?}", logs(&effects));
    assert_eq!(effects.iter().filter(|effect| matches!(effect, Effect::Send { .. })).count(), 2);
    // Numbered as the remote numbers them: 1, then 2 (the body's channel,
    // then its sequence, after the record's 4-byte header).
    let mut remote = Remote::new(Vec::new());
    let play = json::parse(br#"{"channel":1,"handle":3}"#).unwrap();
    let first = remote.command(&play, PLAY, 3, 0, 0, 0, None).unwrap();
    let second = remote.command(&play, PLAY, 3, 0, 0, 0, None).unwrap();
    assert_eq!((first[4], first[5], first[6]), (1, 0, 1));
    assert_eq!((second[4], second[5], second[6]), (1, 0, 2));
}

/// The application can send a viewer to another page.
#[test]
fn goto_sends_the_viewer_on() {
    let mut channel = channel();
    channel.opened(VIEWER, 0);
    send(&mut channel, &InputItem::Event(Event::Hello { version: neoteletext_input::VERSION, caps: OFFERED, agent: None }));
    channel.take_effects();
    channel.from_application(0, alloc::format!(r#"{{"type":"goto","session":{VIEWER},"page":"1A0"}}"#).as_bytes(), 0);
    let effects = channel.take_effects();
    assert_eq!(logs(&effects), Vec::<String>::new());
    assert!(!sent_to(&effects, VIEWER).is_empty(), "a cursor record naming the page");
}

/// A line that is not valid is refused whole and reported, never acted on
/// in part.
#[test]
fn bad_lines_are_refused_and_reported() {
    let mut channel = channel();
    channel.take_effects();
    let too_long_title = alloc::format!(r#"{{"type":"title","page":"100","title":"{}"}}"#, "x".repeat(TITLE_LIMIT + 1));
    for (line, reason) in [
        (&b"{not json"[..], "not JSON"),
        (br#"{"type":"publish","page":"200","owner":"channel","tti":"PN,20000\n"}"#, "200 was not declared"),
        (br#"{"type":"publish","page":"100","owner":"channel","tti":"OL,1,no page line\n"}"#, "MissingPageNumber"),
        (br#"{"type":"publish","page":"100","owner":"channel","tti":"PN,10000\nZZ,what\n"}"#, "TTI"),
        (br#"{"type":"publish","page":"100","owner":"everyone","tti":"PN,10000\n"}"#, "owner"),
        (br#"{"type":"teleport"}"#, "unknown type"),
        (br#"{"type":"sound","to":{"page":"100"},"play":{"channel":4,"handle":1}}"#, "channel is 0 to 3"),
        (br#"{"type":"sound","to":{"page":"100"},"object":{"handle":1,"rate":8000,"pcm":"***"}}"#, "not base64"),
        (br#"{"type":"sound","to":{"page":"100"},"object":{"handle":1,"rate":12345,"pcm":"AAA="}}"#, "sound.object"),
        (br#"{"type":"pages","pages":[]}"#, "declared once"),
        (br#"{"type":"title","page":"200","title":"Not ours"}"#, "200 was not declared"),
        (too_long_title.as_bytes(), "over 120"),
    ] {
        channel.from_application(0, line, 0);
        let effects = channel.take_effects();
        let said = logs(&effects);
        assert!(said.len() == 1 && said[0].contains(reason), "{}: {said:?}", String::from_utf8_lossy(line));
        assert!(!effects.iter().any(|effect| matches!(effect, Effect::Shared { .. } | Effect::Send { .. })), "nothing done");
    }
}

/// A page retitled names what it holds now in `contents.txt`.
#[test]
fn a_title_renames_a_page_in_the_contents() {
    let mut channel = channel();
    for page in ["100", "300"] {
        let line = alloc::format!(r#"{{"type":"publish","page":"{page}","owner":"channel","tti":"PN,{page}00\nOL,1,Page\n"}}"#);
        channel.from_application(0, line.as_bytes(), 0);
    }
    channel.take_effects();
    channel.from_application(0, br#"{"type":"title","page":"300","title":"Tiny Robot Arm..."}"#, 0);
    let effects = channel.take_effects();
    assert_eq!(logs(&effects), Vec::<String>::new());
    assert_eq!(effects.iter().filter(|effect| **effect == Effect::Contents).count(), 1, "the host is told to serve the contents anew");
    let contents = String::from_utf8(channel.resource("contents.txt").unwrap().body).unwrap();
    assert!(contents.contains("p\t300\tTiny Robot Arm...\n") && contents.contains("p\t100\tFront\n"), "{contents}");
    channel.from_application(0, br#"{"type":"title","page":"300","title":"Tiny Robot Arm..."}"#, 0);
    assert!(!channel.take_effects().contains(&Effect::Contents), "the same title again changes nothing");
}

/// A viewer's own copy released, the channel's copy is theirs again: the
/// host is told, and the shared page is sent to them at once.
#[test]
fn a_released_page_is_the_channels_again() {
    let mut channel = channel();
    channel.from_application(0, br#"{"type":"publish","page":"100","owner":"channel","tti":"PN,10000\nOL,5,Everyone's\n"}"#, 0);
    channel.opened(VIEWER, 0);
    send(&mut channel, &InputItem::Event(Event::Hello { version: neoteletext_input::VERSION, caps: OFFERED, agent: None }));
    send(&mut channel, &InputItem::Event(Event::Page(0x100)));
    let own = alloc::format!(r#"{{"type":"publish","page":"100","owner":{VIEWER},"tti":"PN,10000\nOL,5,Yours alone\n"}}"#);
    channel.from_application(0, own.as_bytes(), 0);
    let effects = channel.take_effects();
    assert!(effects.contains(&Effect::Own { session: VIEWER, page: 0x100 }));
    let release = alloc::format!(r#"{{"type":"release","session":{VIEWER},"page":"100"}}"#);
    channel.from_application(0, release.as_bytes(), 0);
    let effects = channel.take_effects();
    assert_eq!(logs(&effects), Vec::<String>::new());
    assert!(effects.contains(&Effect::Disown { session: VIEWER, page: 0x100 }));
    let pages = texts(&sent_to(&effects, VIEWER));
    assert!(pages.iter().any(|(page, text)| *page == 0x100 && text.contains("Everyone's")), "{pages:?}");
    channel.from_application(0, release.as_bytes(), 0);
    assert!(!channel.take_effects().iter().any(|effect| matches!(effect, Effect::Disown { .. })), "a page not owned is not released twice");
}

/// A page that starts a section heads it in `contents.txt`, which names
/// its magazine.
#[test]
fn a_section_heads_its_pages_in_the_contents() {
    let pages = declared_pages(br#"{"type":"pages","pages":[{"page":"100","title":"Front"},{"page":"200","title":"Retro front","section":"Retrocomputing"},{"page":"201","title":"Challenge"}]}"#).unwrap();
    assert_eq!(pages[1].section.as_deref(), Some("Retrocomputing"));
    let config = ChannelConfig { name: "test".to_string(), push_address: "ws://127.0.0.1:1/test/push".to_string(), initial_page: 0x100, epoch_at_start_ms: 0 };
    let mut channel = Channel::new(config, alloc::vec![Box::new(Remote::new(pages))], 1);
    let contents = |channel: &Channel| String::from_utf8(channel.resource("contents.txt").unwrap().body).unwrap();
    assert_eq!(contents(&channel), "s\ttest\n", "nothing published, nothing listed");
    for page in ["100", "201"] {
        let line = alloc::format!(r#"{{"type":"publish","page":"{page}","owner":"channel","tti":"PN,{page}00\nOL,1,Page\n"}}"#);
        channel.from_application(0, line.as_bytes(), 0);
        assert!(channel.take_effects().contains(&Effect::Contents), "a page published for the first time is listed");
    }
    assert_eq!(
        contents(&channel),
        "s\ttest\np\t100\tFront\ns\tRetrocomputing\np\t201\tChallenge\n",
        "200, not published, is left out; its section heads the next listed"
    );
    channel.from_application(0, br#"{"type":"publish","page":"201","owner":"channel","tti":"PN,20100\nOL,1,Again\n"}"#, 0);
    assert!(!channel.take_effects().contains(&Effect::Contents), "published again, the contents are the same");
    let too_long = alloc::format!(r#"{{"type":"pages","pages":[{{"page":"200","section":"{}"}}]}}"#, "x".repeat(TITLE_LIMIT + 1));
    assert!(declared_pages(too_long.as_bytes()).is_err());
}

/// The channel counts viewers, what they open and press, and its lines,
/// and never anything that says who a viewer is.
#[test]
fn the_channel_counts_what_happens() {
    let mut channel = channel();
    channel.opened(VIEWER, 0);
    channel.opened(VIEWER + 1, 0);
    send(&mut channel, &InputItem::Event(Event::Hello { version: neoteletext_input::VERSION, caps: OFFERED, agent: None }));
    send(&mut channel, &InputItem::Event(Event::Page(0x300)));
    send(&mut channel, &InputItem::Text("a"));
    channel.from_application(0, br#"{"type":"publish","page":"100","owner":"channel","tti":"PN,10000\n"}"#, 0);
    channel.closed(VIEWER + 1, 0);
    let stats = channel.stats();
    assert_eq!((stats.viewers, stats.viewers_peak, stats.viewers_total), (1, 2, 2));
    assert_eq!(stats.most_opened(), [(0x300, 1)]);
    assert_eq!((stats.inputs, stats.keys), (1, 1));
    assert_eq!(stats.shared_published, 1);
    assert_eq!(stats.lines_from_application, 1);
    assert!(stats.lines_to_application >= 4, "start, joins, open, input and close went to it");
}

/// Every mouse event names its action, a press and a release as a move
/// does (docs/applications.md section 2), since a program tells a press
/// that grips a scroll bar from the release that lets it go by it alone.
#[test]
fn mouse_events_name_their_action() {
    use neoteletext_input::event::{Mouse, MouseAction, Point};
    let mut channel = channel();
    channel.opened(VIEWER, 0);
    send(&mut channel, &InputItem::Event(Event::Hello { version: neoteletext_input::VERSION, caps: OFFERED, agent: None }));
    send(&mut channel, &InputItem::Event(Event::Page(0x300)));
    channel.take_effects();
    let at = Point { column: 39, row: 12, pixels: None, mods: 0 };
    for (action, button) in [(MouseAction::Press, 0), (MouseAction::Move, 1), (MouseAction::Release, 0)] {
        send(&mut channel, &InputItem::Event(Event::Mouse(Mouse { action, button, at })));
    }
    let told = lines(&channel.take_effects());
    let actions: Vec<Option<&str>> = told.iter().map(|line| line.get("event").and_then(|event| event.get("action")).and_then(Value::text)).collect();
    assert_eq!(actions, [Some("press"), Some("move"), Some("release")]);
    let pressed: Vec<Option<bool>> = told.iter().map(|line| line.get("event").and_then(|event| event.get("pressed")).and_then(Value::boolean)).collect();
    assert_eq!(pressed, [Some(true), None, Some(false)]);
}
