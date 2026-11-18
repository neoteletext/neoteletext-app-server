//! `teletext-server`: the NeoTeletext server on Linux, macOS and Windows.
//!
//! ```text
//! teletext-server [--listen ADDRESS] [--origin URL] [--channels FILE]
//!                 [--state DIRECTORY] [--admin-listen ADDRESS] [--log-format auto|json|text]
//!                 [--trusted-proxy ADDRESS]
//! teletext-server rotate-viewer-secret [--state DIRECTORY]
//! ```
//!
//! `--listen` is where it accepts connections (`0.0.0.0:8090` by
//! default), unless systemd passes it a listening socket, which it then
//! takes instead (requirement 5.6); `--origin` is the address viewers reach
//! it at, which each channel's push WebSocket is named on (the engine
//! requires the push address on the channel's own origin); `--channels` is
//! the file of the channels it serves, each a program of its own, in any language
//! (DOC-1964); `--state` is where it keeps the viewer-handle
//! secret and the accumulated figures across a restart (`$STATE_DIRECTORY`, as systemd sets it, by default; none
//! without either, when the secret lasts one run); `--admin-listen` is where `/metrics`, `/healthz` and
//! `/readyz` are served (none unless given); `--log-format` is how events
//! are written, and `RUST_LOG` chooses which; `--trusted-proxy` is the
//! address of the reverse proxy in front of the server, whose
//! `X-Forwarded-For` then names each viewer's address for the bound on new
//! viewers (VS-304).
//!
//! `rotate-viewer-secret` makes a new viewer-handle secret in the state
//! directory, for the server's next start (VS-306). **Every channel then
//! forgets every viewer**: each viewer is given a new handle no program can
//! relate to the old, and each program is told `viewers_reset` when it
//! starts. Do it only after a breach of a program's data or of the state
//! directory.

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
use std::net::IpAddr;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use listenfd::ListenFd;
use teletext_native::http::{Routes, answer};
use teletext_native::logging::{self, Format};
use teletext_native::state::{StateDirectory, SystemRandomSource, random_secret};
use teletext_native::stats::ServerStats;
use teletext_native::{admin, apps, hub};
use teletext_runtime::{Channel, ChannelConfig, HandleSecret};
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
/// The sockets systemd may pass: the viewers' listener, and no other.
const PASSED_SOCKETS: usize = 1;

struct Arguments {
    listen: String,
    origin: String,
    channels: Option<PathBuf>,
    state: Option<PathBuf>,
    admin: Option<String>,
    log_format: Format,
    trusted_proxy: Option<IpAddr>,
    /// `rotate-viewer-secret`: rotate, and serve nothing.
    rotate: bool,
}

/// How the server is run.
const USAGE: &str = "usage: teletext-server [--listen ADDRESS] [--origin URL] [--channels FILE] [--state DIRECTORY] [--admin-listen ADDRESS] [--log-format auto|json|text] [--trusted-proxy ADDRESS], or teletext-server rotate-viewer-secret [--state DIRECTORY]";

fn arguments() -> Result<Arguments, String> {
    // systemd separates several state directories with colons; the first is ours.
    let state = std::env::var(STATE_VARIABLE).ok().and_then(|value| value.split(':').next().filter(|first| !first.is_empty()).map(PathBuf::from));
    let mut parsed = Arguments {
        listen: LISTEN.to_owned(),
        origin: ORIGIN.to_owned(),
        channels: None,
        state,
        admin: None,
        log_format: Format::Automatic,
        trusted_proxy: None,
        rotate: false,
    };
    let mut list = std::env::args().skip(1).peekable();
    if list.next_if(|first| first == "rotate-viewer-secret").is_some() {
        parsed.rotate = true;
    }
    while let Some(argument) = list.next() {
        let mut value = || list.next().ok_or_else(|| format!("{argument} needs a value"));
        match argument.as_str() {
            "--listen" => parsed.listen = value()?,
            "--origin" => value()?.trim_end_matches('/').clone_into(&mut parsed.origin),
            "--channels" => parsed.channels = Some(PathBuf::from(value()?)),
            "--state" => parsed.state = Some(PathBuf::from(value()?)),
            "--admin-listen" => parsed.admin = Some(value()?),
            "--log-format" => parsed.log_format = value()?.parse()?,
            "--trusted-proxy" => {
                let text = value()?;
                parsed.trusted_proxy = Some(text.parse().map_err(|error| format!("--trusted-proxy {text}: {error}"))?);
            }
            other => return Err(format!("unknown argument {other}; {USAGE}")),
        }
    }
    Ok(parsed)
}

/// The listening socket systemd passed, if it did (requirement 5.6), held
/// by systemd across a restart so that connections wait in its queue
/// (`sd_listen_fds(3)`). It is made close-on-exec, or every program started
/// would hold it too.
///
/// Called before any thread starts: `ListenFd` removes `LISTEN_PID` and
/// `LISTEN_FDS`, and changing the environment races with threads reading it.
fn passed_socket() -> Result<Option<std::net::TcpListener>, String> {
    let mut passed = ListenFd::from_env();
    if passed.len() > PASSED_SOCKETS {
        return Err(format!("systemd passed {} sockets; the server takes {PASSED_SOCKETS}", passed.len()));
    }
    passed.take_tcp_listener(0).map_err(|error| format!("cannot take the socket systemd passed: {error}"))
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
    if arguments.rotate {
        return rotate(arguments.state.as_deref());
    }
    let passed = match passed_socket() {
        Ok(passed) => passed,
        Err(error) => {
            tracing::error!("{error}");
            return ExitCode::FAILURE;
        }
    };
    let Ok(runtime) = tokio::runtime::Builder::new_multi_thread().enable_all().build() else {
        tracing::error!("cannot start the async runtime");
        return ExitCode::FAILURE;
    };
    let result = runtime.block_on(serve(arguments, passed));
    runtime.shutdown_timeout(RUNTIME_STOP_WITHIN);
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!("{error}");
            ExitCode::FAILURE
        }
    }
}

