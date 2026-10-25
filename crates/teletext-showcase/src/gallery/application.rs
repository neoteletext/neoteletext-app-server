//! A gallery: its menu, a page for each picture with the levels as
//! sub-pages, the credits pages and, on the showcase, the page that
//! compares the levels. Everything is decided when the gallery is made,
//! so the contents name every page, and published once at start.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use neoteletext_core::consts::COLUMNS;
use neoteletext_formats::page::{FastextLink, PageFile};
use teletext_runtime::screen::{Colour, FIRST_PAGE, Screen};
use teletext_runtime::{Application, Context, Owner, PageEntry};

use super::{Collection, Group, levels, text};
use crate::channels::Picture;
use crate::common::page;

/// Where a gallery's pages are, as decimal page numbers: `contents.txt`
/// names decimal pages only.
#[derive(Clone, Copy, Debug)]
pub struct Plan {
    /// The menu.
    pub menu: u16,
    /// True when the menu is the channel's own index, page 100.
    pub channel_index: bool,
    /// The first picture's page.
    pub first_picture: u16,
    /// The last page a picture may have; pictures beyond are left out.
    pub last_picture: u16,
    /// The page that compares the levels, where there is one.
    pub levels: Option<u16>,
    /// The first credits page.
    pub first_credits: u16,
    /// The last page credits may have.
    pub last_credits: u16,
}

impl Plan {
    /// The showcase channel's gallery, pages 600 to 699.
    pub const SHOWCASE: Self =
        Self { menu: 600, channel_index: false, first_picture: 601, last_picture: 689, levels: Some(690), first_credits: 695, last_credits: 699 };
    /// A collection's channel: its index at 100, pictures from 101 and
    /// credits from 800, so at most 800 pages, the engine's limit.
    pub const COLLECTION: Self =
        Self { menu: 100, channel_index: true, first_picture: 101, last_picture: 799, levels: None, first_credits: 800, last_credits: 899 };
}

/// Page `decimal`, such as 601, as the runtime numbers pages, `0x601`.
#[must_use]
pub const fn page_number(decimal: u16) -> u16 {
    ((decimal / 100) << 8) | (((decimal / 10) % 10) << 4) | (decimal % 10)
}

/// The credits' first row, under the title band.
const CREDITS_FIRST_ROW: usize = 4;
/// The credits' last row: row 23 is left clear above the Fastext row.
const CREDITS_LAST_ROW: usize = 22;
/// The credits' lines on a sub-page.
const CREDITS_LINES: usize = CREDITS_LAST_ROW.saturating_sub(CREDITS_FIRST_ROW).saturating_add(1);
/// The column of an entry's text colour: after its page number in
/// columns 1 to 3, whose colour is in column 0.
const CREDITS_COLOUR_COLUMN: usize = 4;
/// An entry's text, after its colour.
const CREDITS_WIDTH: usize = COLUMNS.saturating_sub(CREDITS_COLOUR_COLUMN).saturating_sub(1);
/// The most lines a caption takes; longer ones are shortened
/// ([`text::credit_lines`]).
const ENTRY_LINES: usize = 6;
/// The most sub-pages of a credits page: the browser shows at most 64
/// (its `MAX_SUBPAGES`), and fewer keep each page quick to fill.
const CREDITS_SUBPAGES: usize = 40;
/// The column of the text on the gallery's own pages, after its colour.
const TEXT_COLUMN: usize = 0;
/// The menus' list: the page numbers' colour, then the names'.
const NAME_COLUMN: usize = 8;
/// The showcase menu's list of the other channels.
const COUNT_COLUMN: usize = 13;
const FOLDER_COLUMN: usize = 27;

/// A picture as the gallery shows it.
#[derive(Clone, Debug)]
struct Shown {
    /// Its page, in decimal.
    number: u16,
    /// Its contents title.
    title: String,
    /// Its caption as credits lines.
    lines: Vec<String>,
    /// Its levels: the sub-page and the TTI of each.
    levels: Vec<(u8, Vec<u8>)>,
    /// Its credits page, in decimal, and the sub-page.
    credit: Option<(u16, u8)>,
}

/// A group's pages, for the menu.
#[derive(Clone, Copy, Debug)]
struct Span {
    group: Group,
    first: u16,
    last: u16,
}

/// A credits page: its number, in decimal, and the pictures on each of
/// its sub-pages.
#[derive(Clone, Debug)]
struct Credits {
    number: u16,
    subpages: Vec<Vec<usize>>,
}

/// One gallery: a menu, its pictures and their credits.
#[derive(Clone, Debug)]
pub struct Gallery {
    title: &'static str,
    plan: Plan,
    pictures: Vec<Shown>,
    spans: Vec<Span>,
    credits: Vec<Credits>,
    elsewhere: Vec<(Collection, usize)>,
}

