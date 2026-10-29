//! A channel's hub: its runtime in one task, and what every connection
//! reads without asking it (decision 7). The shared pages' latest frames
//! sit in a table and their changes in a log of generations. Each
//! connection catches up from the generation it last sent, so a change
//! costs the channel one entry, and a connection that fell behind skips to
//! each page's latest frame.
//!
//! The task also keeps what the channel asks to be kept (requirement
//! 5.2), at most every [`KEEP_EVERY_S`] seconds and when it stops, and
//! times itself over every event (requirement 8.2).

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::{Arc, Mutex, PoisonError, RwLock};
use std::time::Duration;

use teletext_runtime::layout::{Part, Resource, SharedPage, bundle, page_file};
use teletext_runtime::{Channel, Effect, SessionId};
use tokio::sync::{mpsc, oneshot, watch};
use tokio::time::Instant;

use crate::apps::{FromApplication, Link};
use crate::logging;
use crate::state::{Changes, StateDirectory};
use crate::stats::{ChannelFigures, Histogram, ServerStats, count};
use crate::websocket::{CLOSE_PROTOCOL_ERROR, CLOSE_SERVICE_RESTART};

/// How often a channel's counts are sent to its application, in seconds:
/// often enough for a live figure on a page, rarely enough to cost nothing.
pub const STATS_EVERY_S: u64 = 10;
/// How often the channel's figures are recorded for `/metrics`. Nothing
/// is sent to viewers for the time's sake (requirement 11.2).
pub const RECORD_EVERY_S: u64 = 1;
/// How often what the channel keeps is written, at most (requirement 5.2).
pub const KEEP_EVERY_S: u64 = 5;

/// How many changes the log keeps: a connection further behind than this
/// is sent every shared page again, which is the same as catching up.
const LOG_CAPACITY: usize = 4_096;
/// How many commands may wait for the channel's task.
const COMMANDS: usize = 65_536;
/// How many messages of its own may wait for one connection: a
/// connection that falls this far behind loses the oldest of its own
/// (its shared pages are never queued).
pub const OUTBOX: usize = 256;

/// What a connection asks of its channel's task.
#[derive(Debug)]
pub enum Command {
    /// A connection opened, with the queue for what is its own.
    Opened {
        /// The connection's session.
        session: SessionId,
        /// Its queue.
        outbox: mpsc::Sender<Outbound>,
    },
    /// Its input stream.
    Received {
        /// The session.
        session: SessionId,
        /// One binary message.
        bytes: Vec<u8>,
    },
    /// It closed.
    Closed {
        /// The session.
        session: SessionId,
    },
    /// The server stops (requirement 5.1): every connection is closed as a
    /// restart, what is kept is written, and `done` is answered.
    Shutdown {
        /// Answered when the channel has stopped.
        done: oneshot::Sender<()>,
    },
}

/// What the channel's task sends one connection.
#[derive(Clone, Debug)]
pub enum Outbound {
    /// A binary message.
    Bytes(Arc<[u8]>),
    /// The connection owns this page from now on: the shared page of
    /// that number is not sent to it.
    Own(u16),
    /// The connection no longer owns this page: the shared page of that
    /// number is sent to it again.
    Disown(u16),
    /// Close the connection with this status (RFC 6455 section 7.4).
    Close(u16),
}

/// A shared page's latest frame, and where each of its sub-pages is in it.
type Latest = (Arc<[u8]>, Arc<[Part]>);

/// The shared pages, as every connection and request reads them.
#[derive(Debug, Default)]
pub struct Pages {
    frames: RwLock<BTreeMap<u16, Latest>>,
    log: Mutex<VecDeque<(u64, u16)>>,
}

impl Pages {
    /// The latest frame of `page`.
    #[must_use]
    pub fn frame(&self, page: u16) -> Option<Arc<[u8]>> {
        self.frames.read().unwrap_or_else(PoisonError::into_inner).get(&page).map(|(frame, _)| Arc::clone(frame))
    }

    /// Every page's latest frame.
    #[must_use]
    pub fn all(&self) -> Vec<(u16, Arc<[u8]>)> {
        self.frames.read().unwrap_or_else(PoisonError::into_inner).iter().map(|(&page, (frame, _))| (page, Arc::clone(frame))).collect()
    }

    /// A page's file: `NNN.t42`, `NNN-SSSS.t42` or `NNN.sub`, as the
    /// runtime lays them out (its `layout::SharedPage`), or none.
    #[must_use]
    pub fn file(&self, name: &str) -> Option<Resource> {
        let (page, file) = page_file(name)?;
        let frames = self.frames.read().unwrap_or_else(PoisonError::into_inner);
        let (frame, parts) = frames.get(&page)?;
        SharedPage { frame, parts }.file(file)
    }

