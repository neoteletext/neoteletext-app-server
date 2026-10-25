//! The server's figures: what every connection, request and channel adds
//! up to, for the log, for `/stats.json`, and for the channels' own pages
//! (`docs/applications.md`, `stats`). Counts only: no address, no text.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::Instant;

use teletext_runtime::ChannelStats;
use teletext_runtime::json::Writer;

/// The figures, shared by every task.
#[derive(Debug)]
pub struct ServerStats {
    started: Instant,
    /// WebSocket connections open now.
    pub connections: AtomicU64,
    /// The most open at once.
    pub connections_peak: AtomicU64,
    /// Every connection made.
    pub connections_total: AtomicU64,
    /// WebSocket messages sent.
    pub messages_sent: AtomicU64,
    /// Bytes sent in them.
    pub bytes_sent: AtomicU64,
    /// HTTP requests answered.
    pub requests: AtomicU64,
    /// Of them, answered "not modified" from the client's `ETag`.
    pub not_modified: AtomicU64,
    /// Applications started again after they ended.
    pub restarts: AtomicU64,
    /// Each channel's latest counts, by folder.
    channels: Mutex<BTreeMap<String, ChannelStats>>,
}

impl Default for ServerStats {
    fn default() -> Self {
        Self {
            started: Instant::now(),
            connections: AtomicU64::new(0),
            connections_peak: AtomicU64::new(0),
            connections_total: AtomicU64::new(0),
            messages_sent: AtomicU64::new(0),
            bytes_sent: AtomicU64::new(0),
            requests: AtomicU64::new(0),
            not_modified: AtomicU64::new(0),
            restarts: AtomicU64::new(0),
            channels: Mutex::new(BTreeMap::new()),
        }
    }
}

/// Adds one to `counter`.
pub fn count(counter: &AtomicU64) {
    counter.fetch_add(1, Ordering::Relaxed);
}

/// Reads `counter`.
fn read(counter: &AtomicU64) -> i64 {
    i64::try_from(counter.load(Ordering::Relaxed)).unwrap_or(i64::MAX)
}

impl ServerStats {
    /// A connection opened.
    pub fn connected(&self) {
        count(&self.connections_total);
        let now = self.connections.fetch_add(1, Ordering::Relaxed).saturating_add(1);
        self.connections_peak.fetch_max(now, Ordering::Relaxed);
    }

    /// A connection closed.
    pub fn disconnected(&self) {
        let _ = self.connections.try_update(Ordering::Relaxed, Ordering::Relaxed, |now| Some(now.saturating_sub(1)));
    }

    /// A message of `bytes` was sent.
    pub fn sent(&self, bytes: usize) {
        count(&self.messages_sent);
        self.bytes_sent.fetch_add(u64::try_from(bytes).unwrap_or(u64::MAX), Ordering::Relaxed);
    }

    /// Records a channel's latest counts.
    pub fn record(&self, folder: &str, stats: &ChannelStats) {
        self.channels.lock().unwrap_or_else(PoisonError::into_inner).insert(folder.to_owned(), stats.clone());
    }

    /// Seconds since the server started.
    #[must_use]
    pub fn uptime_s(&self) -> u64 {
        self.started.elapsed().as_secs()
    }

    /// The server's figures, as an object.
    #[must_use]
    pub fn server(&self) -> Writer {
        let mut server = Writer::nested();
        server
            .integer("uptime_s", i64::try_from(self.uptime_s()).unwrap_or(i64::MAX))
            .integer("connections", read(&self.connections))
            .integer("connections_peak", read(&self.connections_peak))
            .integer("connections_total", read(&self.connections_total))
            .integer("messages_sent", read(&self.messages_sent))
            .integer("bytes_sent", read(&self.bytes_sent))
            .integer("requests", read(&self.requests))
            .integer("not_modified", read(&self.not_modified))
            .integer("restarts", read(&self.restarts))
            .integer("memory_kb", memory_kb().and_then(|kb| i64::try_from(kb).ok()).unwrap_or(-1));
        let channels = self.channels.lock().unwrap_or_else(PoisonError::into_inner);
        server.integer("channels", i64::try_from(channels.len()).unwrap_or(i64::MAX));
        let mut viewers = Writer::nested();
        for (folder, stats) in channels.iter() {
            viewers.integer(folder, i64::try_from(stats.viewers).unwrap_or(i64::MAX));
        }
        server.object("viewers", viewers);
        server
    }

    /// The `stats` line for a channel's application (`docs/applications.md`).
    #[must_use]
    pub fn line(&self, stats: &ChannelStats) -> Vec<u8> {
        let mut line = Writer::new("stats");
        line.object("server", self.server()).object("channel", channel(stats));
        line.line()
    }

    /// Every figure, for `/stats.json`.
    #[must_use]
    pub fn document(&self) -> Vec<u8> {
        let mut document = Writer::new("stats");
        document.object("server", self.server());
        let mut channels = Writer::nested();
        for (folder, stats) in self.channels.lock().unwrap_or_else(PoisonError::into_inner).iter() {
            channels.object(folder, channel(stats));
        }
        document.object("channels", channels);
        document.line()
    }

    /// One line for the log: the server and each channel at a glance.
    #[must_use]
    pub fn summary(&self) -> String {
        let channels = self.channels.lock().unwrap_or_else(PoisonError::into_inner);
        let each: Vec<String> = channels.iter().map(|(folder, stats)| format!("{folder} {} (peak {})", stats.viewers, stats.viewers_peak)).collect();
        format!(
            "up {} s; {} connections (peak {}, {} in all); {} messages, {} KB sent; {} requests ({} not modified); viewers: {}",
            self.uptime_s(),
            read(&self.connections),
            read(&self.connections_peak),
            read(&self.connections_total),
            read(&self.messages_sent),
            read(&self.bytes_sent) / 1024,
            read(&self.requests),
            read(&self.not_modified),
            each.join(", ")
        )
    }
}

/// A channel's counts, as an object.
fn channel(stats: &ChannelStats) -> Writer {
    let number = |value: u64| i64::try_from(value).unwrap_or(i64::MAX);
    let mut channel = Writer::nested();
    channel
        .integer("viewers", number(stats.viewers))
        .integer("viewers_peak", number(stats.viewers_peak))
        .integer("viewers_total", number(stats.viewers_total))
        .integer("opens", number(stats.total_opens()))
        .integer("inputs", number(stats.inputs))
        .integer("keys", number(stats.keys))
        .integer("shared_published", number(stats.shared_published))
        .integer("own_published", number(stats.own_published))
        .integer("gotos", number(stats.gotos))
        .integer("sound_records", number(stats.sound_records))
        .integer("lines_to_application", number(stats.lines_to_application))
        .integer("lines_from_application", number(stats.lines_from_application));
    let mut opened = Writer::nested();
    for (page, opens) in stats.most_opened() {
        opened.integer(&format!("{page:03X}"), number(opens));
    }
    channel.object("most_opened", opened);
    channel
}

/// The server's resident memory in KB, where the system says
/// (`/proc/self/status`'s `VmRSS`, Linux); elsewhere none.
#[must_use]
pub fn memory_kb() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    status.lines().find_map(|line| line.strip_prefix("VmRSS:")).and_then(|rest| rest.split_whitespace().next()).and_then(|kb| kb.parse().ok())
}

#[cfg(test)]
mod tests;
