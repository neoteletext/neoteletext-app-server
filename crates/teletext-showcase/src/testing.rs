//! Driving a channel as a receiver drives it, for the areas' tests: a
//! channel of some applications, sessions that say HELLO, open pages,
//! type, press keys, click and turn the wheel, and the pages and cursor
//! records the channel sends back, read as a receiver reads them.

use alloc::boxed::Box;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use neoteletext_audio::idl::FRAGMENT_DATA;
use neoteletext_core::consts::{COLUMNS, PACKET_BYTES};
use neoteletext_core::packet::{Decoded, decode};
use neoteletext_input::control::Control;
use neoteletext_input::cursor::Cursor;
use neoteletext_input::event::{Action, Event, Item, Key, KeyCode, Mouse, MouseAction, Point, Wheel, write};
use neoteletext_input::keys;
use neoteletext_input::modes::{ALL_MODES, CURSOR_CAP, ECHO_CAP, PAGE_CAP};
use teletext_runtime::screen::ROWS;
use teletext_runtime::{Application, Channel, ChannelConfig, Effect, Packet, SessionId};

/// A page as a receiver holds it: each row's codes, parity stripped.
pub(crate) type Rows = Vec<[u8; COLUMNS]>;

/// A channel of some applications, started, and the effects it has
/// answered with.
pub(crate) struct Harness {
    pub(crate) channel: Channel,
    pub(crate) now: u64,
    pub(crate) effects: Vec<Effect>,
}

impl Harness {
    pub(crate) fn new(applications: Vec<Box<dyn Application>>) -> Self {
        let config =
            ChannelConfig { name: "Test".to_string(), push_address: "ws://127.0.0.1:1/test/push".to_string(), initial_page: 0x100, epoch_at_start_ms: 0 };
        let mut channel = Channel::new(config, applications, 7);
        channel.start(0);
        let effects = channel.take_effects();
        Self { channel, now: 0, effects }
    }

    /// Sends one input item from `session`, and keeps what came back.
    pub(crate) fn send(&mut self, session: SessionId, item: &Item<'_>) {
        let mut bytes = [0_u8; 256];
        let length = write(item, &mut bytes).unwrap();
        self.channel.received(session, &bytes[..length], self.now);
        self.take();
    }

    pub(crate) fn take(&mut self) {
        self.effects.extend(self.channel.take_effects());
    }

    /// A session that said HELLO with what the browser offers, and opened
    /// `page`.
    pub(crate) fn join(&mut self, session: SessionId, page: u16) {
        self.channel.opened(session, self.now);
        let hello = Event::Hello { version: neoteletext_input::VERSION, caps: PAGE_CAP | CURSOR_CAP | ECHO_CAP | ALL_MODES, agent: None };
        self.send(session, &Item::Event(hello));
        self.open(session, page);
    }

    pub(crate) fn open(&mut self, session: SessionId, page: u16) {
        self.send(session, &Item::Event(Event::Page(page)));
    }

    pub(crate) fn type_text(&mut self, session: SessionId, text: &str) {
        self.send(session, &Item::Text(text));
    }

    /// Presses and releases a key by its INPUT.md name.
    pub(crate) fn press(&mut self, session: SessionId, name: &str) {
        let code = keys::code(name).unwrap();
        for action in [Action::Press, Action::Release] {
            self.send(session, &Item::Event(Event::Key(Key { action, key: KeyCode::Functional(code), mods: 0, text: None, base: None })));
        }
    }

    pub(crate) fn click(&mut self, session: SessionId, button: u8, column: usize, row: usize) {
        let at = Point { column: u32::try_from(column).unwrap(), row: u32::try_from(row).unwrap(), pixels: None, mods: 0 };
        for action in [MouseAction::Press, MouseAction::Release] {
            self.send(session, &Item::Event(Event::Mouse(Mouse { action, button, at })));
        }
    }

    /// Turns the wheel over a cell: `dy` in 1/120ths of a line, down
    /// positive.
    pub(crate) fn wheel(&mut self, session: SessionId, dy: i32, column: usize, row: usize) {
        let at = Point { column: u32::try_from(column).unwrap(), row: u32::try_from(row).unwrap(), pixels: None, mods: 0 };
        self.send(session, &Item::Event(Event::Wheel(Wheel { dx: 0, dy, at })));
    }

