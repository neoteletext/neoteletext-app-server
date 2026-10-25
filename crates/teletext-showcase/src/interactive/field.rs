//! A field a viewer types into, what of their typing a page accepts, and
//! the keys that edit it.

use alloc::string::String;

use neoteletext_core::consts::COLUMNS;
use neoteletext_input::cursor::{ECHO, MASKED};
use teletext_runtime::screen::{Colour, Screen, attribute};
use teletext_runtime::{Input, Named};

/// The characters the English option set shows as themselves besides
/// letters, digits and the space (EN 300 706 Table 36): `[`, `\`, `]`,
/// `^`, `_`, `` ` ``, `{`, `|`, `}` and `~` are others there (arrows,
/// fractions, `#`, a dash), so they are not accepted. `#` is accepted,
/// since the screen writes it at its English code.
const PUNCTUATION: &str = "!\"#$%&'()*+,-./:;<=>?@£";

/// `character` if a teletext page in the English set shows it as typed:
/// whatever a viewer sends, only these reach a page.
pub(super) fn printable(character: char) -> Option<char> {
    (character.is_ascii_alphanumeric() || character == ' ' || PUNCTUATION.contains(character)).then_some(character)
}

/// What a viewer's input does to a field.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Edit<'a> {
    /// Text typed or pasted.
    Text(&'a str),
    /// One character, from a key press in `ALL_KEYS`.
    Character(char),
    /// Delete the character before the cursor.
    Backspace,
    /// Submit.
    Enter,
    /// The next field.
    Tab,
    /// Cancel or clear.
    Escape,
    /// Anything else, which the field ignores.
    Other,
}

impl<'a> Edit<'a> {
    /// The edit `input` asks for. Raw text, a paste and a character key's
    /// press (in `ALL_KEYS`, INPUT.md section 6.3) all type; releases do
    /// nothing.
    pub(super) fn of(input: &Input<'a>) -> Self {
        match *input {
            Input::Text(text) | Input::Paste(text) => Self::Text(text),
            _ if input.pressed(Named::Backspace) => Self::Backspace,
            _ if input.pressed(Named::Enter) => Self::Enter,
            _ if input.pressed(Named::Tab) => Self::Tab,
            _ if input.pressed(Named::Escape) => Self::Escape,
            _ => input.character().map_or(Self::Other, Self::Character),
        }
    }
}

/// A line of typing, at most `limit` characters, each one a page can show.
#[derive(Clone, Debug)]
pub(super) struct Field {
    text: String,
    limit: usize,
}

impl Field {
    /// An empty field of at most `limit` characters.
    pub(super) const fn new(limit: usize) -> Self {
        Self { text: String::new(), limit }
    }

    /// What is typed.
    pub(super) fn text(&self) -> &str {
        &self.text
    }

    /// How many characters are typed.
    pub(super) fn length(&self) -> usize {
        self.text.chars().count()
    }

    /// How many more characters the field takes.
    pub(super) fn room(&self) -> usize {
        self.limit.saturating_sub(self.length())
    }

    /// Whether nothing but spaces is typed.
    pub(super) fn is_blank(&self) -> bool {
        self.text.trim().is_empty()
    }

    /// Types `typed`, each character through `accept`, which drops it or
    /// gives the character to keep; it stops at the limit. The number of
    /// characters kept.
    pub(super) fn type_text(&mut self, typed: &str, accept: impl Fn(char) -> Option<char>) -> usize {
        let mut kept = 0_usize;
        for character in typed.chars().filter_map(accept) {
            if self.length() >= self.limit {
                break;
            }
            self.text.push(character);
            kept = kept.saturating_add(1);
        }
        kept
    }

    /// Deletes the last character: whether there was one.
    pub(super) fn backspace(&mut self) -> bool {
        self.text.pop().is_some()
    }

    /// Empties the field.
    pub(super) fn clear(&mut self) {
        self.text.clear();
    }

    /// Puts the cursor after the text at `row`, the text starting at
    /// `column`, with the room left for echo: width 0 when full, so the
    /// receiver echoes nothing the field would refuse. Echo requires a
    /// width and MASKED requires echo (INPUT.md section 7.1, which also
    /// bounds column + width by 40), so a full field's cursor has neither.
    pub(super) fn cursor(&self, screen: &mut Screen, row: usize, column: usize, flags: u8) {
        let at = column.saturating_add(self.length()).min(COLUMNS.saturating_sub(1));
        let width = self.room().min(COLUMNS.saturating_sub(at));
        let flags = if width == 0 { flags & !(ECHO | MASKED) } else { flags };
        let (Ok(row), Ok(at), Ok(width)) = (u8::try_from(row), u8::try_from(at), u8::try_from(width)) else { return };
        screen.cursor(row, at, width, flags);
    }
}

/// The cells a field band takes before its text: the band's colour, New
/// Background, and the text's colour (EN 300 706 Table 26).
pub(super) const BAND_LEAD: usize = 3;

/// Draws a field's band on `row` from `column`: a `band` background under
/// `limit` characters and the cursor cell after them, `codes` in `ink`,
/// and Black Background after it. The text starts at `column +`
/// [`BAND_LEAD`].
pub(super) fn band(screen: &mut Screen, row: usize, column: usize, limit: usize, band: Colour, ink: Colour, codes: &[u8]) {
    screen.codes(row, column, &[band.text(), attribute::NEW_BACKGROUND, ink.text()]);
    let start = column.saturating_add(BAND_LEAD);
    screen.codes(row, start, codes);
    // The cell after the last character holds the cursor of a full field.
    screen.code(row, start.saturating_add(limit).saturating_add(1), attribute::BLACK_BACKGROUND);
}

/// `text` as the English set's codes, as [`Screen::text`] writes them, for
/// [`band`].
pub(super) fn codes_of(text: &str) -> alloc::vec::Vec<u8> {
    let mut screen = Screen::new();
    screen.text(0, 0, text);
    screen.row(0).map(|row| row.iter().take(text.chars().count()).copied().collect()).unwrap_or_default()
}

/// The choice a digit key makes among `count`, numbered from 1: 0 for the
/// first.
pub(super) fn choice(input: &Input<'_>, count: usize) -> Option<usize> {
    let digit = input.character()?.to_digit(10)?;
    usize::try_from(digit).ok()?.checked_sub(1).filter(|&chosen| chosen < count)
}
