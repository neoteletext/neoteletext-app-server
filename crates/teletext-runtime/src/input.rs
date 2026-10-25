//! Input as an application sees it: the engine's INPUT.md events
//! (`neoteletext_input::event`), with each functional key named.

use neoteletext_input::event::{Action, Event, Item, KeyCode, MouseAction};
use neoteletext_input::keys;

#[cfg(test)]
mod tests;

/// A key by name.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[allow(missing_docs, reason = "the keys name themselves")]
pub enum Named {
    Up,
    Down,
    Left,
    Right,
    Enter,
    Escape,
    Backspace,
    Delete,
    Tab,
    Home,
    End,
    PageUp,
    PageDown,
    /// A character key: the layout's character without Shift.
    Character(char),
    /// Any other functional key, by its INPUT.md code.
    Other(u32),
}

impl Named {
    fn of(code: KeyCode) -> Self {
        match code {
            KeyCode::Character(character) => Self::Character(character),
            KeyCode::Functional(code) => match keys::name(code) {
                Some("ArrowUp") => Self::Up,
                Some("ArrowDown") => Self::Down,
                Some("ArrowLeft") => Self::Left,
                Some("ArrowRight") => Self::Right,
                Some("Enter") => Self::Enter,
                Some("Escape") => Self::Escape,
                Some("Backspace") => Self::Backspace,
                Some("Delete") => Self::Delete,
                Some("Tab") => Self::Tab,
                Some("Home") => Self::Home,
                Some("End") => Self::End,
                Some("PageUp") => Self::PageUp,
                Some("PageDown") => Self::PageDown,
                _ => Self::Other(code),
            },
        }
    }
}

/// One thing the viewer did on the page with the keys.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Input<'a> {
    /// Text typed: whole characters, no control characters.
    Text(&'a str),
    /// A key pressed, repeating or released (releases in `KEY_RELEASE`).
    Key {
        /// Press, repeat or release.
        action: Action,
        /// Which key.
        key: Named,
        /// The text the press produced, in `ALL_KEYS`.
        text: Option<&'a str>,
        /// The United States layout's character of the same key, in
        /// `BASE_KEY`: the letter of a key whatever the layout.
        base: Option<char>,
    },
    /// A mouse button pressed or released on a cell.
    Mouse {
        /// Pressed.
        pressed: bool,
        /// The button: 0 left, 1 middle, 2 right.
        button: u8,
        /// The cell's column.
        column: u32,
        /// The cell's row.
        row: u32,
    },
    /// The pointer moved over a cell: in `MOUSE_DRAG` while a button is
    /// held, in `MOUSE_ALL` always (INPUT.md section 6.4).
    Moved {
        /// The buttons held, a bit each: 1 left, 2 middle, 4 right.
        buttons: u8,
        /// The cell's column.
        column: u32,
        /// The cell's row.
        row: u32,
    },
    /// The wheel turned over a cell, in `MOUSE_BUTTONS` (INPUT.md section
    /// 5.4): in 1/120ths of a line, positive right and down.
    Wheel {
        /// Across.
        dx: i32,
        /// Down.
        dy: i32,
        /// The cell's column.
        column: u32,
        /// The cell's row.
        row: u32,
    },
    /// A paste: data, never keystrokes.
    Paste(&'a str),
    /// The receiver has the focus.
    FocusIn,
    /// The receiver lost the focus: release what is held.
    FocusOut,
}

impl<'a> Input<'a> {
    /// The input an item is, if an application acts on it; the session's
    /// own events (HELLO, PAGE, RESIZE) are the runtime's.
    #[must_use]
    pub fn of(item: &Item<'a>) -> Option<Self> {
        match *item {
            Item::Text(text) => Some(Self::Text(text)),
            Item::Event(Event::Key(key)) => Some(Self::Key { action: key.action, key: Named::of(key.key), text: key.text, base: key.base }),
            Item::Event(Event::Mouse(mouse)) if mouse.action == MouseAction::Move => {
                Some(Self::Moved { buttons: mouse.button, column: mouse.at.column, row: mouse.at.row })
            }
            Item::Event(Event::Mouse(mouse)) => {
                Some(Self::Mouse { pressed: mouse.action == MouseAction::Press, button: mouse.button, column: mouse.at.column, row: mouse.at.row })
            }
            Item::Event(Event::Wheel(wheel)) => Some(Self::Wheel { dx: wheel.dx, dy: wheel.dy, column: wheel.at.column, row: wheel.at.row }),
            Item::Event(Event::Paste(text)) => Some(Self::Paste(text)),
            Item::Event(Event::FocusIn) => Some(Self::FocusIn),
            Item::Event(Event::FocusOut) => Some(Self::FocusOut),
            Item::Event(_) => None,
        }
    }

    /// Whether this is a press (or a repeat) of `key`.
    #[must_use]
    pub fn pressed(&self, key: Named) -> bool {
        matches!(*self, Self::Key { action: Action::Press | Action::Repeat, key: pressed, .. } if pressed == key)
    }

    /// Whether this is a release of `key`.
    #[must_use]
    pub fn released(&self, key: Named) -> bool {
        matches!(*self, Self::Key { action: Action::Release, key: released, .. } if released == key)
    }

    /// The character this input typed, if it typed one character: text,
    /// or a character key's press.
    #[must_use]
    pub fn character(&self) -> Option<char> {
        match *self {
            Self::Text(text) => {
                let mut characters = text.chars();
                characters.next().filter(|_| characters.next().is_none())
            }
            Self::Key { action: Action::Press | Action::Repeat, key: Named::Character(character), text, .. } => {
                text.and_then(|text| text.chars().next()).or(Some(character))
            }
            _ => None,
        }
    }
}
