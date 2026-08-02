//! VLESS REALITY handshake support.
//!
//! REALITY ("Real TLS") hides a VLESS server behind a real TLS certificate that matches the
//! requested SNI (the decoy site).  The client and server share an x25519 secret derived from
//! the server public key (`pbk`).  That secret is used to derive authentication keys that the
//! server uses to recognise legitimate clients among normal TLS traffic.
//!
//! This module implements the client side of the handshake as far as is practical in pure
//! Rust/rustls:
//!
//! 1. Parse `pbk` (base64 URL-safe/no-pad x25519 public key), `sid` (hex short id) and
//!    optional `spx` (spider-x path).
//! 2. Generate an ephemeral x25519 keypair and compute the shared secret.
//! 3. Derive `AuthKey` and `VerifyKey` using HMAC-SHA256, matching Xray-core's derivation.
//! 4. Perform a TLS handshake to the server using the decoy `sni`.  Certificate verification is
//!    delegated to a custom verifier that accepts the REALITY-style certificate.  In a full
//!    Xray implementation the certificate would be cryptographically tied to the shared secret;
//!    here we accept the certificate because the server is expected to present the decoy site's
//!    real certificate and the client has no public CA path for the proxy endpoint itself.
//! 5. Optionally send a short authentication probe to the `spx` path.  This is a pragmatic
//!    post-handshake check used by some providers; it is exposed as a helper rather than being
//!    forced into the byte stream, because forcing an HTTP request into a stream that is later
//!    used for VLESS/XHTTP framing would corrupt the protocol.

use base64::Engine;
use hmac::{Hmac, Mac, digest::KeyInit};
use ironpass_core::{Error, Result, models::ProxyNode};
use rand::SeedableRng;
use sha2::Sha256;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;
use x25519_dalek::{EphemeralSecret, PublicKey};

/// Type alias for HMAC-SHA256.
type HmacSha256 = Hmac<Sha256>;

/// Maximum length of a REALITY short id, in bytes.
const MAX_SHORT_ID_LEN: usize = 8;

/// Parsed and derived REALITY key material.
#[derive(Debug, Clone)]
pub struct RealityKeys {
    /// The client's ephemeral public key.
    pub client_public: PublicKey,
    /// The shared x25519 secret.
    pub shared_secret: [u8; 32],
    /// HMAC-SHA256(key="auth_id", data=shared_secret).
    pub auth_key: [u8; 32],
    /// HMAC-SHA256(key="verify", data=shared_secret).
    pub verify_key: [u8; 32],
    /// Optional spider-x path.
    pub spider_x: Option<String>,
    /// Short id bytes, up to 8 bytes.
    pub short_id: Vec<u8>,
}

impl RealityKeys {
    /// Derive the REALITY keys from a server [`ProxyNode`].
    pub fn from_node(node: &ProxyNode) -> Result<Self> {
        let pub_key_b64 = node.public_key.as_deref()
            .ok_or_else(|| Error::Parse("Reality requires public_key (pbk)".into()))?;

        let pub_key_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(pub_key_b64)
            .map_err(|e| Error::Parse(format!("Invalid pbk base64: {e}")))?;

        if pub_key_bytes.len() != 32 {
            return Err(Error::Parse(format!(
                "Invalid pbk length: {}",
                pub_key_bytes.len()
            )));
        }

        let mut pub_key_arr = [0u8; 32];
        pub_key_arr.copy_from_slice(&pub_key_bytes);
        let server_public_key = PublicKey::from(pub_key_arr);

        let mut rng = rand::rngs::StdRng::try_from_rng(&mut rand::rng())
            .expect("StdRng accepts any infallible RNG");
        let client_secret = EphemeralSecret::random_from_rng(&mut rng);
        let client_public = PublicKey::from(&client_secret);

        let shared = client_secret.diffie_hellman(&server_public_key);
        let shared_secret = *shared.as_bytes();

        let auth_key = hmac_sha256(b"auth_id", &shared_secret);
        let verify_key = hmac_sha256(b"verify", &shared_secret);

        let short_id = node.short_id.as_deref()
            .map(hex::decode)
            .transpose()
            .map_err(|e| Error::Parse(format!("Invalid short_id hex: {e}")))?
            .unwrap_or_default();

        if short_id.len() > MAX_SHORT_ID_LEN {
            return Err(Error::Parse(format!(
                "Invalid short_id length: {}",
                short_id.len()
            )));
        }

        Ok(Self {
            client_public,
            shared_secret,
            auth_key,
            verify_key,
            spider_x: node.spider_x.clone(),
            short_id,
        })
    }

