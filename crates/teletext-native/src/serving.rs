//! The viewers' address, served (REQ-6965): plain HTTP/1.1 on TCP for a
//! proxy in front, as before; or, given a certificate and key, TLS on TCP
//! with HTTP/2 or HTTP/1.1 as the client chooses, and HTTP/3 over QUIC on
//! UDP at the same address and port, which every answer over TCP then
//! names in `Alt-Svc` at the port viewers reach the server at.

use std::convert::Infallible;
use std::future::Future;
use std::net::SocketAddr;
use std::sync::Arc;

use hyper::Uri;
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
    /// `Alt-Svc: h3=":<port>"` (RFC 7838 section 3), the port viewers
    /// reach QUIC at ([`Direct::public_port`]).
    alt_svc: HeaderValue,
}

/// What the server serves TLS with, and where viewers reach it.
#[derive(Clone, Copy, Debug)]
pub struct Direct<'tls> {
    /// The certificate and key, read.
    pub tls: &'tls Configured,
    /// The port viewers reach the server at, which `Alt-Svc` names
    /// (REQ-6965): [`public_port`] of the server's origin. It is the
    /// listening port only when nothing relays connections to the server
    /// from another port (DEC-6972).
    pub public_port: u16,
}

/// The default port of `http` (RFC 9110 section 4.2.1).
const HTTP_PORT: u16 = 80;
/// The default port of `https` (RFC 9110 section 4.2.2).
const HTTPS_PORT: u16 = 443;

/// The port viewers reach the server at (REQ-6965): the one `origin`
/// names, or its scheme's default when it names none.
///
/// # Errors
/// An origin that is not an `http` or `https` address, naming it.
pub fn public_port(origin: &str) -> Result<u16, String> {
    let address: Uri = origin.parse().map_err(|error| format!("cannot read the public port from the origin {origin}: {error}"))?;
    let default = match address.scheme_str() {
        Some("https") => HTTPS_PORT,
        Some("http") => HTTP_PORT,
        _ => return Err(format!("cannot read the public port from the origin {origin}: it must start http:// or https://")),
    };
    match address.authority().map(|authority| (authority.port_u16(), authority.as_str() == authority.host())) {
        Some((Some(port), _)) => Ok(port),
        // The host alone: no port is named.
        Some((None, true)) => Ok(default),
        // More than the host, and not a port: one over 65,535, or a user.
        _ => Err(format!("cannot read the public port from the origin {origin}: what follows its host is not a port")),
    }
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
    /// Takes `listener`, and with `direct` binds UDP at its address and
    /// port for QUIC. Without `direct` no UDP socket is made.
    ///
    /// # Errors
    /// An address that cannot be read, or a UDP socket that cannot be
    /// bound there.
    pub fn start(listener: TcpListener, direct: Option<Direct<'_>>, routes: Arc<Routes>) -> Result<Self, String> {
        let address = listener.local_addr().map_err(|error| format!("cannot read the address listened on: {error}"))?;
        let Some(Direct { tls, public_port }) = direct else { return Ok(Self { listener, secured: None, quic: None, routes }) };
        let quic = http3::endpoint(tls, address)?;
        let alt_svc = HeaderValue::from_str(&format!("h3=\":{public_port}\"")).map_err(|error| format!("cannot name the QUIC port: {error}"))?;
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

#[cfg(test)]
mod tests {
    use super::public_port;

    /// REQ-6965: the public port is the origin's, or its scheme's default
    /// (RFC 9110 sections 4.2.1 and 4.2.2), whatever the host is written
    /// as; and text that is no `http` or `https` origin is refused, named.
    #[test]
    fn the_public_port_is_the_origins_or_its_schemes_default() {
        assert_eq!(public_port("https://app.neottx.com"), Ok(443));
        assert_eq!(public_port("http://app.neottx.com"), Ok(80));
        assert_eq!(public_port("https://app.neottx.com:9443"), Ok(9443));
        assert_eq!(public_port("http://127.0.0.1:8090"), Ok(8090));
        assert_eq!(public_port("https://[2001:db8::1]"), Ok(443));
        assert_eq!(public_port("https://[2001:db8::1]:8443"), Ok(8443));
        for refused in ["app.neottx.com:9443", "wss://app.neottx.com", "", "https://app.neottx.com:99999", "https://user@app.neottx.com"] {
            let error = public_port(refused).unwrap_err();
            assert!(error.starts_with(&format!("cannot read the public port from the origin {refused}: ")), "{error}");
        }
    }
}
