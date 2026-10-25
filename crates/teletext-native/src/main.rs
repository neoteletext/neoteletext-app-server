//! `teletext-server`: the NeoTeletext server on Linux, macOS and Windows.
//!
//! ```text
//! teletext-server [--listen ADDRESS] [--origin URL] [--pictures FOLDER]
//! ```
//!
//! `--listen` is where it accepts connections (`0.0.0.0:8090` by
//! default); `--origin` is the address viewers reach it at, which each
//! channel's push WebSocket is named on (the engine requires the push
//! address on the channel's own origin); `--pictures` is the folder of the
//! showcase's converted pictures (decision 6).

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
use std::time::{SystemTime, UNIX_EPOCH};

use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use teletext_native::http::{PUSH, Routes, answer};
use teletext_native::{hub, pictures};
use teletext_runtime::{Channel, ChannelConfig};
use tokio::net::TcpListener;

/// Where the server listens unless told otherwise.
const LISTEN: &str = "0.0.0.0:8090";
/// The origin viewers reach it at unless told otherwise.
const ORIGIN: &str = "http://127.0.0.1:8090";

struct Arguments {
    listen: String,
    origin: String,
    pictures: Option<PathBuf>,
}

fn arguments() -> Result<Arguments, String> {
    let mut parsed = Arguments { listen: LISTEN.to_owned(), origin: ORIGIN.to_owned(), pictures: None };
    let mut list = std::env::args().skip(1);
    while let Some(argument) = list.next() {
        let mut value = || list.next().ok_or_else(|| format!("{argument} needs a value"));
        match argument.as_str() {
            "--listen" => parsed.listen = value()?,
            "--origin" => value()?.trim_end_matches('/').clone_into(&mut parsed.origin),
            "--pictures" => parsed.pictures = Some(PathBuf::from(value()?)),
            other => return Err(format!("unknown argument {other}; usage: teletext-server [--listen ADDRESS] [--origin URL] [--pictures FOLDER]")),
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
            eprintln!("teletext-server: {error}");
            return ExitCode::FAILURE;
        }
    };
    let Ok(runtime) = tokio::runtime::Builder::new_multi_thread().enable_all().build() else {
        eprintln!("teletext-server: cannot start the async runtime");
        return ExitCode::FAILURE;
    };
    match runtime.block_on(serve(arguments)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("teletext-server: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn serve(arguments: Arguments) -> Result<(), String> {
    let start = tokio::time::Instant::now();
    let pictures = arguments.pictures.as_deref().map(pictures::load).unwrap_or_default();
    // The games' randomness: the time is enough, nothing here is a secret.
    let seed = SystemTime::now().duration_since(UNIX_EPOCH).map_or(1, |since| u64::try_from(since.as_nanos() & u128::from(u64::MAX)).unwrap_or(1));
    let push = push_origin(&arguments.origin);
    let epoch_at_start_ms = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| u64::try_from(since.as_millis()).unwrap_or(0));
    let mut hubs = BTreeMap::new();
    for spec in teletext_showcase::channels(&pictures) {
        let config =
            ChannelConfig { name: spec.name.to_owned(), push_address: format!("{push}/{}/{PUSH}", spec.folder), initial_page: 0x100, epoch_at_start_ms };
        let channel = Channel::new(config, spec.applications, seed ^ u64::try_from(spec.folder.len()).unwrap_or(0));
        hubs.insert(spec.folder.to_owned(), hub::spawn(spec.folder, channel, start));
    }
    let listener = TcpListener::bind(&arguments.listen).await.map_err(|error| format!("cannot listen on {}: {error}", arguments.listen))?;
    eprintln!("teletext-server: {} pictures; serving {} at {}/", pictures.len(), hubs.keys().cloned().collect::<Vec<_>>().join(", "), arguments.origin);
    let routes = Arc::new(Routes { hubs, sessions: AtomicU64::new(1) });
    loop {
        let Ok((stream, _)) = listener.accept().await else { continue };
        let _ = stream.set_nodelay(true);
        let routes = Arc::clone(&routes);
        tokio::spawn(async move {
            let service = service_fn(move |request| std::future::ready(Ok::<_, Infallible>(answer(&routes, request))));
            let _ = http1::Builder::new().serve_connection(TokioIo::new(stream), service).with_upgrades().await;
        });
    }
}
