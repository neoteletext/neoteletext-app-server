//! Applications: the programs behind a channel's live and interactive
//! pages (`SERVER.md` section 7). Here they run in the server's process,
//! the in-process binding of section 7.1, as plain state machines like
//! the runtime: they are called with what happened and answer through a
//! [`Context`], so they run unchanged on every host.

use alloc::string::String;
use alloc::vec::Vec;

use neoteletext_formats::page::PageFile;

use crate::channel::SessionId;
use crate::screen::Screen;

/// Who a publication belongs to (`SERVER.md` section 2: ownership decides
/// sharing).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Owner {
    /// The channel's: one shared copy, sent to every viewer.
    Channel,
    /// One session's: its viewer alone.
    Session(SessionId),
}

/// Who hears a sound.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Audience {
    /// One session.
    Session(SessionId),
    /// Every session with this page open.
    Viewers(u16),
}

/// One page an application serves, for the channel's contents.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct PageEntry {
    /// The page, such as `0x100`.
    pub number: u16,
    /// Its title, at most 32 printable characters (the engine's
    /// `contents.txt` rule).
    pub title: String,
}

impl PageEntry {
    /// An entry.
    #[must_use]
    pub fn new(number: u16, title: &str) -> Self {
        Self { number, title: String::from(title) }
    }
}

/// What an application asks of the runtime.
#[derive(Debug)]
pub(crate) enum Action {
    /// Publish a page, all its sub-pages.
    Publish { owner: Owner, number: u16, subpages: Vec<PageFile> },
    /// Send audio records.
    Records { audience: Audience, records: Vec<Vec<u8>> },
    /// Send the session's viewer to a page.
    Goto { session: SessionId, page: u16 },
    /// Call `wake` at this time.
    WakeAt(u64),
}

/// The application's view of the runtime during one call.
pub struct Context<'a> {
    pub(crate) now: u64,
    pub(crate) actions: &'a mut Vec<Action>,
    pub(crate) seed: &'a mut u64,
}

impl Context<'_> {
    /// The time, in milliseconds, as the host gave it.
    #[must_use]
    pub const fn now(&self) -> u64 {
        self.now
    }

    /// Publishes a page drawn on a screen, as its one sub-page.
    pub fn publish(&mut self, owner: Owner, number: u16, screen: &Screen) {
        self.publish_subpages(owner, number, alloc::vec![screen.page(number, 0)]);
    }

    /// Publishes a page with its sub-pages, each numbered as given.
    pub fn publish_subpages(&mut self, owner: Owner, number: u16, subpages: Vec<PageFile>) {
        self.actions.push(Action::Publish { owner, number, subpages });
    }

    /// Sends audio records (`neoteletext_audio::encode`): sound objects,
    /// their data, commands and live records.
    pub fn sound(&mut self, audience: Audience, records: Vec<Vec<u8>>) {
        if !records.is_empty() {
            self.actions.push(Action::Records { audience, records });
        }
    }

    /// Sends the session's viewer to `page`, as a channel that takes input
    /// may (INPUT.md section 7.1, REQ-INT-113).
    pub fn goto(&mut self, session: SessionId, page: u16) {
        self.actions.push(Action::Goto { session, page });
    }

    /// Asks for `wake` at `at`, in milliseconds.
    pub fn wake_at(&mut self, at: u64) {
        self.actions.push(Action::WakeAt(at));
    }

    /// Asks for `wake` in `delay` milliseconds.
    pub fn wake_in(&mut self, delay: u64) {
        let at = self.now.saturating_add(delay);
        self.wake_at(at);
    }

    /// A pseudo-random number, from the seed the host gave the channel
    /// (xorshift64*, Vigna 2016): for games, never for secrets.
    pub fn random(&mut self) -> u32 {
        let mut state = *self.seed;
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        *self.seed = state;
        // The multiplier of xorshift64*.
        let mixed = state.wrapping_mul(0x2545_F491_4F6C_DD1D);
        u32::try_from(mixed >> 32).unwrap_or(0)
    }

    /// A pseudo-random number below `bound` (0 when `bound` is 0).
    pub fn below(&mut self, bound: u32) -> u32 {
        self.random().checked_rem(bound).unwrap_or(0)
    }
}

/// A program behind some of a channel's pages.
pub trait Application: Send {
    /// The pages it serves, for the channel's contents and to route input.
    fn pages(&self) -> Vec<PageEntry>;

    /// The channel started: publish the shared pages.
    fn start(&mut self, context: &mut Context<'_>);

    /// A session began: a viewer connected.
    fn joined(&mut self, _session: SessionId, _context: &mut Context<'_>) {}

    /// The session's viewer opened one of this application's pages.
    fn opened(&mut self, _session: SessionId, _page: u16, _context: &mut Context<'_>) {}

    /// The session's viewer left one of this application's pages.
    fn left_page(&mut self, _session: SessionId, _page: u16, _context: &mut Context<'_>) {}

    /// Input on one of this application's pages, open with the keys.
    fn input(&mut self, _session: SessionId, _page: u16, _input: &crate::input::Input<'_>, _context: &mut Context<'_>) {}

    /// The time asked for with [`Context::wake_at`] has come.
    fn wake(&mut self, _context: &mut Context<'_>) {}

    /// The session ended: forget it.
    fn ended(&mut self, _session: SessionId, _context: &mut Context<'_>) {}
}
