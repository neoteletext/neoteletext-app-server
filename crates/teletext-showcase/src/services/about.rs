//! Page 108, about NeoTeletext, and page 109, facts about teletext: two
//! carousels that never change, so they are published once.
//!
//! The facts are kept to what is well established about teletext and its
//! standard, ETSI EN 300 706.

use alloc::vec::Vec;

use teletext_runtime::screen::{Colour, Screen};
use teletext_runtime::{Application, Context, PageEntry};

use super::draw::{FIRST_ROW, counter, next, paragraph, publish_carousel};
use super::{ABOUT, FACTS, navigation};
use crate::common::page;

/// A sub-page of text: its heading and its paragraphs.
struct Topic {
    heading: &'static str,
    paragraphs: &'static [&'static str],
}

/// Page 108's sub-pages.
const ABOUT_TOPICS: [Topic; 4] = [
    Topic {
        heading: "Teletext for the web",
        paragraphs: &[
            "NeoTeletext brings teletext to the web: channels of pages, pushed live to a teletext browser as broadcasters once sent them over the air.",
            "It has three parts: the engine, the browser and the server. The next sub-pages take each in turn.",
            "Teletext is push: a page arrives when it changes, whether or not you are looking at it.",
        ],
    },
    Topic {
        heading: "The engine",
        paragraphs: &[
            "The engine decodes and renders teletext as ETSI EN 300 706 defines it, at Levels 1, 1.5, 2.5 and 3.5.",
            "It reads T42 packet files, transport streams, sliced VBI and TTI page files, and carries the input protocol and sound.",
            "It is written in Rust, and its core libraries use no floating point.",
        ],
    },
    Topic {
        heading: "The browser",
        paragraphs: &[
            "A desktop browser for teletext channels on the web, for macOS, Linux and Windows, built on the engine.",
            "Its parts run as separate processes, each confined by the operating system. It has no script engine.",
            "Tabs, history, bookmarks, sound, and a phone as a keyboard.",
        ],
    },
    Topic {
        heading: "The server",
        paragraphs: &[
            "Applications publish pages; the server sends each viewer the pages as 42-byte packets over a WebSocket.",
            "A shared page is encoded once for every viewer. A viewer's own page, such as a game, goes to that viewer alone.",
            "Its runtime does no input or output itself, so one code runs natively and builds for WebAssembly.",
        ],
    },
];

/// Page 109's sub-pages.
const FACTS_TOPICS: [Topic; 9] = [
    Topic {
        heading: "Ceefax, 1974 to 2012",
        paragraphs: &[
            "Ceefax, the BBC's teletext service, began in 1974. ITV's service, ORACLE, also began in the 1970s.",
            "Ceefax closed in 2012, when the United Kingdom switched off analogue television.",
            "Teletext is still broadcast in several European countries, Germany among them.",
        ],
    },
    Topic {
        heading: "Between the pictures",
        paragraphs: &[
            "Teletext travels in the vertical blanking interval: lines of each television frame that carry no picture.",
            "A receiver picks the pages out of that stream as they pass.",
        ],
    },
    Topic {
        heading: "Forty by twenty-four",
        paragraphs: &[
            "A page is 40 characters wide and 24 rows deep, the first row being the header: the page number, the service's name and usually the time.",
            "A row below them, row 24, carries Fastext prompts.",
        ],
    },
    Topic {
        heading: "Pages 100 to 899",
        paragraphs: &[
            "Pages are numbered from 100 to 899. The first digit is the magazine, one of eight.",
            "The other two digits are sent in hexadecimal, so some pages, such as 1FF, cannot be keyed in on a remote control.",
        ],
    },
    Topic {
        heading: "The carousel",
        paragraphs: &[
            "Pages are sent one after another, round and round. A receiver waits for the page asked for to come round.",
            "A page can have sub-pages, sent in turn, which the receiver shows as they arrive.",
        ],
    },
    Topic {
        heading: "Mosaic graphics",
        paragraphs: &[
            "Pictures are drawn with mosaic characters, which divide a character cell into six blocks, two across and three down.",
            "Every map and chart on these pages is drawn that way.",
        ],
    },
    Topic {
        heading: "Eight colours",
        paragraphs: &[
            "Level 1 has eight colours: black, red, green, yellow, blue, magenta, cyan and white.",
            "Attributes add flash, double height, conceal, and graphics that are contiguous or separated.",
        ],
    },
    Topic {
        heading: "Fastext",
        paragraphs: &[
            "Fastext gives four coloured keys, linked to pages the editor chooses.",
            "A receiver can fetch the linked pages ahead, so they show at once.",
        ],
    },
    Topic {
        heading: "Checked as it goes",
        paragraphs: &[
            "Characters are sent with odd parity, so a receiver can spot a damaged one.",
            "Page addresses and headers are Hamming coded, so a single wrong bit is corrected.",
        ],
    },
];

/// The column of a heading's double-height attribute.
const HEADING_COLUMN: usize = 1;

/// Draws `topics` as sub-pages under `title`.
fn carousel(number: u16, title: &str, band: Colour, topics: &[Topic]) -> Vec<Screen> {
    let count = topics.len();
    topics
        .iter()
        .zip(1_usize..)
        .map(|(topic, subpage)| {
            let mut screen = page(title, band, navigation(number));
            counter(&mut screen, subpage, count);
            screen.double_height(FIRST_ROW.saturating_add(1), HEADING_COLUMN, Colour::Yellow, topic.heading);
            let mut row = FIRST_ROW.saturating_add(4);
            for (text, colour) in topic.paragraphs.iter().zip([Colour::White, Colour::Cyan].into_iter().cycle()) {
                row = paragraph(&mut screen, row, colour, text).saturating_add(1);
            }
            // The last sub-page leads back to the first.
            let following = topics.get(subpage).or(topics.first()).map_or("", |topic| topic.heading);
            next(&mut screen, following);
            screen
        })
        .collect()
}

/// Page 108's sub-pages.
#[must_use]
pub fn about_screens() -> Vec<Screen> {
    carousel(ABOUT, "About NeoTeletext", Colour::Magenta, &ABOUT_TOPICS)
}

/// Page 109's sub-pages.
#[must_use]
pub fn facts_screens() -> Vec<Screen> {
    carousel(FACTS, "Teletext facts", Colour::Green, &FACTS_TOPICS)
}

/// The two pages.
pub struct About;

impl Application for About {
    fn pages(&self) -> Vec<PageEntry> {
        alloc::vec![PageEntry::new(ABOUT, "About NeoTeletext"), PageEntry::new(FACTS, "Teletext facts")]
    }

    fn start(&mut self, context: &mut Context<'_>) {
        publish_carousel(context, ABOUT, &about_screens());
        publish_carousel(context, FACTS, &facts_screens());
    }
}
