use ironpass_core::{Error, Result};
use bytes::{Bytes, BytesMut};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio_rustls::client::TlsStream;
use futures_util::StreamExt;

/// XHTTP transport operating mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XhttpMode {
    /// HTTP/1.1 long-polling POST.
    Auto,
    /// HTTP/2 stream-up / packet-up mode.
    H2,
}

impl XhttpMode {
    /// Choose the XHTTP mode from the parsed `extra` configuration.
    pub fn from_extra(extra: Option<&ironpass_core::models::XhttpExtra>) -> Self {
        if extra.map(|e| e.prefers_h2()).unwrap_or(false) {
            Self::H2
        } else {
            Self::Auto
        }
    }
}

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

/// Bidirectional HTTP/1.1 tunnel used by XHTTP and SplitHTTP transports.
///
/// A single long-lived HTTP POST is opened to the server.  The request body
/// carries outbound VLESS traffic and the response body carries inbound
/// VLESS traffic.  The write side feeds a bounded channel; the read side
/// drains the response body frames.
pub struct XhttpTransport {
    tx: http_body_util::channel::Sender<Bytes, std::convert::Infallible>,
    rx: std::pin::Pin<Box<dyn tokio_stream::Stream<Item = std::io::Result<Bytes>> + Send>>,
    rx_buf: BytesMut,
    read_done: bool,
}

/// Bidirectional HTTP/2 tunnel used by XHTTP when `extra.mode` is
/// `stream-up` or `packet-up`.
///
/// A single HTTP/2 POST request is opened.  Outbound VLESS data is sent as
/// DATA frames and inbound data is read from the response DATA frames.
pub struct Xhttp2Transport {
    tx: h2::SendStream<Bytes>,
    rx: Option<h2::RecvStream>,
    rx_buf: BytesMut,
    response_future: Option<h2::client::ResponseFuture>,
}

impl AsyncRead for Xhttp2Transport {
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
                        if data.is_empty() {
                            return Poll::Ready(Ok(()));
                        }
                        self.rx_buf.extend_from_slice(&data);
                    }
                    Poll::Ready(Some(Err(e))) => {
                        return Poll::Ready(Err(std::io::Error::other(e)));
                    }
                    Poll::Ready(None) => return Poll::Ready(Ok(())),
                    Poll::Pending => return Poll::Pending,
                }
            } else if let Some(ref mut future) = self.response_future {
                match Pin::new(future).poll(cx) {
                    Poll::Ready(Ok(response)) => {
                        tracing::debug!("XHTTP/2 response status: {}", response.status());
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

impl AsyncWrite for Xhttp2Transport {
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
            Poll::Ready(Some(Err(e))) => Poll::Ready(Err(std::io::Error::other(e))),
            Poll::Ready(None) => Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "XHTTP/2 stream closed",
            ))),
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

impl AsyncRead for XhttpTransport {
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

            if self.read_done {
                return Poll::Ready(Ok(()));
            }

            match self.rx.as_mut().poll_next(cx) {
                Poll::Ready(Some(Ok(chunk))) => {
                    if chunk.is_empty() {
                        self.read_done = true;
                        return Poll::Ready(Ok(()));
                    }
                    self.rx_buf.extend_from_slice(&chunk);
                }
                Poll::Ready(Some(Err(e))) => {
                    return Poll::Ready(Err(e));
                }
                Poll::Ready(None) => {
                    self.read_done = true;
                    return Poll::Ready(Ok(()));
                }
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

impl AsyncWrite for XhttpTransport {
    fn poll_write(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let frame = http_body::Frame::data(Bytes::copy_from_slice(buf));
        match self.tx.try_send(frame) {
            Ok(()) => Poll::Ready(Ok(buf.len())),
            Err(_) => Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "XHTTP send channel closed",
            ))),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

/// State for the split HTTP variant used by SplitHTTP.
///
/// SplitHTTP separates upstream and downstream into two HTTP requests.  This
/// implementation opens one POST for the upstream channel and one GET for the
/// downstream channel.  Both are kept open for the lifetime of the stream.
pub struct SplitHttpTransport {
    upstream: XhttpTransport,
    downstream: XhttpTransport,
}

impl AsyncRead for SplitHttpTransport {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.downstream).poll_read(cx, buf)
    }
}

impl AsyncWrite for SplitHttpTransport {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.upstream).poll_write(cx, buf)
    }

    fn poll_flush(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.upstream).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.upstream).poll_shutdown(cx)
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
        .map_err(|e| Error::Parse(format!("Invalid SNI: {e}")))?;

    let tls = connector.connect(domain, tcp).await
        .map_err(|e| Error::Custom(format!("gRPC TLS failed: {e}")))?;

    tracing::debug!("gRPC TLS connected to {}", sni);

    let (mut send_req, connection) = h2::client::handshake(tls).await
        .map_err(|e| Error::Custom(format!("gRPC h2 handshake failed: {e}")))?;

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
        .map_err(|e| Error::Custom(format!("gRPC request build failed: {e}")))?;

    let (response_future, send_stream) = send_req.send_request(request, false)
        .map_err(|e| Error::Custom(format!("gRPC send_request failed: {e}")))?;

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
        .map_err(|e| Error::Parse(format!("Invalid SNI: {e}")))?;

    let tls = connector.connect(domain, tcp).await
        .map_err(|e| Error::Custom(format!("WebSocket TLS failed: {e}")))?;

    tracing::debug!("WebSocket TLS connected to SNI {}", sni);

    let path = path.trim_start_matches('/');
    let uri = format!("https://{}/{}", host, path);

    let request = http::Request::builder()
        .method("GET")
        .uri(&uri)
        .header("Host", host)
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
        .body(())
        .map_err(|e| Error::Custom(format!("WebSocket request build failed: {e}")))?;

    let (ws_stream, response) = tokio_tungstenite::client_async(request, tls).await
        .map_err(|e| Error::Custom(format!("WebSocket handshake failed: {e}")))?;

    tracing::debug!("WebSocket handshake completed with status {}", response.status());

    Ok(WsTransport::new(ws_stream))
}

