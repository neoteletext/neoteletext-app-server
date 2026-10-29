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
use neoteletext_formats::page::CursorLine;
use neoteletext_input::caps::{CURSOR_CAP, PAGE_CAP};
use neoteletext_input::control::{Control, packet_of};
use neoteletext_input::cursor::{BLOCK, Cursor, NO_CURSOR, Place};
use neoteletext_input::event::{Event, Item, MAX_STREAM_ITEM, StreamDecoder, units};

use crate::application::{Action, Application, Audience, Context, Level, Owner, PageEntry};
use crate::held::Held;
use crate::input::Input;
use crate::layout::Part;
use crate::session::Session;
use crate::stats::{ChannelStats, count};
use crate::store::{KeptPage, StoreError, StoreKey, page_record, pages_record, read_page, read_pages, read_subpage, subpage_record};

/// A connection's number, which the host chooses and never reuses.
pub type SessionId = u64;

/// The service number audio and input records are sent on (the
/// reference channel's, INPUT.md section 7).
const SERVICE: u8 = 0;

/// The capabilities the channel uses of what a receiver offers in HELLO
/// (INPUT.md section 6.2). There are no modes to agree: every event is
/// sent and the channel ignores what it does not use (section 6.3).
const USED: u32 = PAGE_CAP | CURSOR_CAP;

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
    /// How long a session whose connection closed waits for its viewer to
    /// come back before its applications are told it ended, in
    /// milliseconds; 0 ends it at once (requirement 5.5).
    pub session_grace_ms: u64,
}

/// How long a session waits for its viewer to come back: 5 minutes, long
/// enough for a phone that closed a background socket or a server that
/// restarted (`SERVER.md` section 8.1: "a grace period of minutes").
pub const SESSION_GRACE_MS: u64 = 5 * 60 * 1_000;

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
        /// Where each sub-page is in the frame, for the host that serves
        /// the page's files itself ([`crate::layout::SharedPage`]).
        parts: Arc<[Part]>,
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
    /// The channel no longer has this shared page: the host stops serving
    /// it (a page its application no longer declares).
    Withdrawn {
        /// The page.
        page: u16,
    },
    /// Keep `bytes` under `key`, in place of what was kept there, and give
    /// them back to [`Channel::load`] when the channel starts again
    /// (requirement 5.2).
    Store {
        /// Where.
        key: StoreKey,
        /// The record.
        bytes: Vec<u8>,
    },
    /// What was kept under `key` is no longer needed.
    Forget {
        /// Where.
        key: StoreKey,
    },
    /// A line for an application's process (`docs/applications.md`).
    ToApplication {
        /// The application, by its place in the channel.
        application: usize,
        /// The line, with its newline.
        line: Vec<u8>,
    },
    /// Something for the server's operator to read (requirement 7.6).
    Log {
        /// How much it matters.
        level: Level,
        /// The kind of event, a name that does not change.
        event: &'static str,
        /// What happened.
        text: String,
    },
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
    /// Shared pages given back by [`Channel::load`] whose sub-pages have
    /// not all come yet, and sub-pages whose page has not.
    loading: BTreeMap<u16, KeptPage>,
    loaded: BTreeMap<(u16, u16), Vec<u8>>,
    sessions: BTreeMap<SessionId, Session>,
    effects: Vec<Effect>,
    seed: u64,
    stats: ChannelStats,
}

impl Channel {
    /// A channel of `applications`; `seed` seeds its games' randomness.
    #[must_use]
    pub fn new(config: ChannelConfig, applications: Vec<Box<dyn Application>>, seed: u64) -> Self {
        let applications: Vec<Hosted> = applications
            .into_iter()
            .map(|application| {
                let pages = application.pages();
                Hosted { application, pages, wake: None }
            })
            .collect();
        // xorshift needs a seed other than zero.
        let mut channel = Self {
            config,
            applications,
            owners: BTreeMap::new(),
            shared: BTreeMap::new(),
            loading: BTreeMap::new(),
            loaded: BTreeMap::new(),
            sessions: BTreeMap::new(),
            effects: Vec::new(),
            seed: seed | 1,
            stats: ChannelStats::default(),
        };
        channel.find_owners();
        channel
    }

