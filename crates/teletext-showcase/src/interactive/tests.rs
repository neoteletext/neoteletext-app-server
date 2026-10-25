//! The area driven as a receiver drives it: a channel of its one
//! application, sessions that say HELLO, open pages, type, press keys and
//! click, and the pages and cursor records the channel sends back, read
//! as a receiver reads them.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use neoteletext_audio::idl::FRAGMENT_DATA;
use neoteletext_core::consts::{COLUMNS, PACKET_BYTES};
use neoteletext_core::packet::{Decoded, decode};
use neoteletext_input::control::Control;
use neoteletext_input::cursor::{Cursor, ECHO, MASKED, Place};
use neoteletext_input::event::{Action, Event, Item, Key, KeyCode, Mouse, MouseAction, Point, write};
use neoteletext_input::keys;
use neoteletext_input::modes::{ALL_MODES, CURSOR_CAP, ECHO_CAP, MOUSE_BUTTONS, PAGE_CAP};
use teletext_runtime::screen::{ROWS, address};
use teletext_runtime::{Channel, ChannelConfig, Colour, Effect, Packet, SessionId};

use super::calculator::{Problem, evaluate};
use super::field::printable;
use super::guestbook::{ENTRIES_KEPT, MESSAGE_LIMIT, POST_INTERVAL_MS};
use super::paint::{FIRST_COLUMN, PALETTE, PALETTE_COLUMN, PALETTE_ROW, SOLID, SWATCH_CELLS, TOP};
use super::poll::{BAR_CELLS, CHANGE_INTERVAL_MS};
use super::quiz::QUESTIONS;
use super::{CALCULATOR, FORM, GUESTBOOK, MENU, PAINT, POLL, QUIZ, REPUBLISH_INTERVAL_MS, SIGN, applications};

/// A page as a receiver holds it: each row's codes, parity stripped.
type Rows = Vec<[u8; COLUMNS]>;

/// A mosaic cell with its top two rows of sixels set, as a poll bar's
/// cells are (EN 300 706 Table 47).
const BAR_CELL: u8 = 0x2F;
/// The left mouse button.
const LEFT: u8 = 0;
/// The right mouse button.
const RIGHT: u8 = 2;

/// A channel of the area, started, and the effects it has answered with.
struct Harness {
    channel: Channel,
    now: u64,
    effects: Vec<Effect>,
}

impl Harness {
    fn new() -> Self {
        let config =
            ChannelConfig { name: "Test".to_string(), push_address: "ws://127.0.0.1:1/test/push".to_string(), initial_page: 0x100, epoch_at_start_ms: 0 };
        let mut channel = Channel::new(config, applications(&[]), 7);
        channel.start(0);
        let effects = channel.take_effects();
        Self { channel, now: 0, effects }
    }

    /// Sends one input item from `session`, and keeps what came back.
    fn send(&mut self, session: SessionId, item: &Item<'_>) {
        let mut bytes = [0_u8; 256];
        let length = write(item, &mut bytes).unwrap();
        self.channel.received(session, &bytes[..length], self.now);
        self.take();
    }

    fn take(&mut self) {
        self.effects.extend(self.channel.take_effects());
    }

    /// A session that said HELLO with what the browser offers, and opened
    /// `page`.
    fn join(&mut self, session: SessionId, page: u16) {
        self.channel.opened(session, self.now);
        let hello = Event::Hello { version: neoteletext_input::VERSION, caps: PAGE_CAP | CURSOR_CAP | ECHO_CAP | ALL_MODES, agent: None };
        self.send(session, &Item::Event(hello));
        self.open(session, page);
    }

    fn open(&mut self, session: SessionId, page: u16) {
        self.send(session, &Item::Event(Event::Page(page)));
    }

    fn type_text(&mut self, session: SessionId, text: &str) {
        self.send(session, &Item::Text(text));
    }

    /// Presses and releases a key by its INPUT.md name.
    fn press(&mut self, session: SessionId, name: &str) {
        let code = keys::code(name).unwrap();
        for action in [Action::Press, Action::Release] {
            self.send(session, &Item::Event(Event::Key(Key { action, key: KeyCode::Functional(code), mods: 0, text: None, base: None })));
        }
    }

    fn click(&mut self, session: SessionId, button: u8, column: usize, row: usize) {
        let at = Point { column: u32::try_from(column).unwrap(), row: u32::try_from(row).unwrap(), pixels: None, mods: 0 };
        for action in [MouseAction::Press, MouseAction::Release] {
            self.send(session, &Item::Event(Event::Mouse(Mouse { action, button, at })));
        }
    }

