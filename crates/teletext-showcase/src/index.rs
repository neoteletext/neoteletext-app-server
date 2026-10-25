//! Page 100: the showcase's index, with Fastext to each section.

use alloc::vec::Vec;

use teletext_runtime::screen::Colour;
use teletext_runtime::{Application, Context, Owner, PageEntry};

use crate::common::page;

/// A section of the index: its page, its name and what it holds.
pub struct Section {
    /// Its first page.
    pub page: u16,
    /// Its name.
    pub name: &'static str,
    /// What it holds, in a few words.
    pub about: &'static str,
}

/// The index of `sections`.
pub struct Index {
    sections: Vec<Section>,
}

impl Index {
    /// An index.
    #[must_use]
    pub const fn new(sections: Vec<Section>) -> Self {
        Self { sections }
    }
}

/// The index page's number.
pub const INDEX: u16 = 0x100;
/// The first row of the list.
const FIRST_ROW: usize = 4;
/// Rows between sections.
const ROW_STEP: usize = 2;

impl Application for Index {
    fn pages(&self) -> Vec<PageEntry> {
        alloc::vec![PageEntry::new(INDEX, "Index")]
    }

    fn start(&mut self, context: &mut Context<'_>) {
        let links = [self.sections.first(), self.sections.get(1), self.sections.get(2), self.sections.get(3)];
        let labels = links.map(|section| section.map_or(("", None), |section| (section.name, Some(section.page))));
        let mut screen = page("NeoTeletext showcase", Colour::Blue, labels);
        let mut row = FIRST_ROW;
        for section in &self.sections {
            let number = alloc::format!("{:03X}", section.page);
            screen.coloured(row, 1, Colour::Yellow, &number);
            screen.coloured(row, 5, Colour::White, section.name);
            screen.coloured(row.saturating_add(1), 5, Colour::Cyan, section.about);
            row = row.saturating_add(ROW_STEP);
        }
        context.publish(Owner::Channel, INDEX, &screen);
    }
}