impl Gallery {
    /// A gallery called `title` of `groups`, laid out by `plan`; its menu
    /// names `elsewhere`, the collections on channels of their own and
    /// their sizes. A picture without a level of [`levels::LEVELS`] is
    /// left out, as are pictures beyond the plan's last page.
    #[must_use]
    pub fn new(title: &'static str, plan: Plan, groups: &[(Group, Vec<&Picture>)], elsewhere: Vec<(Collection, usize)>) -> Self {
        let mut pictures = Vec::new();
        let mut spans = Vec::new();
        let mut number = plan.first_picture;
        for (group, members) in groups {
            let first = number;
            for picture in members {
                if number > plan.last_picture {
                    break;
                }
                let mut shown_levels: Vec<(u8, Vec<u8>)> =
                    picture.levels.iter().filter_map(|(level, bytes)| Some((levels::subpage_of(level)?, bytes.clone()))).collect();
                shown_levels.sort_by_key(|&(subpage, _)| subpage);
                if shown_levels.is_empty() {
                    continue;
                }
                let mut title = text::contents_title(&picture.caption);
                if title.is_empty() {
                    title = text::cut(&picture.name, text::TITLE_CHARACTERS);
                }
                pictures.push(Shown {
                    number,
                    title,
                    lines: text::credit_lines(&picture.caption, CREDITS_WIDTH, ENTRY_LINES),
                    levels: shown_levels,
                    credit: None,
                });
                number = number.saturating_add(1);
            }
            if number > first {
                spans.push(Span { group: *group, first, last: number.saturating_sub(1) });
            }
        }
        let credits = credits(&mut pictures, plan);
        Self { title, plan, pictures, spans, credits, elsewhere }
    }

    /// Every page and its sub-pages, as published at start: the runtime's
    /// page number of each.
    #[must_use]
    pub fn published(&self) -> Vec<(u16, Vec<PageFile>)> {
        let mut pages = Vec::new();
        let menu = page_number(self.plan.menu);
        let screen = if self.plan.channel_index { self.collection_menu() } else { self.showcase_menu() };
        pages.push((menu, alloc::vec![shown_page(&screen, menu, 0)]));
        if let Some(decimal) = self.plan.levels {
            let number = page_number(decimal);
            pages.push((number, alloc::vec![shown_page(&self.levels_page(), number, 0)]));
        }
        for index in 0..self.pictures.len() {
            if let Some(shown) = self.pictures.get(index) {
                pages.push((page_number(shown.number), self.picture(index, shown)));
            }
        }
        for credits in &self.credits {
            pages.push((page_number(credits.number), self.credits_page(credits)));
        }
        pages
    }

    fn first_credits(&self) -> Option<u16> {
        self.credits.first().map(|credits| page_number(credits.number))
    }

    /// The Fastext of a span's first page, by its short name.
    fn span_link(&self, position: usize) -> (&'static str, Option<u16>) {
        self.spans.get(position).map_or(("", None), |span| (span.group.short, Some(page_number(span.first))))
    }

    fn showcase_menu(&self) -> Screen {
        let levels = self.plan.levels.map(page_number);
        let credits = self.first_credits();
        let mut screen = page(
            self.title,
            Colour::Blue,
            [self.span_link(0), self.span_link(1), ("Levels", levels), (if credits.is_some() { "Credits" } else { "" }, credits)],
        );
        screen.coloured(4, TEXT_COLUMN, Colour::Cyan, "Each picture is one page, and its three");
        screen.coloured(5, TEXT_COLUMN, Colour::Cyan, "sub-pages are Levels 1.5, 2.5 and 3.5.");
        let mut row = 7_usize;
        if self.pictures.is_empty() {
            for line in ["No pictures are installed here: they", "are copied in when the server is", "deployed, never kept in git."] {
                screen.coloured(row, TEXT_COLUMN, Colour::White, line);
                row = row.saturating_add(1);
            }
            row = row.saturating_add(1);
        }
        for span in &self.spans {
            list_line(&mut screen, row, &format!("{}-{}", span.first, span.last), span.group.name);
            row = row.saturating_add(1);
        }
        if let Some(decimal) = self.plan.levels {
            list_line(&mut screen, row, &format!("{decimal}"), "Levels compared");
            row = row.saturating_add(1);
        }
        if let Some(credits) = self.credits.first() {
            list_line(&mut screen, row, &format!("{}", credits.number), "Picture credits");
            row = row.saturating_add(1);
        }
        if !self.elsewhere.is_empty() {
            row = row.saturating_add(1);
            screen.coloured(row, TEXT_COLUMN, Colour::Cyan, "More pictures, each on its own channel:");
            for (collection, count) in &self.elsewhere {
                row = row.saturating_add(1);
                screen.coloured(row, TEXT_COLUMN, Colour::White, collection.group.name);
                screen.coloured(row, COUNT_COLUMN, Colour::Cyan, &format!("{count} pictures"));
                screen.coloured(row, FOLDER_COLUMN, Colour::Yellow, collection.folder);
            }
        }
        screen
    }