    /// Which application serves each page: the first to declare it.
    fn find_owners(&mut self) {
        self.owners.clear();
        for (index, hosted) in self.applications.iter().enumerate() {
            for entry in &hosted.pages {
                self.owners.entry(entry.number).or_insert(index);
            }
        }
    }

    /// Gives the channel back what it kept under `key` (requirement 5.2),
    /// before it starts, in any order: a shared page is served once its
    /// record and every one of its sub-pages' have come, as it was last
    /// published, until its application publishes it again.
    ///
    /// # Errors
    /// A record that is not whole and of this format, or names an
    /// application the channel does not have; nothing of it is taken.
    pub fn load(&mut self, key: StoreKey, record: &[u8]) -> Result<(), StoreError> {
        match key {
            StoreKey::Page(page) => {
                let kept = read_page(page, record)?;
                self.loading.insert(page, kept);
                self.assemble(page);
            }
            StoreKey::SubPage { page, subcode } => {
                let packets = read_subpage(page, subcode, record)?;
                self.loaded.insert((page, subcode), packets.to_vec());
                self.assemble(page);
            }
            StoreKey::Pages(application) => {
                let pages = read_pages(record)?;
                let hosted = self.applications.get_mut(application).ok_or(StoreError::NoApplication)?;
                hosted.application.declared(&pages);
                hosted.pages = pages;
                self.find_owners();
                self.effects.push(Effect::Contents);
            }
        }
        Ok(())
    }

    /// Serves `page` from what was loaded, once its record and all its
    /// sub-pages' are in.
    fn assemble(&mut self, page: u16) {
        let Some(kept) = self.loading.get(&page) else { return };
        if !kept.subcodes.iter().all(|&subcode| self.loaded.contains_key(&(page, subcode))) {
            return;
        }
        let Some(kept) = self.loading.remove(&page) else { return };
        let loaded = &self.loaded;
        let held = Held::of_encoded(kept.subcodes.iter().map(|&subcode| (subcode, loaded.get(&(page, subcode)).map_or(&[][..], Vec::as_slice))), kept.cursor);
        self.loaded.retain(|&(each, _), _| each != page);
        self.effects.push(Effect::Shared { page, frame: Arc::clone(&held.frame), parts: Arc::clone(&held.parts) });
        self.shared.insert(page, held);
    }

    /// Application `application`'s process declared `pages`, as it does
    /// each time it starts (requirement 5.3): it serves them from now on,
    /// and a shared page no application declares any more is withdrawn.
    /// The host then calls [`Channel::application_started`].
    pub fn declared(&mut self, application: usize, pages: Vec<PageEntry>) {
        let Some(hosted) = self.applications.get_mut(application) else { return };
        hosted.application.declared(&pages);
        hosted.pages = pages;
        self.find_owners();
        let owners = &self.owners;
        let gone: Vec<u16> = self.shared.keys().filter(|page| !owners.contains_key(page)).copied().collect();
        for page in gone {
            self.withdraw(page);
        }
        self.effects.push(Effect::Contents);
        self.store_pages(application);
    }

    /// Keeps application `application`'s declared pages.
    fn store_pages(&mut self, application: usize) {
        if let Some(hosted) = self.applications.get(application) {
            self.effects.push(Effect::Store { key: StoreKey::Pages(application), bytes: pages_record(&hosted.pages) });
        }
    }

    /// Whether the channel has pages to serve: kept from before, or
    /// declared by its applications (requirement 8.4).
    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.entries().next().is_some()
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

    /// The shared pages, in page order.
    pub(crate) fn shared_pages(&self) -> impl Iterator<Item = (u16, &Held)> {
        self.shared.iter().map(|(&page, held)| (page, held))
    }

    /// Starts the applications: each publishes its shared pages.
    pub fn start(&mut self, now: u64) {
        for index in 0..self.applications.len() {
            self.call(index, now, |application, context| application.start(context));
        }
    }

    /// A connection opened for `session`. The host sends it every shared
    /// page itself. A session waiting for its viewer continues
    /// (requirement 5.5): a new input session (INPUT.md section 4), sent
    /// its own pages again, its applications not told it joined again.
    pub fn opened(&mut self, session: SessionId, now: u64) {
        if let Some(state) = self.sessions.get_mut(&session) {
            if state.waiting_since.take().is_some() {
                state.reconnected();
                self.stats.resumed();
                let own: Vec<(u16, Arc<[u8]>)> = state.own.iter().map(|(&page, held)| (page, Arc::clone(&held.frame))).collect();
                for (page, frame) in own {
                    self.effects.push(Effect::Own { session, page });
                    self.effects.push(Effect::Send { session, bytes: frame });
                }
            }
            return;
        }
        self.sessions.insert(session, Session::new());
        self.stats.joined();
        for index in 0..self.applications.len() {
            self.call(index, now, |application, context| application.joined(session, context));
        }
    }

