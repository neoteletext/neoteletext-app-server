// Tests count with small numbers they choose.
#![allow(clippy::arithmetic_side_effects, reason = "tests: small known numbers")]

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use neoteletext_core::consts::PACKET_BYTES;
use neoteletext_core::packet::{Decoded, decode};
use neoteletext_core::types::PageAddress;
use neoteletext_input::event::{Action, Event, Item, Key, KeyCode, write};
use neoteletext_input::keys;
use neoteletext_input::modes::{ALL_MODES, CURSOR_CAP, ECHO_CAP, PAGE_CAP};
use teletext_runtime::{Channel, ChannelConfig, Colour, Effect, Packet, Screen, SessionId};

use super::breakout::Breakout;
use super::cave::Cave;
use super::invaders::Invaders;
use super::snake::Snake;
use super::*;
use crate::sound::Effect as Sound;

/// Dice that always throw the same: `self.0`, or the highest number below
/// the bound.
struct Loaded(u32);

impl Dice for Loaded {
    fn below(&mut self, bound: u32) -> u32 {
        self.0.min(bound.saturating_sub(1))
    }
}

fn channel(pictures: &[Picture]) -> Channel {
    let config = ChannelConfig { name: "Games".to_string(), push_address: "ws://127.0.0.1:1/games/push".to_string(), initial_page: MENU, epoch_at_start_ms: 0 };
    let mut channel = Channel::new(config, applications(pictures), 7);
    channel.start(0);
    channel
}

fn message(item: &Item<'_>) -> Vec<u8> {
    let mut bytes = alloc::vec![0_u8; neoteletext_input::MAX_MESSAGE];
    let length = write(item, &mut bytes).unwrap();
    bytes.truncate(length);
    bytes
}

/// A viewer: connected, welcomed, with `page` open.
fn viewer(channel: &mut Channel, session: SessionId, page: u16) {
    channel.opened(session, 0);
    let hello = Event::Hello { version: neoteletext_input::VERSION, caps: PAGE_CAP | CURSOR_CAP | ECHO_CAP | ALL_MODES, agent: None };
    channel.received(session, &message(&Item::Event(hello)), 0);
    open(channel, session, page, 0);
}

fn open(channel: &mut Channel, session: SessionId, page: u16, now: u64) {
    channel.received(session, &message(&Item::Event(Event::Page(page))), now);
}

fn key(channel: &mut Channel, session: SessionId, name: &str, action: Action, now: u64) {
    let mut characters = name.chars();
    let code = match (characters.next(), characters.next()) {
        (Some(character), None) => KeyCode::Character(character),
        _ => KeyCode::Functional(keys::code(name).unwrap()),
    };
    let key = Key { action, key: code, mods: 0, text: None, base: None };
    channel.received(session, &message(&Item::Event(Event::Key(key))), now);
}

fn tap(channel: &mut Channel, session: SessionId, name: &str, now: u64) {
    key(channel, session, name, Action::Press, now);
    key(channel, session, name, Action::Release, now);
}

/// Runs the channel's time on from `from` to `until`, a tick at a time.
fn run(channel: &mut Channel, from: u64, until: u64) {
    let mut now = from;
    while now < until {
        now += arcade::TICK_MS;
        if channel.next_wake().is_some_and(|at| at <= now) {
            channel.deadline(now);
        }
    }
}

/// The pages in `bytes`: each page's number and its rows' text.
fn pages_in(bytes: &[u8]) -> Vec<(u16, Vec<String>)> {
    let mut pages: Vec<(u16, Vec<String>)> = Vec::new();
    for chunk in bytes.chunks(PACKET_BYTES) {
        let Ok(chunk) = chunk.try_into() else { continue };
        match decode(&Packet { bytes: chunk }) {
            Ok(Decoded::Header(header)) => {
                let number = PageAddress { magazine: header.magazine, page: header.page.unwrap() }.mpp();
                pages.push((number, alloc::vec![String::new(); 25]));
            }
            Ok(Decoded::Row(row)) => {
                if let Some((_, rows)) = pages.last_mut()
                    && let Some(text) = rows.get_mut(usize::from(row.row))
                {
                    *text = row.bytes.iter().map(|byte| char::from(byte & 0x7F)).collect();
                }
            }
            _ => {}
        }
    }
    pages
}

