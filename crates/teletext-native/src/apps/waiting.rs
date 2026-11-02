//! What waits for a channel's program (DOC-1926, DOC-1945): its lines in
//! order, and each connection's latest move, which replaces the move before
//! it until the program takes it. A move takes none of the room for lines,
//! so a flood of moves can cost a program no key, and however slowly it
//! reads it is given the latest position and never a backlog (REQ-1360).
//!
//! A move keeps its place among its connection's lines. A line about the
//! connection closes the move waiting before it, which no later move
//! replaces, so that a release never overtakes the move that ended a drag
//! and no move overtakes the selection it came after.

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use teletext_runtime::ConnectionId;
use tokio::sync::mpsc;

use super::WAITING_LINES;

/// One thing waiting.
#[derive(Debug)]
struct Entry {
    /// What is written, with its newline.
    line: Vec<u8>,
    /// The connection it is a move of; a line is of none.
    move_of: Option<ConnectionId>,
}

/// Everything waiting: at most [`WAITING_LINES`] lines, at most as many
/// moves closed by one of them, and one open move a connection.
#[derive(Debug, Default)]
struct Queue {
    entries: VecDeque<Entry>,
    /// The number of `entries`' first: each entry has one, for life.
    first: u64,
    /// The lines among `entries`.
    lines: usize,
    /// Each connection's move a newer one may replace, by its number.
    open: BTreeMap<ConnectionId, u64>,
}

impl Queue {
    fn line(&mut self, about: Option<ConnectionId>, line: Vec<u8>) -> bool {
        if self.lines >= WAITING_LINES {
            return false;
        }
        if let Some(connection) = about {
            self.open.remove(&connection);
        }
        self.lines = self.lines.saturating_add(1);
        self.entries.push_back(Entry { line, move_of: None });
        true
    }

    fn latest_move(&mut self, connection: ConnectionId, line: Vec<u8>) {
        if let Some(entry) = self.open_entry(connection) {
            entry.line = line;
            return;
        }
        let number = self.first.saturating_add(u64::try_from(self.entries.len()).unwrap_or(u64::MAX));
        self.entries.push_back(Entry { line, move_of: Some(connection) });
        self.open.insert(connection, number);
    }

    /// `connection`'s open move.
    fn open_entry(&mut self, connection: ConnectionId) -> Option<&mut Entry> {
        let place = self.open.get(&connection)?.checked_sub(self.first)?;
        self.entries.get_mut(usize::try_from(place).ok()?)
    }

    fn take(&mut self) -> Option<Vec<u8>> {
        let entry = self.entries.pop_front()?;
        match entry.move_of {
            Some(connection) => {
                if self.open.get(&connection) == Some(&self.first) {
                    self.open.remove(&connection);
                }
            }
            None => self.lines = self.lines.saturating_sub(1),
        }
        self.first = self.first.saturating_add(1);
        Some(entry.line)
    }

    fn clear(&mut self) {
        *self = Self { first: self.first, ..Self::default() };
    }
}

/// Puts a poisoned lock's queue back in use: it is whole between calls, and
/// the channel must go on.
fn held(queue: &Mutex<Queue>) -> MutexGuard<'_, Queue> {
    queue.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The channel's end. It never waits, since the network loop never waits
/// for an application (DOC-1965).
#[derive(Clone, Debug)]
pub struct ToProgram {
    queue: Arc<Mutex<Queue>>,
    waiting: mpsc::Sender<()>,
}

impl ToProgram {
    /// Queues a line, `about` a connection if it is, unless the room for
    /// lines is full, when it answers false and the line is dropped.
    #[must_use]
    pub fn line(&self, about: Option<ConnectionId>, line: Vec<u8>) -> bool {
        let queued = held(&self.queue).line(about, line);
        self.wake();
        queued
    }

    /// `connection`'s latest move, which takes the place of one not taken.
    pub fn latest_move(&self, connection: ConnectionId, line: Vec<u8>) {
        held(&self.queue).latest_move(connection, line);
        self.wake();
    }

    /// One waiting wake is enough for all there is, so one that does not
    /// fit needs none.
    fn wake(&self) {
        let _ = self.waiting.try_send(());
    }
}

/// The end that writes to the program.
#[derive(Debug)]
pub struct Outgoing {
    queue: Arc<Mutex<Queue>>,
    waiting: mpsc::Receiver<()>,
}

impl Outgoing {
    /// The next thing to write, when there is one; none once the channel
    /// has stopped and nothing is left.
    pub async fn next(&mut self) -> Option<Vec<u8>> {
        loop {
            if let Some(line) = held(&self.queue).take() {
                return Some(line);
            }
            self.waiting.recv().await?;
        }
    }

    /// Drops everything waiting.
    pub fn clear(&mut self) {
        held(&self.queue).clear();
    }
}

