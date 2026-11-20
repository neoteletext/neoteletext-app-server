//! The viewers' address, served (REQ-6965): plain HTTP/1.1 on TCP for a
//! proxy in front, as before; or, given a certificate and key, TLS on TCP
//! with HTTP/2 or HTTP/1.1 as the client chooses, and HTTP/3 over QUIC on
//! UDP at the same address and port, which every answer over TCP then
//! names in `Alt-Svc`.

use std::convert::Infallible;
use std::future::Future;
use std::net::SocketAddr;
use std::sync::Arc;

use hyper::header::{ALT_SVC, HeaderValue};
use hyper::server::conn::{http1, http2};
use hyper::service::service_fn;
use hyper_util::rt::{TokioExecutor, TokioIo, TokioTimer};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::TlsAcceptor;

use crate::http::{Routes, answer};
use crate::limits::{H2_DEAD_AFTER_PING, H2_MAX_FRAME_BYTES, H2_PING_AFTER, HANDSHAKE_WITHIN, MAX_CONCURRENT_STREAMS, MAX_HEADER_LIST_BYTES};
use crate::tls::{ALPN_H2, Configured};
use crate::{http3, tls};

/// TLS on TCP, and what answers over it say of HTTP/3.
#[derive(Clone)]
struct Secured {
    acceptor: TlsAcceptor,
    /// `Alt-Svc: h3=":<port>"` (RFC 7838 section 3), the port QUIC is
    /// served on.
    alt_svc: HeaderValue,
}

/// The viewers' address, listening.
pub struct Server {
    listener: TcpListener,
    secured: Option<Secured>,
    quic: Option<quinn::Endpoint>,
    routes: Arc<Routes>,
}

impl std::fmt::Debug for Server {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Server").field("listener", &self.listener).field("quic", &self.quic).finish_non_exhaustive()
    }
}

impl Server {
    /// Takes `listener`, and with `tls` binds UDP at its address and port
    /// for QUIC. Without `tls` no UDP socket is made.
    ///
    /// # Errors
    /// An address that cannot be read, or a UDP socket that cannot be
    /// bound there.
    pub fn start(listener: TcpListener, tls: Option<&Configured>, routes: Arc<Routes>) -> Result<Self, String> {
        let address = listener.local_addr().map_err(|error| format!("cannot read the address listened on: {error}"))?;
        let Some(tls) = tls else { return Ok(Self { listener, secured: None, quic: None, routes }) };
        let quic = http3::endpoint(tls, address)?;
        let alt_svc = HeaderValue::from_str(&format!("h3=\":{}\"", address.port())).map_err(|error| format!("cannot name the QUIC port: {error}"))?;
        Ok(Self { listener, secured: Some(Secured { acceptor: TlsAcceptor::from(Arc::clone(&tls.tcp)), alt_svc }), quic: Some(quic), routes })
    }

    /// The UDP address QUIC is served on, when it is.
    #[must_use]
    pub fn quic_address(&self) -> Option<SocketAddr> {
        self.quic.as_ref().and_then(|endpoint| endpoint.local_addr().ok())
    }

    /// Serves until `stopped` is ready, and then listens on TCP no more.
    /// Connections already made go on; what is returned ends QUIC's, once
    /// the channels have closed theirs.
    pub async fn run(self, stopped: impl Future<Output = ()>) -> Stopped {
        if let Some(endpoint) = &self.quic {
            tokio::spawn(http3::accept(endpoint.clone(), Arc::clone(&self.routes)));
        }
        tokio::pin!(stopped);
        loop {
            tokio::select! {
                accepted = self.listener.accept() => {
                    let Ok((stream, peer)) = accepted else { continue };
                    let _ = stream.set_nodelay(true);
                    tokio::spawn(connection(stream, peer, Arc::clone(&self.routes), self.secured.clone()));
                }
                () = &mut stopped => break,
            }
        }
        Stopped { quic: self.quic }
    }
}

/// The viewers' address after it stopped listening on TCP.
#[derive(Debug)]
pub struct Stopped {
    quic: Option<quinn::Endpoint>,
}

impl Stopped {
    /// Closes every QUIC connection, waiting until each has been told.
    pub async fn close(self) {
        if let Some(endpoint) = self.quic {
            http3::close(&endpoint).await;
        }
    }
}

/// Serves one TCP connection: HTTP/1.1 as it is, or, under TLS, what the
/// client chose of HTTP/2 and HTTP/1.1 (RFC 7301).
async fn connection(stream: TcpStream, peer: SocketAddr, routes: Arc<Routes>, secured: Option<Secured>) {
    let alt_svc = secured.as_ref().map(|secured| secured.alt_svc.clone());
    let service = service_fn(move |request| {
        let routes = Arc::clone(&routes);
        let alt_svc = alt_svc.clone();
        async move {
            let mut response = answer(&routes, request, Some(peer.ip())).await;
            if let Some(alt_svc) = alt_svc {
                response.headers_mut().insert(ALT_SVC, alt_svc);
            }
            Ok::<_, Infallible>(response)
        }
    });
    let Some(secured) = secured else {
        let _ = http1::Builder::new().serve_connection(TokioIo::new(stream), service).with_upgrades().await;
        return;
    };
    let Ok(Ok(session)) = tokio::time::timeout(HANDSHAKE_WITHIN, secured.acceptor.accept(stream)).await else { return };
    if session.get_ref().1.alpn_protocol() == Some(ALPN_H2) {
        let _ = http2::Builder::new(TokioExecutor::new())
            .timer(TokioTimer::new())
            // RFC 8441 section 3: SETTINGS_ENABLE_CONNECT_PROTOCOL = 1.
            .enable_connect_protocol()
            .max_concurrent_streams(MAX_CONCURRENT_STREAMS)
            .max_header_list_size(MAX_HEADER_LIST_BYTES)
            .max_frame_size(H2_MAX_FRAME_BYTES)
            .keep_alive_interval(H2_PING_AFTER)
            .keep_alive_timeout(H2_DEAD_AFTER_PING)
            .serve_connection(TokioIo::new(session), service)
            .await;
    } else {
        let _ = http1::Builder::new().serve_connection(TokioIo::new(session), service).with_upgrades().await;
    }
}

/// What `--tls-certificate` and `--tls-key` name, read: see [`tls::load`].
///
/// # Errors
/// As [`tls::load`].
pub fn secure(files: Option<&tls::Files>) -> Result<Option<Configured>, String> {
    files.map(tls::load).transpose()
}
