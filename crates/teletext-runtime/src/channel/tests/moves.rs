//! A pointer's moves, at their pace and the latest kept (REQ-1360, DOC-1945,
//! DOC-1926).

use neoteletext_input::MOVES_PER_SECOND;

use super::*;

/// A second, in the host's milliseconds.
const SECOND_MS: u64 = 1_000;
/// Moves a client sends in that second: far more than the pace.
const FLOOD_MS_APART: u64 = 1;

/// When [`selected`]'s channel next wakes with no move waiting: its selection's
/// deadline, since the tests' program answers nothing.
const NOTHING_MOVING: Option<u64> = Some(PROGRAM_REPLY_LIMIT_MS);

/// A move to a dot of its own: its column and row show which it was.
fn moved_to(position: u32) -> Item<'static> {
    let at = Point { column: position % 40, row: position % 25, pixels: Some((position, 0)), mods: 0 };
    Item::Event(Event::Mouse(Mouse { on: ON, action: MouseAction::Move, button: 0, at, device: Device::Mouse }))
}

/// The column and row of a line's event, which `moved_to` made.
fn cell(line: &Value) -> (u64, u64) {
    (line["event"]["column"].as_u64().unwrap(), line["event"]["row"].as_u64().unwrap())
}

fn kind_and_action(line: &Value) -> (&str, &str) {
    (line["event"]["kind"].as_str().unwrap(), line["event"]["action"].as_str().unwrap_or(""))
}

/// Runs the channel's deadline if it has come.
fn wake(channel: &mut Channel, now: u64) {
    if channel.next_wake().is_some_and(|wake| wake <= now) {
        channel.deadline(now);
    }
}

/// REQ-1360: 1,000 moves a second from a client reach the program at most
/// 60 a second, and the last it is given is the latest.
#[test]
fn a_flood_of_moves_reaches_a_program_at_the_pace() {
    let mut channel = selected();
    let mut heard = Vec::new();
    let mut now = 0;
    for position in 1..=u32::try_from(SECOND_MS / FLOOD_MS_APART).unwrap() {
        now += FLOOD_MS_APART;
        wake(&mut channel, now);
        channel.received(CONNECTION, &message(&[moved_to(position)]), now);
        heard.extend(inputs(&channel.take_effects()));
    }
    let pace = usize::try_from(MOVES_PER_SECOND).unwrap();
    assert!(heard.len() <= pace, "{} moves in a second", heard.len());
    assert!(heard.len() >= pace - 2, "{} moves in a second: the pace is 60, not less", heard.len());
    let wake_at = channel.next_wake().expect("the latest waits its turn");
    channel.deadline(wake_at);
    let last = inputs(&channel.take_effects());
    assert_eq!(last.len(), 1, "{last:?}");
    assert_eq!(cell(&last[0]), (0, 0), "the thousandth move, not one before it: {last:?}");
    assert_eq!(channel.next_wake(), NOTHING_MOVING, "nothing runs while the pointer is still");
}

/// DOC-1945: any other event brings the move waiting first, so that a release
/// never comes before the move that ended the drag.
#[test]
fn a_release_brings_the_move_that_ended_the_drag_first() {
    let mut channel = selected();
    channel.received(CONNECTION, &message(&[mouse(ON, MouseAction::Press, 0)]), 1);
    channel.received(CONNECTION, &message(&[moved_to(1), moved_to(2), moved_to(3)]), 2);
    let _ = channel.take_effects();
    channel.received(CONNECTION, &message(&[mouse(ON, MouseAction::Release, 0)]), 3);
    let lines = inputs(&channel.take_effects());
    let order: Vec<_> = lines.iter().map(kind_and_action).collect();
    assert_eq!(order, [("mouse", "move"), ("mouse", "release")], "{lines:?}");
    assert_eq!(cell(&lines[0]), (3, 3), "the last of the drag");
}

/// DOC-1945: a key is not held behind a move, and takes it along.
#[test]
fn a_key_is_never_held_behind_a_move() {
    let mut channel = selected();
    channel.received(CONNECTION, &message(&[moved_to(1), moved_to(2)]), 1);
    let _ = channel.take_effects();
    channel.received(CONNECTION, &message(&[key(ON, Action::Press, false)]), 2);
    let lines = inputs(&channel.take_effects());
    let order: Vec<_> = lines.iter().map(kind_and_action).collect();
    assert_eq!(order, [("mouse", "move"), ("key", "press")], "{lines:?}");
}

