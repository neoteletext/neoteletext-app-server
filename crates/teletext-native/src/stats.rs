//! The server's figures: what every connection, request and channel adds
//! up to, for the log, for `/stats.json`, for the channels' own pages
//! (`docs/applications.md`, `stats`) and for `/metrics` (decision 19).
//! Counts only: no address, no text.
//!
//! Two clocks: the figures since the process started, which `/metrics`
//! gives as Prometheus expects, and the totals since the state directory
//! was first used, which `all_time` gives (requirement 8.3).

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use teletext_runtime::ChannelStats;
use teletext_runtime::json::{Value, Writer};

/// The upper bounds of the channel's time over an event, in microseconds
/// (requirement 8.2): from what a key costs to what a whole channel's
/// republication may.
pub const EVENT_BUCKETS_US: [u64; 11] = [50, 100, 250, 500, 1_000, 2_500, 5_000, 10_000, 25_000, 50_000, 100_000];
/// The accumulated figures' format, in `figures.json`.
const FIGURES_VERSION: i64 = 1;
/// Microseconds in a second, for Prometheus's base unit.
const MICROSECONDS_PER_SECOND: f64 = 1_000_000.0;
/// Bytes in the kilobyte `/proc/self/status` counts in.
const BYTES_PER_KB: u64 = 1_024;

/// How a figure accumulates across restarts.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    /// It adds up.
    Counter,
    /// The most there ever was.
    Peak,
}

/// The server's figures accumulated across restarts.
const SERVER_TOTALS: [(&str, Kind); 7] = [
    ("connections_total", Kind::Counter),
    ("connections_peak", Kind::Peak),
    ("messages_sent", Kind::Counter),
    ("bytes_sent", Kind::Counter),
    ("requests", Kind::Counter),
    ("not_modified", Kind::Counter),
    ("restarts", Kind::Counter),
];
/// A channel's figures accumulated across restarts.
const CHANNEL_TOTALS: [(&str, Kind); 13] = [
    ("viewers_total", Kind::Counter),
    ("viewers_peak", Kind::Peak),
    ("opens", Kind::Counter),
    ("inputs", Kind::Counter),
    ("keys", Kind::Counter),
    ("shared_published", Kind::Counter),
    ("own_published", Kind::Counter),
    ("gotos", Kind::Counter),
    ("sound_records", Kind::Counter),
    ("lines_to_application", Kind::Counter),
    ("lines_from_application", Kind::Counter),
    ("lines_dropped", Kind::Counter),
    ("sessions_resumed", Kind::Counter),
];

/// How long a channel's task takes over one event and its effects.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Histogram {
    /// Events in each bucket of [`EVENT_BUCKETS_US`], not cumulative; the
    /// last counts those over every bound.
    buckets: [u64; EVENT_BUCKETS_US.len() + 1],
    count: u64,
    sum_us: u64,
}

impl Histogram {
    /// Counts one event that took `took`.
    pub fn observe(&mut self, took: Duration) {
        let micros = u64::try_from(took.as_micros()).unwrap_or(u64::MAX);
        let bucket = EVENT_BUCKETS_US.iter().position(|&bound| micros <= bound).unwrap_or(EVENT_BUCKETS_US.len());
        if let Some(count) = self.buckets.get_mut(bucket) {
            *count = count.saturating_add(1);
        }
        self.count = self.count.saturating_add(1);
        self.sum_us = self.sum_us.saturating_add(micros);
    }
}

/// What a channel's task reports of itself.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChannelFigures {
    /// What the runtime counted.
    pub stats: ChannelStats,
    /// Lines for its program dropped because it fell behind.
    pub lines_dropped: u64,
    /// Whether it has pages to serve (requirement 8.4).
    pub ready: bool,
    /// Its time over each event.
    pub events: Histogram,
}

/// Figures by name, and each page's opens, accumulated across restarts.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Totals {
    counts: BTreeMap<String, u64>,
    opens: BTreeMap<u16, u64>,
}

impl Totals {
    /// `base` from before this run with `now`, this run's, added or the
    /// greater taken as each figure's kind says.
    fn combined(base: &Self, now: &Self, kinds: &[(&str, Kind)]) -> Self {
        let mut counts = BTreeMap::new();
        for &(name, kind) in kinds {
            let before = base.counts.get(name).copied().unwrap_or(0);
            let here = now.counts.get(name).copied().unwrap_or(0);
            let value = match kind {
                Kind::Counter => before.saturating_add(here),
                Kind::Peak => before.max(here),
            };
            counts.insert(name.to_owned(), value);
        }
        let mut opens = base.opens.clone();
        for (&page, &count) in &now.opens {
            let total = opens.entry(page).or_insert(0);
            *total = total.saturating_add(count);
        }
        Self { counts, opens }
    }