/// The last version of `page` among `effects`: sent to `session`, or for
/// `None`, shared.
fn last_page(effects: &[Effect], session: Option<SessionId>, page: u16) -> Option<Vec<String>> {
    let mut found = None;
    for effect in effects {
        let bytes = match (effect, session) {
            (Effect::Shared { frame, .. }, None) => frame,
            (Effect::Send { session: to, bytes }, Some(session)) if *to == session => bytes,
            _ => continue,
        };
        for (number, rows) in pages_in(bytes) {
            if number == page {
                found = Some(rows);
            }
        }
    }
    found
}

fn contains(rows: &[String], text: &str) -> bool {
    rows.iter().any(|row| row.contains(text))
}

/// Every shared page is published at the start, once.
#[test]
fn every_page_is_published_at_the_start() {
    let mut channel = channel(&[]);
    let effects = channel.take_effects();
    for page in [MENU, INVADERS, SNAKE, BREAKOUT, CAVE, HALL] {
        assert_eq!(effects.iter().filter(|effect| matches!(effect, Effect::Shared { page: shared, .. } if *shared == page)).count(), 1, "page {page:X}");
    }
    assert!(contains(&last_page(&effects, None, MENU).unwrap(), "Space invaders"));
    assert!(contains(&last_page(&effects, None, HALL).unwrap(), "No scores yet"));
    assert!(contains(&last_page(&effects, None, SNAKE).unwrap(), "Press Space to start"));
    let numbers: Vec<u16> = channel.entries().map(|entry| entry.number).filter(|number| (MENU..=0x399).contains(number)).collect();
    assert_eq!(numbers, [MENU, INVADERS, SNAKE, BREAKOUT, CAVE, HALL]);
}

/// Viewers who only look cost nothing: no timer runs, and no page is made
/// for any of them (decision 7).
#[test]
fn looking_costs_nothing() {
    let mut channel = channel(&[]);
    channel.take_effects();
    let pages = [MENU, INVADERS, SNAKE, HALL];
    for session in 1..=50_u64 {
        viewer(&mut channel, session, pages[usize::try_from(session).unwrap() % pages.len()]);
    }
    let effects = channel.take_effects();
    assert!(!effects.iter().any(|effect| matches!(effect, Effect::Own { .. } | Effect::Shared { .. })), "no page of anyone's own, none changed");
    assert_eq!(channel.next_wake(), None, "no timer");
}

/// Space starts a game of the viewer's own, which ticks, and a held arrow
/// moves the paddle until it is released.
#[test]
fn a_game_is_the_viewers_own_and_answers_the_keys() {
    let mut channel = channel(&[]);
    viewer(&mut channel, 1, BREAKOUT);
    viewer(&mut channel, 2, BREAKOUT);
    channel.take_effects();
    tap(&mut channel, 1, " ", 0);
    let effects = channel.take_effects();
    assert!(effects.contains(&Effect::Own { session: 1, page: BREAKOUT }));
    assert!(!effects.iter().any(|effect| matches!(effect, Effect::Own { session: 2, .. })), "the other viewer still sees the title");
    let first = last_page(&effects, Some(1), BREAKOUT).unwrap();
    assert!(contains(&first, "SCORE 000000"));
    let paddle_row = 22;

    key(&mut channel, 1, "ArrowLeft", Action::Press, 0);
    run(&mut channel, 0, 500);
    let moved = last_page(&channel.take_effects(), Some(1), BREAKOUT).unwrap();
    assert_ne!(moved[paddle_row], first[paddle_row], "the paddle moved");

    key(&mut channel, 1, "ArrowLeft", Action::Release, 500);
    run(&mut channel, 500, 1_000);
    // With the paddle still and the ball on it, nothing changes, so no
    // page is sent.
    assert_eq!(last_page(&channel.take_effects(), Some(1), BREAKOUT), None, "released, it stays");
}