/// A selection brings the move waiting first: it was made on the page left.
#[test]
fn a_selection_brings_the_move_first() {
    let mut channel = selected();
    channel.received(CONNECTION, &message(&[moved_to(1), moved_to(2)]), 1);
    let _ = channel.take_effects();
    channel.received(CONNECTION, &message(&[select(0x200, 0)]), 2);
    let lines = told(&channel.take_effects());
    let order: Vec<&str> = lines.iter().map(|line| line["type"].as_str().unwrap()).collect();
    assert_eq!(order, ["input", "left", "selected"], "{lines:?}");
    assert_eq!(lines[0]["page"], json!("100"));
}

/// A connection's going brings the move waiting first.
#[test]
fn a_connection_that_closes_sends_its_last_move() {
    let mut channel = selected();
    channel.received(CONNECTION, &message(&[moved_to(1), moved_to(2)]), 1);
    let _ = channel.take_effects();
    channel.closed(CONNECTION, 2);
    assert_eq!(inputs(&channel.take_effects()).len(), 1);
}

/// DOC-1945: a position the program has is not told again, and one waiting
/// is dropped when the pointer comes back to it.
#[test]
fn an_unchanged_position_is_not_told_again() {
    let mut channel = selected();
    channel.received(CONNECTION, &message(&[moved_to(1)]), 1);
    assert_eq!(inputs(&channel.take_effects()).len(), 1);
    channel.received(CONNECTION, &message(&[moved_to(2), moved_to(1)]), 2);
    assert_eq!(inputs(&channel.take_effects()), [] as [Value; 0]);
    assert_eq!(channel.next_wake(), NOTHING_MOVING, "nothing is left waiting");
    channel.received(CONNECTION, &message(&[moved_to(1)]), SECOND_MS);
    assert_eq!(inputs(&channel.take_effects()), [] as [Value; 0]);
}

/// REQ-1360: a move counts no input units and is never acknowledged, whether
/// it goes at once or waits.
#[test]
fn moves_are_never_acknowledged() {
    let mut channel = selected();
    for position in 1..=3 {
        channel.received(CONNECTION, &message(&[moved_to(position)]), u64::from(position));
    }
    let _ = channel.take_effects();
    let mut now = SECOND_MS;
    while let Some(wake_at) = channel.next_wake().filter(|_| channel.next_wake() != NOTHING_MOVING) {
        now = now.max(wake_at);
        channel.deadline(now);
    }
    assert_eq!(sends(&channel.take_effects()), 0, "no cursor record for moves");
}

/// A move stamped with a page no longer selected is dropped and counted,
/// like any other input (REQ-1557), and never waits.
#[test]
fn a_move_on_another_page_is_dropped() {
    let mut channel = selected();
    let at = Point { column: 1, row: 1, pixels: None, mods: 0 };
    let stale = Item::Event(Event::Mouse(Mouse { on: OTHER, action: MouseAction::Move, button: 0, at, device: Device::Mouse }));
    channel.received(CONNECTION, &message(&[stale]), 1);
    assert_eq!(inputs(&channel.take_effects()), [] as [Value; 0]);
    assert_eq!(channel.stats().stale_input, 1);
    assert_eq!(channel.next_wake(), NOTHING_MOVING);
}

/// A connection of the session and not the session: two tabs have a pointer
/// each, and one's moves take nothing of the other's pace.
#[test]
fn each_connection_has_a_pace_of_its_own() {
    const SECOND: u64 = CONNECTION + 1;
    let mut channel = selected();
    channel.opened(SECOND, 1, Vec::new(), 0);
    channel.received(SECOND, &message(&[hello(), select(0x100, 0)]), 0);
    let _ = channel.take_effects();
    channel.received(CONNECTION, &message(&[moved_to(1)]), 1);
    channel.received(SECOND, &message(&[moved_to(2)]), 1);
    let lines = inputs(&channel.take_effects());
    let connections: Vec<_> = lines.iter().map(|line| line["connection"].as_u64().unwrap()).collect();
    assert_eq!(connections, [CONNECTION, SECOND]);
}