    /// Runs the clock on by `by`, waking the application if it asked.
    fn advance(&mut self, by: u64) {
        self.now = self.now.saturating_add(by);
        if self.channel.next_wake().is_some_and(|at| at <= self.now) {
            self.channel.deadline(self.now);
        }
        self.take();
    }

    /// How many times `page` was published shared since the last call.
    fn shared_publications(&mut self, page: u16) -> usize {
        let count = self.effects.iter().filter(|effect| matches!(effect, Effect::Shared { page: shared, .. } if *shared == page)).count();
        self.effects.clear();
        count
    }

    /// The channel's copy of `page`, as it is now.
    fn shared(&self, page: u16) -> Rows {
        let frame = self.channel.shared_frames().find(|(number, _)| *number == page).map(|(_, frame)| frame.to_vec()).unwrap();
        pages_in(&frame).into_iter().rev().find(|(number, _)| *number == page).unwrap().1
    }

    /// The latest copy of `page` sent to `session` alone.
    fn own(&self, session: SessionId, page: u16) -> Rows {
        let bytes: Vec<u8> = sent(&self.effects, session).iter().flat_map(|packet| packet.bytes).collect();
        pages_in(&bytes).into_iter().rev().find(|(number, _)| *number == page).unwrap().1
    }

    /// The latest cursor record sent to `session`.
    fn cursor(&self, session: SessionId) -> Cursor {
        sent(&self.effects, session).iter().filter_map(neoteletext_input::cursor::from_packet).map(Result::unwrap).next_back().unwrap()
    }

    /// The pages of every cursor record sent to `session`.
    fn cursor_pages(&self, session: SessionId) -> Vec<u16> {
        sent(&self.effects, session).iter().filter_map(neoteletext_input::cursor::from_packet).map(|cursor| cursor.unwrap().page).collect()
    }

