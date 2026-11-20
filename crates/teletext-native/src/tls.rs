//! TLS from a certificate chain and a private key in files (REQ-6965):
//! `rustls` over the `ring` provider, for TCP with TLS 1.3 and 1.2 and for
//! QUIC with TLS 1.3, which RFC 9001 requires. The files are read once, at
//! start; a file that cannot be read, or a key that is not the
//! certificate's, stops the server there.

use std::path::PathBuf;
use std::sync::Arc;

use quinn::crypto::rustls::QuicServerConfig;
use rustls::ServerConfig;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};

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

/// TLS as the server offers it.
#[derive(Clone)]
pub struct Configured {
    /// For TCP: TLS 1.3 and 1.2, offering `h2` then `http/1.1`.
    pub tcp: Arc<ServerConfig>,
    /// For QUIC: TLS 1.3, offering `h3`.
    pub quic: Arc<QuicServerConfig>,
}

impl std::fmt::Debug for Configured {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Configured").field("tcp", &self.tcp.alpn_protocols).finish_non_exhaustive()
    }
}

/// Reads `files` and makes of them what the server offers.
///
/// # Errors
/// What is wrong, naming the file: one that cannot be read or holds
/// nothing of its kind, or a key that is not the certificate's.
pub fn load(files: &Files) -> Result<Configured, String> {
    let certificate = files.certificate.display();
    let key_file = files.key.display();
    let chain = CertificateDer::pem_file_iter(&files.certificate)
        .and_then(Iterator::collect::<Result<Vec<_>, _>>)
        .map_err(|error| format!("cannot read the certificate chain {certificate}: {error}"))?;
    if chain.is_empty() {
        return Err(format!("the certificate chain {certificate} holds no certificate"));
    }
    let key = PrivateKeyDer::from_pem_file(&files.key).map_err(|error| format!("cannot read the private key {key_file}: {error}"))?;
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let unusable = |error: rustls::Error| match error {
        rustls::Error::InconsistentKeys(_) => format!("the private key {key_file} is not the key of the certificate {certificate}"),
        other => format!("cannot use the certificate {certificate} with the private key {key_file}: {other}"),
    };
    let mut tcp = ServerConfig::builder_with_provider(Arc::clone(&provider))
        .with_safe_default_protocol_versions()
        .map_err(unusable)?
        .with_no_client_auth()
        .with_single_cert(chain.clone(), key.clone_key())
        .map_err(unusable)?;
    tcp.alpn_protocols = vec![ALPN_H2.to_vec(), ALPN_HTTP1.to_vec()];
    let mut quic = ServerConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(unusable)?
        .with_no_client_auth()
        .with_single_cert(chain, key)
        .map_err(unusable)?;
    quic.alpn_protocols = vec![ALPN_H3.to_vec()];
    // No early data: a request in it could be replayed (RFC 9001 section
    // 9.2), and nothing here is worth a round trip saved.
    quic.max_early_data_size = 0;
    let quic = QuicServerConfig::try_from(quic).map_err(|error| format!("cannot serve QUIC with the certificate {certificate}: {error}"))?;
    Ok(Configured { tcp: Arc::new(tcp), quic: Arc::new(quic) })
}