    /// `index.t42`: every page's first sub-page, within a receiver's limit.
    #[must_use]
    pub fn bundle(&self) -> Vec<u8> {
        let frames = self.frames.read().unwrap_or_else(PoisonError::into_inner);
        bundle(frames.values().map(|(frame, parts)| SharedPage { frame, parts }.first()))
    }

    /// The pages changed after generation `since`, each once, or none
    /// when the log no longer reaches back that far.
    #[must_use]
    pub fn changed_since(&self, since: u64) -> Option<Vec<u16>> {
        let log = self.log.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(&(oldest, _)) = log.front()
            && oldest > since.saturating_add(1)
        {
            return None;
        }
        let mut pages: Vec<u16> = log.iter().filter(|&&(generation, _)| generation > since).map(|&(_, page)| page).collect();
        pages.sort_unstable();
        pages.dedup();
        Some(pages)
    }

    fn set(&self, page: u16, frame: Arc<[u8]>, parts: Arc<[Part]>, generation: u64) {
        self.frames.write().unwrap_or_else(PoisonError::into_inner).insert(page, (frame, parts));
        let mut log = self.log.lock().unwrap_or_else(PoisonError::into_inner);
        if log.len() == LOG_CAPACITY {
            log.pop_front();
        }
        log.push_back((generation, page));
    }

    /// The page is served no more; a viewer showing it keeps what it has.
    fn remove(&self, page: u16) {
        self.frames.write().unwrap_or_else(PoisonError::into_inner).remove(&page);
    }
}

/// A running channel, as the server's routes see it.
#[derive(Clone, Debug)]
pub struct Hub {
    /// The channel's folder.
    pub folder: String,
    /// Its descriptor, which does not change.
    pub descriptor: Arc<[u8]>,
    /// Its contents file, made again when a page is retitled.
    pub contents: Arc<RwLock<Arc<[u8]>>>,
    /// Its shared pages.
    pub pages: Arc<Pages>,
    /// The latest generation: every connection watches it.
    pub generation: watch::Receiver<u64>,
    /// Its task's commands.
    pub commands: mpsc::Sender<Command>,
    /// The server's figures, which every connection adds to.
    pub stats: Arc<ServerStats>,
    /// The sessions connected now: a session is never shared by two
    /// connections at once (requirement 5.5).
    pub connected: Arc<Mutex<BTreeSet<SessionId>>>,
}

impl Hub {
    /// Claims `session` for a connection, unless one has it already.
    #[must_use]
    pub fn claim(&self, session: SessionId) -> bool {
        self.connected.lock().unwrap_or_else(PoisonError::into_inner).insert(session)
    }

    /// Gives `session` up: its connection closed.
    pub fn release(&self, session: SessionId) {
        self.connected.lock().unwrap_or_else(PoisonError::into_inner).remove(&session);
    }
}

/// Starts `channel` in a task of its own, at `start`, the host's clock's
/// zero; `link`, for a channel whose application is a program of its own,
/// carries that program's declarations and lines (`docs/applications.md`);
/// `kept`, when there is a state directory, is where what the channel
/// keeps is written.
#[must_use]
pub fn spawn(folder: &str, channel: Channel, start: Instant, link: Option<Link>, stats: Arc<ServerStats>, kept: Option<StateDirectory>) -> Hub {
    let descriptor: Arc<[u8]> = channel.resource("channel.json").map(|resource| resource.body).unwrap_or_default().into();
    let contents = Arc::new(RwLock::new(contents_of(&channel)));
    let pages = Arc::new(Pages::default());
    let (generation_sender, generation) = watch::channel(0_u64);
    let (commands, queue) = mpsc::channel::<Command>(COMMANDS);
    let hub = Hub {
        folder: folder.to_owned(),
        descriptor,
        contents: Arc::clone(&contents),
        pages: Arc::clone(&pages),
        generation,
        commands,
        stats: Arc::clone(&stats),
        connected: Arc::new(Mutex::new(BTreeSet::new())),
    };
    let (to_application, from_application) = link.map_or((None, None), |link| (Some(link.to), Some(link.from)));
    let task = Task {
        folder: folder.to_owned(),
        pages,
        generation: generation_sender,
        current: 0,
        outboxes: BTreeMap::new(),
        to_application,
        program_live: false,
        declarations: 0,
        dropped: 0,
        keeping: Changes::new(),
        kept,
        events: Histogram::default(),
        stats,
        contents,
        start,
    };
    tokio::spawn(task.run(channel, queue, from_application));
    hub
}