    fn collection_menu(&self) -> Screen {
        let first = self.pictures.first().map(|shown| shown.number);
        let last = self.pictures.last().map(|shown| shown.number);
        let mut screen =
            page(self.title, Colour::Blue, [("First", first.map(page_number)), ("Last", last.map(page_number)), ("Credits", self.first_credits()), ("", None)]);
        if let (Some(first), Some(last)) = (first, last) {
            let count = self.pictures.len();
            screen.coloured(4, TEXT_COLUMN, Colour::White, &format!("{count} pictures, on pages {first} to {last}."));
        }
        screen.coloured(6, TEXT_COLUMN, Colour::Cyan, "Each picture is one page of three");
        screen.coloured(7, TEXT_COLUMN, Colour::Cyan, "sub-pages, one for each level:");
        for (row, (subpage, level, about)) in
            (8_usize..).zip([("1", "Level 1.5", "8 colours"), ("2", "Level 2.5", "CLUTs 2, 3 redefined"), ("3", "Level 3.5", "CLUTs 0 to 3 redefined")])
        {
            screen.coloured(row, 1, Colour::Yellow, subpage);
            screen.coloured(row, 4, Colour::White, level);
            screen.coloured(row, 15, Colour::Cyan, about);
        }
        screen.coloured(12, TEXT_COLUMN, Colour::White, "Step through the sub-pages to compare.");
        if let Some(credits) = self.credits.first() {
            list_line(&mut screen, 14, &format!("{}", credits.number), "Captions and credits");
        }
        for (row, line) in (16_usize..).zip(["On a picture, Fastext: red the one", "before, green the next, yellow this", "page, cyan its credit."]) {
            screen.coloured(row, TEXT_COLUMN, Colour::Cyan, line);
        }
        screen
    }

    fn levels_page(&self) -> Screen {
        let mut screen = page(
            "Levels compared",
            Colour::Magenta,
            [self.span_link(0), self.span_link(1), ("Gallery", Some(page_number(self.plan.menu))), ("Credits", self.first_credits())],
        );
        let lines: [(usize, Colour, &str); 14] = [
            (4, Colour::White, "Every picture page has three sub-pages:"),
            (6, Colour::Yellow, "1  Level 1.5"),
            (7, Colour::White, "   8 colours. Every colour change is a"),
            (8, Colour::White, "   spacing attribute and takes a cell."),
            (10, Colour::Yellow, "2  Level 2.5"),
            (11, Colour::White, "   32 colours: packet X/28/0 redefines"),
            (12, Colour::White, "   CLUTs 2 and 3 for this picture, and"),
            (13, Colour::White, "   X/26 changes colour without a cell."),
            (15, Colour::Yellow, "3  Level 3.5"),
            (16, Colour::White, "   As 2.5, and X/28/4 redefines CLUTs"),
            (17, Colour::White, "   0 and 1 too: all fitted to it."),
            (19, Colour::Cyan, "Step through the sub-pages to compare."),
            (20, Colour::Cyan, "Each picture is 80 x 75 sixels, rows"),
            (21, Colour::Cyan, "0 to 24; its links are on Fastext."),
        ];
        for (row, colour, line) in lines {
            screen.coloured(row, TEXT_COLUMN, colour, line);
        }
        screen
    }

    /// The Fastext of the picture at `index`: red the one before, green
    /// the next (each wrapping), yellow the menu, cyan its credit at its
    /// sub-page, and the index.
    fn picture_links(&self, index: usize, shown: &Shown) -> [Option<FastextLink>; 5] {
        let count = self.pictures.len();
        let previous = index.checked_sub(1).unwrap_or_else(|| count.saturating_sub(1));
        let next = if index.saturating_add(1) >= count { 0 } else { index.saturating_add(1) };
        let at = |position: usize| self.pictures.get(position).map(|other| levels::link(page_number(other.number), None));
        let credit = shown.credit.map(|(decimal, subpage)| levels::link(page_number(decimal), Some(subpage)));
        [at(previous), at(next), Some(levels::link(page_number(self.plan.menu), None)), credit, Some(levels::link(FIRST_PAGE, None))]
    }

