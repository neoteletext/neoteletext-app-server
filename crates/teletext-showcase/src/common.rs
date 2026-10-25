//! What every showcase page shares: the header, the title band and the
//! Fastext row.

use teletext_runtime::screen::{Colour, Screen};

/// The cells between Fastext labels: the next label's colour code and a
/// space.
const LABEL_GAP: usize = 2;

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
    // The labels one after another, each after its colour code, so a long
    // label is never overwritten by the next label's colour.
    let colours = [Colour::Red, Colour::Green, Colour::Yellow, Colour::Cyan];
    let mut column = 0_usize;
    for (colour, (label, _)) in colours.iter().zip(fastext.iter()) {
        if label.is_empty() {
            continue;
        }
        screen.coloured(24, column, *colour, label);
        column = column.saturating_add(label.chars().count()).saturating_add(LABEL_GAP);
    }
    screen.fastext([fastext[0].1, fastext[1].1, fastext[2].1, fastext[3].1]);
    screen.index_link(0x100);
    screen
}