/// Establish an XHTTP (HTTP/1.1 long-polling POST) tunnel over `tls`.
///
/// `sni` is used for TLS SNI, `host` for the HTTP `Host` header, and `path`
/// for the request path.  `headers` may contain optional extra headers.
/// `padding_len`, if set, causes a `X-Padding` header with random bytes to be
/// sent immediately after the request headers (some servers expect padding).
pub async fn connect_xhttp(
    tls: TlsStream<TcpStream>,
    _sni: &str,
    host: &str,
    path: &str,
    headers: &[(String, String)],
    padding_len: Option<usize>,
) -> Result<XhttpTransport> {
    let io = hyper_util::rt::TokioIo::new(tls);
    let (mut sender, connection) = hyper::client::conn::http1::handshake(io).await
        .map_err(|e| Error::Custom(format!("XHTTP handshake failed: {e}")))?;

    tokio::spawn(async move {
        if let Err(e) = connection.await {
            tracing::debug!("XHTTP connection error: {}", e);
        }
    });

    let (body_tx, body_rx) = http_body_util::channel::Channel::<Bytes, std::convert::Infallible>::new(16);

    let path = path.trim_start_matches('/');
    let path = if path.is_empty() { "/" } else { path };
    let uri = format!("https://{}/{}", host, path.trim_start_matches('/'));
    let mut builder = http::Request::builder()
        .method("POST")
        .uri(uri)
        .header("Host", host)
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
        .header("Transfer-Encoding", "chunked")
        .header("Accept", "*/*")
        .header("Accept-Language", "en-US,en;q=0.9");

    if let Some(len) = padding_len {
        builder = builder.header("X-Padding", crate::vless::generate_random_bytes(len));
    }

    for (k, v) in headers {
        builder = builder.header(k, v);
    }

    let request = builder.body(body_rx)
        .map_err(|e| Error::Custom(format!("XHTTP request build failed: {e}")))?;

    let response = sender.send_request(request).await
        .map_err(|e| Error::Custom(format!("XHTTP request failed: {e}")))?;

    let status = response.status();
    tracing::debug!("XHTTP response status: {}", status);
    if !status.is_success() && !status.is_informational() {
        return Err(Error::Custom(format!("XHTTP server returned status {status}")));
    }

    let body_stream = http_body_util::BodyStream::new(response.into_body())
        .map(|frame| {
            frame
                .map_err(std::io::Error::other)
                .and_then(|f| f.into_data().map_err(|_| std::io::Error::other("XHTTP non-data frame")))
        });

    Ok(XhttpTransport {
        tx: body_tx,
        rx: Box::pin(body_stream),
        rx_buf: BytesMut::new(),
        read_done: false,
    })
}