/// `FOCUS_OUT` releases every key held: no key-up will come for them.
#[test]
fn losing_the_focus_releases_the_keys() {
    let mut channel = channel(&[]);
    viewer(&mut channel, 1, BREAKOUT);
    tap(&mut channel, 1, " ", 0);
    key(&mut channel, 1, "ArrowRight", Action::Press, 0);
    channel.received(1, &message(&Item::Event(Event::FocusOut)), 0);
    run(&mut channel, 0, 100);
    let after_one = last_page(&channel.take_effects(), Some(1), BREAKOUT).unwrap();
    run(&mut channel, 100, 600);
    assert!(contains(&after_one, "SCORE"), "the press moved it a step");
    assert_eq!(last_page(&channel.take_effects(), Some(1), BREAKOUT), None, "then it stopped: nothing changed, nothing sent");
}

/// A game that ends goes into the shared hall of fame, which is published
/// once for every viewer; the timer stops when nobody is playing.
#[test]
fn a_finished_game_enters_the_shared_hall_of_fame() {
    let mut channel = channel(&[]);
    viewer(&mut channel, 1, CAVE);
    viewer(&mut channel, 2, HALL);
    channel.take_effects();
    tap(&mut channel, 1, " ", 0);
    assert!(channel.next_wake().is_some(), "the game ticks");
    // Left alone, the ship meets the cave's wall or a rock in the end.
    run(&mut channel, 0, 600_000);
    let effects = channel.take_effects();
    let mine = last_page(&effects, Some(1), CAVE).unwrap();
    assert!(contains(&mine, "GAME OVER"), "{mine:?}");
    assert_eq!(effects.iter().filter(|effect| matches!(effect, Effect::Shared { page: HALL, .. })).count(), 1, "the hall changed once");
    let hall = last_page(&effects, None, HALL).unwrap();
    assert!(contains(&hall, "Player 0001") && contains(&hall, "Cave flyer"), "{hall:?}");
    assert!(!effects.iter().any(|effect| matches!(effect, Effect::Send { session: 2, .. })), "the hall is shared: nothing sent to its viewer alone");
    assert_eq!(channel.next_wake(), None, "nobody playing, no timer");
}

/// A game left in play counts for the hall of fame too, and is put away.
#[test]
fn a_game_left_in_play_still_counts() {
    let mut channel = channel(&[]);
    viewer(&mut channel, 3, CAVE);
    tap(&mut channel, 3, " ", 0);
    run(&mut channel, 0, 2_000);
    channel.take_effects();
    open(&mut channel, 3, MENU, 2_000);
    let effects = channel.take_effects();
    let hall = last_page(&effects, None, HALL).expect("the hall changed");
    assert!(contains(&hall, "Player 0003") && contains(&hall, "Cave flyer"), "{hall:?}");
    let title = last_page(&effects, None, CAVE).unwrap();
    assert!(contains(&title, "by Player 0003"), "the title shows the best: {title:?}");
    run(&mut channel, 2_000, 2_500);
    assert_eq!(channel.next_wake(), None, "the game was put away");
}

/// Back on a game page after playing, the viewer is given the title page,
/// since the page of their own still overrides the shared one.
#[test]
fn coming_back_shows_the_title_again() {
    let mut channel = channel(&[]);
    viewer(&mut channel, 1, INVADERS);
    tap(&mut channel, 1, " ", 0);
    open(&mut channel, 1, MENU, 0);
    channel.take_effects();
    open(&mut channel, 1, INVADERS, 0);
    let title = last_page(&channel.take_effects(), Some(1), INVADERS).unwrap();
    assert!(contains(&title, "Press Space to start"));
}

/// Escape leaves for the menu.
#[test]
fn escape_goes_to_the_menu() {
    let mut channel = channel(&[]);
    viewer(&mut channel, 1, SNAKE);
    tap(&mut channel, 1, " ", 0);
    channel.take_effects();
    tap(&mut channel, 1, "Escape", 0);
    let pages: Vec<u16> = channel
        .take_effects()
        .iter()
        .filter_map(|effect| match effect {
            Effect::Send { session: 1, bytes } => Some(bytes.clone()),
            _ => None,
        })
        .flat_map(|bytes| {
            bytes
                .chunks(PACKET_BYTES)
                .filter_map(|chunk| neoteletext_input::cursor::from_packet(&Packet { bytes: chunk.try_into().unwrap() }))
                .map(|cursor| cursor.unwrap().page)
                .collect::<Vec<_>>()
        })
        .collect();
    assert!(pages.contains(&MENU), "{pages:x?}");
}