    /// A connection closed. Its session waits for its viewer to come back
    /// for the channel's grace (requirement 5.5): its applications are
    /// told it left the page it had open, and that it ended only when the
    /// grace passes.
    pub fn closed(&mut self, session: SessionId, now: u64) {
        let Some(state) = self.sessions.get_mut(&session) else { return };
        if state.waiting_since.is_some() {
            return;
        }
        if self.config.session_grace_ms == 0 {
            self.sessions.remove(&session);
            self.stats.left();
            self.end(session, now);
            return;
        }
        state.waiting_since = Some(now);
        state.went = None;
        state.decoder = None;
        let page = core::mem::replace(&mut state.page, 0);
        self.stats.waiting();
        if let Some(&index) = self.owners.get(&page) {
            self.call(index, now, |application, context| application.left_page(session, page, context));
        }
    }

    /// Tells every application that `session` ended.
    fn end(&mut self, session: SessionId, now: u64) {
        for index in 0..self.applications.len() {
            self.call(index, now, |application, context| application.ended(session, context));
        }
    }

    /// When the session waiting longest stops waiting.
    fn next_expiry(&self) -> Option<u64> {
        let grace = self.config.session_grace_ms;
        self.sessions.values().filter_map(|state| state.waiting_since).min().map(|since| since.saturating_add(grace))
    }

