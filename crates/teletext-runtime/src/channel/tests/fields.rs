//! Every field a receiver sends reaches the program (REQ-1347, DOC-1969):
//! the modifiers of a key, the mouse and the wheel, the mouse's and the
//! wheel's dot, and an input method's composition.

use neoteletext_input::event::{ALT, CONTROL, SHIFT, SUPER, Wheel};

use super::*;

/// The modifiers a test holds: more than one, so that no bit is lost.
const HELD: u8 = CONTROL | SHIFT;
/// A dot of the page image, past what eight bits hold.
const DOT: (u32, u32) = (480, 250);

fn event_of(channel: &mut Channel, item: Item<'_>, now: u64) -> Value {
    channel.received(CONNECTION, &message(&[item]), now);
    let lines = inputs(&channel.take_effects());
    assert_eq!(lines.len(), 1, "{lines:?}");
    lines[0]["event"].clone()
}

/// Control-C reaches a program as Control and `c`, not as `c`, with each
/// modifier its own bit (DOC-1935).
#[test]
fn a_key_has_its_modifiers() {
    for mods in [SHIFT, ALT, CONTROL, SUPER, HELD, u8::MAX] {
        let mut channel = selected();
        let pressed = Key { on: ON, action: Action::Press, key: KeyCode::Character('c'), mods, text: None, base: None, lost: false };
        let event = event_of(&mut channel, Item::Event(Event::Key(pressed)), 1);
        assert_eq!((&event["key"], &event["mods"]), (&json!("c"), &json!(mods)));
    }
}

#[test]
fn a_key_with_no_modifier_says_so() {
    let event = event_of(&mut selected(), key(ON, Action::Press, false), 1);
    assert_eq!(event["mods"], json!(0));
}

fn pointer(action: MouseAction, button: u8, pixels: Option<(u32, u32)>, mods: u8) -> Item<'static> {
    let at = Point { column: 7, row: 9, pixels, mods };
    Item::Event(Event::Mouse(Mouse { on: ON, action, button, at, device: Device::Mouse }))
}

/// A press has its dot and its modifiers, as has a release.
#[test]
fn a_press_and_a_release_have_their_dot_and_modifiers() {
    let mut channel = selected();
    for (action, now, expected) in [(MouseAction::Press, 1, "press"), (MouseAction::Release, 2, "release")] {
        let event = event_of(&mut channel, pointer(action, 0, Some(DOT), HELD), now);
        assert_eq!(
            (&event["action"], &event["column"], &event["row"], &event["px"], &event["py"], &event["mods"]),
            (&json!(expected), &json!(7), &json!(9), &json!(DOT.0), &json!(DOT.1), &json!(HELD))
        );
    }
}

/// A move has its dot and its modifiers.
#[test]
fn a_move_has_its_dot_and_modifiers() {
    let event = event_of(&mut selected(), pointer(MouseAction::Move, 1, Some(DOT), HELD), 1);
    assert_eq!((&event["px"], &event["py"], &event["mods"], &event["buttons"]["left"]), (&json!(DOT.0), &json!(DOT.1), &json!(HELD), &json!(true)));
}

/// A receiver that does not know the dot says nothing of it, and neither does
/// the server.
#[test]
fn a_dot_not_known_is_null() {
    let event = event_of(&mut selected(), pointer(MouseAction::Press, 0, None, 0), 1);
    assert_eq!((&event["px"], &event["py"], &event["mods"]), (&Value::Null, &Value::Null, &json!(0)));
}

/// The wheel has what the pointer has.
#[test]
fn the_wheel_has_its_dot_and_modifiers() {
    let at = Point { column: 7, row: 9, pixels: Some(DOT), mods: HELD };
    let event = event_of(&mut selected(), Item::Event(Event::Wheel(Wheel { on: ON, dx: 0, dy: 120, at })), 1);
    assert_eq!((&event["dy"], &event["px"], &event["py"], &event["mods"]), (&json!(120), &json!(DOT.0), &json!(DOT.1), &json!(HELD)));
}

/// An input method's preedit reaches a program as it changes, with where the
/// caret is in it, and an empty one ends it (DOC-1969).
#[test]
fn compose_reaches_a_program() {
    let mut channel = selected();
    let event = event_of(&mut channel, Item::Event(Event::Compose { on: ON, cursor: 2, text: "ni\u{7684}" }), 1);
    assert_eq!((&event["kind"], &event["cursor"], &event["text"]), (&json!("compose"), &json!(2), &json!("ni\u{7684}")));
    let event = event_of(&mut channel, Item::Event(Event::Compose { on: ON, cursor: 0, text: "" }), 2);
    assert_eq!((&event["kind"], &event["cursor"], &event["text"]), (&json!("compose"), &json!(0), &json!("")));
}

/// COMPOSE is stamped, and is input like any other: one aimed at a page no
/// longer selected is dropped (REQ-1557), and it counts nothing (DOC-1943).
#[test]
fn compose_is_held_to_its_stamp_and_counts_nothing() {
    let mut channel = selected();
    channel.received(CONNECTION, &message(&[Item::Event(Event::Compose { on: OTHER, cursor: 0, text: "a" })]), 1);
    assert_eq!(inputs(&channel.take_effects()), [] as [Value; 0]);
    assert_eq!(channel.stats().stale_input, 1);
    channel.received(CONNECTION, &message(&[Item::Event(Event::Compose { on: ON, cursor: 0, text: "a" })]), 2);
    assert_eq!(inputs(&channel.take_effects()).len(), 1);
    channel.deadline(2 + ACKNOWLEDGEMENT_WAIT_MS);
    assert_eq!(sends(&channel.take_effects()), 0, "no acknowledgement");
}

/// A lost release has no modifiers and no dot to give: nothing was pressed
/// to make it.
#[test]
fn a_lost_release_has_no_modifiers_or_dot() {
    let mut channel = selected();
    channel.received(CONNECTION, &message(&[pointer(MouseAction::Press, 0, Some(DOT), HELD)]), 1);
    let _ = channel.take_effects();
    let event = event_of(&mut channel, pointer(MouseAction::Gone, 1, Some(DOT), HELD), 2);
    assert_eq!((&event["mods"], &event["px"], &event["py"]), (&json!(0), &Value::Null, &Value::Null));
}
