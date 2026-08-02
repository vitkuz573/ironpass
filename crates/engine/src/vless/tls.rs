use ironpass_core::{Error, Result, models::ProxyNode};
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;

pub async fn connect_tls(
    stream: TcpStream,
    node: &ProxyNode,
) -> Result<TlsStream<TcpStream>> {
    let sni = node.sni.as_deref()
        .unwrap_or(&node.server);

    let mut root_store = rustls::RootCertStore::empty();
    root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

    let mut config = rustls::ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();

    if let Some(ref alpn) = node.alpn {
        config.alpn_protocols = alpn.iter().map(|a| a.as_bytes().to_vec()).collect();
    }

    let connector = tokio_rustls::TlsConnector::from(std::sync::Arc::new(config));
    let domain = rustls::pki_types::ServerName::try_from(sni.to_string())
        .map_err(|e| Error::Parse(format!("Invalid SNI: {}", e)))?;

    let tls_stream = connector.connect(domain, stream).await
        .map_err(|e| Error::Custom(format!("TLS handshake failed: {}", e)))?;

    tracing::debug!("TLS connected to {}", sni);

    Ok(tls_stream)
}
