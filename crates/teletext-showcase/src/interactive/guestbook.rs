//! Page 202, the guestbook every viewer sees, and page 207, where each
//! viewer types a line for it.
//!
//! The book is the channel's page, published once per change (decision 7).
//! What a viewer is typing is theirs, so it is on a page of their own:
//! typing on the book starts a line there and sends them to it; Enter
//! posts the line and sends them back to the book.
//!
//! What viewers type reaches every viewer, so it is bounded: a line is at
//! most [`MESSAGE_LIMIT`] characters the page can show, the book keeps the
//! newest [`ENTRIES_KEPT`], and a session posts at most once each
//! [`POST_INTERVAL_MS`].

use alloc::collections::{BTreeMap, VecDeque};
use alloc::format;
use alloc::string::String;

use neoteletext_input::cursor::ECHO;
use teletext_runtime::screen::{Colour, Screen};
use teletext_runtime::{Context, Input, Owner, SessionId};

use super::field::{BAND_LEAD, Edit, Field, band, codes_of, printable};
use super::{GUESTBOOK, MENU, POLL, Republish, SIGN};
use crate::common::page;
use crate::index::INDEX;
use crate::sound::{Effect, Sounds};

/// The longest line.
pub(super) const MESSAGE_LIMIT: usize = 30;
/// The lines the book keeps, one to a row.
pub(super) const ENTRIES_KEPT: usize = 14;
/// The shortest time between two posts of one session.
pub(super) const POST_INTERVAL_MS: u64 = 5_000;
/// The book's first line's row.
const FIRST_ROW: usize = 4;
/// The field's row on the signing page.
const FIELD_ROW: usize = 6;
/// Where the field's band starts.
const BAND_COLUMN: usize = 1;
/// Milliseconds in a minute and minutes in a day, for the time of a post.
const MINUTE_MS: u64 = 60_000;
/// Minutes in a day.
const DAY_MINUTES: u64 = 1_440;
/// Minutes in an hour.
const HOUR_MINUTES: u64 = 60;
/// Milliseconds in a second, for the wait a viewer is told.
const SECOND_MS: u64 = 1_000;

/// One line of the book.
struct Entry {
    /// The time it was posted, `HH:MM` UTC.
    time: String,
    text: String,
}

/// What the signing page last said.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Note {
    None,
    /// Enter with nothing typed.
    Empty,
    /// Posted too soon after the last post: the seconds left.
    Wait(u64),
}

/// One session's line and when it last posted.
struct Signer {
    field: Field,
    last_post: Option<u64>,
    note: Note,
}

impl Signer {
    const fn new() -> Self {
        Self { field: Field::new(MESSAGE_LIMIT), last_post: None, note: Note::None }
    }
}

/// The book and its signers.
#[derive(Default)]
pub(super) struct Guestbook {
    entries: VecDeque<Entry>,
    signers: BTreeMap<SessionId, Signer>,
    republish: Republish,
}

/// The time of day of `epoch_ms`, `HH:MM` UTC.
fn time_of_day(epoch_ms: u64) -> String {
    let minutes = (epoch_ms / MINUTE_MS) % DAY_MINUTES;
    format!("{:02}:{:02}", minutes / HOUR_MINUTES, minutes % HOUR_MINUTES)
}

impl Guestbook {
    pub(super) fn start(&self, context: &mut Context<'_>) {
        context.publish(Owner::Channel, GUESTBOOK, &self.book());
    }

    /// The signing page as a viewer first sees it, which is also the
    /// channel's copy.
    pub(super) fn first_sign_screen() -> Screen {
        Self::sign(&Signer::new())
    }

    /// The book's page.
    fn book(&self) -> Screen {
        let mut screen = page("Guestbook", Colour::Red, [("Input", Some(MENU)), ("Sign it", Some(SIGN)), ("Poll", Some(POLL)), ("Index", Some(INDEX))]);
        if self.entries.is_empty() {
            screen.centred(8, Colour::Yellow, "No messages yet: be the first!");
        }
        let mut row = FIRST_ROW;
        for (number, entry) in self.entries.iter().enumerate() {
            let ink = if number % 2 == 0 { Colour::White } else { Colour::Yellow };
            screen.coloured(row, 0, Colour::Cyan, &entry.time);
            screen.coloured(row, 6, ink, &entry.text);
            row = row.saturating_add(1);
        }
        screen.coloured(19, 1, Colour::Green, "Start typing to sign the book");
        screen.coloured(20, 1, Colour::White, &format!("One line of {MESSAGE_LIMIT} characters, one every"));
        screen.coloured(21, 1, Colour::White, &format!("{} seconds each; newest at the top.", POST_INTERVAL_MS / SECOND_MS));
        screen.coloured(22, 1, Colour::Cyan, "One shared page: every viewer sees it");
        screen
    }

