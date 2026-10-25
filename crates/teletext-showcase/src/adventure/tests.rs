//! Tests of the adventures, driving a channel as a receiver would: the
//! pages it publishes, the commands typed into Station Zero and what
//! they change, a game played to its end, and The Lost Crown's links.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use neoteletext_core::consts::PACKET_BYTES;
use neoteletext_core::packet::{Decoded, Packet, decode};
use neoteletext_input::cursor::{Cursor, Place};
use neoteletext_input::event::{Action, Event, Item as InputItem, Key, KeyCode, write};
use neoteletext_input::modes::{ALL_MODES, CURSOR_CAP, ECHO_CAP, PAGE_CAP};
use teletext_runtime::{Channel, ChannelConfig, Effect, SessionId};

use super::art::{self, Art};
use super::crown::story::{CROSSROADS, Ending, NODES};
use super::crown::{self, pictures as crown_pictures};
use super::station::world::{self, ROOMS};
use super::station::{
    self, COMMAND_COLUMN, COMMAND_ROW, DESCRIPTION_ROWS, EXITS_ROW, Game, ITEMS_ROW, NARROW_WIDTH, REPLY_ROWS, SAID_ROW, STATUS_ROW, TEXT_WIDTH,
};
use super::text::{words, wrap};
use super::{MENU, applications};

/// A viewer's session.
const VIEWER: SessionId = 1;
/// Another viewer's.
const OTHER: SessionId = 2;

/// One page as received: its number, its rows' text and its Fastext
/// links.
#[derive(Clone, Debug)]
struct Received {
    number: u16,
    rows: Vec<String>,
    links: Vec<Option<u16>>,
}

impl Received {
    fn row(&self, row: usize) -> &str {
        self.rows.get(row).map_or("", String::as_str)
    }

    fn text(&self) -> String {
        self.rows.join("\n")
    }
}

/// The text of a row's bytes: parity stripped, controls as spaces.
fn row_text(bytes: &[u8]) -> String {
    bytes.iter().map(|&byte| byte & 0x7F).map(|code| if code < 0x20 { ' ' } else { char::from(code) }).collect()
}

/// A page number from its magazine and page.
fn number(magazine: u8, page: u8) -> u16 {
    (u16::from(magazine) << 8) | u16::from(page)
}

/// The pages in a stream of packets, in order.
fn pages_in(bytes: &[u8]) -> Vec<Received> {
    let mut pages: Vec<Received> = Vec::new();
    for chunk in bytes.chunks(PACKET_BYTES) {
        let Ok(bytes) = chunk.try_into() else { continue };
        match decode(&Packet { bytes }) {
            Ok(Decoded::Header(header)) => {
                let page = header.page.map_or(0, |page| page.0);
                pages.push(Received { number: number(header.magazine.number(), page), rows: alloc::vec![String::new(); 25], links: Vec::new() });
            }
            Ok(Decoded::Row(row)) => {
                if let Some(slot) = pages.last_mut().and_then(|page| page.rows.get_mut(usize::from(row.row))) {
                    *slot = row_text(&row.bytes);
                }
            }
            Ok(Decoded::Links(links)) => {
                if let Some(page) = pages.last_mut() {
                    page.links = links.links.iter().map(|link| link.map(|link| number(link.magazine.number(), link.page.0))).collect();
                }
            }
            _ => {}
        }
    }
    pages
}

/// The cursor records in a stream of packets.
fn cursors(bytes: &[u8]) -> Vec<Cursor> {
    bytes
        .chunks(PACKET_BYTES)
        .filter_map(|chunk| chunk.try_into().ok())
        .filter_map(|bytes| neoteletext_input::cursor::from_packet(&Packet { bytes }))
        .filter_map(Result::ok)
        .collect()
}

fn message(item: &InputItem<'_>) -> Vec<u8> {
    let mut bytes = alloc::vec![0_u8; neoteletext_input::MAX_MESSAGE];
    let length = write(item, &mut bytes).unwrap();
    bytes.truncate(length);
    bytes
}

/// A channel of the adventures, started.
struct Harness {
    channel: Channel,
    started: Vec<Effect>,
}

