//! Captions as teletext can show them: folded to the English national
//! option set, wrapped to a width, shortened to fit, and cut to the 32
//! characters of a `contents.txt` title.

use alloc::string::String;
use alloc::vec::Vec;

/// The longest title `contents.txt` keeps (the engine's contents rule).
pub const TITLE_CHARACTERS: usize = 32;
/// What marks a cut.
const ELLIPSIS: &str = "..";
/// The characters of [`ELLIPSIS`].
const ELLIPSIS_CHARACTERS: usize = 2;
/// The separator between a caption's parts in `captions.tsv`: title,
/// author, licence (`scripts/pictures.py`, from the levels page).
const PART_SEPARATOR: char = '\u{B7}';
/// The most of a contents title the author takes, so the picture's own
/// title keeps the rest.
const AUTHOR_IN_TITLE: usize = 14;
/// What a run of characters the option set cannot show becomes.
const UNSHOWN: char = '?';
/// The extensions of the source files' names, left out of the titles.
const EXTENSIONS: [&str; 7] = [".jpg", ".jpeg", ".png", ".gif", ".tif", ".tiff", ".webp"];
/// What joins the words of a licence, such as "CC BY-SA 4.0", while
/// wrapping, so that it is never broken over two lines: a control
/// character, which no folded caption holds and [`wrap`] does not break at.
const GLUE: char = '\u{1F}';
/// How much shorter the author becomes at each try to fit.
const AUTHOR_STEP: usize = 8;

/// The Russian alphabet, U+0430 to U+044F, in the BGN/PCGN romanisation.
const CYRILLIC: [&str; 32] = [
    "a", "b", "v", "g", "d", "e", "zh", "z", "i", "y", "k", "l", "m", "n", "o", "p", "r", "s", "t", "u", "f", "kh", "ts", "ch", "sh", "shch", "", "y", "", "e",
    "yu", "ya",
];
/// The Cyrillic letters of other languages, U+0450 to U+045F.
const CYRILLIC_EXTRA: [&str; 16] = ["e", "yo", "dj", "g", "ye", "dz", "i", "yi", "j", "lj", "nj", "c", "k", "i", "u", "dz"];
/// The first lower-case letter of [`CYRILLIC`], and its capital.
const CYRILLIC_LOWER: u32 = 0x0430;
const CYRILLIC_UPPER: u32 = 0x0410;
/// The first of [`CYRILLIC_EXTRA`], and its capital.
const CYRILLIC_EXTRA_LOWER: u32 = 0x0450;
const CYRILLIC_EXTRA_UPPER: u32 = 0x0400;
/// The letters in each block.
const CYRILLIC_LETTERS: u32 = 32;
const CYRILLIC_EXTRA_LETTERS: u32 = 16;
/// Combining diacritical marks, which are left out.
const COMBINING: core::ops::RangeInclusive<u32> = 0x0300..=0x036F;

