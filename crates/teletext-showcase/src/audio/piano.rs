//! Page 502, the piano: an octave from middle C on the letter keys, by
//! position, so it plays the same on any layout (INPUT.md's `base`, the
//! United States layout's character of the key pressed). Each viewer's
//! page shows the keys they hold until they let go.

use alloc::format;

use teletext_runtime::screen::{Colour, Screen};

use super::draw::badge;
use super::tracker::note_name;
use crate::sound::NOTES;

/// The keys of the octave, C to C, by their United States characters: the
/// home row is the white keys, the row above it the black, as on a
/// tracker's keyboard.
pub(crate) const KEYS: [char; NOTES as usize] = ['a', 'w', 's', 'e', 'd', 'f', 't', 'g', 'y', 'h', 'u', 'j', 'k'];
/// The notes of the octave that are white keys.
const WHITE: [u8; 8] = [0, 2, 4, 5, 7, 9, 11, 12];
/// Middle C's MIDI number: the library's first note.
const MIDDLE_C: u8 = 60;

/// The note of `key`, 0 to 12, by its character on a United States layout.
pub(crate) fn note_of(key: char) -> Option<u8> {
    let lower = key.to_ascii_lowercase();
    KEYS.iter().position(|&candidate| candidate == lower).and_then(|note| u8::try_from(note).ok())
}

/// Whether `note` is a black key.
const fn is_black(note: u8) -> bool {
    matches!(note % 12, 1 | 3 | 6 | 8 | 10)
}

/// What a viewer holds.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub(crate) struct Held {
    /// A bit for each note held down by a key.
    pub keys: u16,
    /// The note held down by the mouse.
    pub mouse: Option<u8>,
    /// The last note played.
    pub last: Option<u8>,
}

impl Held {
    /// Whether `note` is held, by a key or the mouse.
    pub(crate) fn holds(self, note: u8) -> bool {
        self.keys & (1 << note) != 0 || self.mouse.is_some_and(|held| held == note)
    }
}

/// The first white key's colour cell.
const KEYS_COLUMN: usize = 4;
/// Columns from one white key to the next: a colour cell and three
/// mosaic cells.
const KEY_STEP: usize = 4;
/// The row of the black keys' letters.
const BLACK_LABELS: usize = 6;
/// The black keys' rows, above the white keys' lower part.
const UPPER_ROWS: core::ops::Range<usize> = 7..11;
/// The white keys' lower part.
const LOWER_ROWS: core::ops::Range<usize> = 11..13;
/// The row of the white keys' letters.
const WHITE_LABELS: usize = 13;
/// The row of the notes' names.
const NOTE_NAMES: usize = 14;
/// A full mosaic cell: every sixel set (EN 300 706 Table 47).
const SOLID: u8 = 0x7F;

/// The colour of `note`'s key, held or not.
fn key_colour(note: u8, held: Held) -> Colour {
    match (held.holds(note), is_black(note)) {
        (true, _) => Colour::Red,
        (false, true) => Colour::Blue,
        (false, false) => Colour::White,
    }
}

/// The first column of white key `index`, its colour cell.
const fn white_column(index: usize) -> usize {
    KEYS_COLUMN.saturating_add(index.saturating_mul(KEY_STEP))
}

/// Draws the keyboard with what is held.
pub(crate) fn draw(screen: &mut Screen, held: Held) {
    screen.coloured(3, 1, Colour::Cyan, "Play with the keys, or click them");
    for (index, &note) in WHITE.iter().enumerate() {
        let column = white_column(index);
        let colour = key_colour(note, held);
        let black_left = note > 0 && is_black(note.saturating_sub(1));
        let black_right = note < NOTES.saturating_sub(1) && is_black(note.saturating_add(1));
        for row in UPPER_ROWS {
            // A black key takes the cell either side of the white keys'
            // boundary: its colour in the left key's last cell, its
            // mosaic in the right key's colour cell.
            if black_left {
                screen.codes(row, column, &[SOLID, colour.mosaic(), SOLID]);
            } else {
                screen.codes(row, column, &[colour.mosaic(), SOLID, SOLID]);
            }
            let last = if black_right { key_colour(note.saturating_add(1), held).mosaic() } else { SOLID };
            screen.code(row, column.saturating_add(3), last);
        }
        for row in LOWER_ROWS {
            screen.codes(row, column, &[colour.mosaic(), SOLID, SOLID, SOLID]);
        }
        let middle = column.saturating_add(2);
        let key = KEYS.get(usize::from(note)).copied().unwrap_or(' ').to_ascii_uppercase();
        screen.coloured(WHITE_LABELS, middle.saturating_sub(1), Colour::Yellow, &format!("{key}"));
        let name = note_name(MIDDLE_C.saturating_add(note));
        screen.coloured(NOTE_NAMES, middle.saturating_sub(1), Colour::White, name.get(..1).unwrap_or(""));
        if black_right {
            let black = KEYS.get(usize::from(note.saturating_add(1))).copied().unwrap_or(' ').to_ascii_uppercase();
            screen.coloured(BLACK_LABELS, column.saturating_add(3), Colour::Yellow, &format!("{black}"));
        }
    }
    let playing = (0..NOTES).filter(|&note| held.holds(note)).map(|note| note_name(MIDDLE_C.saturating_add(note))).collect::<alloc::vec::Vec<_>>();
    if playing.is_empty() {
        let last = held.last.map_or_else(|| alloc::string::String::from("none yet"), |note| note_name(MIDDLE_C.saturating_add(note)));
        screen.coloured(16, 1, Colour::White, &format!("Last note: {last}"));
    } else {
        badge(screen, 16, 0, Colour::Red, Colour::White, &format!(" Playing {} ", playing.join(" ")));
    }
    screen.coloured(18, 1, Colour::Green, "White keys");
    screen.coloured(18, 13, Colour::Yellow, "A S D F G H J K");
    screen.coloured(19, 1, Colour::Green, "Black keys");
    screen.coloured(19, 13, Colour::Yellow, "W E   T Y U");
    screen.coloured(21, 1, Colour::Cyan, "Keys go by where they are, so every");
    screen.coloured(22, 1, Colour::Cyan, "keyboard layout plays the same notes.");
}

/// The note of the key at a cell, for the mouse.
pub(crate) fn note_at(column: usize, row: usize) -> Option<u8> {
    let first = KEYS_COLUMN;
    let last = white_column(WHITE.len());
    if column < first || column >= last || !(UPPER_ROWS.contains(&row) || LOWER_ROWS.contains(&row)) {
        return None;
    }
    let offset = column.saturating_sub(first);
    let index = offset / KEY_STEP;
    let within = offset % KEY_STEP;
    let note = WHITE.get(index).copied()?;
    if UPPER_ROWS.contains(&row) {
        // The black key's two cells: the left key's last, the right key's
        // first.
        if within == 3 && note < NOTES.saturating_sub(1) && is_black(note.saturating_add(1)) {
            return Some(note.saturating_add(1));
        }
        if within == 0 && note > 0 && is_black(note.saturating_sub(1)) {
            return Some(note.saturating_sub(1));
        }
    }
    Some(note)
}
