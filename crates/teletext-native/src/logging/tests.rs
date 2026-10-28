use super::*;
use std::sync::{Arc, Mutex, PoisonError};

use teletext_runtime::json::{self, Value};
use tracing::subscriber::DefaultGuard;

/// What events a test's code writes, as the server writes them in JSON.
#[derive(Clone, Default)]
pub(crate) struct Captured(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for Captured {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Captured {
    /// Captures every event on this thread until the guard is dropped.
    pub(crate) fn install(&self) -> DefaultGuard {
        let captured = self.clone();
        let subscriber = tracing_subscriber::fmt()
            .json()
            .with_max_level(Level::TRACE)
            .with_ansi(false)
            .with_current_span(false)
            .with_span_list(false)
            .with_writer(move || captured.clone())
            .finish();
        tracing::subscriber::set_default(subscriber)
    }

    /// Each event written, one JSON object a line.
    pub(crate) fn events(&self) -> Vec<Value> {
        let bytes = self.0.lock().unwrap_or_else(PoisonError::into_inner).clone();
        bytes.split(|&byte| byte == b'\n').filter(|line| !line.is_empty()).map(|line| json::parse(line).unwrap()).collect()
    }
}

/// The text at `path` in an event.
pub(crate) fn field<'a>(event: &'a Value, path: &[&str]) -> Option<&'a str> {
    path.iter().try_fold(event, |value, name| value.get(name))?.text()
}

/// An event is one JSON object a line, with its time, level, target and
/// fields; the runtime's reports carry their level, their name and the
/// channel (requirements 7.1 and 7.6).
#[test]
fn an_event_is_a_line_of_json_with_its_fields() {
    let captured = Captured::default();
    let guard = captured.install();
    runtime("hackaday", RuntimeLevel::Warn, "line_refused", "refused a line from the application: not JSON");
    runtime("hackaday", RuntimeLevel::Debug, "detail", "a detail");
    drop(guard);
    let events = captured.events();
    assert_eq!(events.len(), 2, "{events:?}");
    let event = &events[0];
    assert!(field(event, &["timestamp"]).is_some_and(|time| time.ends_with('Z')), "RFC 3339, UTC: {event:?}");
    assert_eq!(field(event, &["level"]), Some("WARN"));
    assert_eq!(field(event, &["target"]), Some("channel"));
    assert_eq!(field(event, &["fields", "channel"]), Some("hackaday"));
    assert_eq!(field(event, &["fields", "event"]), Some("line_refused"));
    assert_eq!(field(event, &["fields", "message"]), Some("refused a line from the application: not JSON"));
    assert_eq!(field(&events[1], &["level"]), Some("DEBUG"));
}

/// `RUST_LOG` chooses levels and targets; one that does not read leaves
/// `info` and says so (requirement 7.2).
#[test]
fn the_filter_reads_rust_log() {
    let (targets, refused) = filter(Some("warn,teletext_native::connection=debug"));
    assert!(!refused);
    assert_eq!(targets.to_string(), "teletext_native::connection=debug,warn");
    let (targets, refused) = filter(Some("=nonsense=="));
    assert!(refused);
    assert_eq!(targets.to_string(), "info");
    assert_eq!(filter(None).0.to_string(), "info");
}

/// `--log-format` is `auto`, `json` or `text`.
#[test]
fn the_format_is_named() {
    assert_eq!("auto".parse(), Ok(Format::Automatic));
    assert_eq!("json".parse(), Ok(Format::Json));
    assert_eq!("text".parse(), Ok(Format::Text));
    assert!("xml".parse::<Format>().is_err());
}
