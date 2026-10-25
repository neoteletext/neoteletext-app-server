//! Marked-up text, word-wrapped onto a page in two colours: a span in
//! braces, `{like this}`, is highlighted. A colour change costs no cell
//! of its own, because the attribute takes the space between two words
//! (EN 300 706 clause 12.2: a spacing attribute shows as a space).

use alloc::string::String;
use alloc::vec::Vec;

use teletext_runtime::screen::{Colour, SPACE, Screen};

/// The column text starts at on a full row: after its colour attribute
/// in column 0.
pub const TEXT_COLUMN: usize = 1;
/// The mark that opens a highlighted span.
const OPEN: char = '{';
/// The mark that closes it.
const CLOSE: char = '}';

/// A word, without its marks, and whether it is highlighted.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Word {
    /// Its characters.
    pub text: String,
    /// Whether it is inside braces.
    pub highlight: bool,
}

/// The words of `marked`, split at spaces. A word is highlighted when it
/// begins inside a span or with its opening brace, so punctuation after
/// a closing brace takes the word's colour rather than a cell of its own.
#[must_use]
pub fn words(marked: &str) -> Vec<Word> {
    let mut inside = false;
    let mut list = Vec::new();
    for token in marked.split(' ').filter(|token| !token.is_empty()) {
        let highlight = inside || token.starts_with(OPEN);
        for character in token.chars() {
            if character == OPEN {
                inside = true;
            } else if character == CLOSE {
                inside = false;
            }
        }
        let text: String = token.chars().filter(|&character| character != OPEN && character != CLOSE).collect();
        if !text.is_empty() {
            list.push(Word { text, highlight });
        }
    }
    list
}

/// `word` cut into pieces of at most `width` characters, for a word
/// longer than its line.
fn pieces(word: &Word, width: usize) -> Vec<Word> {
    let width = width.max(1);
    let characters: Vec<char> = word.text.chars().collect();
    characters.chunks(width).map(|chunk| Word { text: chunk.iter().collect(), highlight: word.highlight }).collect()
}

/// `marked` laid out into lines, line `n` at most `width_of(n)`
/// characters wide, and at most `max_lines` of them; what does not fit
/// is left out.
#[must_use]
pub fn wrap(marked: &str, width_of: &dyn Fn(usize) -> usize, max_lines: usize) -> Vec<Vec<Word>> {
    let mut lines: Vec<Vec<Word>> = Vec::new();
    let mut line: Vec<Word> = Vec::new();
    let mut used = 0_usize;
    let mut pending: Vec<Word> = words(marked);
    pending.reverse();
    while let Some(word) = pending.pop() {
        if lines.len() >= max_lines {
            break;
        }
        let width = width_of(lines.len());
        let length = word.text.chars().count();
        let needed = if line.is_empty() { length } else { used.saturating_add(1).saturating_add(length) };
        if needed <= width {
            used = needed;
            line.push(word);
        } else if line.is_empty() {
            // Longer than a whole line: cut it, the rest going on.
            let mut cut = pieces(&word, width);
            cut.reverse();
            pending.extend(cut);
        } else {
            lines.push(core::mem::take(&mut line));
            used = 0;
            pending.push(word);
        }
    }
    if !line.is_empty() && lines.len() < max_lines {
        lines.push(line);
    }
    lines
}

/// The two colours text is drawn in.
#[derive(Clone, Copy, Debug)]
pub struct Style {
    /// Plain words.
    pub plain: Colour,
    /// Highlighted words.
    pub highlight: Colour,
}

/// Where wrapped text goes: rows from `first_row`, text from `column`
/// (its colour attribute in the cell before), the first `narrow_rows`
/// rows `narrow_width` wide (beside a picture) and the rest `width`.
#[derive(Clone, Copy, Debug)]
pub struct Block {
    /// The first row.
    pub first_row: usize,
    /// How many rows it may take.
    pub rows: usize,
    /// The first column of text.
    pub column: usize,
    /// Rows at the top that are narrower.
    pub narrow_rows: usize,
    /// Their width.
    pub narrow_width: usize,
    /// The width of the rest.
    pub width: usize,
}

/// Draws `marked` into `block` on `screen` in `style`; answers how many
/// rows it took.
pub fn draw(screen: &mut Screen, marked: &str, block: Block, style: Style) -> usize {
    let width_of = |line: usize| if line < block.narrow_rows { block.narrow_width } else { block.width };
    let lines = wrap(marked, &width_of, block.rows);
    for (offset, line) in lines.iter().enumerate() {
        let row = block.first_row.saturating_add(offset);
        let mut column = block.column;
        let mut colour: Option<Colour> = None;
        for word in line {
            let wanted = if word.highlight { style.highlight } else { style.plain };
            // The cell before the word: the line's first attribute, or the
            // space between words, which becomes the attribute when the
            // colour changes.
            let before = column.saturating_sub(1);
            if colour == Some(wanted) {
                screen.code(row, before, SPACE);
            } else {
                screen.code(row, before, wanted.text());
                colour = Some(wanted);
            }
            screen.text(row, column, &word.text);
            column = column.saturating_add(word.text.chars().count()).saturating_add(1);
        }
    }
    lines.len()
}