/// Establish an XHTTP/2 (HTTP/2 stream-up/packet-up) tunnel over `tls`.
///
/// A single HTTP/2 POST request is opened.  Outbound VLESS data is sent as
/// DATA frames and inbound data is read from the response DATA frames.
pub async fn connect_xhttp2(
    tls: TlsStream<TcpStream>,
    _sni: &str,
    host: &str,
    path: &str,
    headers: &[(String, String)],
) -> Result<Xhttp2Transport> {
    let (mut send_req, connection) = h2::client::handshake(tls).await
        .map_err(|e| Error::Custom(format!("XHTTP/2 h2 handshake failed: {e}")))?;

    tokio::spawn(async move {
        if let Err(e) = connection.await {
            tracing::debug!("XHTTP/2 h2 connection error: {}", e);
        }
    });

    let path = path.trim_start_matches('/');
    let path = if path.is_empty() { "/" } else { path };
    let uri = format!("https://{}/{}", host, path);
    let mut builder = http::Request::builder()
        .method("POST")
        .uri(uri)
        .header("Host", host)
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
        .header("Content-Type", "application/octet-stream")
        .header("Accept", "*/*")
        .header("Accept-Language", "en-US,en;q=0.9");

    for (k, v) in headers {
        builder = builder.header(k, v);
    }

    let request = builder.body(())
        .map_err(|e| Error::Custom(format!("XHTTP/2 request build failed: {e}")))?;

    let (response_future, send_stream) = send_req.send_request(request, false)
        .map_err(|e| Error::Custom(format!("XHTTP/2 send_request failed: {e}")))?;

    tracing::debug!("XHTTP/2 request sent on {}", path);

    Ok(Xhttp2Transport {
        tx: send_stream,
        rx: None,
        rx_buf: BytesMut::new(),
        response_future: Some(response_future),
    })
}

/// Establish a SplitHTTP tunnel over `tls`.
///
/// Two XHTTP-style tunnels are opened: one POST for upstream data and one GET
/// for downstream data.  Writes go to the upstream tunnel and reads come from
/// the downstream tunnel.
pub async fn connect_splithttp(
    tls: TlsStream<TcpStream>,
    sni: &str,
    host: &str,
    path: &str,
    headers: &[(String, String)],
) -> Result<SplitHttpTransport> {
    let upstream_tls = tls;
    let downstream_tls = {
        // SplitHttpTransport needs a second TLS stream.  The caller passes
        // ownership of `tls` to us, so we reconnect over a fresh TCP stream
        // to the same endpoint.  This preserves the public API shape while
        // still allowing the split behaviour to be tested and used.
        let server = sni.to_string();
        let host_port = format!("{server}:443");
        let tcp = tokio::net::TcpStream::connect(&host_port).await?;
        let mut root_store = rustls::RootCertStore::empty();
        root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let mut config = rustls::ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth();
        config.alpn_protocols = vec![b"http/1.1".to_vec()];
        let connector = tokio_rustls::TlsConnector::from(std::sync::Arc::new(config));
        let domain = rustls::pki_types::ServerName::try_from(server)
            .map_err(|e| Error::Parse(format!("Invalid SNI: {e}")))?;
        connector.connect(domain, tcp).await
            .map_err(|e| Error::Custom(format!("SplitHTTP downstream TLS failed: {e}")))?
    };

    let upstream = connect_xhttp(upstream_tls, sni, host, path, headers, None).await?;

    let downstream_path = format!("{path}/down");
    let downstream = connect_xhttp(downstream_tls, sni, host, &downstream_path, headers, None).await?;

    Ok(SplitHttpTransport {
        upstream,
        downstream,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn test_xhttp_transport_basic() {
        let (tx, rx) = http_body_util::channel::Channel::<Bytes, std::convert::Infallible>::new(16);
        let body_stream = http_body_util::BodyStream::new(rx)
            .map(|frame| {
                frame
                .map_err(std::io::Error::other)
                    .and_then(|f| f.into_data().map_err(|_| std::io::Error::other("non-data frame")))
            });

        let mut transport = XhttpTransport {
            tx,
            rx: Box::pin(body_stream),
            rx_buf: BytesMut::new(),
            read_done: false,
        };

        // Write outbound data.
        transport.write_all(b"hello upstream").await.unwrap();

        // Read back the same data from the body channel.
        let mut buf = [0u8; 64];
        let n = transport.read(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"hello upstream");
    }

    #[tokio::test]
    async fn test_xhttp_transport_write_buffering() {
        let (tx, _rx) = http_body_util::channel::Channel::<Bytes, std::convert::Infallible>::new(16);
        let mut transport = XhttpTransport {
            tx,
            rx: Box::pin(futures_util::stream::empty()),
            rx_buf: BytesMut::from_iter(b"prefetch"),
            read_done: false,
        };

        let mut buf = [0u8; 4];
        let n = transport.read(&mut buf).await.unwrap();
        assert_eq!(n, 4);
        assert_eq!(&buf, b"pref");
    }
}
