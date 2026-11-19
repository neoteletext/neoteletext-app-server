//! What a channel counts about itself, for its operator's log and for its
//! program to show (DOC-1968). Counts only:
//! nothing that identifies a viewer, no address, no text typed.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

/// How many pages [`ChannelStats::most_opened`] names.
pub const MOST_OPENED: usize = 10;

/// A channel's counts since it started.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChannelStats {
    /// Viewers connected now.
    pub viewers: u64,
    /// The most connected at once.
    pub viewers_peak: u64,
    /// Every viewer that has connected.
    pub viewers_total: u64,
    /// How often each page was opened.
    pub opens: BTreeMap<u16, u64>,
    /// Input events from viewers: keys, text, the mouse and the wheel.
    pub inputs: u64,
    /// Key presses among them.
    pub keys: u64,
    /// Pages sent to a connection, each copy counted.
    pub pages_sent: u64,
    /// Sub-pages a program sent to connections it named whose selection
    /// they were not, so not forwarded (REQ-1473). A page sent to every
    /// connection is not counted: most of them select something else.
    pub subpages_not_forwarded: u64,
    /// Lines from the program refused (REQ-1475).
    pub refused: u64,
    /// Packets not sent because they failed the stream's packet check: a
    /// fault of the server or the engine (REQ-1462).
    pub invalid_packets: u64,
    /// Input dropped because it was stamped with a page other than the
    /// connection's selection (REQ-1557).
    pub stale_input: u64,
    /// Answers dropped because they named a selection already replaced
    /// (DEC-1759 lesson 13).
    pub stale_answers: u64,
    /// Viewers the application redirected, to a page or another channel.
    pub redirects: u64,
    /// Audio records sent: sound objects, their data and commands.
    pub sound_records: u64,
    /// Lines sent to the application's process.
    pub lines_to_application: u64,
    /// Lines from it.
    pub lines_from_application: u64,
    /// Viewers' proofs that verified (VS-206): a count, nothing of the
    /// proofs (VS-209).
    pub proofs_accepted: u64,
    /// Viewers' proofs refused: not verified, with no challenge, or past a
    /// bound (VS-206, VS-304).
    pub proofs_refused: u64,
    /// Snapshot connections held (REQ-6881 item 2): the server's own
    /// figure, never among the viewers' or in the program's `stats`.
    pub snapshots: u64,
}

impl ChannelStats {
    /// A viewer connected.
    pub(crate) fn joined(&mut self) {
        self.viewers = self.viewers.saturating_add(1);
        self.viewers_total = self.viewers_total.saturating_add(1);
        self.viewers_peak = self.viewers_peak.max(self.viewers);
    }

    /// A viewer's connection closed.
    pub(crate) fn left(&mut self) {
        self.viewers = self.viewers.saturating_sub(1);
    }

    /// A viewer opened `page`.
    pub(crate) fn opened(&mut self, page: u16) {
        let count = self.opens.entry(page).or_insert(0);
        *count = count.saturating_add(1);
    }

    /// Every page opened, the most opened first, at most [`MOST_OPENED`].
    #[must_use]
    pub fn most_opened(&self) -> Vec<(u16, u64)> {
        let mut ranked: Vec<(u16, u64)> = self.opens.iter().map(|(&page, &count)| (page, count)).collect();
        ranked.sort_by(|left, right| right.1.cmp(&left.1).then(left.0.cmp(&right.0)));
        ranked.truncate(MOST_OPENED);
        ranked
    }

    /// Every page opened, all told.
    #[must_use]
    pub fn total_opens(&self) -> u64 {
        self.opens.values().fold(0, |total, &count| total.saturating_add(count))
    }
}

/// Adds one to `counter`.
pub(crate) const fn count(counter: &mut u64) {
    *counter = counter.saturating_add(1);
}
