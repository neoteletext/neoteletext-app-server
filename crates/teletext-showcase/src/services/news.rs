//! Page 101: NeoTeletext's own news, as a carousel: the headlines on
//! sub-page 1 and a story on each sub-page after it. The stories are about
//! the project and say only what it does.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use teletext_runtime::screen::{Colour, Screen};
use teletext_runtime::{Application, Context, PageEntry};

use super::draw::{BODY_COLUMN, FIRST_ROW, paragraph, publish_carousel, wrap};
use super::{NEWS, navigation};
use crate::common::page;

/// A story: its headline and its paragraphs.
struct Story {
    headline: &'static str,
    paragraphs: &'static [&'static str],
}

/// The stories, in the order of their sub-pages from 2.
const STORIES: [Story; 6] = [
    Story {
        headline: "Teletext server goes live",
        paragraphs: &[
            "The NeoTeletext server is publishing its first channel: the pages you are reading now.",
            "Applications draw pages as a terminal program draws its screen, and the server sends them to the browser over a WebSocket as 42-byte teletext packets.",
        ],
    },
    Story {
        headline: "One page, any number of viewers",
        paragraphs: &[
            "A page the channel shares is encoded once, and the same bytes go to every viewer.",
            "A slow connection skips to each page's latest version, so no queue grows and memory stays bounded. Scale is measured with a load client, not assumed.",
        ],
    },
    Story {
        headline: "Every part in its own sandbox",
        paragraphs: &[
            "The NeoTeletext browser runs as separate processes: a trusted core, a decoder, a fetcher, an interface and a keyboard.",
            "Each is confined by the operating system on macOS, Linux and Windows. There is no script engine: a page is teletext, never a program.",
        ],
    },
    Story {
        headline: "Type on your phone",
        paragraphs: &[
            "Pages that take input can be typed into from a phone: the browser shows a QR code, and the phone becomes its keyboard.",
            "Text, keys and the cursor travel in the input protocol, which the engine defines and the server reads.",
        ],
    },
    Story {
        headline: "Teletext with sound",
        paragraphs: &[
            "The engine carries sound in teletext packets: short sound objects sent once, then commands to play them, and live audio for radio.",
            "The browser plays it on every desktop. The games and the sound pages of this channel use it.",
        ],
    },
    Story {
        headline: "One runtime for every host",
        paragraphs: &[
            "The server's runtime reads no clock and opens no socket: the host gives it each event with the time, and it answers with what to send.",
            "So the same code runs natively under tokio and builds for WebAssembly, and its tests run with no network.",
        ],
    },
];

/// The title band.
const TITLE: &str = "NeoTeletext News";
/// The headline's width: the double-height colour and height attributes
/// take two cells after the margin.
const HEADLINE_WIDTH: usize = 36;
/// The headlines index's rows per story: its line and a gap.
const INDEX_STEP: usize = 2;
/// The column of a headline in the index, after its sub-page number.
const INDEX_TEXT_COLUMN: usize = 5;
/// The width of a headline in the index.
const INDEX_WIDTH: usize = 33;

/// The news carousel; it never changes, so it is published once.
pub struct News;

/// The carousel's sub-pages: the headlines, then each story.
#[must_use]
pub fn screens() -> Vec<Screen> {
    let count = STORIES.len().saturating_add(1);
    let mut screens = Vec::with_capacity(count);
    let mut index = page(TITLE, Colour::Blue, navigation(NEWS));
    super::draw::counter(&mut index, 1, count);
    index.double_height(FIRST_ROW, BODY_COLUMN, Colour::Yellow, "Headlines");
    let mut row = FIRST_ROW.saturating_add(3);
    for (story, subpage) in STORIES.iter().zip(2_usize..) {
        index.coloured(row, BODY_COLUMN, Colour::Cyan, &format!("{subpage}/{count}"));
        // Headlines are written to fit one line; a longer one is cut.
        index.coloured(row, INDEX_TEXT_COLUMN, Colour::White, wrap(story.headline, INDEX_WIDTH).first().map_or("", String::as_str));
        row = row.saturating_add(INDEX_STEP);
    }
    index.coloured(row.saturating_add(1), BODY_COLUMN, Colour::Green, "The stories follow as sub-pages.");
    screens.push(index);
    for (story, subpage) in STORIES.iter().zip(2_usize..) {
        let mut screen = page(TITLE, Colour::Blue, navigation(NEWS));
        super::draw::counter(&mut screen, subpage, count);
        let mut row = FIRST_ROW.saturating_add(1);
        for line in wrap(story.headline, HEADLINE_WIDTH) {
            screen.double_height(row, BODY_COLUMN, Colour::Yellow, &line);
            row = row.saturating_add(2);
        }
        row = row.saturating_add(1);
        for (text, colour) in story.paragraphs.iter().zip([Colour::White, Colour::Cyan].into_iter().cycle()) {
            row = paragraph(&mut screen, row, colour, text).saturating_add(1);
        }
        let next = STORIES.get(subpage.saturating_sub(1)).map_or("Headlines", |story| story.headline);
        super::draw::next(&mut screen, next);
        screens.push(screen);
    }
    screens
}

impl Application for News {
    fn pages(&self) -> Vec<PageEntry> {
        alloc::vec![PageEntry::new(NEWS, "NeoTeletext News")]
    }

    fn start(&mut self, context: &mut Context<'_>) {
        publish_carousel(context, NEWS, &screens());
    }
}