impl Harness {
    fn new() -> Self {
        let config = ChannelConfig {
            name: "Adventures".to_string(),
            push_address: "ws://127.0.0.1:1/adventures/push".to_string(),
            initial_page: MENU,
            epoch_at_start_ms: 0,
        };
        let mut channel = Channel::new(config, applications(&[]), 7);
        channel.start(0);
        let started = channel.take_effects();
        Self { channel, started }
    }

    /// Connects `session` and says HELLO, as the browser does.
    fn hello(&mut self, session: SessionId) {
        self.channel.opened(session, 0);
        let hello = Event::Hello { version: neoteletext_input::VERSION, caps: PAGE_CAP | CURSOR_CAP | ECHO_CAP | ALL_MODES, agent: None };
        self.send(session, &InputItem::Event(hello));
    }

    /// A viewer connects, is welcomed, and opens `page`.
    fn join(&mut self, session: SessionId, page: u16) -> Vec<u8> {
        self.hello(session);
        self.send(session, &InputItem::Event(Event::Page(page)))
    }

    /// What the channel sent `session` after `item`.
    fn send(&mut self, session: SessionId, item: &InputItem<'_>) -> Vec<u8> {
        self.channel.received(session, &message(item), 0);
        let mut bytes = Vec::new();
        for effect in self.channel.take_effects() {
            if let Effect::Send { session: to, bytes: sent } = effect
                && to == session
            {
                bytes.extend_from_slice(&sent);
            }
        }
        bytes
    }

    fn key(&mut self, session: SessionId, name: &str) -> Vec<u8> {
        let code = neoteletext_input::keys::code(name).unwrap();
        let key = Key { action: Action::Press, key: KeyCode::Functional(code), mods: 0, text: None, base: None };
        self.send(session, &InputItem::Event(Event::Key(key)))
    }

    /// Types `command` and presses Enter; answers page 401 as it came last.
    fn command(&mut self, session: SessionId, command: &str) -> Received {
        let mut bytes = self.send(session, &InputItem::Text(command));
        bytes.extend(self.key(session, "Enter"));
        last(&bytes, station::PAGE)
    }
}

/// The last copy of `page` in `bytes`.
fn last(bytes: &[u8], page: u16) -> Received {
    pages_in(bytes).into_iter().rev().find(|received| received.number == page).unwrap()
}

/// The page numbers of the story.
fn story_pages() -> BTreeSet<u16> {
    NODES.iter().map(|node| node.page).collect()
}

/// Every page is published once, shared, at the start: the menu, the
/// game's page before anyone types, and every node of the story
/// (decision 7).
#[test]
fn every_page_is_published_once_and_shared() {
    let harness = Harness::new();
    let mut counts: BTreeMap<u16, usize> = BTreeMap::new();
    for effect in &harness.started {
        if let Effect::Shared { page, .. } = effect {
            *counts.entry(*page).or_default() += 1;
        }
    }
    let mut expected: BTreeSet<u16> = story_pages();
    expected.insert(MENU);
    expected.insert(station::PAGE);
    assert_eq!(counts.keys().copied().collect::<BTreeSet<u16>>(), expected);
    assert!(counts.values().all(|&count| count == 1), "each page once: {counts:?}");
    assert!(harness.started.iter().all(|effect| matches!(effect, Effect::Shared { .. })), "nothing for any viewer at the start");
}

/// Opening the game makes it the viewer's own page, with the cursor on
/// the command line; typing echoes there and moves the cursor.
#[test]
fn the_game_is_the_viewers_own_with_a_cursor_after_the_typing() {
    let mut harness = Harness::new();
    harness.hello(VIEWER);
    harness.channel.received(VIEWER, &message(&InputItem::Event(Event::Page(station::PAGE))), 0);
    let effects = harness.channel.take_effects();
    assert!(effects.contains(&Effect::Own { session: VIEWER, page: station::PAGE }));

    let bytes = harness.send(VIEWER, &InputItem::Text("loo"));
    let page = last(&bytes, station::PAGE);
    assert!(page.row(COMMAND_ROW).contains("> loo"), "the typing is on the command line: {:?}", page.row(COMMAND_ROW));
    let cursor = *cursors(&bytes).last().unwrap();
    let column = u8::try_from(COMMAND_COLUMN + 3).unwrap();
    assert_eq!(cursor.place, Some(Place { row: u8::try_from(COMMAND_ROW).unwrap(), column }), "the cursor after the typing");
    assert_eq!(cursor.flags, neoteletext_input::cursor::ECHO);

    let page = last(&harness.key(VIEWER, "Backspace"), station::PAGE);
    assert!(page.row(COMMAND_ROW).contains("> lo "), "Backspace takes a character: {:?}", page.row(COMMAND_ROW));
    let page = last(&harness.key(VIEWER, "Escape"), station::PAGE);
    assert!(!page.row(COMMAND_ROW).contains("lo"), "Escape clears the line: {:?}", page.row(COMMAND_ROW));
}