/// `rotate-viewer-secret` (VS-306).
fn rotate(state: Option<&std::path::Path>) -> ExitCode {
    let rotated = state.ok_or_else(|| format!("a state directory is needed: --state DIRECTORY or ${STATE_VARIABLE}")).and_then(|root| {
        let kept = StateDirectory::open(root)?;
        kept.rotate_handle_secret()?;
        Ok(kept)
    });
    match rotated {
        Ok(kept) => {
            tracing::warn!(state = %kept.root().display(), "the viewer-handle secret was rotated: at the next start every channel forgets every viewer");
            ExitCode::SUCCESS
        }
        Err(error) => {
            tracing::error!("{error}");
            ExitCode::FAILURE
        }
    }
}

async fn serve(arguments: Arguments, passed: Option<std::net::TcpListener>) -> Result<(), String> {
    let start = tokio::time::Instant::now();
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
    let (secret, rotated) = handle_secret(kept.as_ref())?;
    let mut hubs = BTreeMap::new();
    for spec in arguments.channels.as_deref().map(apps::read_channels).transpose()?.unwrap_or_default() {
        if hubs.contains_key(&spec.folder) {
            return Err(format!("{} is already a channel", spec.folder));
        }
        let folder = spec.folder.clone();
        let config = ChannelConfig {
            name: spec.name.clone(),
            address: format!("{}/{folder}/", arguments.origin),
            initial_page: spec.initial_page,
            epoch_at_start_ms,
            other_addresses: Vec::new(),
            wheel: spec.wheel,
            default_rules: spec.default_rules,
        };
        let link = apps::start(spec)?;
        let mut channel = Channel::new(config, &secret, Box::new(SystemRandomSource::default()));
        if rotated {
            // Before its program starts: it is told `viewers_reset` after
            // `start` (VS-306).
            channel.rotate_viewers(&secret, 0);
        }
        hubs.insert(folder.clone(), hub::spawn(&folder, channel, start, Some(link), Arc::clone(&stats)));
    }
    if rotated && let Some(kept) = &kept {
        kept.rotation_told()?;
    }
    let listener = match passed {
        Some(passed) => adopt(passed).map_err(|error| format!("cannot serve on the socket systemd passed: {error}"))?,
        None => TcpListener::bind(&arguments.listen).await.map_err(|error| format!("cannot listen on {}: {error}", arguments.listen))?,
    };
    let listening = listener.local_addr().map_err(|error| format!("cannot read the address listened on: {error}"))?;
    let mut administering = None;
    if let Some(address) = &arguments.admin {
        let admin = TcpListener::bind(address).await.map_err(|error| format!("cannot listen on {address} for administration: {error}"))?;
        administering = Some(admin.local_addr().map_err(|error| format!("cannot read the address administered on: {error}"))?.to_string());
        tokio::spawn(administer(admin, Arc::clone(&stats)));
    }
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        listen = %listening,
        origin = arguments.origin,
        channels = hubs.keys().cloned().collect::<Vec<_>>().join(", "),
        state = kept.as_ref().map(|kept| kept.root().display().to_string()),
        admin = administering,
        "serving"
    );
    let routes = Arc::new(Routes { hubs, connections: AtomicU64::new(1), stats: Arc::clone(&stats), trusted_proxy: arguments.trusted_proxy });
    tokio::spawn(figures(Arc::clone(&stats), kept.clone()));
    let stopped = stop_signal();
    tokio::pin!(stopped);
    loop {
        tokio::select! {
            accepted = listener.accept() => {
                let Ok((stream, peer)) = accepted else { continue };
                let _ = stream.set_nodelay(true);
                let routes = Arc::clone(&routes);
                tokio::spawn(async move {
                    let service = service_fn(move |request| {
                        let routes = Arc::clone(&routes);
                        async move { Ok::<_, Infallible>(answer(&routes, request, Some(peer.ip())).await) }
                    });
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

/// The passed socket for tokio, which needs it non-blocking; systemd
/// passes it blocking unless its unit says `NonBlocking=true`.
fn adopt(passed: std::net::TcpListener) -> std::io::Result<TcpListener> {
    passed.set_nonblocking(true)?;
    TcpListener::from_std(passed)
}

/// The viewer-handle secret (VS-305), kept in the state directory, or
/// made for this run alone, so that viewers' handles last the run (the
/// `serving` line names no state directory then); and whether it was
/// rotated since the last start.
///
/// # Errors
/// A secret that cannot be read, made or kept: the server does not start
/// rather than give every viewer a handle that a restart would change.
fn handle_secret(kept: Option<&StateDirectory>) -> Result<(HandleSecret, bool), String> {
    if let Some(kept) = kept {
        return kept.handle_secret();
    }
    random_secret().map(|secret| (secret, false))
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
/// as a restart, the connections finish closing,
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