/// A shot kills an enemy: points, a laser and an explosion; the ship
/// moves.
#[test]
fn invaders_shoot_and_score() {
    let enemies = sprites::enemies(&[], invaders::ENEMY_WIDTH, invaders::ENEMY_HEIGHT);
    let mut game = Invaders::new();
    let mut sounds = Vec::new();
    // No bombs: the dice never fall under the bomb chance.
    let mut dice = Loaded(u32::MAX);
    let mut fire = Keys::default();
    fire.press(control::FIRE);
    for _ in 0..200 {
        game.tick(fire, &mut dice, &mut sounds);
        if game.score() > 0 {
            break;
        }
    }
    assert!(game.score() > 0, "a shot from the middle meets the wave");
    assert!(sounds.contains(&Sound::Laser) && sounds.contains(&Sound::Explosion));
    assert_eq!(game.lives(), 3);
    let mut screen = look::play_page(Game::Invaders);
    game.draw(&mut screen, &enemies);
    assert!(screen.row_text(1).contains("SCORE 0000"));

    let start = game.player_x();
    let mut left = Keys::default();
    left.press(control::LEFT);
    game.tick(left, &mut dice, &mut sounds);
    assert!(game.player_x() < start);
}

/// A wave shot down brings the next, of the next creatures, with a
/// fanfare and a banner naming them.
#[test]
fn invaders_come_in_waves() {
    let enemies = sprites::enemies(&[], invaders::ENEMY_WIDTH, invaders::ENEMY_HEIGHT);
    let mut game = Invaders::new();
    let mut sounds = Vec::new();
    let mut dice = Loaded(u32::MAX);
    // Past the first wave's banner.
    for _ in 0..20 {
        game.tick(Keys::default(), &mut dice, &mut sounds);
    }
    game.clear_wave();
    game.tick(Keys::default(), &mut dice, &mut sounds);
    assert_eq!(game.wave(), 1);
    assert!(sounds.contains(&Sound::Win));
    let mut screen = look::play_page(Game::Invaders);
    game.draw(&mut screen, &enemies);
    let second = invaders::enemy(&enemies, 1, 0).unwrap();
    assert_ne!(Some(second), invaders::enemy(&enemies, 0, 0), "new creatures");
    assert!((0..25).any(|row| screen.row_text(row).contains("WAVE 2")));
    assert!((0..25).any(|row| screen.row_text(row).contains(&second.name)));
}

/// Bombs cost lives, and the last one ends the game.
#[test]
fn invaders_bombs_end_the_game() {
    let mut game = Invaders::new();
    let mut sounds = Vec::new();
    // Bombs at every chance, from the middle column, over the ship.
    let mut dice = Loaded(2);
    for _ in 0..2_000 {
        game.tick(Keys::default(), &mut dice, &mut sounds);
        if game.over() {
            break;
        }
    }
    assert!(game.over());
    assert_eq!(game.lives(), 0);
    assert!(sounds.contains(&Sound::Lose));
}

/// The snake turns, never straight back, and dies at the wall.
#[test]
fn the_snake_turns_and_dies_at_the_wall() {
    let mut dice = Loaded(0);
    let mut snake = Snake::new(&mut dice);
    let mut sounds = Vec::new();
    let (x, y) = snake.head();
    snake.turn(control::LEFT);
    snake.turn(control::UP);
    for _ in 0..2 {
        snake.tick(&mut dice, &mut sounds);
    }
    assert_eq!(snake.head(), (x, y - 1), "left was refused, up taken");
    for _ in 0..200 {
        snake.tick(&mut dice, &mut sounds);
    }
    assert!(snake.over());
    assert!(sounds.contains(&Sound::Lose));
}

