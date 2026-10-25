//! Page 200: the area's menu, one shared page.

use alloc::format;

use teletext_runtime::screen::{Colour, Screen};

use super::{CALCULATOR, FORM, GUESTBOOK, PAINT, POLL, QUIZ};
use crate::common::page;
use crate::index::INDEX;

/// The menu's lines: a page and what it shows.
const ENTRIES: [(u16, &str, &str); 6] = [
    (FORM, "A form", "fields that echo, one masked"),
    (GUESTBOOK, "Guestbook", "post a line every viewer sees"),
    (POLL, "Live poll", "vote with 1 to 4, results live"),
    (QUIZ, "Teletext quiz", "ten questions of TV history"),
    (CALCULATOR, "Calculator", "type a sum, Enter works it out"),
    (PAINT, "Mouse paint", "click the cells to draw"),
];
/// The first entry's row.
const FIRST_ROW: usize = 4;
/// Rows between entries: the name, then what it does.
const ROW_STEP: usize = 3;

/// The menu.
pub(super) fn screen() -> Screen {
    let mut screen = page("User input", Colour::Green, [("Form", Some(FORM)), ("Guestbook", Some(GUESTBOOK)), ("Poll", Some(POLL)), ("Index", Some(INDEX))]);
    let mut row = FIRST_ROW;
    for (number, name, about) in ENTRIES {
        screen.coloured(row, 1, Colour::Yellow, &format!("{number:03X}"));
        screen.coloured(row, 5, Colour::White, name);
        screen.coloured(row.saturating_add(1), 5, Colour::Cyan, about);
        row = row.saturating_add(ROW_STEP);
    }
    screen.centred(22, Colour::Green, "Type on these pages: keys go to us");
    screen
}
