//! One channel, as a state machine: the host gives it what happened, with
//! the time, and carries out the [`Effect`]s it answers with.
//!
//! **Scale** (decision 7): a shared page is encoded once, and the
//! channel answers its change with one [`Effect::Shared`], whatever the
//! number of viewers. The host keeps each shared page's latest frame and
//! lets every connection catch up on its own, so a slow connection skips
//! to the latest version. Only what is one session's own is sent to it as
//! [`Effect::Send`].

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use neoteletext_audio::encode::packets;
use neoteletext_formats::page::{CursorLine, PageFile};
use neoteletext_input::control::{Control, packet_of};
use neoteletext_input::cursor::{Cursor, NO_CURSOR, Place};
use neoteletext_input::event::{Event, Item, MAX_STREAM_ITEM, StreamDecoder, units};
use neoteletext_input::modes::{ALL_KEYS, BASE_KEY, COMPOSE, CURSOR_CAP, ECHO_CAP, KEY_RELEASE, MOUSE_ALL, MOUSE_BUTTONS, MOUSE_DRAG, PAGE_CAP};

use crate::application::{Action, Application, Audience, Context, Owner, PageEntry};
use crate::frame::page_frame;
use crate::input::Input;
use crate::session::Session;
use crate::stats::{ChannelStats, count};

/// A connection's number, which the host chooses and never reuses.
pub type SessionId = u64;

/// The service number audio and input records are sent on (the
/// reference channel's, INPUT.md section 7).
const SERVICE: u8 = 0;

/// What the channel uses of what a receiver offers in HELLO: every mode
/// its pages may ask for, since a page's mode not agreed in WELCOME is
/// masked off (INPUT.md section 6).
const USED: u32 = PAGE_CAP | CURSOR_CAP | ECHO_CAP | ALL_KEYS | KEY_RELEASE | BASE_KEY | MOUSE_BUTTONS | MOUSE_DRAG | MOUSE_ALL | COMPOSE;

/// A channel's description, for `channel.json`.
#[derive(Clone, Debug)]
pub struct ChannelConfig {
    /// Its name.
    pub name: String,
    /// The push WebSocket's address, on the channel's own origin (the
    /// engine's descriptor rule).
    pub push_address: String,
    /// The page a viewer starts on.
    pub initial_page: u16,
    /// The time the host's clock started, in milliseconds since the Unix
    /// epoch, so applications can show the real time (the host's clock
    /// itself starts at 0).
    pub epoch_at_start_ms: u64,
}

/// What the host does for the channel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    /// A shared page's frame is now this: the host keeps it and sends it
    /// to every connection that does not own the page itself.
    Shared {
        /// The page.
        page: u16,
        /// Its frame, encoded once.
        frame: Arc<[u8]>,
    },
    /// The session owns this page from now on: the host stops sending it
    /// the shared page of that number.
    Own {
        /// The session.
        session: SessionId,
        /// The page.
        page: u16,
    },
    /// A page was given a new title: the host serves `contents.txt` anew.
    Contents,
    /// The session no longer owns this page: the host sends it the shared
    /// page of that number again.
    Disown {
        /// The session.
        session: SessionId,
        /// The page.
        page: u16,
    },
    /// Bytes for one session, as one binary WebSocket message.
    Send {
        /// The session.
        session: SessionId,
        /// The message.
        bytes: Arc<[u8]>,
    },
    /// Close the session's connection: its input broke the protocol.
    Close {
        /// The session.
        session: SessionId,
    },
    /// A line for an application's process (`docs/applications.md`).
    ToApplication {
        /// The application, by its place in the channel.
        application: usize,
        /// The line, with its newline.
        line: Vec<u8>,
    },
    /// Something for the server's operator to read.
    Log {
        /// What happened.
        text: String,
    },
}

/// A page as the channel keeps it: encoded, with what the cursor and mode
/// records need, not the page model.
#[derive(Clone, Debug)]
pub(crate) struct Held {
    pub frame: Arc<[u8]>,
    pub cursor: Option<CursorLine>,
    pub modes: u32,
}

impl Held {
    fn of(subpages: &[PageFile]) -> Self {
        let first = subpages.first();
        Self {
            frame: Arc::from(page_frame(subpages)),
            cursor: first.and_then(|page| page.cursor),
            modes: first.and_then(|page| page.input_modes).map_or(0, |line| line.modes),
        }
    }
}

struct Hosted {
    application: Box<dyn Application>,
    pages: Vec<PageEntry>,
    wake: Option<u64>,
}

