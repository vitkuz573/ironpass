use ironpass_core::{Error, Result, models::ProxyNode};
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;
use x25519_dalek::{EphemeralSecret, PublicKey};
use base64::Engine;
use rand::{rngs::StdRng, SeedableRng};

pub async fn connect_reality(
    tcp: TcpStream,
    node: &ProxyNode,
) -> Result<TlsStream<TcpStream>> {
    let server_name = node.sni.as_deref()
        .unwrap_or(&node.server)
        .to_string();

    let pub_key_b64 = node.public_key.as_deref()
        .ok_or_else(|| Error::Parse("Reality requires public_key (pbk)".into()))?;

    let pub_key_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(pub_key_b64)
        .map_err(|e| Error::Parse(format!("Invalid pbk base64: {}", e)))?;

    if pub_key_bytes.len() != 32 {
        return Err(Error::Parse(format!("Invalid pbk length: {}", pub_key_bytes.len())));
    }

    let mut pub_key_arr = [0u8; 32];
    pub_key_arr.copy_from_slice(&pub_key_bytes);
    let server_public_key = PublicKey::from(pub_key_arr);

    let mut rng = StdRng::from_rng(&mut rand::rng());
    let client_secret = EphemeralSecret::random_from_rng(&mut rng);
    let _client_public = PublicKey::from(&client_secret);

    let shared_secret = client_secret.diffie_hellman(&server_public_key);

    let mut root_store = rustls::RootCertStore::empty();
    root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

    let mut config = rustls::ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();

    config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];

    let connector = tokio_rustls::TlsConnector::from(std::sync::Arc::new(config));

    let domain = rustls::pki_types::ServerName::try_from(server_name.clone())
        .map_err(|e| Error::Parse(format!("Invalid SNI: {}", e)))?;

    let tls_stream = connector.connect(domain, tcp).await
        .map_err(|e| Error::Custom(format!("Reality TLS handshake failed: {}", e)))?;

    tracing::debug!("Reality TLS connected to {}", server_name);
    tracing::debug!("Shared secret: {:02x?}", shared_secret.as_bytes());

    Ok(tls_stream)
}
