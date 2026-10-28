//! `teletext-server`: the NeoTeletext server on Linux, macOS and Windows.
//!
//! ```text
//! teletext-server [--listen ADDRESS] [--origin URL] [--channels FILE]
//!                 [--state DIRECTORY] [--admin-listen ADDRESS] [--log-format auto|json|text]
//! ```
//!
//! `--listen` is where it accepts connections (`0.0.0.0:8090` by
//! default); `--origin` is the address viewers reach it at, which each
//! channel's push WebSocket is named on (the engine requires the push
//! address on the channel's own origin); `--channels` is the file of the
//! channels it serves, each a program of its own, in any language
//! (`docs/applications.md`); `--state` is where it keeps what survives a
//! restart (`$STATE_DIRECTORY`, as systemd sets it, by default; none
//! without either); `--admin-listen` is where `/metrics`, `/healthz` and
//! `/readyz` are served (none unless given); `--log-format` is how events
//! are written, and `RUST_LOG` chooses which.

#![forbid(unsafe_code)]
// No panicking constructs: everything here reads what clients send
// (DEVELOPMENT.md section 6).
#![deny(
    clippy::indexing_slicing,
    clippy::string_slice,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented
)]

use std::collections::BTreeMap;
use std::convert::Infallible;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use teletext_native::http::{Cookies, PUSH, Routes, answer};
use teletext_native::logging::{self, Format};
use teletext_native::state::{StateDirectory, random_key};
use teletext_native::stats::ServerStats;
use teletext_native::{admin, apps, hub};
use teletext_runtime::session_cookie::first_session;
use teletext_runtime::{Channel, ChannelConfig, Remote, SESSION_GRACE_MS};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

/// Where the server listens unless told otherwise.
const LISTEN: &str = "0.0.0.0:8090";
/// How often the figures go to the log and are kept, in seconds.
const FIGURES_EVERY_S: u64 = 60;
/// The origin viewers reach it at unless told otherwise.
const ORIGIN: &str = "http://127.0.0.1:8090";
/// The variable systemd sets to the unit's `StateDirectory=`.
const STATE_VARIABLE: &str = "STATE_DIRECTORY";
/// How long a stop may take before the server exits anyway (requirement 5.1).
const STOP_WITHIN: Duration = Duration::from_secs(10);
/// How long the async runtime's blocking threads have to finish at exit.
const RUNTIME_STOP_WITHIN: Duration = Duration::from_secs(1);

struct Arguments {
    listen: String,
    origin: String,
    channels: Option<PathBuf>,
    state: Option<PathBuf>,
    admin: Option<String>,
    log_format: Format,
}

fn arguments() -> Result<Arguments, String> {
    // systemd separates several state directories with colons; the first is ours.
    let state = std::env::var(STATE_VARIABLE).ok().and_then(|value| value.split(':').next().filter(|first| !first.is_empty()).map(PathBuf::from));
    let mut parsed = Arguments { listen: LISTEN.to_owned(), origin: ORIGIN.to_owned(), channels: None, state, admin: None, log_format: Format::Automatic };
    let mut list = std::env::args().skip(1);
    while let Some(argument) = list.next() {
        let mut value = || list.next().ok_or_else(|| format!("{argument} needs a value"));
        match argument.as_str() {
            "--listen" => parsed.listen = value()?,
            "--origin" => value()?.trim_end_matches('/').clone_into(&mut parsed.origin),
            "--channels" => parsed.channels = Some(PathBuf::from(value()?)),
            "--state" => parsed.state = Some(PathBuf::from(value()?)),
            "--admin-listen" => parsed.admin = Some(value()?),
            "--log-format" => parsed.log_format = value()?.parse()?,
            other => {
                return Err(format!(
                    "unknown argument {other}; usage: teletext-server [--listen ADDRESS] [--origin URL] [--channels FILE] [--state DIRECTORY] [--admin-listen ADDRESS] [--log-format auto|json|text]"
                ));
            }
        }
    }
    Ok(parsed)
}

/// The WebSocket scheme of an HTTP origin.
fn push_origin(origin: &str) -> String {
    origin.strip_prefix("https://").map_or_else(|| origin.replacen("http://", "ws://", 1), |rest| format!("wss://{rest}"))
}