/// One channel's state.
pub struct Channel {
    config: ChannelConfig,
    applications: Vec<Hosted>,
    /// Which application serves each page.
    owners: BTreeMap<u16, usize>,
    shared: BTreeMap<u16, Held>,
    sessions: BTreeMap<SessionId, Session>,
    effects: Vec<Effect>,
    seed: u64,
    stats: ChannelStats,
}

impl Channel {
    /// A channel of `applications`; `seed` seeds its games' randomness.
    #[must_use]
    pub fn new(config: ChannelConfig, applications: Vec<Box<dyn Application>>, seed: u64) -> Self {
        let mut owners = BTreeMap::new();
        let applications: Vec<Hosted> = applications
            .into_iter()
            .enumerate()
            .map(|(index, application)| {
                let pages = application.pages();
                for entry in &pages {
                    owners.entry(entry.number).or_insert(index);
                }
                Hosted { application, pages, wake: None }
            })
            .collect();
        // xorshift needs a seed other than zero.
        Self {
            config,
            applications,
            owners,
            shared: BTreeMap::new(),
            sessions: BTreeMap::new(),
            effects: Vec::new(),
            seed: seed | 1,
            stats: ChannelStats::default(),
        }
    }

    /// The channel's description.
    #[must_use]
    pub const fn config(&self) -> &ChannelConfig {
        &self.config
    }

    /// Every page the applications serve, in order.
    pub fn entries(&self) -> impl Iterator<Item = &PageEntry> {
        self.applications.iter().flat_map(|hosted| hosted.pages.iter())
    }

    /// Whether the channel has published `page` for every viewer.
    #[must_use]
    pub fn is_shared(&self, page: u16) -> bool {
        self.shared.contains_key(&page)
    }

    /// The shared pages' frames, in page order.
    pub fn shared_frames(&self) -> impl Iterator<Item = (u16, &Arc<[u8]>)> {
        self.shared.iter().map(|(&page, held)| (page, &held.frame))
    }

    /// Starts the applications: each publishes its shared pages.
    pub fn start(&mut self, now: u64) {
        for index in 0..self.applications.len() {
            self.call(index, now, |application, context| application.start(context));
        }
    }

    /// A connection opened. The host sends it every shared page itself.
    pub fn opened(&mut self, session: SessionId, now: u64) {
        if self.sessions.insert(session, Session::new()).is_none() {
            self.stats.joined();
        }
        for index in 0..self.applications.len() {
            self.call(index, now, |application, context| application.joined(session, context));
        }
    }

    /// A connection closed.
    pub fn closed(&mut self, session: SessionId, now: u64) {
        if self.sessions.remove(&session).is_some() {
            self.stats.left();
            for index in 0..self.applications.len() {
                self.call(index, now, |application, context| application.ended(session, context));
            }
        }
    }

    /// A binary message from a session: its input stream (INPUT.md).
    pub fn received(&mut self, session: SessionId, bytes: &[u8], now: u64) {
        let Some(state) = self.sessions.get_mut(&session) else { return };
        let mut decoder = state.decoder.take().unwrap_or_else(|| Box::new(StreamDecoder::<MAX_STREAM_ITEM>::default()));
        let result = decoder.push(bytes, &mut |item| self.item(session, &item, now));
        if result.is_err() {
            // A protocol error ends the session (INPUT.md section 8).
            self.effects.push(Effect::Close { session });
            return;
        }
        if let Some(state) = self.sessions.get_mut(&session) {
            state.decoder = Some(decoder);
        }
        self.acknowledge(session);
    }

    /// The time asked for has come: wakes the applications due.
    pub fn deadline(&mut self, now: u64) {
        for index in 0..self.applications.len() {
            let due = self.applications.get(index).and_then(|hosted| hosted.wake).is_some_and(|at| at <= now);
            if due {
                if let Some(hosted) = self.applications.get_mut(index) {
                    hosted.wake = None;
                }
                self.call(index, now, |application, context| application.wake(context));
            }
        }
    }

    /// A line from the process of application `application` (its place
    /// in the channel), without its newline.
    pub fn from_application(&mut self, application: usize, line: &[u8], now: u64) {
        count(&mut self.stats.lines_from_application);
        self.call(application, now, |hosted, context| hosted.process_line(line, context));
    }

    /// Application `application`'s process started again after it ended:
    /// it is told to start, and publishes its pages afresh.
    pub fn restarted(&mut self, application: usize, now: u64) {
        self.call(application, now, |hosted, context| hosted.start(context));
    }

    /// When the channel next needs [`Channel::deadline`].
    #[must_use]
    pub fn next_wake(&self) -> Option<u64> {
        self.applications.iter().filter_map(|hosted| hosted.wake).min()
    }

