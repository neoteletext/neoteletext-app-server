//! A channel's hub: its runtime in one task, and what every connection
//! reads without asking it (decision 7). The shared pages' latest frames
//! sit in a table and their changes in a log of generations. Each
//! connection catches up from the generation it last sent, so a change
//! costs the channel one entry, and a connection that fell behind skips to
//! each page's latest frame.

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex, PoisonError, RwLock};
use std::time::Duration;

use teletext_runtime::{Channel, Effect, SessionId};
use tokio::sync::{mpsc, watch};
use tokio::time::Instant;

use crate::apps::{FromApplication, Link};
use crate::stats::{ServerStats, count};

/// How often a channel's counts are recorded and sent to its application,
/// in seconds: often enough for a live figure on a page, rarely enough to
/// cost nothing.
pub const STATS_EVERY_S: u64 = 10;

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
    /// Close the connection.
    Close,
}

/// The shared pages, as every connection reads them.
#[derive(Debug, Default)]
pub struct Pages {
    frames: RwLock<BTreeMap<u16, Arc<[u8]>>>,
    log: Mutex<VecDeque<(u64, u16)>>,
}

impl Pages {
    /// The latest frame of `page`.
    #[must_use]
    pub fn frame(&self, page: u16) -> Option<Arc<[u8]>> {
        self.frames.read().unwrap_or_else(PoisonError::into_inner).get(&page).cloned()
    }

    /// Every page's latest frame.
    #[must_use]
    pub fn all(&self) -> Vec<(u16, Arc<[u8]>)> {
        self.frames.read().unwrap_or_else(PoisonError::into_inner).iter().map(|(&page, frame)| (page, Arc::clone(frame))).collect()
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

    fn set(&self, page: u16, frame: Arc<[u8]>, generation: u64) {
        self.frames.write().unwrap_or_else(PoisonError::into_inner).insert(page, frame);
        let mut log = self.log.lock().unwrap_or_else(PoisonError::into_inner);
        if log.len() == LOG_CAPACITY {
            log.pop_front();
        }
        log.push_back((generation, page));
    }
}

/// A running channel, as the server's routes see it.
#[derive(Clone, Debug)]
pub struct Hub {
    /// The channel's folder.
    pub folder: String,
    /// Its descriptor and contents, which do not change.
    pub descriptor: Arc<[u8]>,
    /// Its contents file.
    pub contents: Arc<[u8]>,
    /// Its shared pages.
    pub pages: Arc<Pages>,
    /// The latest generation: every connection watches it.
    pub generation: watch::Receiver<u64>,
    /// Its task's commands.
    pub commands: mpsc::Sender<Command>,
    /// The server's figures, which every connection adds to.
    pub stats: Arc<ServerStats>,
}

/// Starts `channel` in a task of its own, at `start`, the host's clock's
/// zero; `link`, for a channel whose application is a program of its own,
/// carries that program's lines (`docs/applications.md`).
#[must_use]
pub fn spawn(folder: &str, mut channel: Channel, start: Instant, link: Option<Link>, stats: Arc<ServerStats>) -> Hub {
    let now = move || u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
    channel.start(now());
    let descriptor: Arc<[u8]> = channel.resource("channel.json").map(|resource| resource.body).unwrap_or_default().into();
    let contents: Arc<[u8]> = channel.resource("contents.txt").map(|resource| resource.body).unwrap_or_default().into();
    let pages = Arc::new(Pages::default());
    let (generation_sender, generation) = watch::channel(0_u64);
    let (commands, mut queue) = mpsc::channel::<Command>(COMMANDS);
    let hub = Hub { folder: folder.to_owned(), descriptor, contents, pages: Arc::clone(&pages), generation, commands, stats: Arc::clone(&stats) };
    let folder = folder.to_owned();
    tokio::spawn(async move {
        let mut outboxes: BTreeMap<SessionId, mpsc::Sender<Outbound>> = BTreeMap::new();
        let mut current = 0_u64;
        let (mut from_application, to_application) = link.map_or((None, None), |link| (Some(link.from), Some(link.to)));
        let mut dropped = Dropped::default();
        let mut figures = tokio::time::interval(Duration::from_secs(STATS_EVERY_S));
        let mut sink = Sink {
            pages: &pages,
            generation: &generation_sender,
            current: &mut current,
            outboxes: &outboxes,
            to_application: to_application.as_ref(),
            dropped: &mut dropped,
        };
        sink.apply(channel.take_effects());
        loop {
            let wake = channel.next_wake().and_then(|at| start.checked_add(Duration::from_millis(at)));
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
                        continue;
                    };
                    Event::Application(arrived)
                }
                () = sleep_until(wake) => Event::Deadline,
                _ = figures.tick() => Event::Figures,
            };
            match event {
                Event::Command(Command::Opened { session, outbox }) => {
                    outboxes.insert(session, outbox);
                    channel.opened(session, now());
                }
                Event::Command(Command::Received { session, bytes }) => channel.received(session, &bytes, now()),
                Event::Command(Command::Closed { session }) => {
                    outboxes.remove(&session);
                    channel.closed(session, now());
                }
                Event::Application(FromApplication::Line(text)) => channel.from_application(0, &text, now()),
                Event::Application(FromApplication::Restarted) => {
                    count(&stats.restarts);
                    channel.restarted(0, now());
                }
                Event::Deadline => channel.deadline(now()),
                Event::Figures => {
                    stats.record(&folder, channel.stats());
                    if let Some(to) = to_application.as_ref() {
                        // A figure missed is sent again in ten seconds.
                        let _ = to.try_send(stats.line(channel.stats()));
                    }
                }
            }
            let mut sink = Sink {
                pages: &pages,
                generation: &generation_sender,
                current: &mut current,
                outboxes: &outboxes,
                to_application: to_application.as_ref(),
                dropped: &mut dropped,
            };
            sink.apply(channel.take_effects());
        }
    });
    hub
}

