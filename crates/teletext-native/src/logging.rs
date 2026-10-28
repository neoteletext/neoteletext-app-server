//! The server's log (decision 18, requirements 7.1 and 7.2): structured
//! events through `tracing`, one a line on standard error, JSON when
//! standard error is not a terminal and text when it is, the levels and
//! targets chosen by `RUST_LOG`.

use std::io::IsTerminal;
use std::str::FromStr;

use teletext_runtime::Level as RuntimeLevel;
use tracing::Level;
use tracing_subscriber::filter::Targets;
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

/// The variable that chooses levels and targets, as every Rust server's
/// operator knows it.
pub const FILTER_VARIABLE: &str = "RUST_LOG";
/// What is written when `RUST_LOG` is unset or does not read.
const DEFAULT_LEVEL: Level = Level::INFO;

/// How events are written.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Format {
    /// JSON when standard error is not a terminal, text when it is.
    Automatic,
    /// One JSON object a line.
    Json,
    /// Text, for a person.
    Text,
}

impl FromStr for Format {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, String> {
        match text {
            "auto" => Ok(Self::Automatic),
            "json" => Ok(Self::Json),
            "text" => Ok(Self::Text),
            other => Err(format!("--log-format is auto, json or text, not {other}")),
        }
    }
}

/// The filter `RUST_LOG` names, or `info` and whether it was refused.
fn filter(variable: Option<&str>) -> (Targets, bool) {
    match variable.map(Targets::from_str) {
        Some(Ok(targets)) => (targets, false),
        Some(Err(_)) => (Targets::new().with_default(DEFAULT_LEVEL), true),
        None => (Targets::new().with_default(DEFAULT_LEVEL), false),
    }
}

/// Starts the log for the whole process.
pub fn start(format: Format) {
    let variable = std::env::var(FILTER_VARIABLE).ok();
    let (targets, refused) = filter(variable.as_deref());
    let json = match format {
        Format::Automatic => !std::io::stderr().is_terminal(),
        Format::Json => true,
        Format::Text => false,
    };
    let registry = tracing_subscriber::registry().with(targets);
    let layer = tracing_subscriber::fmt::layer().with_writer(std::io::stderr).with_span_events(FmtSpan::NONE).with_ansi(false);
    if json {
        registry.with(layer.json().flatten_event(false).with_current_span(false).with_span_list(false)).init();
    } else {
        registry.with(layer).init();
    }
    if refused {
        tracing::warn!(variable = FILTER_VARIABLE, "the filter did not read; writing info and above");
    }
}

/// Writes one of the runtime's reports (requirement 7.6) as an event of
/// the target `channel`.
pub fn runtime(channel: &str, level: RuntimeLevel, event: &str, text: &str) {
    match level {
        RuntimeLevel::Error => tracing::error!(target: "channel", channel, event, "{text}"),
        RuntimeLevel::Warn => tracing::warn!(target: "channel", channel, event, "{text}"),
        RuntimeLevel::Info => tracing::info!(target: "channel", channel, event, "{text}"),
        RuntimeLevel::Debug => tracing::debug!(target: "channel", channel, event, "{text}"),
    }
}

#[cfg(test)]
pub(crate) mod tests;
