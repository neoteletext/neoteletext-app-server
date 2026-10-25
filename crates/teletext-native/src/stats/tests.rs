use super::*;
use teletext_runtime::json::{Value, parse};

fn value(bytes: &[u8]) -> Value {
    parse(bytes.strip_suffix(b"\n").unwrap()).unwrap()
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
    let mut channel = ChannelStats { viewers: 3, viewers_peak: 5, keys: 42, ..ChannelStats::default() };
    channel.opens.insert(0x100, 7);
    channel.opens.insert(0x200, 9);
    stats.record("hackaday", &channel);
    let line = value(&stats.line(&channel));
    assert_eq!(line.get("type").and_then(Value::text), Some("stats"));
    let server = line.get("server").unwrap();
    assert_eq!(server.get("bytes_sent").and_then(Value::integer), Some(1_000));
    assert_eq!(server.get("viewers").and_then(|viewers| viewers.get("hackaday")).and_then(Value::integer), Some(3));
    let figures = line.get("channel").unwrap();
    assert_eq!(figures.get("keys").and_then(Value::integer), Some(42));
    assert_eq!(figures.get("opens").and_then(Value::integer), Some(16));
    let Some(Value::Object(most)) = figures.get("most_opened") else { panic!("most_opened is an object") };
    assert_eq!(
        most.iter().map(|(page, count)| (page.as_str(), count.integer())).collect::<Vec<_>>(),
        [("200", Some(9)), ("100", Some(7))],
        "most opened first"
    );
    let document = value(&stats.document());
    assert!(document.get("channels").and_then(|channels| channels.get("hackaday")).is_some());
    assert!(stats.summary().contains("hackaday 3 (peak 5)"));
}
