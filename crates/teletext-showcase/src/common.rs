//! What every showcase page shares: the header, the title band and the
//! Fastext row.

use teletext_runtime::screen::{Colour, ROWS, Screen};

/// The cells between Fastext labels: the next label's colour code and a
/// space.
const LABEL_GAP: usize = 2;

/// The row of the Fastext prompts, the page's last (EN 300 706 clause 9.6).
pub const FASTEXT_ROW: usize = ROWS - 1;

/// The service's name in each header.
pub const SERVICE: &str = "NeoTeletext";

/// A screen with the showcase's header, its title band on rows 1 and 2,
/// and a Fastext row 24 naming the four coloured links.
#[must_use]
pub fn page(title: &str, band: Colour, fastext: [(&str, Option<u16>); 4]) -> Screen {
    let mut screen = Screen::new();
    screen.header(SERVICE);
    screen.background(1, 0, band);
    screen.double_height(1, 2, Colour::White, title);
    let colours = [Colour::Red, Colour::Green, Colour::Yellow, Colour::Cyan];
    let columns = label_columns(fastext.map(|(label, _)| label));
    for ((colour, (label, _)), column) in colours.iter().zip(fastext.iter()).zip(columns) {
        if let Some((column, _)) = column {
            screen.coloured(FASTEXT_ROW, column, *colour, label);
        }
    }
    screen.fastext([fastext[0].1, fastext[1].1, fastext[2].1, fastext[3].1]);
    screen.index_link(0x100);
    screen
}

/// Where each Fastext label goes on row 24: the colour code's column and
/// the column after the label's last character, or `None` for an empty
/// label. The labels go one after another, each after its colour code, so
/// a long label is never overwritten by the next label's colour.
#[must_use]
pub fn label_columns(labels: [&str; 4]) -> [Option<(usize, usize)>; 4] {
    let mut column = 0_usize;
    labels.map(|label| {
        if label.is_empty() {
            return None;
        }
        // The colour code takes the label's first cell.
        let end = column.saturating_add(1).saturating_add(label.chars().count());
        let span = (column, end);
        column = column.saturating_add(label.chars().count()).saturating_add(LABEL_GAP);
        Some(span)
    })
}

/// The Fastext label, 0 to 3, drawn at `column` of row 24 by [`page`]
/// with these labels: where a page that takes the mouse finds which
/// prompt was clicked.
#[must_use]
pub fn label_at(labels: [&str; 4], column: usize) -> Option<usize> {
    label_columns(labels).iter().position(|span| span.is_some_and(|(start, end)| (start..end).contains(&column)))
}