/// What woke the channel's task.
enum Event {
    Command(Command),
    Application(FromApplication),
    Deadline,
    Figures,
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

/// Lines for the program dropped because it fell behind: reported once
/// for the first, then at each power of two, so a stuck program cannot
/// flood the log.
#[derive(Debug, Default)]
struct Dropped(u64);

/// Where the channel's effects go.
struct Sink<'a> {
    pages: &'a Pages,
    generation: &'a watch::Sender<u64>,
    current: &'a mut u64,
    outboxes: &'a BTreeMap<SessionId, mpsc::Sender<Outbound>>,
    to_application: Option<&'a mpsc::Sender<Vec<u8>>>,
    dropped: &'a mut Dropped,
}

impl Sink<'_> {
    /// Carries out the channel's effects: a shared page into the table and
    /// the log, once; what is one session's own into its queue; a line for
    /// the program into its queue.
    fn apply(&mut self, effects: Vec<Effect>) {
        let mut changed = false;
        for effect in effects {
            match effect {
                Effect::Shared { page, frame } => {
                    *self.current = self.current.saturating_add(1);
                    self.pages.set(page, frame, *self.current);
                    changed = true;
                }
                Effect::Own { session, page } => send(self.outboxes, session, Outbound::Own(page)),
                Effect::Disown { session, page } => send(self.outboxes, session, Outbound::Disown(page)),
                Effect::Send { session, bytes } => send(self.outboxes, session, Outbound::Bytes(bytes)),
                Effect::Close { session } => send(self.outboxes, session, Outbound::Close),
                Effect::ToApplication { line, .. } => {
                    if self.to_application.is_some_and(|to| to.try_send(line).is_err()) {
                        self.dropped.0 = self.dropped.0.saturating_add(1);
                        if self.dropped.0.is_power_of_two() {
                            eprintln!("teletext-server: the application is behind: {} lines for it dropped", self.dropped.0);
                        }
                    }
                }
                Effect::Log { text } => eprintln!("teletext-server: {text}"),
            }
        }
        if changed {
            self.generation.send_replace(*self.current);
        }
    }
}

/// Queues `outbound` for `session`, dropping it when the queue is full: a
/// connection that far behind is the slowest client's, not the channel's,
/// problem.
fn send(outboxes: &BTreeMap<SessionId, mpsc::Sender<Outbound>>, session: SessionId, outbound: Outbound) {
    if let Some(outbox) = outboxes.get(&session) {
        let _ = outbox.try_send(outbound);
    }
}