    /// What the host must do now.
    pub fn take_effects(&mut self) -> Vec<Effect> {
        core::mem::take(&mut self.effects)
    }

    /// What the channel has counted since it started.
    #[must_use]
    pub const fn stats(&self) -> &ChannelStats {
        &self.stats
    }

    /// The number of sessions.
    #[must_use]
    pub fn sessions(&self) -> usize {
        self.sessions.len()
    }

    fn item(&mut self, session: SessionId, item: &Item<'_>, now: u64) {
        let Some(state) = self.sessions.get_mut(&session) else { return };
        state.acked = state.acked.wrapping_add(u16::try_from(units(item) & 0xFFFF).unwrap_or(0));
        match *item {
            Item::Event(Event::Hello { version, caps, .. }) => self.welcome(session, version, caps),
            Item::Event(Event::Page(page)) => self.page_open(session, page, now),
            _ => {
                let open = state.page;
                if let (Some(input), Some(&index)) = (Input::of(item), self.owners.get(&open)) {
                    count(&mut self.stats.inputs);
                    if matches!(input, Input::Key { action: neoteletext_input::event::Action::Press, .. } | Input::Text(_)) {
                        count(&mut self.stats.keys);
                    }
                    self.call(index, now, |application, context| application.input(session, open, &input, context));
                }
            }
        }
    }

    fn welcome(&mut self, session: SessionId, version: u32, caps: u32) {
        let Some(state) = self.sessions.get_mut(&session) else { return };
        if version != neoteletext_input::VERSION {
            return;
        }
        let record = state.next_record();
        if let Ok(packet) = packet_of(&Control::Welcome { version, caps: caps & USED }, SERVICE, record) {
            state.welcomed = true;
            self.effects.push(Effect::Send { session, bytes: Arc::from(&packet.bytes[..]) });
        }
    }

    fn page_open(&mut self, session: SessionId, page: u16, now: u64) {
        let Some(state) = self.sessions.get_mut(&session) else { return };
        state.went = None;
        let before = core::mem::replace(&mut state.page, page);
        if before != page
            && let Some(&index) = self.owners.get(&before)
        {
            self.call(index, now, |application, context| application.left_page(session, before, context));
        }
        if let Some(&index) = self.owners.get(&page) {
            self.stats.opened(page);
            self.call(index, now, |application, context| application.opened(session, page, context));
        }
        // The page again, with its cursor and modes, so its IM applies
        // (the receiver keeps one page-modes slot).
        self.send_page(session, page, true);
    }

    /// Sends `page` as `session` sees it, with its cursor and mode records
    /// when it is the page open and the session was welcomed.
    fn send_page(&mut self, session: SessionId, page: u16, with_records: bool) {
        let Some(state) = self.sessions.get(&session) else { return };
        let held = state.own.get(&page).or_else(|| self.shared.get(&page)).cloned();
        let Some(held) = held else { return };
        let mut bytes: Vec<u8> = held.frame.to_vec();
        if with_records && state.welcomed && state.page == page {
            self.append_records(session, page, &held, &mut bytes);
        }
        self.effects.push(Effect::Send { session, bytes: Arc::from(bytes) });
    }

    /// The page's mode record and the cursor record with the input count.
    fn append_records(&mut self, session: SessionId, page: u16, held: &Held, bytes: &mut Vec<u8>) {
        let Some(state) = self.sessions.get_mut(&session) else { return };
        let record = state.next_record();
        if let Ok(packet) = packet_of(&Control::PageModes { page, modes: held.modes }, SERVICE, record) {
            bytes.extend_from_slice(&packet.bytes);
        }
        let cursor = cursor_of(page, state.acked, held.cursor.as_ref());
        let record = state.next_record();
        if let Some(packet) = cursor.packet(SERVICE, record) {
            bytes.extend_from_slice(&packet.bytes);
        }
    }

    /// After a message's input: the cursor record says what was acted on.
    fn acknowledge(&mut self, session: SessionId) {
        let Some(state) = self.sessions.get_mut(&session) else { return };
        if !state.welcomed {
            return;
        }
        let page = state.went.unwrap_or(state.page);
        let held = state.own.get(&page).or_else(|| self.shared.get(&page)).map(|held| held.cursor);
        let cursor = cursor_of(page, state.acked, held.flatten().as_ref());
        let record = state.next_record();
        if let Some(packet) = cursor.packet(SERVICE, record) {
            self.effects.push(Effect::Send { session, bytes: Arc::from(&packet.bytes[..]) });
        }
    }