/// The snake grows when it eats.
#[test]
fn the_snake_grows() {
    let mut dice = Loaded(0);
    let mut snake = Snake::new(&mut dice);
    let mut sounds = Vec::new();
    // The food is on the first free square, the grid's top-left, and the
    // snake steps every second tick: up to the top row, then left to it.
    let (x, y) = snake.head();
    snake.turn(control::UP);
    for _ in 0..y * 2 {
        snake.tick(&mut dice, &mut sounds);
    }
    snake.turn(control::LEFT);
    for _ in 0..x * 2 {
        snake.tick(&mut dice, &mut sounds);
    }
    assert_eq!(snake.head(), (0, 0));
    assert!(snake.score() > 0 && sounds.contains(&Sound::Coin));
    snake.turn(control::DOWN);
    for _ in 0..6 {
        snake.tick(&mut dice, &mut sounds);
    }
    assert!(snake.length() > 4, "longer: {}", snake.length());
}

/// Breakout: the ball launches, the wall loses bricks, the paddle moves.
#[test]
fn breakout_launches_and_breaks_bricks() {
    let mut game = Breakout::new();
    let mut sounds = Vec::new();
    let mut fire = Keys::default();
    fire.press(control::FIRE);
    game.tick(fire, &mut sounds);
    assert!(game.in_flight());
    let full = game.bricks_left();
    for _ in 0..100 {
        game.tick(Keys::default(), &mut sounds);
    }
    assert!(game.bricks_left() < full);
    assert!(game.score() > 0);
    let mut right = Keys::default();
    right.press(control::RIGHT);
    let before = game.paddle_x();
    game.tick(right, &mut sounds);
    assert!(game.paddle_x() > before);
}

/// Breakout: a ball missed costs a life, and the last ends the game.
#[test]
fn breakout_ends_when_the_balls_are_lost() {
    let mut game = Breakout::new();
    let mut sounds = Vec::new();
    let mut left = Keys::default();
    // The paddle at the far left, the ball launched rightwards and up,
    // and never met again.
    left.press(control::LEFT);
    for _ in 0..3_000 {
        game.tick(left, &mut sounds);
        if game.over() {
            break;
        }
    }
    assert!(game.over());
    assert!(sounds.contains(&Sound::Lose));
}

/// The cave flyer climbs, and a ship left alone meets the rock.
#[test]
fn the_cave_flyer_climbs_and_crashes() {
    // The cave keeps to the top, and narrows away from the ship.
    let mut dice = Loaded(0);
    let mut cave = Cave::new(&mut dice);
    let mut sounds = Vec::new();
    let start = cave.ship_y();
    let mut up = Keys::default();
    up.press(control::UP);
    cave.tick(up, &mut dice, &mut sounds);
    assert!(cave.ship_y() < start);
    for _ in 0..5_000 {
        cave.tick(Keys::default(), &mut dice, &mut sounds);
        if cave.over() {
            break;
        }
    }
    assert!(cave.over() && cave.score() > 0);
}

/// A TTI page of `rows`, each (row, codes), codes under 0x20 in the TTI
/// escape form.
fn tti(rows: &[(u8, &[u8])]) -> Vec<u8> {
    let mut file = b"PN,10000\r\nSC,0000\r\nPS,8000\r\n".to_vec();
    for (row, codes) in rows {
        file.extend_from_slice(alloc::format!("OL,{row},").as_bytes());
        for &code in *codes {
            if code < 0x20 {
                file.extend_from_slice(&[0x1B, code + 0x40]);
            } else {
                file.push(code);
            }
        }
        file.extend_from_slice(b"\r\n");
    }
    file
}