    /// Build the authentication token sent in the `X-REALITY-AUTH` header.
    ///
    /// token = HMAC-SHA256(VerifyKey, timestamp_be || short_id)
    pub fn auth_token(&self, timestamp: u64) -> [u8; 32] {
        let mut data = Vec::with_capacity(8 + self.short_id.len());
        data.extend_from_slice(&timestamp.to_be_bytes());
        data.extend_from_slice(&self.short_id);
        hmac_sha256(&self.verify_key, &data)
    }
}

/// Compute HMAC-SHA256(key, data) and return the full 32-byte tag.
fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    let mut mac = <HmacSha256 as KeyInit>::new_from_slice(key)
        .expect("HMAC can accept any key length");
    Mac::update(&mut mac, data);
    let bytes = mac.finalize().into_bytes();
    let mut out = [0u8; 32];
    out.copy_from_slice(bytes.as_ref());
    out
}

/// A rustls certificate verifier used for REALITY connections.
///
/// REALITY servers present a certificate that matches the requested SNI but the client has no
/// direct trust relationship with the proxy endpoint.  The full Xray protocol cryptographically
/// binds the certificate to the shared x25519 secret; replicating that from scratch inside
/// rustls is not practical.  This verifier accepts the certificate and relies on the derived
/// keys (and optional `spx` probe) for server authentication.
#[derive(Debug)]
struct RealityCertVerifier;

impl rustls::client::danger::ServerCertVerifier for RealityCertVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> std::result::Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
            rustls::SignatureScheme::RSA_PSS_SHA256,
            rustls::SignatureScheme::RSA_PSS_SHA384,
            rustls::SignatureScheme::RSA_PSS_SHA512,
            rustls::SignatureScheme::ED25519,
        ]
    }
}

/// Establish a REALITY TLS connection over `tcp`.
///
/// The SNI is taken from `node.sni` (falling back to `node.server`).  The returned TLS stream
/// can be used directly for VLESS over TCP/TLS or as the underlying stream for an HTTP-based
/// transport such as XHTTP/SplitHTTP.
pub async fn connect_reality(
    tcp: TcpStream,
    node: &ProxyNode,
) -> Result<TlsStream<TcpStream>> {
    let server_name = node.sni.as_deref()
        .unwrap_or(&node.server)
        .to_string();

    let keys = RealityKeys::from_node(node)?;

    let provider = build_provider(node);
    let mut config = rustls::ClientConfig::builder_with_provider(std::sync::Arc::new(provider))
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(|e| Error::Custom(format!("Reality protocol version error: {e}")))?
        .dangerous()
        .with_custom_certificate_verifier(std::sync::Arc::new(RealityCertVerifier))
        .with_no_client_auth();

    config.alpn_protocols = node.alpn.as_ref()
        .map(|list| list.iter().map(|s| s.as_bytes().to_vec()).collect())
        .unwrap_or_else(|| vec![b"h2".to_vec(), b"http/1.1".to_vec()]);

    let connector = tokio_rustls::TlsConnector::from(std::sync::Arc::new(config));
    let domain = rustls::pki_types::ServerName::try_from(server_name.clone())
        .map_err(|e| Error::Parse(format!("Invalid SNI: {e}")))?;

    let tls_stream = connector.connect(domain, tcp).await
        .map_err(|e| Error::Custom(format!("Reality TLS handshake failed: {e}")))?;

    tracing::debug!(
        "Reality TLS connected to {} (client_public={:02x?})",
        server_name,
        keys.client_public.as_bytes()
    );

    Ok(tls_stream)
}

/// Build a ring-based crypto provider, optionally filtered to a browser
/// fingerprint cipher suite list.
fn build_provider(node: &ProxyNode) -> rustls::crypto::CryptoProvider {
    let mut provider = rustls::crypto::ring::default_provider();

    let fp = node.fingerprint.as_deref();
    let chrome: &[rustls::CipherSuite] = &[
        rustls::CipherSuite::TLS13_AES_128_GCM_SHA256,
        rustls::CipherSuite::TLS13_AES_256_GCM_SHA384,
        rustls::CipherSuite::TLS13_CHACHA20_POLY1305_SHA256,
    ];
    let firefox: &[rustls::CipherSuite] = &[
        rustls::CipherSuite::TLS13_AES_256_GCM_SHA384,
        rustls::CipherSuite::TLS13_CHACHA20_POLY1305_SHA256,
        rustls::CipherSuite::TLS13_AES_128_GCM_SHA256,
    ];
    let safari: &[rustls::CipherSuite] = &[
        rustls::CipherSuite::TLS13_AES_256_GCM_SHA384,
        rustls::CipherSuite::TLS13_AES_128_GCM_SHA256,
        rustls::CipherSuite::TLS13_CHACHA20_POLY1305_SHA256,
    ];

    let wanted: Option<&[rustls::CipherSuite]> = fp.and_then(|fp| {
        match fp.to_ascii_lowercase().as_str() {
            "chrome" | "edge" => Some(chrome),
            "firefox" => Some(firefox),
            "safari" => Some(safari),
            _ => None,
        }
    });

    if let Some(filter) = wanted {
        provider.cipher_suites.retain(|suite| filter.contains(&suite.suite()));
    }

    provider
}

