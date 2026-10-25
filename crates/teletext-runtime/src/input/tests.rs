use super::*;
use neoteletext_input::event::{Point, Wheel};

/// The wheel reaches an application with its turn and its cell, which it
/// needs to scroll what is under the pointer.
#[test]
fn the_wheel_is_input() {
    let wheel = Wheel { dx: 0, dy: -240, at: Point { column: 10, row: 5, pixels: None, mods: 0 } };
    assert_eq!(Input::of(&Item::Event(Event::Wheel(wheel))), Some(Input::Wheel { dx: 0, dy: -240, column: 10, row: 5 }));
}
