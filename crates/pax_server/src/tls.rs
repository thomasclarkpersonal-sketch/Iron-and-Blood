//! TLS for multiplayer off localhost (D24, M4-6).
//!
//! The server presents a certificate, either self-signed at start (a player-hosted
//! game) or loaded from PEM files (a dedicated server). Clients pin the
//! certificate's SHA-256 fingerprint, which the host shares with the players (the
//! server prints it, and can write it to a file): no certificate authority is
//! involved. The client still verifies the handshake's signatures against the
//! pinned certificate, so a stolen fingerprint alone doesn't let anyone pose as the
//! server.

use std::path::Path;
use std::sync::Arc;

use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use tokio_rustls::TlsAcceptor;

/// The name a self-signed certificate carries. Cosmetic: clients pin the fingerprint
/// and ignore the name. The bridge sends the same one as SNI
/// (`pax_godot::transport::SERVER_NAME`).
const SERVER_NAME: &str = "pax-server";

/// A server's TLS: what accepts connections, and what clients pin.
#[derive(Clone)]
pub(crate) struct Tls {
    pub acceptor: TlsAcceptor,
    /// The certificate's SHA-256, as clients pin it: 32 bytes in lowercase hex.
    pub fingerprint: String,
}

impl std::fmt::Debug for Tls {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tls").field("fingerprint", &self.fingerprint).finish_non_exhaustive()
    }
}

/// The SHA-256 of a certificate (DER), in lowercase hex: what clients pin. The
/// client's bridge computes it the same way (`pax_godot::transport::fingerprint`);
/// its TLS test pins what a real server printed, so the two can't drift apart.
pub(crate) fn fingerprint(certificate: &[u8]) -> String {
    ring::digest::digest(&ring::digest::SHA256, certificate).as_ref().iter().map(|b| format!("{b:02x}")).collect()
}

/// A fresh self-signed certificate, for a player-hosted game.
pub(crate) fn self_signed() -> Result<Tls, String> {
    let generated = rcgen::generate_simple_self_signed(vec![SERVER_NAME.to_owned()])
        .map_err(|e| format!("cannot make a certificate: {e}"))?;
    let certificate = generated.cert.der().clone();
    let key = PrivateKeyDer::Pkcs8(generated.key_pair.serialize_der().into());
    tls(vec![certificate], key)
}

/// A certificate chain and its key from PEM files, for a dedicated server.
pub(crate) fn from_pem(certificate: &Path, key: &Path) -> Result<Tls, String> {
    let chain = CertificateDer::pem_file_iter(certificate)
        .and_then(|certs| certs.collect::<Result<Vec<_>, _>>())
        .map_err(|e| format!("cannot read the certificate {}: {e}", certificate.display()))?;
    if chain.is_empty() {
        return Err(format!("{} holds no certificate", certificate.display()));
    }
    let key = PrivateKeyDer::from_pem_file(key).map_err(|e| format!("cannot read the key {}: {e}", key.display()))?;
    tls(chain, key)
}

fn tls(chain: Vec<CertificateDer<'static>>, key: PrivateKeyDer<'static>) -> Result<Tls, String> {
    let fingerprint = fingerprint(&chain[0]);
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = rustls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| format!("TLS: {e}"))?
        .with_no_client_auth()
        .with_single_cert(chain, key)
        .map_err(|e| format!("TLS: {e}"))?;
    Ok(Tls { acceptor: TlsAcceptor::from(Arc::new(config)), fingerprint })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_self_signed_certificate_has_a_stable_fingerprint() {
        let tls = self_signed().unwrap();
        assert_eq!(tls.fingerprint.len(), 64, "SHA-256 in hex");
        assert!(tls.fingerprint.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()));
        assert_ne!(self_signed().unwrap().fingerprint, tls.fingerprint, "each start makes a new certificate");
    }
}