    /// As a JSON object.
    fn writer(&self) -> Writer {
        let mut writer = Writer::nested();
        for (name, &value) in &self.counts {
            writer.integer(name, number(value));
        }
        if !self.opens.is_empty() {
            let mut opens = Writer::nested();
            for (&page, &count) in &self.opens {
                opens.integer(&format!("{page:03X}"), number(count));
            }
            writer.object("pages_opened", opens);
        }
        writer
    }

    /// Read back from [`Totals::writer`]'s object; what does not read is
    /// left out.
    fn read(value: &Value, kinds: &[(&str, Kind)]) -> Self {
        let mut totals = Self::default();
        for &(name, _) in kinds {
            if let Some(count) = value.get(name).and_then(Value::integer).and_then(|count| u64::try_from(count).ok()) {
                totals.counts.insert(name.to_owned(), count);
            }
        }
        if let Some(Value::Object(pages)) = value.get("pages_opened") {
            for (page, count) in pages {
                let page = u16::from_str_radix(page, 16).ok().filter(|page| (0x100..=0x8FF).contains(page));
                if let (Some(page), Some(count)) = (page, count.integer().and_then(|count| u64::try_from(count).ok())) {
                    totals.opens.insert(page, count);
                }
            }
        }
        totals
    }
}

/// The figures, shared by every task.
#[derive(Debug)]
pub struct ServerStats {
    started: Instant,
    started_epoch: Duration,
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
    /// Woken whenever a connection closes, for a stop that waits for them
    /// all (requirement 5.1).
    pub closed: tokio::sync::Notify,
    /// Each channel's latest figures, by folder.
    channels: Mutex<BTreeMap<String, ChannelFigures>>,
    /// The server's totals from before this run.
    server_base: Mutex<Totals>,
    /// Each channel's totals from before this run, by folder.
    channels_base: Mutex<BTreeMap<String, Totals>>,
}

impl Default for ServerStats {
    fn default() -> Self {
        Self {
            started: Instant::now(),
            started_epoch: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default(),
            connections: AtomicU64::new(0),
            connections_peak: AtomicU64::new(0),
            connections_total: AtomicU64::new(0),
            messages_sent: AtomicU64::new(0),
            bytes_sent: AtomicU64::new(0),
            requests: AtomicU64::new(0),
            not_modified: AtomicU64::new(0),
            restarts: AtomicU64::new(0),
            closed: tokio::sync::Notify::new(),
            channels: Mutex::new(BTreeMap::new()),
            server_base: Mutex::new(Totals::default()),
            channels_base: Mutex::new(BTreeMap::new()),
        }
    }
}

/// Adds one to `counter`.
pub fn count(counter: &AtomicU64) {
    counter.fetch_add(1, Ordering::Relaxed);
}

/// Reads `counter`.
fn load(counter: &AtomicU64) -> u64 {
    counter.load(Ordering::Relaxed)
}