/// Commands move the player and change what the page shows; each
/// viewer's game is their own.
#[test]
fn commands_move_the_player_and_each_viewer_has_their_own_game() {
    let mut harness = Harness::new();
    let first = last(&harness.join(VIEWER, station::PAGE), station::PAGE);
    assert!(first.row(1).contains("Cryo Bay"), "{}", first.text());
    assert!(first.row(EXITS_ROW).contains("Exits: south, east"), "{}", first.row(EXITS_ROW));

    let page = harness.command(VIEWER, "south");
    assert!(page.row(1).contains("Crew Corridor"), "{}", page.text());
    assert!(page.row(SAID_ROW).contains("> south"), "the command is repeated: {}", page.row(SAID_ROW));
    assert!(page.row(STATUS_ROW).contains("Moves 1"), "{}", page.row(STATUS_ROW));

    let other = last(&harness.join(OTHER, station::PAGE), station::PAGE);
    assert!(other.row(1).contains("Cryo Bay"), "the other viewer starts afresh: {}", other.text());

    let page = harness.command(VIEWER, "w");
    assert!(page.text().contains("You cannot go that way."), "{}", page.text());
    assert!(page.row(1).contains("Crew Corridor"));
}

/// Taking and dropping move items between the room and the inventory.
#[test]
fn taking_and_dropping_change_the_room_and_the_inventory() {
    let mut harness = Harness::new();
    harness.join(VIEWER, station::PAGE);
    let page = harness.command(VIEWER, "e");
    assert!(page.row(1).contains("Medical Bay"));
    assert!(page.row(ITEMS_ROW).contains("You see: radiation suit"), "{}", page.row(ITEMS_ROW));
    let page = harness.command(VIEWER, "take the suit");
    assert!(page.text().contains("Taken: radiation suit."), "{}", page.text());
    assert!(!page.row(ITEMS_ROW).contains("radiation suit"), "no longer here: {}", page.row(ITEMS_ROW));
    let page = harness.command(VIEWER, "i");
    assert!(page.text().contains("You carry: radiation suit."), "{}", page.text());
    let page = harness.command(VIEWER, "drop suit");
    assert!(page.row(ITEMS_ROW).contains("You see: radiation suit"), "{}", page.row(ITEMS_ROW));
}

/// The solution, command by command, from waking to the docking bay.
const SOLUTION: [&str; 38] = [
    "s",
    "e",
    "open locker",
    "take keycard",
    "w",
    "s",
    "use keycard",
    "e",
    "read datapad",
    "e",
    "take crowbar",
    "w",
    "w",
    "w",
    "use crowbar on machine",
    "take fuse",
    "e",
    "n",
    "n",
    "e",
    "take suit",
    "w",
    "s",
    "s",
    "s",
    "e",
    "use fuse",
    "w",
    "n",
    "up",
    "e",
    "press the beacon",
    "w",
    "use console",
    "d",
    "s",
    "s",
    "s",
];

