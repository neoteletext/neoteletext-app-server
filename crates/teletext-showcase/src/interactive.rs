//! User input (pages 200 to 299): a form with a masked field, a shared
//! guestbook, a live poll, a quiz, a calculator and a mouse paint box.
//!
//! One application serves every page of the area, so a viewer is sent the
//! sound library once however many of its pages they use, and the shared
//! pages' republishing shares one wake.
//!
//! **Scale** (decision 7): the guestbook and the poll are the channel's
//! pages, published once per change whatever the number of viewers, and a
//! burst of changes is coalesced into at most one publication each
//! [`REPUBLISH_INTERVAL_MS`]. What is one viewer's own (a half-typed
//! field, a quiz score, a canvas) is small and forgotten when the session
//! ends.

use alloc::boxed::Box;
use alloc::vec::Vec;

use teletext_runtime::{Application, Context, Input, Owner, PageEntry, SessionId};

use crate::channels::Picture;
use crate::index::Section;
use crate::sound::Sounds;

mod calculator;
mod field;
mod form;
mod guestbook;
mod menu;
mod paint;
mod poll;
mod quiz;
#[cfg(test)]
mod tests;

/// The area's menu.
pub const MENU: u16 = 0x200;
/// A form: a name and a masked field.
pub const FORM: u16 = 0x201;
/// The guestbook everyone sees.
pub const GUESTBOOK: u16 = 0x202;
/// The live poll's results, where viewers vote.
pub const POLL: u16 = 0x203;
/// The quiz.
pub const QUIZ: u16 = 0x204;
/// The calculator.
pub const CALCULATOR: u16 = 0x205;
/// The mouse paint box.
pub const PAINT: u16 = 0x206;
/// Signing the guestbook: each viewer's own field, since a shared page
/// cannot hold what one viewer is typing.
pub const SIGN: u16 = 0x207;

/// The shortest time between two publications of one shared page: four a
/// second is live to a reader and bounds what a burst of posts or votes
/// costs every viewer.
pub const REPUBLISH_INTERVAL_MS: u64 = 250;

/// This area's applications.
#[must_use]
pub fn applications(_pictures: &[Picture]) -> Vec<Box<dyn Application>> {
    alloc::vec![Box::new(Interactive::new())]
}

/// This area's entries in the index.
#[must_use]
pub fn sections() -> Vec<Section> {
    alloc::vec![Section { page: MENU, name: "User input", about: "guestbook, poll, quiz, paint" }]
}

/// When a shared page may next be published: at once if it has rested
/// [`REPUBLISH_INTERVAL_MS`], else once at the end of that time, however
/// many changes come in between.
#[derive(Default)]
struct Republish {
    last: Option<u64>,
    pending: bool,
}

impl Republish {
    /// The time the page may next be published.
    fn due(&self) -> u64 {
        self.last.map_or(0, |last| last.saturating_add(REPUBLISH_INTERVAL_MS))
    }

    /// The page changed: whether to publish it now. If not, a wake is
    /// asked for its due time and the change waits for it.
    fn changed(&mut self, context: &mut Context<'_>) -> bool {
        if self.pending {
            return false;
        }
        let due = self.due();
        if context.now() >= due {
            self.last = Some(context.now());
            return true;
        }
        self.pending = true;
        context.wake_at(due);
        false
    }

    /// A wake came: whether a waiting change is due now. A change still
    /// waiting asks for its wake again, since the runtime keeps one wake
    /// for the application and this one may have been for another page.
    fn woken(&mut self, context: &mut Context<'_>) -> bool {
        if !self.pending {
            return false;
        }
        let due = self.due();
        if context.now() >= due {
            self.pending = false;
            self.last = Some(context.now());
            return true;
        }
        context.wake_at(due);
        false
    }
}

/// The application behind pages 200 to 207.
pub struct Interactive {
    form: form::Form,
    guestbook: guestbook::Guestbook,
    poll: poll::Poll,
    quiz: quiz::Quiz,
    calculator: calculator::Calculator,
    paint: paint::Paint,
    sounds: Sounds,
}

impl Default for Interactive {
    fn default() -> Self {
        Self::new()
    }
}

impl Interactive {
    /// The area, with nothing typed, posted or voted yet.
    #[must_use]
    pub fn new() -> Self {
        Self {
            form: form::Form::default(),
            guestbook: guestbook::Guestbook::default(),
            poll: poll::Poll::default(),
            quiz: quiz::Quiz::default(),
            calculator: calculator::Calculator::default(),
            paint: paint::Paint::default(),
            sounds: Sounds::new(),
        }
    }
}

impl Application for Interactive {
    fn pages(&self) -> Vec<PageEntry> {
        alloc::vec![
            PageEntry::new(MENU, "User input"),
            PageEntry::new(FORM, "A form"),
            PageEntry::new(GUESTBOOK, "Guestbook"),
            PageEntry::new(POLL, "Live poll"),
            PageEntry::new(QUIZ, "Teletext quiz"),
            PageEntry::new(CALCULATOR, "Calculator"),
            PageEntry::new(PAINT, "Mouse paint"),
            PageEntry::new(SIGN, "Sign the guestbook"),
        ]
    }

    fn start(&mut self, context: &mut Context<'_>) {
        context.publish(Owner::Channel, MENU, &menu::screen());
        context.publish(Owner::Channel, FORM, &form::Form::first_screen());
        context.publish(Owner::Channel, QUIZ, &quiz::Quiz::first_screen());
        context.publish(Owner::Channel, CALCULATOR, &calculator::Calculator::first_screen());
        context.publish(Owner::Channel, PAINT, &paint::Paint::first_screen());
        context.publish(Owner::Channel, SIGN, &guestbook::Guestbook::first_sign_screen());
        self.guestbook.start(context);
        self.poll.start(context);
    }

    fn opened(&mut self, session: SessionId, page: u16, context: &mut Context<'_>) {
        match page {
            FORM => self.form.opened(session, context),
            QUIZ => self.quiz.opened(session, context),
            CALCULATOR => self.calculator.opened(session, context),
            PAINT => self.paint.opened(session, context),
            SIGN => self.guestbook.opened_sign(session, context),
            _ => {}
        }
    }

    fn input(&mut self, session: SessionId, page: u16, input: &Input<'_>, context: &mut Context<'_>) {
        let sounds = &mut self.sounds;
        match page {
            FORM => self.form.input(session, input, sounds, context),
            GUESTBOOK | SIGN => self.guestbook.input(session, page, input, sounds, context),
            POLL => self.poll.input(session, input, sounds, context),
            QUIZ => self.quiz.input(session, input, sounds, context),
            CALCULATOR => self.calculator.input(session, input, sounds, context),
            PAINT => self.paint.input(session, input, sounds, context),
            _ => {}
        }
    }

    fn wake(&mut self, context: &mut Context<'_>) {
        self.guestbook.wake(context);
        self.poll.wake(context);
    }

    fn ended(&mut self, session: SessionId, context: &mut Context<'_>) {
        self.form.ended(session);
        self.guestbook.ended(session);
        self.poll.ended(session, context);
        self.quiz.ended(session);
        self.calculator.ended(session);
        self.paint.ended(session);
        self.sounds.forget(session);
    }
}