fn main() -> ExitCode {
    let arguments = match arguments() {
        Ok(arguments) => arguments,
        Err(error) => {
            // Before the log starts: its format may be what was wrong.
            logging::start(Format::Automatic);
            tracing::error!("{error}");
            return ExitCode::FAILURE;
        }
    };
    logging::start(arguments.log_format);
    let Ok(runtime) = tokio::runtime::Builder::new_multi_thread().enable_all().build() else {
        tracing::error!("cannot start the async runtime");
        return ExitCode::FAILURE;
    };
    let result = runtime.block_on(serve(arguments));
    runtime.shutdown_timeout(RUNTIME_STOP_WITHIN);
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!("{error}");
            ExitCode::FAILURE
        }
    }
}

async fn serve(arguments: Arguments) -> Result<(), String> {
    let start = tokio::time::Instant::now();
    // The channels' randomness, for their games: the time is enough,
    // nothing here is a secret.
    let seed = SystemTime::now().duration_since(UNIX_EPOCH).map_or(1, |since| u64::try_from(since.as_nanos() & u128::from(u64::MAX)).unwrap_or(1));
    let push = push_origin(&arguments.origin);
    let epoch_at_start_ms = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| u64::try_from(since.as_millis()).unwrap_or(0));
    let stats = Arc::new(ServerStats::default());
    let kept = arguments.state.as_deref().map(StateDirectory::open).transpose()?;
    if let Some(kept) = &kept {
        match kept.figures() {
            Ok(Some(figures)) => stats.accumulate(&figures),
            Ok(None) => {}
            Err(error) => tracing::warn!(target: "state", %error, "the accumulated figures start again from zero"),
        }
    }
    let cookies = session_keys(kept.as_ref()).map(|(key, previous)| Cookies { key, previous, secure: arguments.origin.starts_with("https://") });
    let mut hubs = BTreeMap::new();
    for spec in arguments.channels.as_deref().map(apps::read_channels).transpose()?.unwrap_or_default() {
        if hubs.contains_key(&spec.folder) {
            return Err(format!("{} is already a channel", spec.folder));
        }
        let folder = spec.folder.clone();
        let config = ChannelConfig {
            name: spec.name.clone(),
            push_address: format!("{push}/{folder}/{PUSH}"),
            initial_page: 0x100,
            epoch_at_start_ms,
            session_grace_ms: SESSION_GRACE_MS,
        };
        let link = apps::start(spec)?;
        let mut channel = Channel::new(config, vec![Box::new(Remote::new(Vec::new()))], seed);
        if let Some(kept) = &kept {
            load(&mut channel, kept, &folder);
        }
        hubs.insert(folder.clone(), hub::spawn(&folder, channel, start, Some(link), Arc::clone(&stats), kept.clone()));
    }
    let listener = TcpListener::bind(&arguments.listen).await.map_err(|error| format!("cannot listen on {}: {error}", arguments.listen))?;
    if let Some(address) = &arguments.admin {
        let admin = TcpListener::bind(address).await.map_err(|error| format!("cannot listen on {address} for administration: {error}"))?;
        tokio::spawn(administer(admin, Arc::clone(&stats)));
    }
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        listen = arguments.listen,
        origin = arguments.origin,
        channels = hubs.keys().cloned().collect::<Vec<_>>().join(", "),
        state = kept.as_ref().map(|kept| kept.root().display().to_string()),
        admin = arguments.admin,
        "serving"
    );
    let routes = Arc::new(Routes { hubs, sessions: AtomicU64::new(first_session(epoch_at_start_ms)), stats: Arc::clone(&stats), cookies });
    tokio::spawn(figures(Arc::clone(&stats), kept.clone()));
    let stopped = stop_signal();
    tokio::pin!(stopped);
    loop {
        tokio::select! {
            accepted = listener.accept() => {
                let Ok((stream, _)) = accepted else { continue };
                let _ = stream.set_nodelay(true);
                let routes = Arc::clone(&routes);
                tokio::spawn(async move {
                    let service = service_fn(move |request| std::future::ready(Ok::<_, Infallible>(answer(&routes, request))));
                    let _ = http1::Builder::new().serve_connection(TokioIo::new(stream), service).with_upgrades().await;
                });
            }
            () = &mut stopped => break,
        }
    }
    drop(listener);
    stop(&routes, kept.as_ref()).await;
    Ok(())
}

