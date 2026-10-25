use super::*;
use crate::Packet;
use crate::screen::Screen;
use alloc::string::ToString;
use neoteletext_audio::idl::FRAGMENT_DATA;
use neoteletext_core::consts::PACKET_BYTES;
use neoteletext_input::cursor::ECHO;
use neoteletext_input::event::write;
use neoteletext_input::modes::{ALL_CAPS, KEY_RELEASE};
use neoteletext_input::modes::{CURSOR_CAP, ECHO_CAP, PAGE_CAP};
use neoteletext_input::session::Session as Receiver;

/// A page that echoes what is typed into a field, each session its own.
#[derive(Default)]
struct Echo {
    typed: BTreeMap<SessionId, String>,
}

const ECHO_PAGE: u16 = 0x200;

impl Echo {
    fn draw(&self, session: SessionId, context: &mut Context<'_>) {
        let text = self.typed.get(&session).cloned().unwrap_or_default();
        let mut screen = Screen::new();
        screen.text(5, 1, &text);
        let column = u8::try_from(text.len().saturating_add(1)).unwrap();
        screen.cursor(5, column, 20, ECHO);
        screen.modes(KEY_RELEASE);
        context.publish(Owner::Session(session), ECHO_PAGE, &screen);
    }
}

impl Application for Echo {
    fn pages(&self) -> Vec<PageEntry> {
        alloc::vec![PageEntry::new(ECHO_PAGE, "Echo")]
    }

    fn start(&mut self, context: &mut Context<'_>) {
        context.publish(Owner::Channel, ECHO_PAGE, &Screen::new());
    }

    fn opened(&mut self, session: SessionId, _: u16, context: &mut Context<'_>) {
        self.draw(session, context);
    }

    fn input(&mut self, session: SessionId, _: u16, input: &Input<'_>, context: &mut Context<'_>) {
        if let Input::Text(text) = input {
            self.typed.entry(session).or_default().push_str(text);
            self.draw(session, context);
        }
    }
}

fn channel() -> Channel {
    let config = ChannelConfig { name: "Test".to_string(), push_address: "ws://127.0.0.1:1/test/push".to_string(), initial_page: 0x100 };
    let mut channel = Channel::new(config, alloc::vec![Box::new(Echo::default())], 7);
    channel.start(0);
    channel
}

fn message(item: &Item<'_>) -> Vec<u8> {
    let mut bytes = [0_u8; 64];
    let length = write(item, &mut bytes).unwrap();
    bytes[..length].to_vec()
}

/// The packets of every message the channel sent `session`.
fn sent(effects: &[Effect], session: SessionId) -> Vec<Packet> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Send { session: to, bytes } if *to == session => Some(bytes),
            _ => None,
        })
        .flat_map(|bytes| bytes.chunks(PACKET_BYTES).map(|chunk| Packet { bytes: chunk.try_into().unwrap() }).collect::<Vec<_>>())
        .collect()
}

/// The first control record among `packets`, as (type, first field,
/// second field).
fn first_control(packets: &[Packet]) -> Option<(u8, u32, u32)> {
    for packet in packets {
        let mut buffer = [0_u8; FRAGMENT_DATA];
        if let Some(Ok(control)) = neoteletext_input::control::from_packet(packet, &mut buffer) {
            return Some(match control {
                Control::Welcome { version, caps } => (1, version, caps),
                Control::PageModes { page, modes } => (4, u32::from(page), modes),
                _ => (0, 0, 0),
            });
        }
    }
    None
}

fn cursors(packets: &[Packet]) -> Vec<Cursor> {
    packets.iter().filter_map(neoteletext_input::cursor::from_packet).map(Result::unwrap).collect()
}

/// INPUT.md as the browser's receiver session reads it: HELLO is answered
/// by a WELCOME it accepts; PAGE brings the page with its modes and the
/// cursor acknowledging both items; typed text is acted on and counted.
#[test]
fn a_receiver_is_welcomed_and_its_input_acknowledged() {
    let mut channel = channel();
    channel.opened(1, 0);
    // The browser's offer (its core/src/typing.rs).
    let mut receiver = Receiver::new(PAGE_CAP | CURSOR_CAP | ECHO_CAP | neoteletext_input::modes::ALL_MODES);
    let hello = receiver.open(0, None);
    let Event::Hello { caps, .. } = hello else { panic!("HELLO") };
    assert_eq!(caps & ALL_CAPS, caps);
    channel.received(1, &message(&Item::Event(hello)), 0);
    let packets = sent(&channel.take_effects(), 1);
    let (kind, version, agreed) = first_control(&packets).unwrap();
    assert_eq!(kind, 1, "WELCOME");
    assert_eq!(receiver.welcome(version, agreed), Ok(()), "the receiver accepts the WELCOME");

    channel.received(1, &message(&Item::Event(Event::Page(ECHO_PAGE))), 0);
    let packets = sent(&channel.take_effects(), 1);
    assert_eq!(first_control(&packets), Some((4, u32::from(ECHO_PAGE), KEY_RELEASE)), "PAGE_MODES");
    let acked: Vec<u16> = cursors(&packets).iter().map(|cursor| cursor.acked).collect();
    assert_eq!(acked.last(), Some(&2), "HELLO and PAGE acted on: {acked:?}");

    channel.received(1, &message(&Item::Text("hi")), 0);
    let packets = sent(&channel.take_effects(), 1);
    let cursor = *cursors(&packets).last().unwrap();
    assert_eq!(cursor.acked, 4, "two characters more");
    assert_eq!(cursor.place, Some(Place { row: 5, column: 3 }), "the cursor after what was typed");
    assert!(packets.len() > 20, "the page itself came too: {} packets", packets.len());
}

/// A shared page is published once, whatever the number of sessions
/// (decision 7); a session's own page goes to it alone, and it is told it
/// owns it.
#[test]
fn a_shared_page_is_published_once_and_an_own_page_to_its_session() {
    let mut channel = channel();
    let started = channel.take_effects();
    assert_eq!(started.iter().filter(|effect| matches!(effect, Effect::Shared { page: ECHO_PAGE, .. })).count(), 1);
    for session in 1..=100 {
        channel.opened(session, 0);
    }
    assert!(channel.take_effects().is_empty(), "joining sends nothing: the host sends the shared pages");
    channel.received(42, &message(&Item::Event(Event::Page(ECHO_PAGE))), 0);
    let effects = channel.take_effects();
    assert!(effects.contains(&Effect::Own { session: 42, page: ECHO_PAGE }));
    assert!(effects.iter().all(|effect| match effect {
        Effect::Send { session, .. } | Effect::Own { session, .. } => *session == 42,
        _ => false,
    }));
}

/// Bytes that break INPUT.md close the session.
#[test]
fn broken_input_closes_the_session() {
    let mut channel = channel();
    channel.opened(3, 0);
    channel.received(3, &[0xFF, 0x00], 0);
    assert!(channel.take_effects().contains(&Effect::Close { session: 3 }));
}