/// The Latin letters with diacritics and the punctuation the captions use,
/// as the letters and signs the English set has (EN 300 706 Table 36).
fn latin(character: char) -> Option<&'static str> {
    Some(match character {
        'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' | 'Ā' | 'Ă' | 'Ą' => "A",
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' | 'ả' => "a",
        'Æ' => "AE",
        'æ' => "ae",
        'Ç' | 'Ć' | 'Ĉ' | 'Ċ' | 'Č' => "C",
        'ç' | 'ć' | 'ĉ' | 'ċ' | 'č' => "c",
        'Ď' | 'Đ' | 'Ð' => "D",
        'ď' | 'đ' | 'ð' => "d",
        'È' | 'É' | 'Ê' | 'Ë' | 'Ē' | 'Ė' | 'Ę' | 'Ě' => "E",
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ė' | 'ę' | 'ě' => "e",
        'Ğ' | 'Ģ' => "G",
        'ğ' | 'ģ' => "g",
        'Ì' | 'Í' | 'Î' | 'Ï' | 'Ī' | 'İ' => "I",
        'ì' | 'í' | 'î' | 'ï' | 'ī' | 'ı' => "i",
        'Ł' | 'Ľ' | 'Ĺ' => "L",
        'ł' | 'ľ' | 'ĺ' => "l",
        'Ñ' | 'Ń' | 'Ň' => "N",
        'ñ' | 'ń' | 'ň' => "n",
        'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' | 'Ø' | 'Ō' | 'Ő' => "O",
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ő' => "o",
        'Œ' => "OE",
        'œ' => "oe",
        'Ř' => "R",
        'ř' => "r",
        'Ś' | 'Š' | 'Ş' => "S",
        'ś' | 'š' | 'ş' => "s",
        'ß' => "ss",
        'Ť' | 'Ţ' => "T",
        'ť' | 'ţ' => "t",
        'Þ' => "Th",
        'þ' => "th",
        'Ù' | 'Ú' | 'Û' | 'Ü' | 'Ū' | 'Ů' | 'Ű' => "U",
        'ù' | 'ú' | 'û' | 'ü' | 'ū' | 'ů' | 'ű' | 'ứ' => "u",
        'Ý' | 'Ÿ' => "Y",
        'ý' | 'ÿ' => "y",
        'Ź' | 'Ż' | 'Ž' => "Z",
        'ź' | 'ż' | 'ž' => "z",
        // The English set puts other signs at the codes of [ \ ] ^ _ `
        // { | } ~ (Table 36: arrows, fractions, # and the like), so those
        // become the nearest sign it has; so do dashes and quotes.
        '\u{B7}' | '\u{2010}' | '\u{2013}' | '\u{2014}' | '~' => "-",
        '\u{2018}' | '\u{2019}' | '\u{2032}' | '`' => "'",
        '\u{201C}' | '\u{201D}' | '\u{201E}' => "\"",
        '\u{2026}' => "...",
        '\u{D7}' => "x",
        '\u{B0}' => " deg",
        '\u{A9}' => "(c)",
        '[' | '{' => "(",
        ']' | '}' => ")",
        '\\' | '|' => "/",
        '^' | '_' => " ",
        '£' => "£",
        _ => return None,
    })
}

/// A Cyrillic letter, romanised, and whether it is a capital.
fn cyrillic(character: char) -> Option<(&'static str, bool)> {
    let code = u32::from(character);
    let found = |table: &'static [&'static str], first: u32, letters: u32| {
        code.checked_sub(first).filter(|&offset| offset < letters).and_then(|offset| usize::try_from(offset).ok()).and_then(|offset| table.get(offset)).copied()
    };
    found(&CYRILLIC, CYRILLIC_LOWER, CYRILLIC_LETTERS)
        .map(|letter| (letter, false))
        .or_else(|| found(&CYRILLIC, CYRILLIC_UPPER, CYRILLIC_LETTERS).map(|letter| (letter, true)))
        .or_else(|| found(&CYRILLIC_EXTRA, CYRILLIC_EXTRA_LOWER, CYRILLIC_EXTRA_LETTERS).map(|letter| (letter, false)))
        .or_else(|| found(&CYRILLIC_EXTRA, CYRILLIC_EXTRA_UPPER, CYRILLIC_EXTRA_LETTERS).map(|letter| (letter, true)))
}

/// `text` in the characters [`teletext_runtime::Screen::text`] shows as
/// themselves: printable ASCII but for the codes the English set uses
/// for other signs, and `£`. Whitespace runs become one space; a run of
/// letters of other scripts becomes one `?`.
#[must_use]
pub fn fold(text: &str) -> String {
    let mut out = String::new();
    let mut unshown = false;
    for character in text.chars() {
        if character.is_whitespace() {
            if !out.ends_with(' ') && !out.is_empty() {
                out.push(' ');
            }
            unshown = false;
        } else if COMBINING.contains(&u32::from(character)) {
            // A mark on the letter before, which is shown without it.
        } else if let Some(piece) = latin(character) {
            out.push_str(piece);
            unshown = false;
        } else if let Some((letter, capital)) = cyrillic(character) {
            let mut letters = letter.chars();
            if let (true, Some(first)) = (capital, letters.next()) {
                out.push(first.to_ascii_uppercase());
                out.push_str(letters.as_str());
            } else {
                out.push_str(letter);
            }
            unshown = false;
        } else if (' '..='~').contains(&character) {
            out.push(character);
            unshown = false;
        } else if !unshown {
            out.push(UNSHOWN);
            unshown = true;
        }
    }
    let kept = out.trim_end().len();
    out.truncate(kept);
    out
}

/// `text` cut to `limit` characters, the cut marked with [`ELLIPSIS`].
#[must_use]
pub fn cut(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return String::from(text);
    }
    let Some(kept) = limit.checked_sub(ELLIPSIS_CHARACTERS) else { return text.chars().take(limit).collect() };
    let mut out: String = text.chars().take(kept).collect();
    let trimmed = out.trim_end().len();
    out.truncate(trimmed);
    out.push_str(ELLIPSIS);
    out
}

