//! TLS from a certificate chain and a private key in files (REQ-6965):
//! `rustls` over the `ring` provider, for TCP with TLS 1.3 and 1.2 and for
//! QUIC with TLS 1.3, which RFC 9001 requires. The files are read at
//! start, where a file that cannot be read, or a key that is not the
//! certificate's, stops the server; and again whenever the operator asks
//! (`SIGHUP`, [`Configured::reload`]), when a pair that loads is used for
//! every handshake from then on and one that does not leaves the pair in
//! use as it was.

use std::path::PathBuf;
use std::sync::{Arc, PoisonError, RwLock};

use quinn::crypto::rustls::QuicServerConfig;
use rustls::ServerConfig;
use rustls::crypto::CryptoProvider;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::server::{ClientHello, ResolvesServerCert};
use rustls::sign::CertifiedKey;

/// HTTP/2's protocol identifier for TLS (RFC 9113 section 3.2).
pub const ALPN_H2: &[u8] = b"h2";
/// HTTP/1.1's (RFC 7301 section 6).
pub const ALPN_HTTP1: &[u8] = b"http/1.1";
/// HTTP/3's (RFC 9114 section 3.1).
pub const ALPN_H3: &[u8] = b"h3";

/// Where the certificate and its key are.
#[derive(Clone, Debug)]
pub struct Files {
    /// The certificate chain, the server's own certificate first, in PEM.
    pub certificate: PathBuf,
    /// The private key of the chain's first certificate, in PEM.
    pub key: PathBuf,
}

/// The certificate and key every handshake is given, TCP's and QUIC's
/// alike, which [`Configured::reload`] replaces. A handshake begun takes
/// the pair in use when its `ClientHello` is read; one begun after a
/// reload takes the new pair, and none is ever given half of each.
#[derive(Debug)]
struct Current(RwLock<Arc<CertifiedKey>>);

impl ResolvesServerCert for Current {
    fn resolve(&self, _: ClientHello<'_>) -> Option<Arc<CertifiedKey>> {
        Some(Arc::clone(&self.0.read().unwrap_or_else(PoisonError::into_inner)))
    }
}

/// TLS as the server offers it.
#[derive(Clone)]
pub struct Configured {
    /// For TCP: TLS 1.3 and 1.2, offering `h2` then `http/1.1`.
    pub tcp: Arc<ServerConfig>,
    /// For QUIC: TLS 1.3, offering `h3`.
    pub quic: Arc<QuicServerConfig>,
    /// The pair both use.
    current: Arc<Current>,
    provider: Arc<CryptoProvider>,
}

impl std::fmt::Debug for Configured {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Configured").field("tcp", &self.tcp.alpn_protocols).finish_non_exhaustive()
    }
}

/// Reads `files` as a certificate chain and its key, checked to belong
/// together.
fn certified(files: &Files, provider: &CryptoProvider) -> Result<CertifiedKey, String> {
    let certificate = files.certificate.display();
    let key_file = files.key.display();
    let chain = CertificateDer::pem_file_iter(&files.certificate)
        .and_then(Iterator::collect::<Result<Vec<_>, _>>)
        .map_err(|error| format!("cannot read the certificate chain {certificate}: {error}"))?;
    if chain.is_empty() {
        return Err(format!("the certificate chain {certificate} holds no certificate"));
    }
    let key = PrivateKeyDer::from_pem_file(&files.key).map_err(|error| format!("cannot read the private key {key_file}: {error}"))?;
    CertifiedKey::from_der(chain, key, provider).map_err(|error| match error {
        rustls::Error::InconsistentKeys(_) => format!("the private key {key_file} is not the key of the certificate {certificate}"),
        other => format!("cannot use the certificate {certificate} with the private key {key_file}: {other}"),
    })
}

/// Reads `files` and makes of them what the server offers.
///
/// # Errors
/// What is wrong, naming the file: one that cannot be read or holds
/// nothing of its kind, or a key that is not the certificate's.
pub fn load(files: &Files) -> Result<Configured, String> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let current = Arc::new(Current(RwLock::new(Arc::new(certified(files, &provider)?))));
    let unusable = |error: rustls::Error| format!("cannot serve TLS with the certificate {}: {error}", files.certificate.display());
    let mut tcp = ServerConfig::builder_with_provider(Arc::clone(&provider))
        .with_safe_default_protocol_versions()
        .map_err(unusable)?
        .with_no_client_auth()
        .with_cert_resolver(Arc::clone(&current) as Arc<dyn ResolvesServerCert>);
    tcp.alpn_protocols = vec![ALPN_H2.to_vec(), ALPN_HTTP1.to_vec()];
    let mut quic = ServerConfig::builder_with_provider(Arc::clone(&provider))
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(unusable)?
        .with_no_client_auth()
        .with_cert_resolver(Arc::clone(&current) as Arc<dyn ResolvesServerCert>);
    quic.alpn_protocols = vec![ALPN_H3.to_vec()];
    // No early data: a request in it could be replayed (RFC 9001 section
    // 9.2), and nothing here is worth a round trip saved.
    quic.max_early_data_size = 0;
    let quic = QuicServerConfig::try_from(quic).map_err(|error| format!("cannot serve QUIC with the certificate {}: {error}", files.certificate.display()))?;
    Ok(Configured { tcp: Arc::new(tcp), quic: Arc::new(quic), current, provider })
}

impl Configured {
    /// Reads `files` again and, when they are a chain and its key, gives
    /// them to every TLS and QUIC handshake from now on; connections
    /// already made keep the pair they were made with. A pair that does
    /// not load changes nothing.
    ///
    /// # Errors
    /// As [`load`]: what is wrong, naming the file.
    pub fn reload(&self, files: &Files) -> Result<(), String> {
        let pair = Arc::new(certified(files, &self.provider)?);
        *self.current.0.write().unwrap_or_else(PoisonError::into_inner) = pair;
        Ok(())
    }
}
