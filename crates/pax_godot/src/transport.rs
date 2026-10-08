//! The bytes under a [`crate::connection::Connection`]: plain TCP, or TLS pinned to a
//! server certificate's fingerprint (D24, M4-6).
//!
//! The socket is non-blocking and polled once per frame, so TLS is driven by hand:
//! bytes from the socket go into rustls, plaintext comes out, and the records
//! rustls wants to send go back to the socket whenever it can take them. Writes
//! never block: rustls buffers what the socket can't take yet, including anything
//! written before the handshake has finished.
//!
//! **Trust:** clients pin the server's certificate by its SHA-256, which the host
//! shares with the players (the server prints it). No certificate authority is
//! involved: a player-hosted game has none. The handshake's signatures are still
//! checked against the pinned certificate, so knowing the fingerprint isn't enough
//! to pose as the server.

use std::io::{self, ErrorKind, Read, Write};
use std::net::TcpStream;
use std::sync::Arc;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::CryptoProvider;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConnection, DigitallySignedStruct, SignatureScheme};

/// The SHA-256 of a certificate (DER) in lowercase hex, as servers print it.
pub fn fingerprint(certificate: &[u8]) -> String {
    ring::digest::digest(&ring::digest::SHA256, certificate).as_ref().iter().map(|b| format!("{b:02x}")).collect()
}

/// A fingerprint as a player may type it (any case, with `:` or spaces), or `None`
/// if it isn't 32 bytes of hex.
pub fn normalise(fingerprint: &str) -> Option<String> {
    let hex: String = fingerprint.chars().filter(|c| !matches!(c, ':' | ' ')).collect::<String>().to_lowercase();
    (hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit())).then_some(hex)
}

#[derive(Debug)]
pub enum Transport {
    Plain(TcpStream),
    Tls { tls: Box<ClientConnection>, socket: TcpStream },
}

impl Transport {
    /// TLS over `socket`, trusting only the certificate whose SHA-256 is `pinned`
    /// (normalised, see [`normalise`]).
    pub fn tls(socket: TcpStream, pinned: String) -> io::Result<Transport> {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let config = rustls::ClientConfig::builder_with_provider(provider.clone())
            .with_safe_default_protocol_versions()
            .map_err(io::Error::other)?
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(Pinned { pinned, provider }))
            .with_no_client_auth();
        // The name only fills SNI, and matches what the server's self-signed
        // certificate names (`pax_server::tls`): trust comes from the pinned
        // fingerprint, never from the name.
        let name = ServerName::try_from("pax-server").expect("a valid DNS name");
        let tls = ClientConnection::new(Arc::new(config), name).map_err(io::Error::other)?;
        Ok(Transport::Tls { tls: Box::new(tls), socket })
    }

    pub fn socket(&self) -> &TcpStream {
        match self {
            Transport::Plain(socket) | Transport::Tls { socket, .. } => socket,
        }
    }

    /// Writes plaintext. Over TLS it is all accepted (rustls buffers it), and as much
    /// as the socket takes is sent now; [`Self::flush`] sends the rest later.
    pub fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Transport::Plain(socket) => socket.write(buf),
            Transport::Tls { tls, socket } => {
                let n = tls.writer().write(buf)?;
                push(tls, socket)?;
                Ok(n)
            }
        }
    }

    /// Sends what TLS has buffered, as far as the socket takes it now.
    pub fn flush(&mut self) -> io::Result<()> {
        match self {
            Transport::Plain(_) => Ok(()),
            Transport::Tls { tls, socket } => push(tls, socket),
        }
    }

    /// Reads plaintext: `Ok(0)` at the end, `WouldBlock` when nothing has arrived.
    pub fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Transport::Plain(socket) => socket.read(buf),
            Transport::Tls { tls, socket } => loop {
                match tls.reader().read(buf) {
                    Err(e) if e.kind() == ErrorKind::WouldBlock => {}
                    other => return other,
                }
                // No plaintext yet: take more from the socket (WouldBlock ends it).
                if tls.read_tls(socket)? == 0 {
                    return Ok(0);
                }
                tls.process_new_packets().map_err(|e| io::Error::new(ErrorKind::InvalidData, e))?;
                // The handshake may have something to answer.
                push(tls, socket)?;
            },
        }
    }
}

/// Sends the records rustls wants to, until the socket would block.
fn push(tls: &mut ClientConnection, socket: &mut TcpStream) -> io::Result<()> {
    while tls.wants_write() {
        match tls.write_tls(socket) {
            Ok(_) => {}
            Err(e) if e.kind() == ErrorKind::WouldBlock => return Ok(()),
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

/// Trusts exactly one certificate, by its SHA-256, and checks the handshake's
/// signatures against it with the provider's algorithms.
#[derive(Debug)]
struct Pinned {
    pinned: String,
    provider: Arc<CryptoProvider>,
}

impl ServerCertVerifier for Pinned {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        if fingerprint(end_entity) == self.pinned {
            Ok(ServerCertVerified::assertion())
        } else {
            Err(rustls::Error::General("the server's certificate is not the one its fingerprint names".to_owned()))
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider.signature_verification_algorithms.supported_schemes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprints_normalise_from_how_players_type_them() {
        let hex = "ab".repeat(32);
        let typed = hex
            .to_uppercase()
            .as_bytes()
            .chunks(2)
            .map(|c| std::str::from_utf8(c).unwrap())
            .collect::<Vec<_>>()
            .join(":");
        assert_eq!(normalise(&typed), Some(hex.clone()));
        assert_eq!(normalise(&format!(" {hex} ")), Some(hex));
        assert_eq!(normalise("abcd"), None);
        assert_eq!(normalise(&"zz".repeat(32)), None);
    }
}
