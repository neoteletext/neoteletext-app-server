//! A channel's hub: its runtime in one task, which every connection and
//! the channel's program talk to through queues. The channel keeps no
//! content (REQ-1474): what the program sends goes to the connections it
//! names, through each connection's own queue.
//!
//! The task times itself over every event (requirement 8.2).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use teletext_runtime::{Channel, Effect, SessionId};
use tokio::sync::{mpsc, oneshot};
use tokio::time::Instant;

use crate::apps::{FromApplication, Link};
use crate::logging;
use crate::stats::{ChannelFigures, Histogram, ServerStats, count};
use crate::websocket::{CLOSE_PROTOCOL_ERROR, CLOSE_SERVICE_RESTART};

/// How often a channel's counts are sent to its program, in seconds:
/// often enough for a live figure on a page, rarely enough to cost nothing.
pub const STATS_EVERY_S: u64 = 10;
/// How often the channel's figures are recorded for `/metrics`. Nothing
/// is sent to viewers for the time's sake (requirement 11.2).
pub const RECORD_EVERY_S: u64 = 1;

/// How many commands may wait for the channel's task.
const COMMANDS: usize = 65_536;
/// How many messages may wait for one connection: a connection that
/// falls this far behind loses the newest.
pub const OUTBOX: usize = 256;

/// What a connection asks of its channel's task.
#[derive(Debug)]
pub enum Command {
    /// A connection opened, with its queue.
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
    /// restart, and `done` is answered.
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
    /// Close the connection with this status (RFC 6455 section 7.4).
    Close(u16),
}

/// A running channel, as the server's routes see it.
#[derive(Clone, Debug)]
pub struct Hub {
    /// The channel's folder.
    pub folder: String,
    /// Its `channel.json`, which does not change.
    pub descriptor: Arc<[u8]>,
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
/// zero; `link`, for a channel whose program is a process of its own,
/// carries that program's starts and lines (`docs/applications.md`).
#[must_use]
pub fn spawn(folder: &str, channel: Channel, start: Instant, link: Option<Link>, stats: Arc<ServerStats>) -> Hub {
    let descriptor: Arc<[u8]> = channel.descriptor().into();
    let (commands, queue) = mpsc::channel::<Command>(COMMANDS);
    let hub = Hub { folder: folder.to_owned(), descriptor, commands, stats: Arc::clone(&stats), connected: Arc::new(Mutex::new(BTreeSet::new())) };
    let (to_application, from_application) = link.map_or((None, None), |link| (Some(link.to), Some(link.from)));
    let task = Task { folder: folder.to_owned(), outboxes: BTreeMap::new(), to_application, starts: 0, dropped: 0, events: Histogram::default(), stats, start };
    tokio::spawn(task.run(channel, queue, from_application));
    hub
}

/// What woke the channel's task.
enum Event {
    Command(Command),
    Application(FromApplication),
    Deadline,
    Figures,
    Record,
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
    outboxes: BTreeMap<SessionId, mpsc::Sender<Outbound>>,
    to_application: Option<mpsc::Sender<Vec<u8>>>,
    /// How many times the program has started.
    starts: u64,
    /// Lines for the program dropped because it fell behind.
    dropped: u64,
    events: Histogram,
    stats: Arc<ServerStats>,
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
        self.apply(channel.take_effects());
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
                        // The program's supervisor ended.
                        from_application = None;
                        channel.program_ended();
                        continue;
                    };
                    Event::Application(arrived)
                }
                () = sleep_until(wake) => Event::Deadline,
                _ = figures.tick() => Event::Figures,
                _ = record.tick() => Event::Record,
            };
            let began = Instant::now();
            if !self.handle(event, &mut channel) {
                return;
            }
            self.apply(channel.take_effects());
            self.events.observe(began.elapsed());
        }
    }

    /// Gives one event to the channel; false when the channel has stopped.
    fn handle(&mut self, event: Event, channel: &mut Channel) -> bool {
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
                self.apply(channel.take_effects());
                self.record(channel);
                let _ = done.send(());
                return false;
            }
            Event::Application(FromApplication::Started) => {
                if self.starts > 0 {
                    count(&self.stats.restarts);
                }
                self.starts = self.starts.saturating_add(1);
                channel.program_started(now);
            }
            Event::Application(FromApplication::Line(text)) => channel.from_program(&text, now),
            Event::Application(FromApplication::Ended) => channel.program_ended(),
            Event::Deadline => channel.deadline(now),
            Event::Record => self.record(channel),
            Event::Figures => {
                if channel.is_ready()
                    && let Some(to) = self.to_application.as_ref()
                {
                    // A figure missed is sent again in ten seconds.
                    let _ = to.try_send(self.stats.line(&self.folder, &self.figures(channel)));
                }
            }
        }
        true
    }

    /// Carries out the channel's effects: bytes into each connection's
    /// queue, a line into the program's queue.
    fn apply(&mut self, effects: Vec<Effect>) {
        for effect in effects {
            match effect {
                Effect::Send { session, bytes } => self.send(session, Outbound::Bytes(bytes)),
                Effect::Close { session } => self.send(session, Outbound::Close(CLOSE_PROTOCOL_ERROR)),
                Effect::ToProgram { line } => {
                    let Some(to) = self.to_application.as_ref() else { continue };
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
    }

    /// Queues `outbound` for `session`, dropping it when the queue is
    /// full: a connection that far behind is the slowest client's, not the
    /// channel's, problem.
    fn send(&self, session: SessionId, outbound: Outbound) {
        if let Some(outbox) = self.outboxes.get(&session) {
            let _ = outbox.try_send(outbound);
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
