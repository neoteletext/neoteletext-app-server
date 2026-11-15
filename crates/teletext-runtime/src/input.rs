//! Input as a program sees it: the engine's DOC-1931 events
//! (`neoteletext_input::event`), with each functional key named, and as
//! the protocol's `input` event.

use alloc::string::{String, ToString};

use neoteletext_core::consts::{CELL_HEIGHT, CELL_WIDTH};

use neoteletext_input::event::{Action, Event, Item, KeyCode, MouseAction as EventMouseAction, Point, Stamp};
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

/// PICK's type among the input extension events (the engine's REQ-6337).
const PICK: u8 = 0x46;
/// The farthest a pick's place lies from its drawing's origin, either way.
const MAX_PICK_OFFSET: i32 = 1 << 20;
/// A pick's place in the picture is below this on each axis.
const MAX_PICK_POSITION: u32 = 1 << 16;

/// `DRAWING_HOVER`'s type among the input extension events (the engine's
/// REQ-6369).
const DRAWING_HOVER: u8 = 0x47;
/// `DRAWING_HOVER`'s flag: the pointer left the drawing.
const DRAWING_HOVER_LEAVE: u8 = 1;

/// The pointer came onto a detectable numbered drawing of a page drawn in
/// ReGIS, or went off it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct DrawingHover {
    /// The page on screen.
    pub on: Stamp,
    /// The numbered drawing, 1 to 65,535.
    pub drawing: u16,
    /// It went off the drawing; false when it came onto it.
    pub leave: bool,
    /// The commits applied to the picture on screen.
    pub commits: u32,
}

impl DrawingHover {
    /// Reads `DRAWING_HOVER`'s payload: the stamp, the flags, the drawing
    /// and the commit count's four bytes. `None` for anything the engine's
    /// reader refuses; it is read here for the reason [`Pick::read`] gives.
    fn read(payload: &[u8]) -> Option<Self> {
        let mut reader = neoteletext_input::wire::Reader::new(payload);
        let on = Stamp::new(u16::try_from(reader.uv().ok()?).ok()?, u16::try_from(reader.uv().ok()?).ok()?)?;
        let flags = reader.byte().ok()?;
        let drawing = u16::try_from(reader.uv().ok()?).ok()?;
        let commits: [u8; 4] = reader.take(4).ok()?.try_into().ok()?;
        let good = flags & !DRAWING_HOVER_LEAVE == 0 && drawing != 0 && reader.used() == payload.len();
        good.then_some(Self { on, drawing, leave: flags & DRAWING_HOVER_LEAVE != 0, commits: u32::from_be_bytes(commits) })
    }
}

/// What lay under a pointer on a page drawn in ReGIS.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Pick {
    /// The page on screen.
    pub on: Stamp,
    /// The numbered drawing picked, or 0 for none.
    pub drawing: u16,
    /// The place within that drawing.
    pub x: i32,
    /// See `x`.
    pub y: i32,
    /// The place in the picture.
    pub picture_x: u32,
    /// See `picture_x`.
    pub picture_y: u32,
    /// The commits applied to the picture on screen.
    pub commits: u32,
}