/// The channel's `contents.txt`.
fn contents_of(channel: &Channel) -> Arc<[u8]> {
    channel.resource("contents.txt").map(|resource| resource.body).unwrap_or_default().into()
}

/// What woke the channel's task.
enum Event {
    Command(Command),
    Application(FromApplication),
    Deadline,
    Figures,
    Record,
    Keep,
}

/// The next line from the program, or never when there is none.
async fn next_line(from: Option<&mut mpsc::Receiver<FromApplication>>) -> Option<FromApplication> {
    match from {
        Some(from) => from.recv().await,
        None => std::future::pending().await,
    }
}

/// Sleeps until `at`, or forever.
async fn sleep_until(at: Option<Instant>) {
    match at {
        Some(at) => tokio::time::sleep_until(at).await,
        None => std::future::pending().await,
    }
}

/// The channel's task's own state: where its effects go.
struct Task {
    folder: String,
    pages: Arc<Pages>,
    generation: watch::Sender<u64>,
    current: u64,
    outboxes: BTreeMap<SessionId, mpsc::Sender<Outbound>>,
    to_application: Option<mpsc::Sender<Vec<u8>>>,
    /// The program has declared its pages and not ended since: lines for
    /// it are dropped meanwhile (requirement 5.3).
    program_live: bool,
    declarations: u64,
    /// Lines for the program dropped because it fell behind.
    dropped: u64,
    /// What is to be kept, by key, since it was last written.
    keeping: Changes,
    kept: Option<StateDirectory>,
    events: Histogram,
    stats: Arc<ServerStats>,
    contents: Arc<RwLock<Arc<[u8]>>>,
    /// The host's clock's zero.
    start: Instant,
}