    /// A picture's page: its levels as sub-pages, or, when none reads, a
    /// page that says so.
    fn picture(&self, index: usize, shown: &Shown) -> Vec<PageFile> {
        let number = page_number(shown.number);
        let links = self.picture_links(index, shown);
        let pages = levels::subpages(&shown.levels, number, links);
        if !pages.is_empty() {
            return pages;
        }
        let target = |link: Option<FastextLink>| link.map(|link| link.address.mpp());
        let mut screen = page(
            &shown.title,
            Colour::Red,
            [("Previous", target(links[0])), ("Next", target(links[1])), ("Menu", target(links[2])), ("Credits", target(links[3]))],
        );
        screen.coloured(6, TEXT_COLUMN, Colour::White, "This picture's files could not be read.");
        alloc::vec![shown_page(&screen, number, 0)]
    }

    fn credits_page(&self, credits: &Credits) -> Vec<PageFile> {
        let number = page_number(credits.number);
        let total = credits.subpages.len();
        let menu = Some(page_number(self.plan.menu));
        let first = self.pictures.first().map(|shown| page_number(shown.number));
        let mut pages = Vec::new();
        for (position, entries) in (1_u8..).zip(&credits.subpages) {
            let mut screen = page(&format!("Picture credits {position}/{total}"), Colour::Blue, [("Menu", menu), ("First", first), ("", None), ("", None)]);
            let mut row = CREDITS_FIRST_ROW;
            for (place, shown) in entries.iter().filter_map(|&index| self.pictures.get(index)).enumerate() {
                let colour = if place % 2 == 0 { Colour::White } else { Colour::Cyan };
                screen.coloured(row, TEXT_COLUMN, Colour::Yellow, &format!("{:03}", shown.number));
                let start = row;
                for line in &shown.lines {
                    screen.coloured(row, CREDITS_COLOUR_COLUMN, colour, line);
                    row = row.saturating_add(1);
                }
                row = row.max(start.saturating_add(1));
            }
            pages.push(shown_page(&screen, number, position));
        }
        pages
    }
}

/// `screen` as sub-page `subpage` of page `number`, its sub-code in
/// decimal digits as a picture's are, with its Fastext row shown: a page
/// with links whose X/27/0 does not say "display row 24" has its row 24
/// hidden (EN 300 706 Table 14, the link control byte's bit 4).
fn shown_page(screen: &Screen, number: u16, subpage: u8) -> PageFile {
    let mut page = screen.page(number, subpage);
    page.subcode = levels::subcode(subpage);
    page.fastext_row_24 = true;
    page
}

/// A line of a menu's list: a page number in yellow, a name in white.
fn list_line(screen: &mut Screen, row: usize, number: &str, name: &str) {
    screen.coloured(row, TEXT_COLUMN, Colour::Yellow, number);
    screen.coloured(row, NAME_COLUMN, Colour::White, name);
}

/// Places each picture's caption on a credits sub-page, filling each in
/// turn, and gives each picture its credit; the pages run from the plan's
/// first credits page, each with at most [`CREDITS_SUBPAGES`].
fn credits(pictures: &mut [Shown], plan: Plan) -> Vec<Credits> {
    let mut pages: Vec<Credits> = Vec::new();
    let mut used = CREDITS_LINES;
    for (index, shown) in pictures.iter_mut().enumerate() {
        let lines = shown.lines.len().clamp(1, CREDITS_LINES);
        if used.saturating_add(lines) > CREDITS_LINES {
            if pages.last().is_none_or(|page| page.subpages.len() >= CREDITS_SUBPAGES) {
                let number = pages.last().map_or(plan.first_credits, |page| page.number.saturating_add(1));
                if number > plan.last_credits {
                    break;
                }
                pages.push(Credits { number, subpages: Vec::new() });
            }
            if let Some(page) = pages.last_mut() {
                page.subpages.push(Vec::new());
            }
            used = 0;
        }
        if let Some(page) = pages.last_mut()
            && let Some(subpage) = page.subpages.last_mut()
        {
            subpage.push(index);
            shown.credit = u8::try_from(page.subpages.len()).ok().map(|position| (page.number, position));
        }
        used = used.saturating_add(lines);
    }
    pages
}

impl Application for Gallery {
    fn pages(&self) -> Vec<PageEntry> {
        let mut entries = alloc::vec![PageEntry::new(page_number(self.plan.menu), self.title)];
        for shown in &self.pictures {
            entries.push(PageEntry::new(page_number(shown.number), &shown.title));
        }
        if let Some(decimal) = self.plan.levels {
            entries.push(PageEntry::new(page_number(decimal), "Levels compared"));
        }
        for (position, credits) in self.credits.iter().enumerate() {
            let title = if position == 0 { String::from("Picture credits") } else { format!("Picture credits {}", position.saturating_add(1)) };
            entries.push(PageEntry::new(page_number(credits.number), &title));
        }
        entries
    }

    fn start(&mut self, context: &mut Context<'_>) {
        for (number, subpages) in self.published() {
            context.publish_subpages(Owner::Channel, number, subpages);
        }
    }
}
