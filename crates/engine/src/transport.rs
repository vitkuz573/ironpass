use ironpass_core::{Error, Result};
use bytes::{Bytes, BytesMut};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio_rustls::client::TlsStream;

pub struct GrpcTransport {
    tx: h2::SendStream<Bytes>,
    rx_buf: BytesMut,
    rx: Option<h2::RecvStream>,
    response_future: Option<h2::client::ResponseFuture>,
}

impl AsyncRead for GrpcTransport {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        loop {
            if !self.rx_buf.is_empty() {
                let n = self.rx_buf.len().min(buf.remaining());
                buf.put_slice(&self.rx_buf.split_to(n));
                return Poll::Ready(Ok(()));
            }

            if let Some(ref mut rx) = self.rx {
                match rx.poll_data(cx) {
                    Poll::Ready(Some(Ok(data))) => {
                        if data.len() >= 5 {
                            let _compression = data[0];
                            let len = u32::from_be_bytes([data[1], data[2], data[3], data[4]]) as usize;
                            if data.len() >= 5 + len {
                                self.rx_buf.extend_from_slice(&data[5..5+len]);
                            }
                        }
                    }
                    Poll::Ready(Some(Err(e))) => {
                        return Poll::Ready(Err(std::io::Error::other(e)));
                    }
                    Poll::Ready(None) => {
                        return Poll::Ready(Ok(()));
                    }
                    Poll::Pending => return Poll::Pending,
                }
            } else if let Some(ref mut future) = self.response_future {
                    match Pin::new(future).poll(cx) {
                    Poll::Ready(Ok(response)) => {
                        tracing::debug!("gRPC response status: {}", response.status());
                        let (_parts, recv_stream) = response.into_parts();
                        self.rx = Some(recv_stream);
                        self.response_future = None;
                        continue;
                    }
                    Poll::Ready(Err(e)) => {
                        return Poll::Ready(Err(std::io::Error::other(e)));
                    }
                    Poll::Pending => return Poll::Pending,
                }
            } else {
                return Poll::Ready(Ok(()));
            }
        }
    }
}

impl AsyncWrite for GrpcTransport {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        match self.tx.poll_capacity(cx) {
            Poll::Ready(Some(Ok(capacity))) => {
                let n = buf.len().min(capacity);
                let data = Bytes::copy_from_slice(&buf[..n]);
                if let Err(e) = self.tx.send_data(data, false) {
                    return Poll::Ready(Err(std::io::Error::other(e)));
                }
                Poll::Ready(Ok(n))
            }
            Poll::Ready(Some(Err(e))) => {
                Poll::Ready(Err(std::io::Error::other(e)))
            }
            Poll::Ready(None) => {
                Poll::Ready(Err(std::io::Error::new(std::io::ErrorKind::BrokenPipe, "stream closed")))
            }
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        let _ = self.tx.send_data(Bytes::new(), true);
        Poll::Ready(Ok(()))
    }
}

pub struct WsTransport {
    inner: tokio_tungstenite::WebSocketStream<TlsStream<TcpStream>>,
    read_buf: BytesMut,
}

impl WsTransport {
    fn new(inner: tokio_tungstenite::WebSocketStream<TlsStream<TcpStream>>) -> Self {
        Self {
            inner,
            read_buf: BytesMut::new(),
        }
    }
}

impl AsyncRead for WsTransport {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        loop {
            if !self.read_buf.is_empty() {
                let n = self.read_buf.len().min(buf.remaining());
                buf.put_slice(&self.read_buf.split_to(n));
                return Poll::Ready(Ok(()));
            }

            use futures_util::Stream;