/// The game can be won, by the solution typed into the channel, and the
/// puzzles hold until solved.
#[test]
fn the_game_can_be_won() {
    let mut harness = Harness::new();
    harness.join(VIEWER, station::PAGE);
    // The puzzles refuse before they are solved.
    assert!(harness.command(VIEWER, "s").row(1).contains("Crew Corridor"));
    assert!(harness.command(VIEWER, "s").row(1).contains("Central Hub"));
    assert!(harness.command(VIEWER, "e").text().contains("sealed"), "the laboratory is locked");
    assert!(harness.command(VIEWER, "u").text().contains("no power"), "the lift is dead");
    assert!(harness.command(VIEWER, "restart").row(1).contains("Cryo Bay"));

    let mut page = None;
    for command in SOLUTION {
        page = Some(harness.command(VIEWER, command));
    }
    let page = page.unwrap();
    assert!(page.row(1).contains("Docking Bay"), "{}", page.text());
    let page = harness.command(VIEWER, "board shuttle");
    assert!(page.row(1).contains("Homeward"), "{}", page.text());
    let moves = SOLUTION.len() + 1;
    assert!(page.text().contains(&alloc::format!("Mission complete in {moves} moves")), "{}", page.text());
    assert!(page.text().contains("rescue"), "the beacon was sent: {}", page.text());
    let page = harness.command(VIEWER, "n");
    assert!(page.text().contains("RESTART"), "the game is over: {}", page.text());
}

/// A session's game is forgotten when it ends.
#[test]
fn an_ended_session_is_forgotten() {
    let mut harness = Harness::new();
    harness.join(VIEWER, station::PAGE);
    assert!(harness.command(VIEWER, "s").row(1).contains("Crew Corridor"));
    harness.channel.closed(VIEWER, 0);
    let page = last(&harness.join(VIEWER, station::PAGE), station::PAGE);
    assert!(page.row(1).contains("Cryo Bay"), "a new game: {}", page.text());
}

/// Whether all of `marked` fits `rows` rows, the first `narrow` of them
/// beside a picture.
fn fits(marked: &str, narrow: usize, rows: usize) -> bool {
    let width = |line: usize| if line < narrow { NARROW_WIDTH } else { TEXT_WIDTH };
    let placed: usize = wrap(marked, &width, rows).iter().map(Vec::len).sum();
    placed == words(marked).len()
}

/// Every description, reply and examination fits its place on the page
/// along the whole solution, and every thing can be examined and used.
#[test]
fn the_games_text_fits_the_page() {
    let check = |game: &Game| {
        assert!(fits(&game.description(), station::pictures::ROWS, DESCRIPTION_ROWS), "{}", game.description());
        assert!(fits(game.reply(), 0, REPLY_ROWS), "{}", game.reply());
        assert!(fits(game.ending(), station::pictures::ROWS, DESCRIPTION_ROWS), "{}", game.ending());
    };
    let mut game = Game::new();
    check(&game);
    for command in SOLUTION {
        game.command(command);
        check(&game);
        for thing in ["keycard", "crowbar", "fuse", "suit", "datapad", "mug", "pod", "locker", "machine", "door", "panel", "console", "radio", "shuttle"] {
            for verb in ["examine", "use", "open", "take", "drop"] {
                let mut copy = game.clone();
                copy.command(&alloc::format!("{verb} {thing}"));
                check(&copy);
            }
        }
        for command in ["help", "hint", "inventory", "look", "take all", "xyzzy", "dance", "go", "wait"] {
            let mut copy = game.clone();
            copy.command(command);
            check(&copy);
        }
    }
    for room in ROOMS {
        assert!(world::about(room).name.chars().count() <= 21, "{room:?}'s name fits the band");
    }
}

/// Every room is reachable from the cryo bay, and every way back exists.
#[test]
fn the_station_map_joins_up() {
    let mut seen = Vec::new();
    let mut queue = alloc::vec![world::Room::CryoBay];
    while let Some(room) = queue.pop() {
        if seen.contains(&room) {
            continue;
        }
        seen.push(room);
        for direction in world::DIRECTIONS {
            if let Some(to) = world::exit(room, direction) {
                let back = world::DIRECTIONS.into_iter().any(|back| world::exit(to, back) == Some(room));
                assert!(back, "{to:?} leads back to {room:?}");
                queue.push(to);
            }
        }
    }
    assert_eq!(seen.len(), ROOMS.len());
}

