use super::*;
use teletext_runtime::json::{Value, parse};

fn value(bytes: &[u8]) -> Value {
    parse(bytes.strip_suffix(b"\n").unwrap()).unwrap()
}

fn figures() -> ChannelFigures {
    let mut stats = ChannelStats { viewers: 3, viewers_peak: 5, viewers_total: 9, keys: 42, ..ChannelStats::default() };
    stats.opens.insert(0x100, 7);
    stats.opens.insert(0x200, 9);
    ChannelFigures { stats, lines_dropped: 2, ready: true, events: Histogram::default() }
}

#[test]
fn connections_count_now_peak_and_total() {
    let stats = ServerStats::default();
    stats.connected();
    stats.connected();
    stats.disconnected();
    stats.connected();
    stats.disconnected();
    stats.disconnected();
    stats.disconnected();
    assert_eq!(stats.connections.load(Ordering::Relaxed), 0, "never below none");
    assert_eq!(stats.connections_peak.load(Ordering::Relaxed), 2);
    assert_eq!(stats.connections_total.load(Ordering::Relaxed), 3);
}

#[test]
fn a_channels_figures_reach_its_line_and_the_document() {
    let stats = ServerStats::default();
    stats.sent(1_000);
    stats.record("hackaday", figures());
    let line = value(&stats.line("hackaday", &figures()));
    assert_eq!(line.get("type").and_then(Value::text), Some("stats"));
    let server = line.get("server").unwrap();
    assert_eq!(server.get("bytes_sent").and_then(Value::integer), Some(1_000));
    assert_eq!(server.get("viewers").and_then(|viewers| viewers.get("hackaday")).and_then(Value::integer), Some(3));
    let channel = line.get("channel").unwrap();
    assert_eq!(channel.get("keys").and_then(Value::integer), Some(42));
    assert_eq!(channel.get("opens").and_then(Value::integer), Some(16));
    assert_eq!(channel.get("lines_dropped").and_then(Value::integer), Some(2));
    let Some(Value::Object(most)) = channel.get("most_opened") else { panic!("most_opened is an object") };
    assert_eq!(
        most.iter().map(|(page, count)| (page.as_str(), count.integer())).collect::<Vec<_>>(),
        [("200", Some(9)), ("100", Some(7))],
        "most opened first"
    );
    let document = value(&stats.document());
    assert!(document.get("channels").and_then(|channels| channels.get("hackaday")).is_some());
    assert_eq!(stats.glance().viewers, "hackaday 3 (peak 5)");
}

/// The totals kept from before are added to this run's: counters add up,
/// peaks keep the greater, each page's opens add up; they reach
/// `/stats.json` and the `stats` line as `all_time`, and `figures.json`
/// reads back as it was written (requirement 8.3).
#[test]
fn totals_accumulate_across_restarts() {
    let before = ServerStats::default();
    for _ in 0..4 {
        before.connected();
    }
    before.record("hackaday", figures());
    let kept = value(&before.all_time_document());

    let after = ServerStats::default();
    after.accumulate(&kept);
    after.connected();
    let mut now = figures();
    now.stats.viewers_peak = 2;
    now.stats.opens.insert(0x100, 1);
    after.record("hackaday", now.clone());
    let all_time = value(&after.all_time_document());
    let server = all_time.get("server").unwrap();
    assert_eq!(server.get("connections_total").and_then(Value::integer), Some(5));
    assert_eq!(server.get("connections_peak").and_then(Value::integer), Some(4), "the greater peak");
    let channel = all_time.get("channels").and_then(|channels| channels.get("hackaday")).unwrap();
    assert_eq!(channel.get("viewers_total").and_then(Value::integer), Some(18));
    assert_eq!(channel.get("viewers_peak").and_then(Value::integer), Some(5));
    assert_eq!(channel.get("keys").and_then(Value::integer), Some(84));
    assert_eq!(channel.get("pages_opened").and_then(|pages| pages.get("100")).and_then(Value::integer), Some(8));
    let document = value(&after.document());
    let shown = document.get("channels").and_then(|channels| channels.get("hackaday")).and_then(|channel| channel.get("all_time"));
    assert_eq!(shown.and_then(|totals| totals.get("keys")).and_then(Value::integer), Some(84));
    let line = value(&after.line("hackaday", &now));
    assert_eq!(
        line.get("server").and_then(|server| server.get("all_time")).and_then(|totals| totals.get("connections_total")).and_then(Value::integer),
        Some(5)
    );
}

/// `/metrics` names every figure of requirement 8.2 with its type, in the
/// Prometheus text format, and the histogram's buckets are cumulative.
#[test]
fn metrics_are_in_the_prometheus_text_format() {
    let stats = ServerStats::default();
    let mut channel = figures();
    channel.events.observe(Duration::from_micros(40));
    channel.events.observe(Duration::from_micros(700));
    channel.events.observe(Duration::from_secs(1));
    stats.record("hackaday", channel);
    let text = stats.metrics();
    for (name, kind) in [
        ("process_start_time_seconds", "gauge"),
        ("teletext_connections", "gauge"),
        ("teletext_connections_total", "counter"),
        ("teletext_messages_sent_total", "counter"),
        ("teletext_sent_bytes_total", "counter"),
        ("teletext_http_requests_total", "counter"),
        ("teletext_http_not_modified_total", "counter"),
        ("teletext_program_restarts_total", "counter"),
        ("teletext_channel_viewers", "gauge"),
        ("teletext_channel_sessions_waiting", "gauge"),
        ("teletext_channel_ready", "gauge"),
        ("teletext_channel_sessions_resumed_total", "counter"),
        ("teletext_channel_page_opens_total", "counter"),
        ("teletext_channel_keys_total", "counter"),
        ("teletext_channel_program_lines_dropped_total", "counter"),
        ("teletext_channel_event_duration_seconds", "histogram"),
    ] {
        assert!(text.contains(&format!("\n# TYPE {name} {kind}\n")) || text.starts_with(&format!("# TYPE {name} {kind}\n")), "{name}: {text}");
    }
    assert!(text.contains("teletext_channel_keys_total{channel=\"hackaday\"} 42\n"), "{text}");
    assert!(text.contains("teletext_channel_event_duration_seconds_bucket{channel=\"hackaday\",le=\"0.00005\"} 1\n"), "{text}");
    assert!(text.contains("teletext_channel_event_duration_seconds_bucket{channel=\"hackaday\",le=\"0.001\"} 2\n"), "{text}");
    assert!(text.contains("teletext_channel_event_duration_seconds_bucket{channel=\"hackaday\",le=\"+Inf\"} 3\n"), "{text}");
    assert!(text.contains("teletext_channel_event_duration_seconds_count{channel=\"hackaday\"} 3\n"), "{text}");
    assert_eq!(stats.not_ready(), Vec::<String>::new());
}