/// The two ends of what waits for one program.
#[must_use]
pub fn waiting() -> (ToProgram, Outgoing) {
    let queue = Arc::new(Mutex::new(Queue::default()));
    let (waiting_sender, waiting_receiver) = mpsc::channel(1);
    (ToProgram { queue: Arc::clone(&queue), waiting: waiting_sender }, Outgoing { queue, waiting: waiting_receiver })
}

#[cfg(test)]
mod tests {
    use super::*;

    const ONE: ConnectionId = 1;
    const TWO: ConnectionId = 2;

    fn everything(queue: &mut Queue) -> Vec<Vec<u8>> {
        std::iter::from_fn(|| queue.take()).collect()
    }

    /// REQ-1360: of any number of moves not yet taken, the program is given
    /// each connection's latest, in the place of its first.
    #[test]
    fn the_latest_move_takes_the_place_of_those_not_taken() {
        let mut queue = Queue::default();
        for position in 0..1_000 {
            queue.latest_move(ONE, format!("one {position}").into_bytes());
            queue.latest_move(TWO, format!("two {position}").into_bytes());
        }
        assert_eq!(everything(&mut queue), [b"one 999".to_vec(), b"two 999".to_vec()]);
        assert!(queue.open.is_empty(), "nothing is left to replace");
        queue.latest_move(ONE, b"later".to_vec());
        assert_eq!(everything(&mut queue), [b"later".to_vec()], "a move after the last taken waits afresh");
    }

    /// DOC-1945: a release never comes before the move that ended the drag,
    /// and a later move never comes before the release.
    #[test]
    fn a_line_about_a_connection_closes_its_move() {
        let mut queue = Queue::default();
        queue.latest_move(ONE, b"move 1".to_vec());
        queue.latest_move(TWO, b"other 1".to_vec());
        assert!(queue.line(Some(ONE), b"release".to_vec()));
        queue.latest_move(ONE, b"move 2".to_vec());
        queue.latest_move(ONE, b"move 3".to_vec());
        queue.latest_move(TWO, b"other 2".to_vec());
        assert_eq!(everything(&mut queue), [&b"move 1"[..], b"other 2", b"release", b"move 3"].map(<[u8]>::to_vec));
    }

    /// A line about nothing, or about another connection, moves no move.
    #[test]
    fn other_lines_leave_a_move_open() {
        let mut queue = Queue::default();
        queue.latest_move(ONE, b"move 1".to_vec());
        assert!(queue.line(None, b"stats".to_vec()));
        assert!(queue.line(Some(TWO), b"key".to_vec()));
        queue.latest_move(ONE, b"move 2".to_vec());
        assert_eq!(everything(&mut queue), [&b"move 2"[..], b"stats", b"key"].map(<[u8]>::to_vec));
    }

    /// DOC-1926: the room is for lines, and moves take none of it, however
    /// many connections send them. The newest line beyond it is dropped.
    #[test]
    fn moves_take_no_room_from_lines() {
        let mut queue = Queue::default();
        for connection in 1..=ConnectionId::try_from(WAITING_LINES * 2).unwrap() {
            queue.latest_move(connection, b"move".to_vec());
        }
        for _ in 0..WAITING_LINES {
            assert!(queue.line(None, b"key".to_vec()), "every key fits");
        }
        assert!(!queue.line(None, b"one too many".to_vec()));
        assert_eq!(queue.take(), Some(b"move".to_vec()));
        assert!(!queue.line(None, b"a move taken makes no room".to_vec()));
    }

    /// What waits when a program ends goes, and what the numbers said of
    /// it: a move after that is not taken for the last program's.
    #[test]
    fn clearing_leaves_nothing_to_replace() {
        let mut queue = Queue::default();
        assert!(queue.line(None, b"key".to_vec()));
        queue.latest_move(ONE, b"move".to_vec());
        let _ = queue.take();
        queue.clear();
        assert_eq!(queue.lines, 0);
        queue.latest_move(ONE, b"after".to_vec());
        assert_eq!(everything(&mut queue), [b"after".to_vec()]);
    }

    #[tokio::test]
    async fn the_writing_end_waits_for_what_comes_and_ends_with_the_channel() {
        let (to, mut outgoing) = waiting();
        assert!(to.line(None, b"first".to_vec()));
        assert_eq!(outgoing.next().await, Some(b"first".to_vec()));
        let later = {
            let to = to.clone();
            tokio::spawn(async move { to.latest_move(ONE, b"second".to_vec()) })
        };
        assert_eq!(outgoing.next().await, Some(b"second".to_vec()));
        later.await.unwrap();
        assert!(to.line(None, b"last".to_vec()));
        drop(to);
        assert_eq!(outgoing.next().await, Some(b"last".to_vec()), "what is left is still written");
        assert_eq!(outgoing.next().await, None);
    }
}