impl Task {
    /// The host's clock, in milliseconds since its zero.
    fn now(&self) -> u64 {
        u64::try_from(self.start.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    /// The channel's task: waits for what comes next, gives it to the
    /// channel, and carries out its effects, until the server stops.
    async fn run(mut self, mut channel: Channel, mut queue: mpsc::Receiver<Command>, mut from_application: Option<mpsc::Receiver<FromApplication>>) {
        // Each first tick one period from now: an interval's first tick
        // is at once, which would send a program its figures before it
        // had been started.
        let every = |seconds: u64| {
            let period = Duration::from_secs(seconds);
            tokio::time::interval_at(Instant::now().checked_add(period).unwrap_or_else(Instant::now), period)
        };
        let mut figures = every(STATS_EVERY_S);
        // A tick missed is not made up: the next records the figures as they are.
        let mut record = every(RECORD_EVERY_S);
        record.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut keep = every(KEEP_EVERY_S);
        keep.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        self.apply(channel.take_effects(), &channel);
        self.record(&channel);
        loop {
            let wake = channel.next_wake().and_then(|at| self.start.checked_add(Duration::from_millis(at)));
            let event = tokio::select! {
                command = queue.recv() => match command {
                    Some(command) => Event::Command(command),
                    None => break,
                },
                arrived = next_line(from_application.as_mut()) => {
                    let Some(arrived) = arrived else {
                        // The program's supervisor ended: the channel goes
                        // on with its pages as last published.
                        from_application = None;
                        self.program_live = false;
                        continue;
                    };
                    Event::Application(arrived)
                }
                () = sleep_until(wake) => Event::Deadline,
                _ = figures.tick() => Event::Figures,
                _ = record.tick() => Event::Record,
                _ = keep.tick() => Event::Keep,
            };
            let began = Instant::now();
            if !self.handle(event, &mut channel).await {
                return;
            }
            self.apply(channel.take_effects(), &channel);
            self.events.observe(began.elapsed());
        }
    }

    /// Gives one event to the channel; false when the channel has stopped.
    async fn handle(&mut self, event: Event, channel: &mut Channel) -> bool {
        let now = self.now();
        match event {
            Event::Command(Command::Opened { session, outbox }) => {
                self.outboxes.insert(session, outbox);
                channel.opened(session, now);
            }
            Event::Command(Command::Received { session, bytes }) => channel.received(session, &bytes, now),
            Event::Command(Command::Closed { session }) => {
                self.outboxes.remove(&session);
                channel.closed(session, now);
            }
            Event::Command(Command::Shutdown { done }) => {
                for outbox in self.outboxes.values() {
                    let _ = outbox.try_send(Outbound::Close(CLOSE_SERVICE_RESTART));
                }
                self.apply(channel.take_effects(), channel);
                self.keep().await;
                self.record(channel);
                let _ = done.send(());
                return false;
            }
            Event::Application(FromApplication::Declared(declared)) => {
                if self.declarations > 0 {
                    count(&self.stats.restarts);
                }
                self.declarations = self.declarations.saturating_add(1);
                self.program_live = true;
                channel.declared(0, declared);
                channel.application_started(0, now);
            }
            Event::Application(FromApplication::Line(text)) => channel.from_application(0, &text, now),
            Event::Application(FromApplication::Ended) => self.program_live = false,
            Event::Deadline => channel.deadline(now),
            Event::Record => self.record(channel),
            Event::Figures => {
                if self.program_live
                    && let Some(to) = self.to_application.as_ref()
                {
                    // A figure missed is sent again in ten seconds.
                    let _ = to.try_send(self.stats.line(&self.folder, &self.figures(channel)));
                }
            }
            Event::Keep => self.keep().await,
        }
        true
    }

    /// Carries out the channel's effects: a shared page into the table and
    /// the log, once; what is one session's own into its queue; a line for
    /// the program into its queue while it runs; what is to be kept noted;
    /// and `contents.txt` made again from `channel` when a page changed
    /// its title.
    fn apply(&mut self, effects: Vec<Effect>, channel: &Channel) {
        let mut changed = false;
        for effect in effects {
            match effect {
                Effect::Contents => *self.contents.write().unwrap_or_else(PoisonError::into_inner) = contents_of(channel),
                Effect::Shared { page, frame, parts } => {
                    self.current = self.current.saturating_add(1);
                    self.pages.set(page, frame, parts, self.current);
                    changed = true;
                }
                Effect::Withdrawn { page } => self.pages.remove(page),
                Effect::Store { key, bytes } => {
                    if self.kept.is_some() {
                        self.keeping.insert(key, Some(bytes));
                    }
                }
                Effect::Forget { key } => {
                    if self.kept.is_some() {
                        self.keeping.insert(key, None);
                    }
                }
                Effect::Own { session, page } => self.send(session, Outbound::Own(page)),
                Effect::Disown { session, page } => self.send(session, Outbound::Disown(page)),
                Effect::Send { session, bytes } => self.send(session, Outbound::Bytes(bytes)),
                Effect::Close { session } => self.send(session, Outbound::Close(CLOSE_PROTOCOL_ERROR)),
                Effect::ToApplication { line, .. } => {
                    let Some(to) = self.to_application.as_ref().filter(|_| self.program_live) else { continue };
                    if to.try_send(line).is_err() {
                        self.dropped = self.dropped.saturating_add(1);
                        // At the first and then at each power of two, so a
                        // stuck program cannot flood the log.
                        if self.dropped.is_power_of_two() {
                            tracing::warn!(target: "program", channel = self.folder, dropped = self.dropped, "the program is behind: lines for it dropped");
                        }
                    }
                }
                Effect::Log { level, event, text } => logging::runtime(&self.folder, level, event, &text),
            }
        }
        if changed {
            self.generation.send_replace(self.current);
        }
    }

    /// Queues `outbound` for `session`, dropping it when the queue is
    /// full: a connection that far behind is the slowest client's, not the
    /// channel's, problem.
    fn send(&self, session: SessionId, outbound: Outbound) {
        if let Some(outbox) = self.outboxes.get(&session) {
            let _ = outbox.try_send(outbound);
        }
    }

    /// Writes what is to be kept, if anything (requirement 5.2).
    async fn keep(&mut self) {
        let Some(state) = self.kept.clone() else { return };
        if self.keeping.is_empty() {
            return;
        }
        let changes = std::mem::take(&mut self.keeping);
        let folder = self.folder.clone();
        let records = changes.len();
        let written = tokio::task::spawn_blocking(move || state.apply(&folder, &changes)).await;
        match written {
            Ok(Ok(())) => tracing::debug!(target: "state", channel = self.folder, records, "kept"),
            Ok(Err(error)) => tracing::error!(target: "state", channel = self.folder, %error, "what the channel keeps could not be written"),
            Err(error) => tracing::error!(target: "state", channel = self.folder, %error, "writing what the channel keeps failed"),
        }
    }

    /// The channel's figures now.
    fn figures(&self, channel: &Channel) -> ChannelFigures {
        ChannelFigures { stats: channel.stats().clone(), lines_dropped: self.dropped, ready: channel.is_ready(), events: self.events.clone() }
    }

    /// Records the channel's figures for the server's.
    fn record(&self, channel: &Channel) {
        self.stats.record(&self.folder, self.figures(channel));
    }
}

#[cfg(test)]
mod tests;
