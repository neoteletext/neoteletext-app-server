//! Page 201: a form of two fields, a name and a masked one, each viewer's
//! own. It shows what the page asks of the receiver: the cursor record's
//! flags and width, which turn the receiver's echo on (INPUT.md section
//! 7.1), and the page's input modes.

use alloc::collections::BTreeMap;
use alloc::format;

use neoteletext_input::cursor::{ECHO, MASKED};
use teletext_runtime::screen::{Colour, Screen};
use teletext_runtime::{Context, Input, Owner, SessionId};

use super::field::{BAND_LEAD, Edit, Field, band, codes_of, printable};
use super::{FORM, GUESTBOOK, MENU};
use crate::common::page;
use crate::index::INDEX;
use crate::sound::{Effect, Sounds};

/// The longest name.
const NAME_LIMIT: usize = 16;
/// The longest secret.
const SECRET_LIMIT: usize = 16;
/// The character a masked field shows for each one typed, as the
/// receiver's echo does (INPUT.md section 9.3).
const MASK: char = '*';
/// The name's row.
const NAME_ROW: usize = 5;
/// The secret's row.
const SECRET_ROW: usize = 7;
/// Where each field's band starts.
const BAND_COLUMN: usize = 11;
/// Where each field's text starts.
const TEXT_COLUMN: usize = BAND_COLUMN + BAND_LEAD;

/// Which field has the cursor.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Focus {
    Name,
    Secret,
}

/// What the last Enter did.
#[derive(Clone, Debug)]
enum Answer {
    /// Nothing yet.
    None,
    /// No name was typed.
    NoName,
    /// The greeting, with how long the secret was.
    Greeted { secret_length: usize },
}

/// One viewer's form.
struct State {
    name: Field,
    secret: Field,
    focus: Focus,
    /// Characters the page accepted, to show the echo answered.
    typed: u32,
    answer: Answer,
}

impl State {
    const fn new() -> Self {
        Self { name: Field::new(NAME_LIMIT), secret: Field::new(SECRET_LIMIT), focus: Focus::Name, typed: 0, answer: Answer::None }
    }

    const fn focused(&mut self) -> &mut Field {
        match self.focus {
            Focus::Name => &mut self.name,
            Focus::Secret => &mut self.secret,
        }
    }

    /// Acts on one edit: whether it changed what the page shows.
    fn edit(&mut self, edit: Edit<'_>) -> Option<Effect> {
        match edit {
            Edit::Text(text) => {
                let kept = self.focused().type_text(text, printable);
                self.typed = self.typed.saturating_add(u32::try_from(kept).unwrap_or(u32::MAX));
                Some(Effect::Click)
            }
            Edit::Character(character) => {
                let mut buffer = [0_u8; 4];
                self.edit(Edit::Text(character.encode_utf8(&mut buffer)))
            }
            Edit::Backspace => self.focused().backspace().then_some(Effect::Click),
            Edit::Tab => {
                self.focus = if self.focus == Focus::Name { Focus::Secret } else { Focus::Name };
                Some(Effect::Click)
            }
            Edit::Escape => {
                self.focused().clear();
                Some(Effect::Click)
            }
            Edit::Enter => {
                if self.name.is_blank() {
                    self.answer = Answer::NoName;
                    self.focus = Focus::Name;
                    Some(Effect::Hit)
                } else {
                    self.answer = Answer::Greeted { secret_length: self.secret.length() };
                    Some(Effect::Bell)
                }
            }
            Edit::Other => None,
        }
    }

    fn screen(&self) -> Screen {
        let mut screen = page("A form", Colour::Blue, [("Input", Some(MENU)), ("Guestbook", Some(GUESTBOOK)), ("", None), ("Index", Some(INDEX))]);
        screen.coloured(3, 1, Colour::Cyan, "Type, then Tab to the next field");
        let masked: alloc::string::String = core::iter::repeat_n(MASK, self.secret.length()).collect();
        for (row, label, focus, codes) in
            [(NAME_ROW, "Name", Focus::Name, codes_of(self.name.text())), (SECRET_ROW, "Secret", Focus::Secret, codes_of(&masked))]
        {
            let here = self.focus == focus;
            let (marker, ink) = if here { ("\u{2192}", Colour::Yellow) } else { (" ", Colour::White) };
            screen.coloured(row, 0, ink, marker);
            screen.text(row, 3, label);
            let limit = if focus == Focus::Name { NAME_LIMIT } else { SECRET_LIMIT };
            band(&mut screen, row, BAND_COLUMN, limit, Colour::Blue, Colour::Yellow, &codes);
        }
        let (field, row, flags, flag_names) = match self.focus {
            Focus::Name => (&self.name, NAME_ROW, ECHO, "ECHO"),
            Focus::Secret => (&self.secret, SECRET_ROW, ECHO | MASKED, "ECHO + MASKED"),
        };
        field.cursor(&mut screen, row, TEXT_COLUMN, flags);
        let width = field.room();
        screen.coloured(10, 1, Colour::Cyan, "What this page asks of the receiver");
        screen.coloured(11, 1, Colour::White, "Cursor:");
        screen.coloured(11, 13, Colour::Yellow, &format!("{flag_names}, width {width}"));
        screen.coloured(12, 1, Colour::White, "Page modes:");
        screen.coloured(12, 13, Colour::Yellow, "none: text and keys");
        screen.coloured(13, 1, Colour::White, "Accepted:");
        screen.coloured(13, 13, Colour::Yellow, &format!("{} characters", self.typed));
        screen.coloured(14, 1, Colour::Green, "Your keys echo at once in amber; the");
        screen.coloured(15, 1, Colour::Green, "page's own text replaces the echo.");
        match self.answer {
            Answer::None => {}
            Answer::NoName => screen.coloured(17, 1, Colour::Red, "Type a name first, then press Enter"),
            Answer::Greeted { secret_length } => {
                screen.double_height(17, 1, Colour::Yellow, &format!("Hello, {}!", self.name.text().trim()));
                screen.coloured(19, 1, Colour::White, &format!("Your secret has {secret_length} characters;"));
                screen.coloured(20, 1, Colour::White, "the page only ever showed stars.");
            }
        }
        screen.coloured(22, 1, Colour::Magenta, "Tab next  Backspace  Esc clears");
        screen.coloured(23, 1, Colour::Magenta, "Enter submits");
        screen
    }
}

/// Every viewer's form.
#[derive(Default)]
pub(super) struct Form {
    sessions: BTreeMap<SessionId, State>,
}

impl Form {
    /// The page as a viewer first sees it, which is also the channel's
    /// copy, for a receiver that shows the page before saying it opened it.
    pub(super) fn first_screen() -> Screen {
        State::new().screen()
    }

    fn publish(&self, session: SessionId, context: &mut Context<'_>) {
        if let Some(state) = self.sessions.get(&session) {
            context.publish(Owner::Session(session), FORM, &state.screen());
        }
    }

    pub(super) fn opened(&mut self, session: SessionId, context: &mut Context<'_>) {
        self.sessions.entry(session).or_insert_with(State::new);
        self.publish(session, context);
    }

    pub(super) fn input(&mut self, session: SessionId, input: &Input<'_>, sounds: &mut Sounds, context: &mut Context<'_>) {
        let state = self.sessions.entry(session).or_insert_with(State::new);
        if let Some(effect) = state.edit(Edit::of(input)) {
            self.publish(session, context);
            sounds.play(session, effect, context);
        }
    }

    pub(super) fn ended(&mut self, session: SessionId) {
        self.sessions.remove(&session);
    }
}