/// A count as JSON writes it.
fn number(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
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
        self.closed.notify_waiters();
    }

    /// Waits until no connection is open.
    pub async fn all_closed(&self) {
        loop {
            let closed = self.closed.notified();
            tokio::pin!(closed);
            closed.as_mut().enable();
            if load(&self.connections) == 0 {
                return;
            }
            closed.await;
        }
    }

    /// A message of `bytes` was sent.
    pub fn sent(&self, bytes: usize) {
        count(&self.messages_sent);
        self.bytes_sent.fetch_add(u64::try_from(bytes).unwrap_or(u64::MAX), Ordering::Relaxed);
    }

    /// Records a channel's latest figures.
    pub fn record(&self, folder: &str, figures: ChannelFigures) {
        self.channels.lock().unwrap_or_else(PoisonError::into_inner).insert(folder.to_owned(), figures);
    }

    /// Seconds since the server started.
    #[must_use]
    pub fn uptime_s(&self) -> u64 {
        self.started.elapsed().as_secs()
    }

    /// The channels that have no pages to serve yet (requirement 8.4).
    #[must_use]
    pub fn not_ready(&self) -> Vec<String> {
        self.channels.lock().unwrap_or_else(PoisonError::into_inner).iter().filter(|(_, figures)| !figures.ready).map(|(folder, _)| folder.clone()).collect()
    }

    /// Takes the totals kept from before this run (requirement 8.3).
    pub fn accumulate(&self, figures: &Value) {
        if let Some(server) = figures.get("server") {
            *self.server_base.lock().unwrap_or_else(PoisonError::into_inner) = Totals::read(server, &SERVER_TOTALS);
        }
        if let Some(Value::Object(channels)) = figures.get("channels") {
            let mut base = self.channels_base.lock().unwrap_or_else(PoisonError::into_inner);
            for (folder, totals) in channels {
                base.insert(folder.clone(), Totals::read(totals, &CHANNEL_TOTALS));
            }
        }
    }

    /// This run's server totals.
    fn server_now(&self) -> Totals {
        let counts = [
            ("connections_total", load(&self.connections_total)),
            ("connections_peak", load(&self.connections_peak)),
            ("messages_sent", load(&self.messages_sent)),
            ("bytes_sent", load(&self.bytes_sent)),
            ("requests", load(&self.requests)),
            ("not_modified", load(&self.not_modified)),
            ("restarts", load(&self.restarts)),
        ];
        Totals { counts: counts.into_iter().map(|(name, value)| (name.to_owned(), value)).collect(), opens: BTreeMap::new() }
    }

    /// The server's totals across restarts.
    fn server_all_time(&self) -> Totals {
        Totals::combined(&self.server_base.lock().unwrap_or_else(PoisonError::into_inner), &self.server_now(), &SERVER_TOTALS)
    }

    /// A channel's totals across restarts.
    fn channel_all_time(&self, folder: &str, figures: Option<&ChannelFigures>) -> Totals {
        let base = self.channels_base.lock().unwrap_or_else(PoisonError::into_inner).get(folder).cloned().unwrap_or_default();
        let now = figures.map(channel_now).unwrap_or_default();
        Totals::combined(&base, &now, &CHANNEL_TOTALS)
    }

    /// The accumulated figures, for `figures.json` (requirement 8.3).
    #[must_use]
    pub fn all_time_document(&self) -> Vec<u8> {
        let mut document = Writer::new("figures");
        document.integer("version", FIGURES_VERSION).object("server", self.server_all_time().writer());
        let mut channels = Writer::nested();
        let latest = self.channels.lock().unwrap_or_else(PoisonError::into_inner).clone();
        let mut folders: Vec<String> = latest.keys().cloned().collect();
        folders.extend(self.channels_base.lock().unwrap_or_else(PoisonError::into_inner).keys().cloned());
        folders.sort();
        folders.dedup();
        for folder in folders {
            channels.object(&folder, self.channel_all_time(&folder, latest.get(&folder)).writer());
        }
        document.object("channels", channels);
        document.line()
    }

    /// The server's figures, as an object.
    #[must_use]
    pub fn server(&self) -> Writer {
        let mut server = Writer::nested();
        server
            .integer("uptime_s", number(self.uptime_s()))
            .integer("connections", number(load(&self.connections)))
            .integer("connections_peak", number(load(&self.connections_peak)))
            .integer("connections_total", number(load(&self.connections_total)))
            .integer("messages_sent", number(load(&self.messages_sent)))
            .integer("bytes_sent", number(load(&self.bytes_sent)))
            .integer("requests", number(load(&self.requests)))
            .integer("not_modified", number(load(&self.not_modified)))
            .integer("restarts", number(load(&self.restarts)))
            .integer("memory_kb", memory_kb().map_or(-1, number));
        let channels = self.channels.lock().unwrap_or_else(PoisonError::into_inner);
        server.integer("channels", i64::try_from(channels.len()).unwrap_or(i64::MAX));
        let mut viewers = Writer::nested();
        for (folder, figures) in channels.iter() {
            viewers.integer(folder, number(figures.stats.viewers));
        }
        server.object("viewers", viewers);
        drop(channels);
        server.object("all_time", self.server_all_time().writer());
        server
    }

    /// The `stats` line for a channel's application (`docs/applications.md`).
    #[must_use]
    pub fn line(&self, folder: &str, figures: &ChannelFigures) -> Vec<u8> {
        let mut line = Writer::new("stats");
        line.object("server", self.server()).object("channel", self.channel(folder, figures));
        line.line()
    }

    /// Every figure, for `/stats.json`.
    #[must_use]
    pub fn document(&self) -> Vec<u8> {
        let mut document = Writer::new("stats");
        document.object("server", self.server());
        let mut channels = Writer::nested();
        let latest = self.channels.lock().unwrap_or_else(PoisonError::into_inner).clone();
        for (folder, figures) in &latest {
            channels.object(folder, self.channel(folder, figures));
        }
        document.object("channels", channels);
        document.line()
    }

    /// A channel's figures, as an object.
    fn channel(&self, folder: &str, figures: &ChannelFigures) -> Writer {
        let stats = &figures.stats;
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
            .integer("lines_from_application", number(stats.lines_from_application))
            .integer("lines_dropped", number(figures.lines_dropped))
            .integer("sessions_waiting", number(stats.sessions_waiting))
            .integer("sessions_resumed", number(stats.sessions_resumed));
        let mut opened = Writer::nested();
        for (page, opens) in stats.most_opened() {
            opened.integer(&format!("{page:03X}"), number(opens));
        }
        channel.object("most_opened", opened);
        channel.object("all_time", self.channel_all_time(folder, Some(figures)).writer());
        channel
    }

    /// The figures at a glance, for the log's event once a minute.
    #[must_use]
    pub fn glance(&self) -> Glance {
        let channels = self.channels.lock().unwrap_or_else(PoisonError::into_inner);
        let each: Vec<String> =
            channels.iter().map(|(folder, figures)| format!("{folder} {} (peak {})", figures.stats.viewers, figures.stats.viewers_peak)).collect();
        Glance {
            uptime_s: self.uptime_s(),
            connections: load(&self.connections),
            connections_peak: load(&self.connections_peak),
            connections_total: load(&self.connections_total),
            messages_sent: load(&self.messages_sent),
            bytes_sent: load(&self.bytes_sent),
            requests: load(&self.requests),
            not_modified: load(&self.not_modified),
            viewers: each.join(", "),
        }
    }

    /// Every figure in the Prometheus text exposition format, version
    /// 0.0.4 (requirements 8.1 and 8.2).
    #[must_use]
    pub fn metrics(&self) -> String {
        let mut text = String::new();
        let mut family = |name: &str, kind: &str, help: &str, samples: &[(String, f64)]| {
            let _ = writeln!(text, "# HELP {name} {help}\n# TYPE {name} {kind}");
            for (labels, value) in samples {
                let _ = writeln!(text, "{name}{labels} {value}");
            }
        };
        let one = |value: u64| vec![(String::new(), as_float(value))];
        family("teletext_build_info", "gauge", "The server's version.", &[(format!("{{version=\"{}\"}}", env!("CARGO_PKG_VERSION")), 1.0)]);
        family(
            "process_start_time_seconds",
            "gauge",
            "When the process started, in seconds since the Unix epoch.",
            &[(String::new(), self.started_epoch.as_secs_f64())],
        );
        if let Some(kb) = memory_kb() {
            family("process_resident_memory_bytes", "gauge", "Resident memory.", &one(kb.saturating_mul(BYTES_PER_KB)));
        }
        family("teletext_connections", "gauge", "WebSocket connections open now.", &one(load(&self.connections)));
        family("teletext_connections_peak", "gauge", "The most WebSocket connections open at once.", &one(load(&self.connections_peak)));
        family("teletext_connections_total", "counter", "WebSocket connections made.", &one(load(&self.connections_total)));
        family("teletext_messages_sent_total", "counter", "WebSocket messages sent.", &one(load(&self.messages_sent)));
        family("teletext_sent_bytes_total", "counter", "Bytes sent in WebSocket messages.", &one(load(&self.bytes_sent)));
        family("teletext_http_requests_total", "counter", "HTTP requests answered.", &one(load(&self.requests)));
        family("teletext_http_not_modified_total", "counter", "HTTP requests answered not modified.", &one(load(&self.not_modified)));
        family("teletext_program_restarts_total", "counter", "Programs started again after they ended.", &one(load(&self.restarts)));

        let channels = self.channels.lock().unwrap_or_else(PoisonError::into_inner).clone();
        let each = |value: fn(&ChannelFigures) -> u64| -> Vec<(String, f64)> {
            channels.iter().map(|(folder, figures)| (format!("{{channel=\"{}\"}}", label(folder)), as_float(value(figures)))).collect()
        };
        let gauges: [Figure; 4] = [
            ("teletext_channel_viewers", "Viewers connected now.", |figures| figures.stats.viewers),
            ("teletext_channel_viewers_peak", "The most viewers connected at once.", |figures| figures.stats.viewers_peak),
            ("teletext_channel_sessions_waiting", "Sessions waiting for their viewer to come back.", |figures| figures.stats.sessions_waiting),
            ("teletext_channel_ready", "Whether the channel has pages to serve.", |figures| u64::from(figures.ready)),
        ];
        for (name, help, value) in gauges {
            family(name, "gauge", help, &each(value));
        }
        let counters: [Figure; 13] = [
            ("teletext_channel_viewers_total", "Viewers that connected.", |figures| figures.stats.viewers_total),
            ("teletext_channel_sessions_resumed_total", "Sessions a viewer came back to.", |figures| figures.stats.sessions_resumed),
            ("teletext_channel_page_opens_total", "Pages opened.", |figures| figures.stats.total_opens()),
            ("teletext_channel_inputs_total", "Input events from viewers.", |figures| figures.stats.inputs),
            ("teletext_channel_keys_total", "Key presses from viewers.", |figures| figures.stats.keys),
            ("teletext_channel_shared_published_total", "Shared pages published.", |figures| figures.stats.shared_published),
            ("teletext_channel_own_published_total", "Pages published for one viewer.", |figures| figures.stats.own_published),
            ("teletext_channel_gotos_total", "Viewers sent to a page.", |figures| figures.stats.gotos),
            ("teletext_channel_sound_records_total", "Audio records sent.", |figures| figures.stats.sound_records),
            ("teletext_channel_program_lines_sent_total", "Lines sent to the channel's program.", |figures| figures.stats.lines_to_application),
            ("teletext_channel_program_lines_received_total", "Lines from the channel's program.", |figures| figures.stats.lines_from_application),
            ("teletext_channel_program_lines_dropped_total", "Lines for the program dropped because it fell behind.", |figures| figures.lines_dropped),
            ("teletext_channel_events_total", "Events the channel's task handled.", |figures| figures.events.count),
        ];
        for (name, help, value) in counters {
            family(name, "counter", help, &each(value));
        }
        let mut samples = Vec::new();
        for (folder, figures) in &channels {
            let channel = label(folder);
            let mut cumulative = 0_u64;
            for (bound, &bucket) in EVENT_BUCKETS_US.iter().zip(&figures.events.buckets) {
                cumulative = cumulative.saturating_add(bucket);
                let seconds = as_float(*bound) / MICROSECONDS_PER_SECOND;
                samples.push((format!("_bucket{{channel=\"{channel}\",le=\"{seconds}\"}}"), as_float(cumulative)));
            }
            samples.push((format!("_bucket{{channel=\"{channel}\",le=\"+Inf\"}}"), as_float(figures.events.count)));
            samples.push((format!("_sum{{channel=\"{channel}\"}}"), as_float(figures.events.sum_us) / MICROSECONDS_PER_SECOND));
            samples.push((format!("_count{{channel=\"{channel}\"}}"), as_float(figures.events.count)));
        }
        family("teletext_channel_event_duration_seconds", "histogram", "The channel's time over one event and its effects.", &samples);
        text
    }
}

