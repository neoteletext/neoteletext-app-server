use super::*;
use neoteletext_input::event::{Mouse, MouseAction, Point, Wheel};

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
    let moved = Mouse { action: MouseAction::Move, button: 1, at };
    assert_eq!(Input::of(&Item::Event(Event::Mouse(moved))), Some(Input::Moved { buttons: 1, column: 39, row: 12 }));
    let pressed = Mouse { action: MouseAction::Press, button: 0, at };
    assert_eq!(Input::of(&Item::Event(Event::Mouse(pressed))), Some(Input::Mouse { pressed: true, button: 0, column: 39, row: 12 }));
}