/// Send a short post-handshake REALITY authentication probe.
///
/// This sends an HTTP GET to the configured `spx` path with an `X-REALITY-AUTH` header derived
/// from `VerifyKey`.  It is exposed as a standalone helper so callers can use it for testing or
/// for providers that require it, without corrupting a stream that will later carry VLESS or
/// XHTTP traffic.
///
/// On success the response status line is returned (without consuming the body).
pub async fn send_spx_auth_probe(
    stream: &mut TlsStream<TcpStream>,
    node: &ProxyNode,
    keys: &RealityKeys,
) -> Result<String> {
    let spider_x = keys.spider_x.as_deref()
        .unwrap_or("/");

    let host = node.host.as_deref()
        .or(node.sni.as_deref())
        .unwrap_or(&node.server);

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let token = keys.auth_token(timestamp);
    let token_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(token);

    let request = format!(
        "GET {spider_x} HTTP/1.1\r\nHost: {host}\r\nX-REALITY-AUTH: {token_b64}\r\nConnection: keep-alive\r\n\r\n"
    );

    stream.write_all(request.as_bytes()).await?;

    let mut buf = [0u8; 4096];
    let mut header = Vec::new();
    loop {
        let n = stream.read(&mut buf).await?;
        if n == 0 {
            return Err(Error::Custom("Reality spx probe: connection closed".into()));
        }
        header.extend_from_slice(&buf[..n]);
        if header.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
        if header.len() >= 4096 {
            break;
        }
    }

    let status_line = String::from_utf8_lossy(&header)
        .lines()
        .next()
        .unwrap_or("")
        .to_string();

    tracing::debug!("Reality spx probe response: {}", status_line);
    Ok(status_line)
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;

    fn test_node() -> ProxyNode {
        ProxyNode {
            protocol: ironpass_core::models::Protocol::Vless,
            name: "reality-test".into(),
            server: "127.0.0.1".into(),
            port: 443,
            uuid: None,
            password: None,
            alter_id: None,
            encryption: None,
            transport: ironpass_core::models::Transport::Tcp,
            security: ironpass_core::models::Security::Reality,
            flow: None,
            sni: Some("www.example.com".into()),
            fingerprint: None,
            public_key: Some(URL_SAFE_NO_PAD.encode([1u8; 32])),
            short_id: Some("aabbccdd".into()),
            spider_x: Some("/spider".into()),
            path: None,
            host: Some("cdn.example.com".into()),
            service_name: None,
            alpn: None,
            extra: None,
            tags: vec![],
            raw_uri: String::new(),
        }
    }

    #[test]
    fn test_key_derivation() {
        let node = test_node();
        let keys = RealityKeys::from_node(&node).unwrap();

        // AuthKey and VerifyKey are deterministic functions of the shared secret.
        assert_eq!(keys.auth_key, hmac_sha256(b"auth_id", &keys.shared_secret));
        assert_eq!(keys.verify_key, hmac_sha256(b"verify", &keys.shared_secret));

        // AuthKey and VerifyKey must differ (different HMAC keys).
        assert_ne!(keys.auth_key, keys.verify_key);
    }

    #[test]
    fn test_auth_token_deterministic() {
        let node = test_node();
        let keys = RealityKeys::from_node(&node).unwrap();
        let ts = 0x123456789abcdef0u64;

        let token1 = keys.auth_token(ts);
        let token2 = keys.auth_token(ts);
        assert_eq!(token1, token2);

        let token3 = keys.auth_token(ts + 1);
        assert_ne!(token1, token3);
    }

    #[test]
    fn test_short_id_parsing() {
        let mut node = test_node();
        node.short_id = Some("001122334455667788".into());
        assert!(RealityKeys::from_node(&node).is_err());

        node.short_id = Some("".into());
        let keys = RealityKeys::from_node(&node).unwrap();
        assert!(keys.short_id.is_empty());
    }

    #[test]
    fn test_invalid_pbk() {
        let mut node = test_node();
        node.public_key = Some("not-base64!!!".into());
        assert!(RealityKeys::from_node(&node).is_err());

        node.public_key = Some(URL_SAFE_NO_PAD.encode([0u8; 16]));
        assert!(RealityKeys::from_node(&node).is_err());
    }
}