            match Pin::new(&mut self.inner).poll_next(cx) {
                Poll::Ready(Some(Ok(msg))) => match msg {
                    tokio_tungstenite::tungstenite::protocol::Message::Binary(data) => {
                        self.read_buf.extend_from_slice(&data);
                    }
                    tokio_tungstenite::tungstenite::protocol::Message::Text(data) => {
                        self.read_buf.extend_from_slice(data.as_bytes());
                    }
                    tokio_tungstenite::tungstenite::protocol::Message::Close(_) => {
                        return Poll::Ready(Ok(()));
                    }
                    _ => {
                        // Ping/Pong: ignore and continue waiting for data.
                        continue;
                    }
                },
                Poll::Ready(Some(Err(e))) => {
                    return Poll::Ready(Err(std::io::Error::other(e)));
                }
                Poll::Ready(None) => {
                    return Poll::Ready(Ok(()));
                }
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

impl AsyncWrite for WsTransport {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        use futures_util::Sink;

        match Sink::poll_ready(Pin::new(&mut self.inner), cx) {
            Poll::Ready(Ok(())) => {
                let msg = tokio_tungstenite::tungstenite::protocol::Message::Binary(Bytes::copy_from_slice(buf));
                if let Err(e) = Sink::start_send(Pin::new(&mut self.inner), msg) {
                    return Poll::Ready(Err(std::io::Error::other(e)));
                }
                Poll::Ready(Ok(buf.len()))
            }
            Poll::Ready(Err(e)) => Poll::Ready(Err(std::io::Error::other(e))),
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        use futures_util::Sink;

        match Sink::poll_flush(Pin::new(&mut self.inner), cx) {
            Poll::Ready(Ok(())) => Poll::Ready(Ok(())),
            Poll::Ready(Err(e)) => Poll::Ready(Err(std::io::Error::other(e))),
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        use futures_util::Sink;

        match Sink::poll_close(Pin::new(&mut self.inner), cx) {
            Poll::Ready(Ok(())) => Poll::Ready(Ok(())),
            Poll::Ready(Err(e)) => Poll::Ready(Err(std::io::Error::other(e))),
            Poll::Pending => Poll::Pending,
        }
    }
}

pub async fn connect_grpc(
    tcp: TcpStream,
    sni: &str,
    path: &str,
) -> Result<GrpcTransport> {
    let mut root_store = rustls::RootCertStore::empty();
    root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

    let mut config = rustls::ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();
    config.alpn_protocols = vec![b"h2".to_vec()];

    let connector = tokio_rustls::TlsConnector::from(std::sync::Arc::new(config));
    let domain = rustls::pki_types::ServerName::try_from(sni.to_string())
        .map_err(|e| Error::Parse(format!("Invalid SNI: {}", e)))?;

    let tls = connector.connect(domain, tcp).await
        .map_err(|e| Error::Custom(format!("gRPC TLS failed: {}", e)))?;

    tracing::debug!("gRPC TLS connected to {}", sni);

    let (mut send_req, connection) = h2::client::handshake(tls).await
        .map_err(|e| Error::Custom(format!("gRPC h2 handshake failed: {}", e)))?;

    tokio::spawn(async move {
        if let Err(e) = connection.await {
            tracing::debug!("gRPC h2 connection error: {}", e);
        }
    });

    let uri = format!("https://{}/{}", sni, path.trim_start_matches('/'));
    let request = http::Request::builder()
        .method("POST")
        .uri(&uri)
        .header("content-type", "application/grpc")
        .header("te", "trailers")
        .header("user-agent", "grpc-go/1.62.0")
        .body(())
        .map_err(|e| Error::Custom(format!("gRPC request build failed: {}", e)))?;

    let (response_future, send_stream) = send_req.send_request(request, false)
        .map_err(|e| Error::Custom(format!("gRPC send_request failed: {}", e)))?;

    tracing::debug!("gRPC request sent on {}", path);

    Ok(GrpcTransport {
        tx: send_stream,
        rx_buf: BytesMut::new(),
        rx: None,
        response_future: Some(response_future),
    })
}

pub async fn connect_ws(
    tcp: TcpStream,
    sni: &str,
    path: &str,
    host: &str,
) -> Result<WsTransport> {
    let mut root_store = rustls::RootCertStore::empty();
    root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

    let mut config = rustls::ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();
    // WebSocket runs over HTTP/1.1; do not negotiate h2.
    config.alpn_protocols = vec![b"http/1.1".to_vec()];

    let connector = tokio_rustls::TlsConnector::from(std::sync::Arc::new(config));
    let domain = rustls::pki_types::ServerName::try_from(sni.to_string())
        .map_err(|e| Error::Parse(format!("Invalid SNI: {}", e)))?;

    let tls = connector.connect(domain, tcp).await
        .map_err(|e| Error::Custom(format!("WebSocket TLS failed: {}", e)))?;

    tracing::debug!("WebSocket TLS connected to SNI {}", sni);

    let path = path.trim_start_matches('/');
    let uri = format!("https://{}/{}", host, path);

    let request = http::Request::builder()
        .method("GET")
        .uri(&uri)
        .header("Host", host)
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
        .body(())
        .map_err(|e| Error::Custom(format!("WebSocket request build failed: {}", e)))?;

    let (ws_stream, response) = tokio_tungstenite::client_async(request, tls).await
        .map_err(|e| Error::Custom(format!("WebSocket handshake failed: {}", e)))?;

    tracing::debug!("WebSocket handshake completed with status {}", response.status());

    Ok(WsTransport::new(ws_stream))
}