    /// Ends the sessions whose grace has passed at `now`.
    fn expire(&mut self, now: u64) {
        let grace = self.config.session_grace_ms;
        let expired: Vec<SessionId> = self
            .sessions
            .iter()
            .filter(|(_, state)| state.waiting_since.is_some_and(|since| since.saturating_add(grace) <= now))
            .map(|(&session, _)| session)
            .collect();
        for session in expired {
            self.sessions.remove(&session);
            self.stats.expired();
            self.end(session, now);
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

    /// The time asked for has come: ends the sessions whose grace has
    /// passed, and wakes the applications due.
    pub fn deadline(&mut self, now: u64) {
        self.expire(now);
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

    /// Application `application`'s process started, the first time or
    /// again after it ended, and declared its pages ([`Channel::declared`]):
    /// it is told to start, and publishes its pages afresh. What an
    /// earlier process drew for each viewer ended with it, so each
    /// session's own copies of its pages are dropped, and it is told of
    /// every session and the page each viewer has open, as if they had
    /// just come.
    pub fn application_started(&mut self, application: usize, now: u64) {
        self.call(application, now, |hosted, context| hosted.start(context));
        let owners = &self.owners;
        let mine = |page: &u16| owners.get(page) == Some(&application);
        let stale: Vec<(SessionId, u16)> =
            self.sessions.iter().flat_map(|(&session, state)| state.own.keys().filter(|page| mine(page)).map(move |&page| (session, page))).collect();
        let viewers: Vec<(SessionId, Option<u16>)> =
            self.sessions.iter().map(|(&session, state)| (session, Some(state.page).filter(|page| mine(page)))).collect();
        for (session, page) in stale {
            self.act(application, Action::Release { session, page });
        }
        for (session, page) in viewers {
            self.call(application, now, |hosted, context| hosted.joined(session, context));
            if let Some(page) = page {
                self.call(application, now, |hosted, context| hosted.opened(session, page, context));
            }
        }
    }

    /// When the channel next needs [`Channel::deadline`].
    #[must_use]
    pub fn next_wake(&self) -> Option<u64> {
        self.applications.iter().filter_map(|hosted| hosted.wake).chain(self.next_expiry()).min()
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
        if let Ok(packet) = packet_of(&Control::Welcome { version, caps: caps & USED, resume: None }, SERVICE, record) {
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
        // The page again, with its cursor record.
        self.send_page(session, page, true);
    }

    /// Sends `page` as `session` sees it, with its cursor record when it
    /// is the page open and the session was welcomed.
    fn send_page(&mut self, session: SessionId, page: u16, with_cursor: bool) {
        let Some(state) = self.sessions.get(&session) else { return };
        let held = state.own.get(&page).or_else(|| self.shared.get(&page)).cloned();
        let Some(held) = held else { return };
        let mut bytes: Vec<u8> = held.frame.to_vec();
        if with_cursor && state.welcomed && state.page == page {
            self.append_cursor(session, page, &held, &mut bytes);
        }
        self.effects.push(Effect::Send { session, bytes: Arc::from(bytes) });
    }

    /// The page's cursor record with the input count.
    fn append_cursor(&mut self, session: SessionId, page: u16, held: &Held, bytes: &mut Vec<u8>) {
        let Some(state) = self.sessions.get_mut(&session) else { return };
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
            Action::Publish { owner, number, subpages } => self.change(owner, number, |_| Some(Held::of(&subpages))),
            Action::Put { owner, number, subpages } => {
                self.change(owner, number, |held| Some(held.map_or_else(|| Held::of(&subpages), |held| held.with(&subpages))));
            }
            Action::Remove { owner, number, subcodes } => self.change(owner, number, |held| held.and_then(|held| held.without(&subcodes))),
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
                    self.store_pages(index);
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
            Action::Log { level, event, text } => self.effects.push(Effect::Log { level, event, text }),
        }
    }

    /// Makes `page` of `owner` what `make` makes of it as it is: the
    /// channel's copy, kept and sent to every viewer, or one session's;
    /// none ends it.
    fn change(&mut self, owner: Owner, page: u16, make: impl FnOnce(Option<&Held>) -> Option<Held>) {
        match owner {
            Owner::Channel => {
                let Some(held) = make(self.shared.get(&page)) else {
                    if self.shared.contains_key(&page) {
                        self.withdraw(page);
                        self.effects.push(Effect::Contents);
                    }
                    return;
                };
                count(&mut self.stats.shared_published);
                self.keep(page, &held);
                self.effects.push(Effect::Shared { page, frame: Arc::clone(&held.frame), parts: Arc::clone(&held.parts) });
                // A page published for the first time is listed from now on.
                if self.shared.insert(page, held).is_none() {
                    self.effects.push(Effect::Contents);
                }
            }
            Owner::Session(session) => {
                let Some(state) = self.sessions.get_mut(&session) else { return };
                let Some(held) = make(state.own.get(&page)) else {
                    if state.own.remove(&page).is_some() {
                        self.effects.push(Effect::Disown { session, page });
                        self.send_page(session, page, true);
                    }
                    return;
                };
                count(&mut self.stats.own_published);
                if state.own.insert(page, held).is_none() {
                    self.effects.push(Effect::Own { session, page });
                }
                self.send_page(session, page, true);
            }
        }
    }

    /// Keeps the shared `page`, now `held` (requirement 5.2): its record,
    /// each sub-page that changed, and the forgetting of each that ended.
    fn keep(&mut self, page: u16, held: &Held) {
        let before = self.shared.get(&page);
        for (subcode, packets) in held.subpages() {
            if before.and_then(|before| before.subpage(subcode)) != Some(packets) {
                self.effects.push(Effect::Store { key: StoreKey::SubPage { page, subcode }, bytes: subpage_record(page, subcode, packets) });
            }
        }
        if let Some(before) = before {
            for subcode in before.subcodes().filter(|&subcode| held.subpage(subcode).is_none()) {
                self.effects.push(Effect::Forget { key: StoreKey::SubPage { page, subcode } });
            }
        }
        self.effects.push(Effect::Store { key: StoreKey::Page(page), bytes: page_record(page, held) });
    }

    /// The channel no longer has the shared `page`: it is not served, and
    /// nothing of it is kept.
    fn withdraw(&mut self, page: u16) {
        let Some(held) = self.shared.remove(&page) else { return };
        self.effects.push(Effect::Withdrawn { page });
        for subcode in held.subcodes() {
            self.effects.push(Effect::Forget { key: StoreKey::SubPage { page, subcode } });
        }
        self.effects.push(Effect::Forget { key: StoreKey::Page(page) });
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
    // Only BLOCK is a flag now: echo and masked echo are withdrawn (the
    // engine's E.134), and a program written before may still set them.
    Cursor { page, acked, place, flags: line.map_or(0, |line| line.flags & BLOCK) }
}

#[cfg(test)]
mod tests;
