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
}

/// Starts `channel` in a task of its own, at `start`, the host's clock's
/// zero.
#[must_use]
pub fn spawn(folder: &str, mut channel: Channel, start: Instant) -> Hub {
    let now = move || u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
    channel.start(now());
    let descriptor: Arc<[u8]> = channel.resource("channel.json").map(|resource| resource.body).unwrap_or_default().into();
    let contents: Arc<[u8]> = channel.resource("contents.txt").map(|resource| resource.body).unwrap_or_default().into();
    let pages = Arc::new(Pages::default());
    let (generation_sender, generation) = watch::channel(0_u64);
    let (commands, mut queue) = mpsc::channel::<Command>(COMMANDS);
    let hub = Hub { folder: folder.to_owned(), descriptor, contents, pages: Arc::clone(&pages), generation, commands };
    tokio::spawn(async move {
        let mut outboxes: BTreeMap<SessionId, mpsc::Sender<Outbound>> = BTreeMap::new();
        let mut current = 0_u64;
        apply(channel.take_effects(), &pages, &generation_sender, &mut current, &outboxes);
        loop {
            let wake = channel.next_wake().and_then(|at| start.checked_add(Duration::from_millis(at)));
            let command = match wake {
                Some(at) => tokio::select! {
                    command = queue.recv() => command,
                    () = tokio::time::sleep_until(at) => {
                        channel.deadline(now());
                        apply(channel.take_effects(), &pages, &generation_sender, &mut current, &outboxes);
                        continue;
                    }
                },
                None => queue.recv().await,
            };
            let Some(command) = command else { break };
            match command {
                Command::Opened { session, outbox } => {
                    outboxes.insert(session, outbox);
                    channel.opened(session, now());
                }
                Command::Received { session, bytes } => channel.received(session, &bytes, now()),
                Command::Closed { session } => {
                    outboxes.remove(&session);
                    channel.closed(session, now());
                }
            }
            apply(channel.take_effects(), &pages, &generation_sender, &mut current, &outboxes);
        }
    });
    hub
}

/// Carries out the channel's effects: a shared page into the table and the
/// log, once; what is one session's own into its queue.
fn apply(effects: Vec<Effect>, pages: &Pages, generation: &watch::Sender<u64>, current: &mut u64, outboxes: &BTreeMap<SessionId, mpsc::Sender<Outbound>>) {
    let mut changed = false;
    for effect in effects {
        match effect {
            Effect::Shared { page, frame } => {
                *current = current.saturating_add(1);
                pages.set(page, frame, *current);
                changed = true;
            }
            Effect::Own { session, page } => send(outboxes, session, Outbound::Own(page)),
            Effect::Send { session, bytes } => send(outboxes, session, Outbound::Bytes(bytes)),
            Effect::Close { session } => send(outboxes, session, Outbound::Close),
        }
    }
    if changed {
        generation.send_replace(*current);
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