/// A sprite is read back from its picture: the mosaics in their colour,
/// cut to the lit sixels, its label under it left out, and scaled.
#[test]
fn a_sprite_is_extracted_from_a_picture() {
    // A red block four cells wide and two rows tall, and a white label
    // two rows of cells under it.
    let block: &[u8] = &[0x11, 0x7F, 0x7F, 0x7F, 0x7F];
    let label: &[u8] = &[0x17, 0x23, 0x2C, 0x23, 0x2C, 0x23, 0x2C];
    let picture = tti(&[(5, block), (6, block), (9, label)]);
    let sprite = sprites::extract(&picture, 8, 6).unwrap();
    assert_eq!((sprite.width, sprite.height, sprite.colour), (8, 6, Colour::Red));
    assert_eq!(sprite.count(), 48, "every sixel of the block, none of the label");

    let small = sprites::extract(&picture, 4, 4).unwrap();
    assert_eq!((small.width, small.height, small.count()), (4, 3, 12), "scaled, keeping its shape");

    // Hold Mosaics repeats the last mosaic in an attribute's cell.
    let held: &[u8] = &[0x1E, 0x12, 0x7F, 0x13, 0x7F];
    let sprite = sprites::extract(&tti(&[(5, held), (6, held)]), 16, 12).unwrap();
    assert_eq!((sprite.width, sprite.count()), (6, 36), "the held block fills the attribute's cell");
    // In alphanumerics, the same codes are letters, not sixels.
    let text: &[u8] = &[0x02, 0x7F, 0x7F];
    assert!(sprites::extract(&tti(&[(5, text), (6, text)]), 8, 6).is_none());

    assert!(sprites::extract(b"not a page", 8, 6).is_none());
    assert!(sprites::extract(&tti(&[(5, &[0x11, 0x20, 0x20])]), 8, 6).is_none(), "nothing lit");
}

/// Pictures of the sci-fi group become the enemies, named from their
/// captions, in name order; without them there are drawn ones.
#[test]
fn the_enemies_come_from_the_sprite_pictures() {
    let block: &[u8] = &[0x16, 0x7F, 0x7F, 0x7F];
    let picture = |name: &str, caption: &str, group: char| Picture {
        name: name.to_string(),
        caption: caption.to_string(),
        group,
        levels: alloc::vec![(String::from("l15"), tti(&[(5, block), (6, block)]))],
    };
    let pictures = [picture("s02", "02_void_drifter (sheet)", 's'), picture("s01", "01_razor_crawler (sheet)", 's'), picture("a01", "Some art", 'a')];
    let enemies = sprites::enemies(&pictures, 10, 6);
    let names: Vec<&str> = enemies.iter().map(|sprite| sprite.name.as_str()).collect();
    assert_eq!(names, ["Razor crawler", "Void drifter"]);
    assert_eq!(enemies[0].colour, Colour::Cyan);
    assert_ne!(sprites::enemies(&[], 10, 6).len(), 0, "drawn ones");
}

/// The hall keeps the best, best first; a tie goes under the score it
/// ties.
#[test]
fn the_hall_keeps_the_best() {
    let mut hall = hall::Hall::default();
    assert!(!hall.record(Game::Snake, 1, 0), "nothing for nothing");
    assert!(hall.record(Game::Snake, 1, 50));
    assert!(hall.record(Game::Invaders, 2, 70));
    assert!(hall.record(Game::Cave, 3, 50));
    let order: Vec<SessionId> = hall.entries().iter().map(|entry| entry.player).collect();
    assert_eq!(order, [2, 1, 3]);
    for score in 100..112 {
        hall.record(Game::Breakout, 4, score);
    }
    assert!(!hall.record(Game::Snake, 5, 1), "the table is full of better");
    assert_eq!(hall.best(Game::Breakout).map(|(score, _)| score), Some(111));
    assert_eq!(hall.best(Game::Snake), None, "pushed off the table");
    assert!(hall.screen().row_text(5).contains("Player 0004"));
}

/// The canvas puts a colour change in the empty cell before a sprite, and
/// none where the colour stays.
#[test]
fn the_canvas_places_colours() {
    let mut canvas = canvas::Canvas::new(20, 3);
    canvas.rectangle(2, 0, 4, 3, Colour::Red);
    canvas.rectangle(10, 0, 2, 3, Colour::Red);
    canvas.rectangle(14, 0, 2, 3, Colour::Green);
    let mut screen = Screen::new();
    canvas.draw(&mut screen, 5, 1);
    let row: Vec<u8> = screen.row(5).unwrap()[..12].to_vec();
    // Cells 1 to 10 hold sixel columns 0 to 19: the red block in cells 2
    // and 3, its colour in cell 1; the second red block in cell 6 with no
    // new colour; green in cell 8, its colour in cell 7.
    assert_eq!(row, [0x20, 0x11, 0x7F, 0x7F, 0x20, 0x20, 0x7F, 0x12, 0x7F, 0x20, 0x20, 0x20]);
}