/// `text` wrapped into lines of at most `width` characters, at spaces; a
/// word longer than a line is broken.
#[must_use]
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    if width == 0 {
        return lines;
    }
    let mut line = String::new();
    let mut length = 0_usize;
    for word in text.split_whitespace() {
        let mut rest = word;
        while !rest.is_empty() {
            let size = rest.chars().count();
            let needed = if length == 0 { size } else { length.saturating_add(1).saturating_add(size) };
            if needed <= width {
                if length > 0 {
                    line.push(' ');
                }
                line.push_str(rest);
                length = needed;
                break;
            }
            if length > 0 {
                lines.push(core::mem::take(&mut line));
                length = 0;
                continue;
            }
            let at = rest.char_indices().nth(width).map_or(rest.len(), |(at, _)| at);
            let Some((head, tail)) = rest.split_at_checked(at) else { break };
            lines.push(String::from(head));
            rest = tail;
        }
    }
    if length > 0 {
        lines.push(line);
    }
    lines
}

/// A caption's parts: its title, without the file's extension, and the
/// rest (author and licence, where the caption names them), each folded.
#[must_use]
pub fn parts(caption: &str) -> Vec<String> {
    let mut parts: Vec<String> = caption.split(PART_SEPARATOR).map(fold).filter(|part| !part.is_empty()).collect();
    if let Some(title) = parts.first_mut() {
        let lower = title.to_ascii_lowercase();
        if let Some(extension) = EXTENSIONS.iter().find(|extension| lower.ends_with(*extension)) {
            let kept = title.len().saturating_sub(extension.len());
            title.truncate(kept);
        }
    }
    parts
}

/// A caption as one line of reading: "title, by author, licence" where it
/// names them, its parts joined with commas otherwise.
fn credit_text(parts: &[String], title_limit: usize, author_limit: usize) -> String {
    let title = parts.first().map(|title| cut(title, title_limit)).unwrap_or_default();
    match parts {
        [_, author, licence] => alloc::format!("{title}, by {}, {}", cut(author, author_limit), glued(licence)),
        [_, rest @ ..] => {
            let mut text = title;
            for part in rest {
                text.push_str(", ");
                text.push_str(part);
            }
            text
        }
        [] => title,
    }
}

/// `text` with its spaces glued.
fn glued(text: &str) -> String {
    text.chars().map(|character| if character == ' ' { GLUE } else { character }).collect()
}

/// `lines` with their glued spaces as spaces again.
fn unglued(lines: Vec<String>) -> Vec<String> {
    lines.into_iter().map(|line| line.replace(GLUE, " ")).collect()
}

/// A caption as the lines of a credits entry, at most `max_lines` of
/// `width`: whole where it fits; otherwise its title and then its author
/// are shortened, and the licence is always kept whole.
#[must_use]
pub fn credit_lines(caption: &str, width: usize, max_lines: usize) -> Vec<String> {
    let parts = parts(caption);
    let longest = parts.iter().map(|part| part.chars().count()).max().unwrap_or(0);
    let whole = wrap(&credit_text(&parts, longest, longest), width);
    if whole.len() <= max_lines {
        return unglued(whole);
    }
    // The title keeps two lines; the author what is left.
    let title_limit = width.saturating_mul(2);
    let mut author_limit = longest;
    loop {
        let lines = wrap(&credit_text(&parts, title_limit, author_limit), width);
        if lines.len() <= max_lines || author_limit == 0 {
            return unglued(lines.into_iter().take(max_lines).collect());
        }
        author_limit = author_limit.saturating_sub(AUTHOR_STEP);
    }
}

/// A caption as a `contents.txt` title of at most 32 characters: the
/// picture's title and, where the caption names one, its author.
#[must_use]
pub fn contents_title(caption: &str) -> String {
    let parts = parts(caption);
    let title = parts.first().map(String::as_str).unwrap_or_default();
    if let [_, author, _] = parts.as_slice() {
        let author = cut(author, AUTHOR_IN_TITLE);
        let room = TITLE_CHARACTERS.saturating_sub(author.chars().count()).saturating_sub(", ".len());
        return alloc::format!("{}, {author}", cut(title, room));
    }
    // A caption of one part ends with its source's folder in brackets,
    // which says nothing a viewer needs.
    let plain = match (title.ends_with(')'), title.rfind(" (")) {
        (true, Some(at)) => title.get(..at).unwrap_or(title),
        _ => title,
    };
    cut(plain, TITLE_CHARACTERS)
}