impl Pick {
    /// Reads PICK's payload: the stamp, the drawing, the place within it,
    /// the place in the picture and the commit count's four bytes. `None`
    /// for anything the engine's reader refuses; the engine this server is
    /// pinned to predates PICK and gives it as an extension it does not
    /// know, so it is read here.
    fn read(payload: &[u8]) -> Option<Self> {
        let mut reader = neoteletext_input::wire::Reader::new(payload);
        let on = Stamp::new(u16::try_from(reader.uv().ok()?).ok()?, u16::try_from(reader.uv().ok()?).ok()?)?;
        let drawing = u16::try_from(reader.uv().ok()?).ok()?;
        let (x, y) = (reader.sv().ok()?, reader.sv().ok()?);
        let (picture_x, picture_y) = (reader.uv().ok()?, reader.uv().ok()?);
        let commits: [u8; 4] = reader.take(4).ok()?.try_into().ok()?;
        let within = x.checked_abs().is_some_and(|x| x <= MAX_PICK_OFFSET)
            && y.checked_abs().is_some_and(|y| y <= MAX_PICK_OFFSET)
            && picture_x < MAX_PICK_POSITION
            && picture_y < MAX_PICK_POSITION;
        let placed = drawing != 0 || (x, y) == (0, 0);
        (within && placed && reader.used() == payload.len()).then_some(Self { on, drawing, x, y, picture_x, picture_y, commits: u32::from_be_bytes(commits) })
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
        /// The modifier keys held, a bit each.
        mods: u8,
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
        /// Where, and the modifier keys held.
        at: Point,
    },
    /// The pointer left the page, or the receiver lost it (MOUSE action 3,
    /// DOC-1938), so that a channel can stop showing what follows it
    /// (REQ-1355, REQ-1367).
    Gone {
        /// Where it last was, and the modifier keys held.
        at: Point,
    },
    /// A contact after the first: down, up, moved or gone (TOUCH, REQ-1366).
    Touch {
        /// What it did, as MOUSE's actions.
        action: EventMouseAction,
        /// Its number, 1 to the engine's `MAX_TOUCH_ID`.
        contact: u8,
        /// Where.
        at: Point,
    },
    /// The pointer moved over the page: every move that changes the dot
    /// under it, a button held or not (DOC-1945).
    Moved {
        /// The buttons held, a bit each: 1 left, 2 middle, 4 right.
        buttons: u8,
        /// Where, and the modifier keys held.
        at: Point,
    },
    /// The wheel turned over a cell (DOC-1938): in 1/120ths of
    /// a line, positive right and down.
    Wheel {
        /// Across.
        dx: i32,
        /// Down.
        dy: i32,
        /// Where, and the modifier keys held.
        at: Point,
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
    /// On a page drawn in ReGIS, what lay under the pointer (PICK, the
    /// engine's REQ-6337).
    Pick(Pick),
    /// On a page drawn in ReGIS, the pointer came onto a detectable
    /// numbered drawing or went off it (`DRAWING_HOVER`, the engine's
    /// REQ-6369).
    DrawingHover(DrawingHover),
    /// An input method's text as it stands: empty when composing ended.
    Compose {
        /// Where the caret is, in code points from the text's start.
        cursor: u32,
        /// The text.
        text: &'a str,
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
            Item::Event(Event::Key(key)) => Some(Self::Key { action: key.action, key: Named::of(key.key), mods: key.mods, text: key.text, base: key.base }),
            // Gone is not a release (DOC-1938, MOUSE action 3): a program is
            // told it as gone (protocol 11).
            Item::Event(Event::Mouse(mouse)) if mouse.action == EventMouseAction::Gone => Some(Self::Gone { at: mouse.at }),
            Item::Event(Event::Touch(touch)) => {
                Some(Self::Touch { action: touch.action, contact: touch.id, at: Point { column: touch.column, row: touch.row, pixels: touch.pixels, mods: 0 } })
            }
            Item::Event(Event::Mouse(mouse)) if mouse.action == EventMouseAction::Move => Some(Self::Moved { buttons: mouse.button, at: mouse.at }),
            Item::Event(Event::Mouse(mouse)) => Some(Self::Mouse { pressed: mouse.action == EventMouseAction::Press, button: mouse.button, at: mouse.at }),
            Item::Event(Event::Wheel(wheel)) => Some(Self::Wheel { dx: wheel.dx, dy: wheel.dy, at: wheel.at }),
            Item::Event(Event::Hover { leave, column, row, .. }) => Some(Self::Hover { entered: !leave, column, row }),
            Item::Event(Event::Paste { text, .. }) => Some(Self::Paste(text)),
            Item::Event(Event::Compose { cursor, text, .. }) => Some(Self::Compose { cursor, text }),
            Item::Event(Event::FocusIn) => Some(Self::FocusIn),
            Item::Event(Event::FocusOut) => Some(Self::FocusOut),
            Item::Event(Event::Extension { kind: PICK, payload }) => Pick::read(payload).map(Self::Pick),
            Item::Event(Event::Extension { kind: DRAWING_HOVER, payload }) => DrawingHover::read(payload).map(Self::DrawingHover),
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
pub(crate) fn stamp(item: &Item<'_>) -> Option<Stamp> {
    let Item::Event(event) = item else { return None };
    match event {
        Event::Key(key) => Some(key.on),
        Event::Mouse(mouse) => Some(mouse.on),
        Event::Wheel(wheel) => Some(wheel.on),
        Event::Touch(touch) => Some(touch.on),
        Event::Paste { on, .. } | Event::Compose { on, .. } | Event::Hover { on, .. } => Some(*on),
        Event::Extension { kind: PICK, payload } => Pick::read(payload).map(|pick| pick.on),
        Event::Extension { kind: DRAWING_HOVER, payload } => DrawingHover::read(payload).map(|hover| hover.on),
        _ => None,
    }
}

/// Whether an item is a pointer's move, which goes through its connection's
/// pace (REQ-1360), a contact's in its own slot (DEC-2137).
#[must_use]
pub(crate) fn is_move(item: &Item<'_>) -> bool {
    matches!(item, Item::Event(Event::Mouse(mouse)) if mouse.action == EventMouseAction::Move)
        || matches!(item, Item::Event(Event::Touch(touch)) if touch.action == EventMouseAction::Move)
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

/// A release the page lost, as the protocol sends it: a contact's as a
/// TOUCH release. `at` is where the pointer last was: a lost button or
/// contact has no place of its own.
#[must_use]
pub(crate) fn lost_release(released: Released, at: (u8, u8)) -> Option<InputEvent> {
    match released {
        Released::Key(key) => {
            Some(InputEvent::Key { action: KeyAction::Release, key: KeyName(key_name(Named::of(key))), mods: NO_MODIFIERS, text: None, base: None })
        }
        Released::Button(button) => {
            let button = (button <= LAST_BUTTON).then_some(button)?;
            Some(InputEvent::Mouse {
                action: MouseAction::Release,
                button: Some(button),
                buttons: None,
                mods: NO_MODIFIERS,
                column: at.0,
                row: at.1,
                px: None,
                py: None,
            })
        }
        Released::Contact(contact) => Some(InputEvent::Touch { action: MouseAction::Release, contact, column: at.0, row: at.1, px: None, py: None }),
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

/// The dot of the page image under the pointer, across and down, when the
/// receiver knows it, or none when the dot lies outside the pointer's cell
/// (DOC-1946: the dot divided by the cell's size in dots is the cell). A
/// receiver never sends such a dot, so the event is dropped, as one whose
/// cell is beyond the page is.
fn dot(at: Point) -> Option<(Option<u32>, Option<u32>)> {
    let Some((across, down)) = at.pixels else { return Some((None, None)) };
    let within = |dot: u32, cell: u32, dots_per_cell: usize| matches!((usize::try_from(dot), usize::try_from(cell)), (Ok(dot), Ok(cell)) if dot.checked_div(dots_per_cell) == Some(cell));
    (within(across, at.column, CELL_WIDTH) && within(down, at.row, CELL_HEIGHT)).then_some((Some(across), Some(down)))
}

/// A MOUSE or TOUCH action as the protocol names it.
const fn mouse_action(action: EventMouseAction) -> MouseAction {
    match action {
        EventMouseAction::Press => MouseAction::Press,
        EventMouseAction::Release => MouseAction::Release,
        EventMouseAction::Move => MouseAction::Move,
        EventMouseAction::Gone => MouseAction::Gone,
    }
}

/// A cell's column and row within the page's 40 by 25, or none: the
/// protocol's bounds (REQ-1476).
fn cell(column: u32, row: u32) -> Option<(u8, u8)> {
    let column = u8::try_from(column).ok().filter(|&column| usize::from(column) < protocol::COLUMNS)?;
    let row = u8::try_from(row).ok().filter(|&row| usize::from(row) < protocol::ROWS)?;
    Some((column, row))
}

/// What a lost release says of the modifiers: nothing was pressed to make it.
const NO_MODIFIERS: u8 = 0;

/// The buttons held, a bit each: 1 left, 2 middle, 4 right (DOC-1929).
const LEFT: u8 = 1;
const MIDDLE: u8 = 2;
const RIGHT: u8 = 4;
/// The last button a press or release names: 2, the right (DOC-1929:
/// 0 left, 1 middle, 2 right).
const LAST_BUTTON: u8 = 2;

impl Input<'_> {
    /// The event as the protocol sends it (REQ-1476), or none for one
    /// outside its bounds: a cell beyond the page's 40 by 25, a button
    /// above the right, or a dot outside its cell.
    #[must_use]
    pub fn event(&self) -> Option<InputEvent> {
        Some(match *self {
            Self::Text(text) => InputEvent::Text { text: text.to_string() },
            Self::Paste(text) => InputEvent::Paste { text: text.to_string() },
            Self::Compose { cursor, text } => InputEvent::Compose { cursor, text: text.to_string() },
            Self::Key { action, key, mods, text, base } => InputEvent::Key {
                action: match action {
                    Action::Press => KeyAction::Press,
                    Action::Repeat => KeyAction::Repeat,
                    Action::Release => KeyAction::Release,
                },
                key: KeyName(key_name(key)),
                mods,
                text: text.map(ToString::to_string),
                base: base.map(|base| base.to_string()),
            },
            Self::Mouse { pressed, button, at } => {
                let (column, row) = cell(at.column, at.row)?;
                let button = (button <= LAST_BUTTON).then_some(button)?;
                let (px, py) = dot(at)?;
                let action = if pressed { MouseAction::Press } else { MouseAction::Release };
                InputEvent::Mouse { action, button: Some(button), buttons: None, mods: at.mods, column, row, px, py }
            }
            Self::Moved { buttons, at } => {
                let (column, row) = cell(at.column, at.row)?;
                let held = Buttons { left: buttons & LEFT != 0, middle: buttons & MIDDLE != 0, right: buttons & RIGHT != 0 };
                let (px, py) = dot(at)?;
                InputEvent::Mouse { action: MouseAction::Move, button: None, buttons: Some(held), mods: at.mods, column, row, px, py }
            }
            Self::Gone { at } => {
                let (column, row) = cell(at.column, at.row)?;
                let (px, py) = dot(at)?;
                InputEvent::Mouse { action: MouseAction::Gone, button: None, buttons: None, mods: at.mods, column, row, px, py }
            }
            Self::Touch { action, contact, at } => {
                let (column, row) = cell(at.column, at.row)?;
                let (px, py) = dot(at)?;
                InputEvent::Touch { action: mouse_action(action), contact, column, row, px, py }
            }
            Self::Wheel { dx, dy, at } => {
                let (column, row) = cell(at.column, at.row)?;
                let bounded = |delta: i32| i16::try_from(delta).unwrap_or(if delta < 0 { i16::MIN } else { i16::MAX });
                let (px, py) = dot(at)?;
                InputEvent::Wheel { dx: bounded(dx), dy: bounded(dy), mods: at.mods, column, row, px, py }
            }
            Self::Hover { entered, column, row } => {
                let (column, row) = cell(column, row)?;
                InputEvent::Hover { action: if entered { HoverAction::Enter } else { HoverAction::Leave }, column, row }
            }
            Self::Pick(pick) => {
                InputEvent::Pick { drawing: pick.drawing, x: pick.x, y: pick.y, picture_x: pick.picture_x, picture_y: pick.picture_y, commits: pick.commits }
            }
            Self::DrawingHover(hover) => InputEvent::DrawingHover { drawing: hover.drawing, leave: hover.leave, commits: hover.commits },
            Self::FocusIn => InputEvent::Focus,
            Self::FocusOut => InputEvent::Blur,
        })
    }
}