    /// The latest `PAGE_MODES` record sent to `session`.
    fn page_modes(&self, session: SessionId) -> Option<(u16, u32)> {
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

/// Every page of the area in `bytes`, in order: its number and rows.
fn pages_in(bytes: &[u8]) -> Vec<(u16, Rows)> {
    let mut pages: Vec<(u16, Rows)> = Vec::new();
    for chunk in bytes.chunks(PACKET_BYTES) {
        let packet = Packet { bytes: chunk.try_into().unwrap() };
        match decode(&packet) {
            Ok(Decoded::Header(header)) => {
                let number = [MENU, FORM, GUESTBOOK, POLL, QUIZ, CALCULATOR, PAINT, SIGN]
                    .into_iter()
                    .find(|&number| {
                        let wanted = address(number);
                        wanted.magazine == header.magazine && Some(wanted.page) == header.page
                    })
                    .unwrap_or(0);
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
fn text(rows: &Rows, row: usize) -> String {
    rows[row].iter().map(|&code| char::from(code)).collect()
}

/// The first row holding `wanted`.
fn row_of(rows: &Rows, wanted: &str) -> Option<usize> {
    (0..ROWS).find(|&row| text(rows, row).contains(wanted))
}

/// Whether any row holds `wanted`.
fn shows(rows: &Rows, wanted: &str) -> bool {
    row_of(rows, wanted).is_some()
}

/// Typed text goes into the focused field, is acknowledged, and the cursor
/// follows it with ECHO and the room left; the masked field shows stars
/// and asks for MASKED; Enter greets by name.
#[test]
fn the_form_echoes_masks_and_greets() {
    let mut harness = Harness::new();
    harness.join(1, FORM);
    let first = harness.cursor(1);
    assert_eq!(first.flags, ECHO);
    let start = first.place.unwrap();
    assert_eq!(first.width, 16, "an empty field's room");

    harness.type_text(1, "Ada");
    let cursor = harness.cursor(1);
    assert_eq!(cursor.place, Some(Place { row: start.row, column: start.column.saturating_add(3) }), "the cursor after what was typed");
    assert_eq!(cursor.width, 13);
    assert_eq!(cursor.acked, 5, "HELLO, PAGE and three characters acted on");
    assert!(text(&harness.own(1, FORM), usize::from(start.row)).contains("Ada"));

    harness.press(1, "Backspace");
    harness.type_text(1, "a\u{1F600}[ ");
    assert!(text(&harness.own(1, FORM), usize::from(start.row)).contains("Ada "), "what the page cannot show is dropped");
    assert_eq!(harness.cursor(1).place, Some(Place { row: start.row, column: start.column.saturating_add(4) }));

    harness.press(1, "Tab");
    harness.type_text(1, "secret");
    let cursor = harness.cursor(1);
    assert_eq!(cursor.flags, ECHO | MASKED);
    let page = harness.own(1, FORM);
    let secret_row = usize::from(cursor.place.unwrap().row);
    assert_ne!(secret_row, usize::from(start.row), "Tab moved the cursor");
    assert!(text(&page, secret_row).contains("******"), "{}", text(&page, secret_row));
    assert!(!shows(&page, "secret"), "the secret is never on the page");

    harness.press(1, "Enter");
    let page = harness.own(1, FORM);
    assert!(shows(&page, "Hello, Ada!"));
    assert!(shows(&page, "has 6 characters"));
}

/// A field takes no more than its limit, and says so with a cursor of no
/// width.
#[test]
fn a_full_field_stops_and_stops_the_echo() {
    let mut harness = Harness::new();
    harness.join(1, FORM);
    harness.type_text(1, "abcdefghijklmnopqrstuvwxyz");
    let cursor = harness.cursor(1);
    assert_eq!((cursor.width, cursor.flags), (0, 0), "no room, so no echo");
    assert!(cursor.place.is_some(), "the cursor stays at the end");
    let page = harness.own(1, FORM);
    assert!(shows(&page, "abcdefghijklmnop"));
    assert!(!shows(&page, "abcdefghijklmnopq"));
}

/// A post appears on the shared book at once, for everyone; a second
/// post from the same session within the interval is refused; a long
/// line is cut to the limit; the book keeps only the newest lines.
#[test]
fn the_guestbook_is_shared_and_bounded() {
    let mut harness = Harness::new();
    harness.join(1, GUESTBOOK);
    harness.effects.clear();
    harness.type_text(1, "Hello from one");
    assert!(harness.cursor_pages(1).contains(&SIGN), "typing on the book goes to the signing page");
    harness.open(1, SIGN);
    assert!(shows(&harness.own(1, SIGN), "Hello from one"), "with what was typed");
    harness.shared_publications(GUESTBOOK);
    harness.press(1, "Enter");
    assert!(harness.cursor_pages(1).contains(&GUESTBOOK), "back to the book");
    assert_eq!(harness.shared_publications(GUESTBOOK), 1, "one publication, shared");
    assert!(shows(&harness.shared(GUESTBOOK), "Hello from one"));

    harness.open(1, SIGN);
    harness.type_text(1, "Again");
    harness.press(1, "Enter");
    assert!(!shows(&harness.shared(GUESTBOOK), "Again"), "too soon");
    assert!(shows(&harness.own(1, SIGN), "Wait 5 more seconds"));

    harness.advance(POST_INTERVAL_MS);
    harness.press(1, "Enter");
    assert!(shows(&harness.shared(GUESTBOOK), "Again"), "after the interval");

    let long = "x".repeat(MESSAGE_LIMIT.saturating_add(10));
    harness.join(2, SIGN);
    harness.type_text(2, &long);
    harness.press(2, "Enter");
    harness.advance(REPUBLISH_INTERVAL_MS);
    let book = harness.shared(GUESTBOOK);
    assert!(shows(&book, &"x".repeat(MESSAGE_LIMIT)));
    assert!(!shows(&book, &"x".repeat(MESSAGE_LIMIT.saturating_add(1))));

    for session in 10_u64..30 {
        harness.join(session, SIGN);
        harness.type_text(session, &alloc::format!("Viewer {session}"));
        harness.press(session, "Enter");
    }
    harness.advance(REPUBLISH_INTERVAL_MS);
    let book = harness.shared(GUESTBOOK);
    assert!(row_of(&book, "Viewer 29").unwrap() < row_of(&book, "Viewer 28").unwrap(), "newest first");
    let kept = (10_u64..30).filter(|session| shows(&book, &alloc::format!("Viewer {session}"))).count();
    assert_eq!(kept, ENTRIES_KEPT);
    assert!(!shows(&book, "Hello from one"), "the oldest went");
}

/// A burst of changes to a shared page is published at once and then at
/// most once more per interval, with the last change.
#[test]
fn a_burst_is_coalesced() {
    let mut harness = Harness::new();
    for session in 1_u64..=20 {
        harness.join(session, POLL);
    }
    harness.shared_publications(POLL);
    for session in 1_u64..=20 {
        harness.type_text(session, "1");
    }
    assert_eq!(harness.shared_publications(POLL), 1, "the first change at once, the rest wait");
    harness.advance(REPUBLISH_INTERVAL_MS);
    assert_eq!(harness.shared_publications(POLL), 1, "the rest in one");
    let poll = harness.shared(POLL);
    let news = row_of(&poll, "1 News").unwrap();
    assert!(text(&poll, news).contains(" 20  100%"), "{}", text(&poll, news));
    harness.advance(REPUBLISH_INTERVAL_MS);
    assert_eq!(harness.shared_publications(POLL), 0, "nothing more to send");
}

/// A session has one vote: voting again for the same answer counts
/// nothing, changing it moves it (not sooner than the interval), and it
/// goes when the session ends.
#[test]
fn votes_count_once_per_session() {
    let mut harness = Harness::new();
    for session in 1..=3 {
        harness.join(session, POLL);
    }
    harness.type_text(1, "2");
    harness.type_text(1, "2");
    harness.type_text(2, "2");
    harness.type_text(3, "4");
    harness.type_text(3, "9");
    harness.advance(REPUBLISH_INTERVAL_MS);
    let poll = harness.shared(POLL);
    assert!(shows(&poll, "3 votes"));
    let weather = row_of(&poll, "2 Weather").unwrap();
    assert!(text(&poll, weather).contains("  2   66%"), "{}", text(&poll, weather));

    harness.type_text(1, "3");
    harness.advance(REPUBLISH_INTERVAL_MS);
    let line = text(&harness.shared(POLL), weather);
    assert!(line.contains("  2   66%"), "a change within the interval is refused: {line}");

    harness.advance(CHANGE_INTERVAL_MS);
    harness.type_text(1, "3");
    harness.advance(REPUBLISH_INTERVAL_MS);
    let poll = harness.shared(POLL);
    assert!(text(&poll, weather).contains("  1   33%"), "{}", text(&poll, weather));
    assert!(shows(&poll, "3 votes"));

    harness.channel.closed(3, harness.now);
    harness.advance(REPUBLISH_INTERVAL_MS);
    assert!(shows(&harness.shared(POLL), "2 votes"), "the ended session's vote went");
}

/// A bar is mosaic graphics as long as its share.
#[test]
fn the_results_have_bars() {
    let mut harness = Harness::new();
    harness.join(1, POLL);
    harness.join(2, POLL);
    harness.type_text(1, "1");
    harness.type_text(2, "3");
    harness.advance(REPUBLISH_INTERVAL_MS);
    let poll = harness.shared(POLL);
    #[allow(clippy::naive_bytecount, reason = "a row of 40 cells, and no new dependency for counting them")]
    let bar_cells = |label: &str| poll[row_of(&poll, label).unwrap().saturating_add(1)].iter().filter(|&&code| code == BAR_CELL).count();
    assert_eq!(bar_cells("1 News"), BAR_CELLS / 2);
    assert_eq!(bar_cells("3 Sport"), BAR_CELLS / 2);
    assert_eq!(bar_cells("2 Weather"), 0);
}

/// The quiz scores right answers and ends with the score.
#[test]
fn the_quiz_scores() {
    let mut harness = Harness::new();
    harness.join(1, QUIZ);
    for (number, question) in QUESTIONS.iter().enumerate() {
        // The first two answered wrong, the rest right.
        let answer = if number < 2 { question.right.saturating_add(1) % 4 } else { question.right };
        harness.type_text(1, &alloc::format!("{}", answer.saturating_add(1)));
        if number == 0 {
            assert!(shows(&harness.own(1, QUIZ), "No: it was 2, 1974"));
        }
    }
    let page = harness.own(1, QUIZ);
    assert!(shows(&page, "You scored 8 out of 10"), "{}", text(&page, 8));
    harness.type_text(1, "1");
    assert!(shows(&harness.own(1, QUIZ), "You scored 8"), "a digit after the end does nothing");
    harness.press(1, "Enter");
    assert!(shows(&harness.own(1, QUIZ), "Question 1 of 10"), "Enter plays again");
}

/// Sums: precedence, parentheses, signs, and every failure an answer.
#[test]
fn sums_are_checked() {
    assert_eq!(evaluate("2+3*4"), Ok(14));
    assert_eq!(evaluate("(2+3)*4"), Ok(20));
    assert_eq!(evaluate(" -7 / 2 "), Ok(-3));
    assert_eq!(evaluate("--5"), Ok(5));
    assert_eq!(evaluate("10-2-3"), Ok(5));
    assert_eq!(evaluate("1/0"), Err(Problem::DivisionByZero));
    assert_eq!(evaluate("5/(3-3)"), Err(Problem::DivisionByZero));
    assert_eq!(evaluate("9223372036854775807+1"), Err(Problem::Overflow));
    assert_eq!(evaluate("99999999999999999999"), Err(Problem::Overflow));
    assert_eq!(evaluate("-9223372036854775807-1"), Ok(i64::MIN));
    assert_eq!(evaluate("(-9223372036854775807-1)/-1"), Err(Problem::Overflow));
    assert_eq!(evaluate("3000000000*4000000000"), Err(Problem::Overflow));
    assert_eq!(evaluate("(2"), Err(Problem::Unreadable));
    assert_eq!(evaluate("2)"), Err(Problem::Unreadable));
    assert_eq!(evaluate("2 3"), Err(Problem::Unreadable));
    assert_eq!(evaluate(""), Err(Problem::Unreadable));
    assert_eq!(evaluate(&"(".repeat(1_000)), Err(Problem::Unreadable), "deep nesting is refused, not recursed");
    assert_eq!(evaluate(&"-".repeat(1_000)), Err(Problem::Unreadable));
}

/// The calculator page takes what is typed, with x for times, and shows
/// the answer or the problem.
#[test]
fn the_calculator_answers_on_its_page() {
    let mut harness = Harness::new();
    harness.join(1, CALCULATOR);
    harness.type_text(1, "6x7");
    assert!(shows(&harness.own(1, CALCULATOR), "6x7"));
    harness.press(1, "Enter");
    assert!(shows(&harness.own(1, CALCULATOR), "= 42"));
    harness.type_text(1, "1/0");
    harness.press(1, "Enter");
    assert!(shows(&harness.own(1, CALCULATOR), "Cannot divide by zero"));
    harness.type_text(1, "9223372036854775807*2");
    harness.press(1, "Enter");
    assert!(shows(&harness.own(1, CALCULATOR), "Too big for 64 bits"));
}

/// The paint page asks for mouse buttons; a click paints a block in the
/// chosen colour, a second click rubs it out, the palette chooses, and C
/// clears.
#[test]
fn the_paint_box_paints() {
    let mut harness = Harness::new();
    harness.join(1, PAINT);
    assert_eq!(harness.page_modes(1), Some((PAINT, MOUSE_BUTTONS)));

    let (column, row) = (FIRST_COLUMN.saturating_add(4), TOP.saturating_add(2));
    harness.click(1, LEFT, column, row);
    assert_eq!(harness.own(1, PAINT)[row][column], SOLID);

    let red = PALETTE.iter().position(|&colour| colour == Colour::Red).unwrap();
    harness.click(1, LEFT, PALETTE_COLUMN.saturating_add(red.saturating_mul(SWATCH_CELLS)).saturating_add(1), PALETTE_ROW);
    harness.click(1, LEFT, column.saturating_add(2), row);
    let page = harness.own(1, PAINT);
    assert_eq!(page[row][column.saturating_add(2)], SOLID);
    assert_eq!(page[row][column.saturating_add(1)], Colour::Red.mosaic(), "the block's own colour");
    assert_ne!(page[row][column.saturating_sub(1)], Colour::Red.mosaic(), "the first block keeps its colour");

    harness.click(1, LEFT, column.saturating_add(2), row);
    assert_eq!(harness.own(1, PAINT)[row][column.saturating_add(2)], b' ', "clicked again in its colour: rubbed out");
    harness.click(1, RIGHT, column, row);
    assert_eq!(harness.own(1, PAINT)[row][column], b' ', "the right button rubs out");
    harness.click(1, LEFT, column.saturating_add(2), row);

    harness.type_text(1, "c");
    assert_eq!(harness.own(1, PAINT)[row][column.saturating_add(2)], b' ', "cleared");
}

/// Only what a page in the English set shows as typed passes.
#[test]
fn only_printable_characters_pass() {
    for character in ['a', 'Z', '7', ' ', '!', '#', '\u{a3}', '@', '?'] {
        assert_eq!(printable(character), Some(character), "{character}");
    }
    for character in ['[', '\\', ']', '^', '_', '`', '{', '|', '}', '~', '\u{7f}', '\n', '\u{e9}', '\u{1F600}'] {
        assert_eq!(printable(character), None, "{character:?}");
    }
}

/// Ending a session forgets it everywhere.
#[test]
fn an_ended_session_is_forgotten() {
    let mut harness = Harness::new();
    harness.join(1, FORM);
    harness.type_text(1, "Ada");
    harness.channel.closed(1, harness.now);
    harness.effects.clear();
    harness.join(1, FORM);
    assert!(!shows(&harness.own(1, FORM), "Ada"), "a new session with the same number starts afresh");
}
