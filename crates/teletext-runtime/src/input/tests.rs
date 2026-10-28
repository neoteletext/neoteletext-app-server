use super::*;
use neoteletext_input::event::{Device, Mouse, MouseAction, Point, Wheel};

/// The wheel reaches an application with its turn and its cell, which it
/// needs to scroll what is under the pointer.
#[test]
fn the_wheel_is_input() {
    let wheel = Wheel { dx: 0, dy: -240, at: Point { column: 10, row: 5, pixels: None, mods: 0 } };
    assert_eq!(Input::of(&Item::Event(Event::Wheel(wheel))), Some(Input::Wheel { dx: 0, dy: -240, column: 10, row: 5 }));
}

/// A drag reaches an application as moves with the buttons held, which it
/// needs to drag a scroll bar.
#[test]
fn a_drag_is_input() {
    let at = Point { column: 39, row: 12, pixels: None, mods: 0 };
    let moved = Mouse { action: MouseAction::Move, button: 1, at, device: Device::Mouse };
    assert_eq!(Input::of(&Item::Event(Event::Mouse(moved))), Some(Input::Moved { buttons: 1, column: 39, row: 12 }));
    let pressed = Mouse { action: MouseAction::Press, button: 0, at, device: Device::Mouse };
    assert_eq!(Input::of(&Item::Event(Event::Mouse(pressed))), Some(Input::Mouse { pressed: true, button: 0, column: 39, row: 12 }));
}

/// A pointer gone is not a release, which would click what it was over:
/// an application is not told of it.
#[test]
fn a_pointer_gone_is_not_a_release() {
    let at = Point { column: 39, row: 12, pixels: None, mods: 0 };
    let gone = Mouse { action: MouseAction::Gone, button: 1, at, device: Device::Mouse };
    assert_eq!(Input::of(&Item::Event(Event::Mouse(gone))), None);
}

/// A hover reaches an application with the control's first cell, entered
/// or left, which it needs to say what the control under the pointer does.
#[test]
fn a_hover_is_input() {
    assert_eq!(Input::of(&Item::Event(Event::Hover { leave: false, column: 2, row: 20 })), Some(Input::Hover { entered: true, column: 2, row: 20 }));
    assert_eq!(Input::of(&Item::Event(Event::Hover { leave: true, column: 2, row: 20 })), Some(Input::Hover { entered: false, column: 2, row: 20 }));
}

/// Typing comes from a key's press or repeat with its text, and from raw
/// text (INPUT.md section 6.3); a release, a key that produced no text and
/// a control character type nothing. Before, a character key's press
/// without text typed its key, so Control and a letter typed the letter.
#[test]
fn typing_is_a_keys_text_or_raw_text() {
    let key = |action, text| Input::Key { action, key: Named::Character('a'), text, base: None };
    assert_eq!(key(Action::Press, Some("a")).character(), Some('a'));
    assert_eq!(key(Action::Repeat, Some("a")).character(), Some('a'));
    assert_eq!(key(Action::Release, Some("a")).character(), None);
    assert_eq!(key(Action::Press, None).character(), None);
    assert_eq!(key(Action::Press, Some("\r")).character(), None);
    assert_eq!(Input::Text("\u{e9}").character(), Some('\u{e9}'));
    assert_eq!(Input::Text("ab").character(), None);
}