/// Gives `channel` what was kept for it, logging what was ignored.
fn load(channel: &mut Channel, kept: &StateDirectory, folder: &str) {
    let (records, ignored) = kept.records(folder);
    let mut loaded = 0_usize;
    for (key, bytes) in records {
        match channel.load(key, &bytes) {
            Ok(()) => loaded = loaded.saturating_add(1),
            Err(error) => tracing::warn!(target: "state", channel = folder, key = key.name(), ?error, "a kept record was ignored"),
        }
    }
    for name in ignored {
        tracing::warn!(target: "state", channel = folder, file = name, "a file in the channel's state was ignored");
    }
    tracing::info!(target: "state", channel = folder, records = loaded, ready = channel.is_ready(), "loaded what was kept");
}

/// The session cookie's keys: kept in the state directory, or made for
/// this run alone; none where there is no source of random bytes.
fn session_keys(kept: Option<&StateDirectory>) -> Option<(teletext_runtime::session_cookie::Key, Option<teletext_runtime::session_cookie::Key>)> {
    let keys = match kept {
        Some(kept) => kept.session_keys(),
        None => random_key().map(|key| (key, None)),
    };
    match keys {
        Ok(keys) => Some(keys),
        Err(error) => {
            tracing::warn!(target: "state", %error, "sessions will not continue across connections");
            None
        }
    }
}

/// Serves the administration listener.
async fn administer(listener: TcpListener, stats: Arc<ServerStats>) {
    loop {
        let Ok((stream, _)) = listener.accept().await else { continue };
        let stats = Arc::clone(&stats);
        tokio::spawn(async move {
            let service = service_fn(move |request| std::future::ready(Ok::<_, Infallible>(admin::answer(&stats, &request))));
            let _ = http1::Builder::new().serve_connection(TokioIo::new(stream), service).await;
        });
    }
}

/// The figures at a glance in the log, and kept, once a minute.
async fn figures(stats: Arc<ServerStats>, kept: Option<StateDirectory>) {
    let mut minute = tokio::time::interval(Duration::from_secs(FIGURES_EVERY_S));
    minute.tick().await;
    loop {
        minute.tick().await;
        let glance = stats.glance();
        tracing::info!(
            target: "figures",
            uptime_s = glance.uptime_s,
            connections = glance.connections,
            connections_peak = glance.connections_peak,
            connections_total = glance.connections_total,
            messages_sent = glance.messages_sent,
            bytes_sent = glance.bytes_sent,
            requests = glance.requests,
            not_modified = glance.not_modified,
            viewers = glance.viewers,
            "figures"
        );
        keep_figures(&stats, kept.as_ref()).await;
    }
}

/// Keeps the accumulated figures (requirement 8.3).
async fn keep_figures(stats: &ServerStats, kept: Option<&StateDirectory>) {
    let Some(kept) = kept.cloned() else { return };
    let document = stats.all_time_document();
    match tokio::task::spawn_blocking(move || kept.keep_figures(&document)).await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => tracing::error!(target: "state", %error, "the accumulated figures could not be kept"),
        Err(error) => tracing::error!(target: "state", %error, "keeping the accumulated figures failed"),
    }
}

/// `SIGTERM` or `SIGINT`, whichever comes first.
async fn stop_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        if let Ok(mut terminate) = signal(SignalKind::terminate()) {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {}
                _ = terminate.recv() => {}
            }
            return;
        }
    }
    let _ = tokio::signal::ctrl_c().await;
}

/// Stops cleanly (requirement 5.1): every channel closes its connections
/// as a restart and keeps what it keeps, the connections finish closing,
/// and the figures are kept, all within [`STOP_WITHIN`].
async fn stop(routes: &Routes, kept: Option<&StateDirectory>) {
    tracing::info!("stopping");
    let deadline = tokio::time::Instant::now().checked_add(STOP_WITHIN);
    let mut stopping = Vec::new();
    for hub in routes.hubs.values() {
        let (done, stopped) = oneshot::channel();
        if hub.commands.send(hub::Command::Shutdown { done }).await.is_ok() {
            stopping.push(stopped);
        }
    }
    let finished = async {
        for stopped in stopping {
            let _ = stopped.await;
        }
        routes.stats.all_closed().await;
    };
    let in_time = if let Some(deadline) = deadline {
        tokio::time::timeout_at(deadline, finished).await.is_ok()
    } else {
        finished.await;
        true
    };
    if !in_time {
        tracing::warn!(within_s = STOP_WITHIN.as_secs(), "not every channel or connection stopped in time");
    }
    keep_figures(&routes.stats, kept).await;
    tracing::info!("stopped");
}