/// A channel's figure for `/metrics`: its name, its help, and how it is read.
type Figure = (&'static str, &'static str, fn(&ChannelFigures) -> u64);

/// The figures at a glance (requirement 7.3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Glance {
    /// Seconds since the start.
    pub uptime_s: u64,
    /// Connections open now.
    pub connections: u64,
    /// The most at once.
    pub connections_peak: u64,
    /// Connections made.
    pub connections_total: u64,
    /// WebSocket messages sent.
    pub messages_sent: u64,
    /// Bytes sent in them.
    pub bytes_sent: u64,
    /// HTTP requests answered.
    pub requests: u64,
    /// Of them, not modified.
    pub not_modified: u64,
    /// Each channel's viewers and their peak.
    pub viewers: String,
}

/// A channel's figures that accumulate.
fn channel_now(figures: &ChannelFigures) -> Totals {
    let stats = &figures.stats;
    let counts = [
        ("viewers_total", stats.viewers_total),
        ("viewers_peak", stats.viewers_peak),
        ("opens", stats.total_opens()),
        ("inputs", stats.inputs),
        ("keys", stats.keys),
        ("shared_published", stats.shared_published),
        ("own_published", stats.own_published),
        ("gotos", stats.gotos),
        ("sound_records", stats.sound_records),
        ("lines_to_application", stats.lines_to_application),
        ("lines_from_application", stats.lines_from_application),
        ("lines_dropped", figures.lines_dropped),
        ("sessions_resumed", stats.sessions_resumed),
    ];
    Totals { counts: counts.into_iter().map(|(name, value)| (name.to_owned(), value)).collect(), opens: stats.opens.clone() }
}

/// A count as Prometheus writes it: exact below 2^53, which no count here
/// reaches.
#[expect(clippy::cast_precision_loss, reason = "a count beyond 2^53 is beyond any this server makes; Prometheus's samples are floats")]
const fn as_float(value: u64) -> f64 {
    value as f64
}

/// A label's value with its backslashes, quotes and newlines escaped (the
/// Prometheus text format); a channel's folder has none, but the rule is kept.
fn label(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n")
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