    /// Runs the clock on by `by`, waking the application if it asked.
    pub(crate) fn advance(&mut self, by: u64) {
        self.now = self.now.saturating_add(by);
        if self.channel.next_wake().is_some_and(|at| at <= self.now) {
            self.channel.deadline(self.now);
        }
        self.take();
    }

    /// How many times `page` was published shared since the last call.
    pub(crate) fn shared_publications(&mut self, page: u16) -> usize {
        let count = self.effects.iter().filter(|effect| matches!(effect, Effect::Shared { page: shared, .. } if *shared == page)).count();
        self.effects.clear();
        count
    }

    /// The channel's copy of `page`, as it is now.
    pub(crate) fn shared(&self, page: u16) -> Rows {
        let frame = self.channel.shared_frames().find(|(number, _)| *number == page).map(|(_, frame)| frame.to_vec()).unwrap();
        pages_in(&frame).into_iter().rev().find(|(number, _)| *number == page).unwrap().1
    }

    /// The latest copy of `page` sent to `session` alone.
    pub(crate) fn own(&self, session: SessionId, page: u16) -> Rows {
        let bytes: Vec<u8> = sent(&self.effects, session).iter().flat_map(|packet| packet.bytes).collect();
        pages_in(&bytes).into_iter().rev().find(|(number, _)| *number == page).unwrap().1
    }

    /// The latest cursor record sent to `session`.
    pub(crate) fn cursor(&self, session: SessionId) -> Cursor {
        sent(&self.effects, session).iter().filter_map(neoteletext_input::cursor::from_packet).map(Result::unwrap).next_back().unwrap()
    }

    /// The pages of every cursor record sent to `session`.
    pub(crate) fn cursor_pages(&self, session: SessionId) -> Vec<u16> {
        sent(&self.effects, session).iter().filter_map(neoteletext_input::cursor::from_packet).map(|cursor| cursor.unwrap().page).collect()
    }

    /// The latest `PAGE_MODES` record sent to `session`.
    pub(crate) fn page_modes(&self, session: SessionId) -> Option<(u16, u32)> {
        sent(&self.effects, session)
            .iter()
            .filter_map(|packet| {
                let mut buffer = [0_u8; FRAGMENT_DATA];
                match neoteletext_input::control::from_packet(packet, &mut buffer) {
                    Some(Ok(Control::PageModes { page, modes })) => Some((page, modes)),
                    _ => None,
                }
            })
            .next_back()
    }
}

/// The packets of every message sent to `session`.
pub(crate) fn sent(effects: &[Effect], session: SessionId) -> Vec<Packet> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Send { session: to, bytes } if *to == session => Some(bytes),
            _ => None,
        })
        .flat_map(|bytes| bytes.chunks(PACKET_BYTES).map(|chunk| Packet { bytes: chunk.try_into().unwrap() }).collect::<Vec<_>>())
        .collect()
}

/// Every page in `bytes`, in order: its number and rows.
pub(crate) fn pages_in(bytes: &[u8]) -> Vec<(u16, Rows)> {
    let mut pages: Vec<(u16, Rows)> = Vec::new();
    for chunk in bytes.chunks(PACKET_BYTES) {
        let packet = Packet { bytes: chunk.try_into().unwrap() };
        match decode(&packet) {
            Ok(Decoded::Header(header)) => {
                let number = header.page.map_or(0, |page| (u16::from(header.magazine.number()) << 8) | u16::from(page.0));
                pages.push((number, alloc::vec![[b' '; COLUMNS]; ROWS]));
            }
            Ok(Decoded::Row(row)) => {
                if let Some((_, rows)) = pages.last_mut()
                    && let Some(line) = rows.get_mut(usize::from(row.row))
                {
                    for (slot, byte) in line.iter_mut().zip(row.bytes) {
                        *slot = byte & 0x7F;
                    }
                }
            }
            _ => {}
        }
    }
    pages
}

/// A row's codes as text.
pub(crate) fn text(rows: &Rows, row: usize) -> String {
    rows[row].iter().map(|&code| char::from(code)).collect()
}

/// The first row holding `wanted`.
pub(crate) fn row_of(rows: &Rows, wanted: &str) -> Option<usize> {
    (0..ROWS).find(|&row| text(rows, row).contains(wanted))
}

/// Whether any row holds `wanted`.
pub(crate) fn shows(rows: &Rows, wanted: &str) -> bool {
    row_of(rows, wanted).is_some()
}
