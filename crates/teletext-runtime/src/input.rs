//! Input as a program sees it: the engine's DOC-1931 events
//! (`neoteletext_input::event`), with each functional key named, and as
//! the protocol's `input` event.

use alloc::string::{String, ToString};

use neoteletext_input::event::{Action, Event, Item, KeyCode, MouseAction as EventMouseAction, Stamp};
use neoteletext_input::held::Released;
use neoteletext_input::keys;

use crate::protocol::{self, Buttons, HoverAction, InputEvent, KeyAction, KeyName, MouseAction};

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
    /// Any other functional key, by its DOC-1936 code.
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

/// One thing the viewer did on the page.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Input<'a> {
    /// Text typed: whole characters, no control characters.
    Text(&'a str),
    /// A key pressed, repeating or released: every one is sent (DOC-1935).
    Key {
        /// Press, repeat or release.
        action: Action,
        /// Which key.
        key: Named,
        /// The text the press produced, if any.
        text: Option<&'a str>,
        /// The United States layout's character of the same key, when it
        /// differs: the letter of a key whatever the layout.
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
    /// The pointer moved over the page: every move that changes the dot
    /// under it, a button held or not (DOC-1945).
    Moved {
        /// The buttons held, a bit each: 1 left, 2 middle, 4 right.
        buttons: u8,
        /// The cell's column.
        column: u32,
        /// The cell's row.
        row: u32,
    },
    /// The wheel turned over a cell (DOC-1938): in 1/120ths of
    /// a line, positive right and down.
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
    /// The pointer came onto a control of the page or went off it (DOC-1940), while keys go to the channel.
    Hover {
        /// It came onto the control, rather than went off it.
        entered: bool,
        /// The control's first column.
        column: u32,
        /// The control's top row.
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
    /// own events (HELLO, SELECT, RESIZE) are the runtime's.
    #[must_use]
    pub fn of(item: &Item<'a>) -> Option<Self> {
        match *item {
            Item::Text(text) => Some(Self::Text(text)),
            Item::Event(Event::Key(key)) => Some(Self::Key { action: key.action, key: Named::of(key.key), text: key.text, base: key.base }),
            // Gone is not a release (DOC-1938, MOUSE action 3): the protocol
            // for programs has no event for it, so they are not told.
            Item::Event(Event::Mouse(mouse)) if mouse.action == EventMouseAction::Gone => None,
            Item::Event(Event::Mouse(mouse)) if mouse.action == EventMouseAction::Move => {
                Some(Self::Moved { buttons: mouse.button, column: mouse.at.column, row: mouse.at.row })
            }
            Item::Event(Event::Mouse(mouse)) => {
                Some(Self::Mouse { pressed: mouse.action == EventMouseAction::Press, button: mouse.button, column: mouse.at.column, row: mouse.at.row })
            }
            Item::Event(Event::Wheel(wheel)) => Some(Self::Wheel { dx: wheel.dx, dy: wheel.dy, column: wheel.at.column, row: wheel.at.row }),
            Item::Event(Event::Hover { leave, column, row, .. }) => Some(Self::Hover { entered: !leave, column, row }),
            Item::Event(Event::Paste { text, .. }) => Some(Self::Paste(text)),
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

    /// The character this input typed, if it typed one character: raw
    /// text, or the text of a key's press or repeat (DOC-1935).
    /// A key that produced no text (Control and a letter) or a control
    /// character (Enter) types nothing.
    #[must_use]
    pub fn character(&self) -> Option<char> {
        let (Self::Text(text) | Self::Key { action: Action::Press | Action::Repeat, text: Some(text), .. }) = *self else { return None };
        let mut characters = text.chars();
        characters.next().filter(|character| !character.is_control() && characters.next().is_none())
    }
}

/// The page and sub-code an item says were on screen, if it carries a
/// stamp (REQ-1555): focus and the session's own events have none, and
/// raw text cannot.
#[must_use]
pub(crate) const fn stamp(item: &Item<'_>) -> Option<Stamp> {
    let Item::Event(event) = item else { return None };
    match event {
        Event::Key(key) => Some(key.on),
        Event::Mouse(mouse) => Some(mouse.on),
        Event::Wheel(wheel) => Some(wheel.on),
        Event::Touch(touch) => Some(touch.on),
        Event::Paste { on, .. } | Event::Compose { on, .. } | Event::Hover { on, .. } => Some(*on),
        _ => None,
    }
}

/// Whether an item lets go of something: never dropped for its stamp, so
/// nothing stays held (REQ-1555 item 4).
#[must_use]
pub(crate) const fn releases(item: &Item<'_>) -> bool {
    match item {
        Item::Event(Event::Key(key)) => matches!(key.action, Action::Release),
        Item::Event(Event::Mouse(mouse)) => matches!(mouse.action, EventMouseAction::Release | EventMouseAction::Gone),
        Item::Event(Event::Touch(touch)) => matches!(touch.action, EventMouseAction::Release | EventMouseAction::Gone),
        _ => false,
    }
}

/// Whether an item is a release the receiver marked as lost (REQ-207).
#[must_use]
pub(crate) const fn marked_lost(item: &Item<'_>) -> bool {
    matches!(item, Item::Event(Event::Key(key)) if key.lost)
}

/// A release the page lost, as the protocol sends it, or none for a TOUCH
/// contact, which programs are not told of (TODO-1525). `at` is where the
/// pointer last was: a lost button has no place of its own.
#[must_use]
pub(crate) fn lost_release(released: Released, at: (u8, u8)) -> Option<InputEvent> {
    match released {
        Released::Key(key) => Some(InputEvent::Key { action: KeyAction::Release, key: KeyName(key_name(Named::of(key))), text: None, base: None }),
        Released::Button(button) => {
            let button = (button <= LAST_BUTTON).then_some(button)?;
            Some(InputEvent::Mouse { action: MouseAction::Release, button: Some(button), buttons: None, column: at.0, row: at.1 })
        }
        Released::Contact(_) => None,
    }
}

/// A pointer event's cell within the page, if the item has one.
#[must_use]
pub(crate) fn pointer_cell(item: &Item<'_>) -> Option<(u8, u8)> {
    match item {
        Item::Event(Event::Mouse(mouse)) => cell(mouse.at.column, mouse.at.row),
        _ => None,
    }
}

/// A key's name as the protocol writes it: a W3C `KeyboardEvent.key`
/// name (DOC-1936), or the character of a character key.
fn key_name(key: Named) -> String {
    let name = match key {
        Named::Up => "ArrowUp",
        Named::Down => "ArrowDown",
        Named::Left => "ArrowLeft",
        Named::Right => "ArrowRight",
        Named::Enter => "Enter",
        Named::Escape => "Escape",
        Named::Backspace => "Backspace",
        Named::Delete => "Delete",
        Named::Tab => "Tab",
        Named::Home => "Home",
        Named::End => "End",
        Named::PageUp => "PageUp",
        Named::PageDown => "PageDown",
        Named::Character(character) => return character.to_string(),
        Named::Other(code) => keys::name(code).unwrap_or("Unidentified"),
    };
    name.to_string()
}

/// A cell's column and row within the page's 40 by 25, or none: the
/// protocol's bounds (REQ-1476).
fn cell(column: u32, row: u32) -> Option<(u8, u8)> {
    let column = u8::try_from(column).ok().filter(|&column| usize::from(column) < protocol::COLUMNS)?;
    let row = u8::try_from(row).ok().filter(|&row| usize::from(row) < protocol::ROWS)?;
    Some((column, row))
}

/// The buttons held, a bit each: 1 left, 2 middle, 4 right (DOC-1929).
const LEFT: u8 = 1;
const MIDDLE: u8 = 2;
const RIGHT: u8 = 4;
/// The last button a press or release names: 2, the right (DOC-1929:
/// 0 left, 1 middle, 2 right).
const LAST_BUTTON: u8 = 2;

impl Input<'_> {
    /// The event as the protocol sends it (REQ-1476), or none for one
    /// outside its bounds: a cell beyond the page's 40 by 25.
    #[must_use]
    pub fn event(&self) -> Option<InputEvent> {
        Some(match *self {
            Self::Text(text) => InputEvent::Text { text: text.to_string() },
            Self::Paste(text) => InputEvent::Paste { text: text.to_string() },
            Self::Key { action, key, text, base } => InputEvent::Key {
                action: match action {
                    Action::Press => KeyAction::Press,
                    Action::Repeat => KeyAction::Repeat,
                    Action::Release => KeyAction::Release,
                },
                key: KeyName(key_name(key)),
                text: text.map(ToString::to_string),
                base: base.map(|base| base.to_string()),
            },
            Self::Mouse { pressed, button, column, row } => {
                let (column, row) = cell(column, row)?;
                let button = (button <= LAST_BUTTON).then_some(button)?;
                InputEvent::Mouse { action: if pressed { MouseAction::Press } else { MouseAction::Release }, button: Some(button), buttons: None, column, row }
            }
            Self::Moved { buttons, column, row } => {
                let (column, row) = cell(column, row)?;
                let held = Buttons { left: buttons & LEFT != 0, middle: buttons & MIDDLE != 0, right: buttons & RIGHT != 0 };
                InputEvent::Mouse { action: MouseAction::Move, button: None, buttons: Some(held), column, row }
            }
            Self::Wheel { dx, dy, column, row } => {
                let (column, row) = cell(column, row)?;
                let bounded = |delta: i32| i16::try_from(delta).unwrap_or(if delta < 0 { i16::MIN } else { i16::MAX });
                InputEvent::Wheel { dx: bounded(dx), dy: bounded(dy), column, row }
            }
            Self::Hover { entered, column, row } => {
                let (column, row) = cell(column, row)?;
                InputEvent::Hover { action: if entered { HoverAction::Enter } else { HoverAction::Leave }, column, row }
            }
            Self::FocusIn => InputEvent::Focus,
            Self::FocusOut => InputEvent::Blur,
        })
    }
}