    /// Runs one application with a context, then carries out what it asked.
    fn call(&mut self, index: usize, now: u64, run: impl FnOnce(&mut dyn Application, &mut Context<'_>)) {
        let mut actions = Vec::new();
        {
            let Some(hosted) = self.applications.get_mut(index) else { return };
            let mut context = Context { now, epoch_at_start: self.config.epoch_at_start_ms, actions: &mut actions, seed: &mut self.seed };
            run(hosted.application.as_mut(), &mut context);
        }
        for action in actions {
            self.act(index, action);
        }
    }

    fn act(&mut self, index: usize, action: Action) {
        match action {
            Action::Publish { owner: Owner::Channel, number, subpages } => {
                count(&mut self.stats.shared_published);
                let held = Held::of(&subpages);
                self.effects.push(Effect::Shared { page: number, frame: Arc::clone(&held.frame) });
                // A page published for the first time is listed from now on.
                if self.shared.insert(number, held).is_none() {
                    self.effects.push(Effect::Contents);
                }
            }
            Action::Publish { owner: Owner::Session(session), number, subpages } => {
                let Some(state) = self.sessions.get_mut(&session) else { return };
                count(&mut self.stats.own_published);
                let held = Held::of(&subpages);
                if state.own.insert(number, held).is_none() {
                    self.effects.push(Effect::Own { session, page: number });
                }
                self.send_page(session, number, true);
            }
            Action::Records { audience, records } => self.records(audience, &records),
            Action::Goto { session, page } => {
                let Some(state) = self.sessions.get_mut(&session) else { return };
                count(&mut self.stats.gotos);
                state.went = Some(page);
                let held = state.own.get(&page).or_else(|| self.shared.get(&page)).map(|held| held.cursor);
                let cursor = cursor_of(page, state.acked, held.flatten().as_ref());
                let record = state.next_record();
                if let Some(packet) = cursor.packet(SERVICE, record) {
                    self.effects.push(Effect::Send { session, bytes: Arc::from(&packet.bytes[..]) });
                }
            }
            Action::Title { number, title } => {
                let entry = self.applications.get_mut(index).and_then(|hosted| hosted.pages.iter_mut().find(|entry| entry.number == number));
                if let Some(entry) = entry
                    && entry.title != title
                {
                    entry.title = title;
                    self.effects.push(Effect::Contents);
                }
            }
            Action::Release { session, page } => {
                let Some(state) = self.sessions.get_mut(&session) else { return };
                if state.own.remove(&page).is_some() {
                    self.effects.push(Effect::Disown { session, page });
                    self.send_page(session, page, true);
                }
            }
            Action::WakeAt(at) => {
                if let Some(hosted) = self.applications.get_mut(index) {
                    hosted.wake = Some(hosted.wake.map_or(at, |wake| wake.min(at)));
                }
            }
            Action::Message(line) => {
                count(&mut self.stats.lines_to_application);
                self.effects.push(Effect::ToApplication { application: index, line });
            }
            Action::Log(text) => self.effects.push(Effect::Log { text: alloc::format!("{}: {text}", self.config.name) }),
        }
    }

    fn records(&mut self, audience: Audience, records: &[Vec<u8>]) {
        self.stats.sound_records = self.stats.sound_records.saturating_add(u64::try_from(records.len()).unwrap_or(u64::MAX));
        let targets: Vec<SessionId> = match audience {
            Audience::Session(session) => alloc::vec![session],
            Audience::Viewers(page) => self.sessions.iter().filter(|(_, state)| state.page == page).map(|(&id, _)| id).collect(),
        };
        for session in targets {
            let Some(state) = self.sessions.get_mut(&session) else { continue };
            let (packets, next) = packets(SERVICE, state.record_id, records);
            state.record_id = next;
            let mut bytes = Vec::with_capacity(packets.len().saturating_mul(neoteletext_core::consts::PACKET_BYTES));
            for packet in &packets {
                bytes.extend_from_slice(&packet.bytes);
            }
            self.effects.push(Effect::Send { session, bytes: Arc::from(bytes) });
        }
    }
}

/// The cursor record for `page`: the page's cursor if it has one, else
/// none, with the input count.
fn cursor_of(page: u16, acked: u16, line: Option<&CursorLine>) -> Cursor {
    let place = line.filter(|line| line.row != NO_CURSOR).map(|line| Place { row: line.row, column: line.column });
    Cursor { page, acked, place, width: line.map_or(0, |line| line.width), flags: line.map_or(0, |line| line.flags) }
}

#[cfg(test)]
mod tests;