/// Every Fastext link of the story leads to a node or the menu, every
/// node can be reached from the first, every ending leads back to the
/// start, and the published pages carry exactly those links.
#[test]
fn every_fastext_link_leads_to_a_node() {
    let pages = story_pages();
    assert!((20..=30).contains(&NODES.len()), "{} nodes", NODES.len());
    let mut reached = BTreeSet::new();
    let mut queue = alloc::vec![CROSSROADS];
    while let Some(page) = queue.pop() {
        if !reached.insert(page) {
            continue;
        }
        let node = NODES.iter().find(|node| node.page == page).unwrap();
        assert!(!node.choices.is_empty() && node.choices.len() <= 4, "{page:03X}");
        for choice in node.choices {
            assert!(pages.contains(&choice.to) || choice.to == MENU, "{page:03X} leads to {:03X}", choice.to);
            assert!(choice.label.chars().count() <= 9, "{:?} fits its Fastext slot", choice.label);
            assert!(choice.text.chars().count() <= 36, "{:?} fits its row", choice.text);
            if pages.contains(&choice.to) {
                queue.push(choice.to);
            }
        }
        if node.ending != Ending::Not {
            assert!(node.choices.iter().any(|choice| choice.to == CROSSROADS), "{page:03X} leads back to the start");
        }
    }
    assert_eq!(reached, pages, "every node is reached");
    let endings = |ending: Ending| NODES.iter().filter(|node| node.ending == ending).count();
    assert!(endings(Ending::Won) >= 2 && endings(Ending::Lost) >= 3, "several endings");

    let harness = Harness::new();
    let mut checked = 0_usize;
    for effect in &harness.started {
        let Effect::Shared { page, frame } = effect else { continue };
        let Some(node) = NODES.iter().find(|node| node.page == *page) else { continue };
        let received = last(frame, *page);
        let mut expected: Vec<Option<u16>> = node.choices.iter().map(|choice| Some(choice.to)).collect();
        expected.resize(4, None);
        expected.push(Some(crate::index::INDEX));
        assert_eq!(received.links.get(..5).map(<[Option<u16>]>::to_vec), Some(expected), "{page:03X}'s links");
        assert!(received.text().contains(node.title), "{page:03X} shows its title");
        checked += 1;
    }
    assert_eq!(checked, NODES.len());
}

/// Every node's text fits the rows it is given.
#[test]
fn the_storys_text_fits_the_page() {
    for node in NODES {
        let placed: usize = wrap(node.text, &|_| crown::TEXT_WIDTH, crown::TEXT_ROWS).iter().map(Vec::len).sum();
        assert_eq!(placed, words(node.text).len(), "{:03X} does not fit: {}", node.page, node.text);
        assert!(node.title.chars().count() <= 36, "{:03X}'s title", node.page);
    }
}

/// Every picture is a whole grid of known sixels, its backgrounds one a
/// cell row, at the size its page leaves it.
#[test]
fn every_picture_is_whole() {
    let check = |picture: &Art, size: (usize, usize)| {
        assert_eq!(picture.size(), size);
        assert!(picture.sixels.iter().all(|row| row.chars().count() == size.0), "every row as wide");
        assert!(picture.sixels.iter().all(|row| row.chars().all(art::known)), "known letters only");
        assert_eq!(picture.backgrounds.chars().count(), picture.cells().1);
        assert!(picture.backgrounds.chars().all(art::known));
    };
    for picture in crown_pictures::ALL {
        check(picture, (40, 18));
    }
    for picture in station::pictures::ALL {
        check(picture, (28, 18));
    }
}

/// Marked-up text wraps at word boundaries, highlights the braced
/// words, and cuts a word longer than its line.
#[test]
fn text_wraps_and_highlights() {
    let lines = wrap("one {two three} four", &|_| 10, 5);
    let shown: Vec<Vec<(&str, bool)>> = lines.iter().map(|line| line.iter().map(|word| (word.text.as_str(), word.highlight)).collect()).collect();
    assert_eq!(shown, alloc::vec![alloc::vec![("one", false), ("two", true)], alloc::vec![("three", true), ("four", false)]]);
    let cut: Vec<String> = wrap("abcdefghij", &|_| 4, 5).iter().map(|line| line.iter().map(|word| word.text.clone()).collect()).collect();
    assert_eq!(cut, ["abcd", "efgh", "ij"]);
    assert_eq!(wrap("a b c", &|_| 1, 2).len(), 2, "no more lines than asked");
}