    /// One session's signing page.
    fn sign(signer: &Signer) -> Screen {
        let mut screen = page("Sign the guestbook", Colour::Red, [("Input", Some(MENU)), ("The book", Some(GUESTBOOK)), ("", None), ("Index", Some(INDEX))]);
        screen.coloured(4, 1, Colour::Cyan, &format!("Your message, up to {MESSAGE_LIMIT} characters:"));
        band(&mut screen, FIELD_ROW, BAND_COLUMN, MESSAGE_LIMIT, Colour::Red, Colour::Yellow, &codes_of(signer.field.text()));
        signer.field.cursor(&mut screen, FIELD_ROW, BAND_COLUMN.saturating_add(BAND_LEAD), ECHO);
        match signer.note {
            Note::None => {}
            Note::Empty => screen.coloured(8, 1, Colour::Red, "Type something first"),
            Note::Wait(seconds) => screen.coloured(8, 1, Colour::Red, &format!("Wait {seconds} more seconds to post")),
        }
        screen.coloured(10, 1, Colour::White, "Enter posts it to the book, where");
        screen.coloured(11, 1, Colour::White, "every viewer sees it at once.");
        screen.coloured(13, 1, Colour::White, "Letters, digits and punctuation the");
        screen.coloured(14, 1, Colour::White, "page can show are kept; the rest go.");
        screen.coloured(22, 1, Colour::Magenta, "Backspace deletes  Esc goes back");
        screen
    }

    fn publish_sign(&self, session: SessionId, context: &mut Context<'_>) {
        if let Some(signer) = self.signers.get(&session) {
            context.publish(Owner::Session(session), SIGN, &Self::sign(signer));
        }
    }

    pub(super) fn opened_sign(&mut self, session: SessionId, context: &mut Context<'_>) {
        self.signers.entry(session).or_insert_with(Signer::new);
        self.publish_sign(session, context);
    }

    /// Input on the book or the signing page: both edit the session's line,
    /// and input on the book sends the viewer to the signing page.
    pub(super) fn input(&mut self, session: SessionId, page: u16, input: &Input<'_>, sounds: &mut Sounds, context: &mut Context<'_>) {
        let edit = Edit::of(input);
        let signer = self.signers.entry(session).or_insert_with(Signer::new);
        let effect = match edit {
            Edit::Text(text) => {
                signer.field.type_text(text, printable);
                signer.note = Note::None;
                Some(Effect::Click)
            }
            Edit::Character(character) => {
                let mut buffer = [0_u8; 4];
                signer.field.type_text(character.encode_utf8(&mut buffer), printable);
                signer.note = Note::None;
                Some(Effect::Click)
            }
            Edit::Backspace => signer.field.backspace().then_some(Effect::Click),
            Edit::Escape => {
                if page == SIGN {
                    context.goto(session, GUESTBOOK);
                }
                return;
            }
            Edit::Enter if page == GUESTBOOK && signer.field.is_blank() => Some(Effect::Click),
            Edit::Enter => {
                self.post(session, sounds, context);
                return;
            }
            Edit::Tab | Edit::Other => None,
        };
        let Some(effect) = effect else { return };
        self.publish_sign(session, context);
        if page == GUESTBOOK {
            context.goto(session, SIGN);
        }
        sounds.play(session, effect, context);
    }

    /// Posts the session's line to the book, if it may.
    fn post(&mut self, session: SessionId, sounds: &mut Sounds, context: &mut Context<'_>) {
        let now = context.now();
        let Some(signer) = self.signers.get_mut(&session) else { return };
        let since = signer.last_post.map(|last| now.saturating_sub(last));
        if signer.field.is_blank() {
            signer.note = Note::Empty;
        } else if let Some(since) = since.filter(|&since| since < POST_INTERVAL_MS) {
            signer.note = Note::Wait(POST_INTERVAL_MS.saturating_sub(since).div_ceil(SECOND_MS));
        } else {
            let text = String::from(signer.field.text().trim());
            signer.field.clear();
            signer.last_post = Some(now);
            signer.note = Note::None;
            self.entries.push_front(Entry { time: time_of_day(context.epoch_ms()), text });
            self.entries.truncate(ENTRIES_KEPT);
            if self.republish.changed(context) {
                context.publish(Owner::Channel, GUESTBOOK, &self.book());
            }
            self.publish_sign(session, context);
            context.goto(session, GUESTBOOK);
            sounds.play(session, Effect::Bell, context);
            return;
        }
        self.publish_sign(session, context);
        context.goto(session, SIGN);
        sounds.play(session, Effect::Hit, context);
    }

    pub(super) fn wake(&mut self, context: &mut Context<'_>) {
        if self.republish.woken(context) {
            context.publish(Owner::Channel, GUESTBOOK, &self.book());
        }
    }

    pub(super) fn ended(&mut self, session: SessionId) {
        self.signers.remove(&session);
    }
}
