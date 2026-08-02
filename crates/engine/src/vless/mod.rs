pub mod header;
pub mod reality;
pub mod tls;

use ironpass_core::{Error, Result, models::ProxyNode, models::Transport};
use tokio::io::{AsyncRead, AsyncWrite};
use bytes::{Buf, BufMut, BytesMut};
use sha2::{Sha256, Digest};
use rand::RngCore;

use crate::transport;

const VLESS_VERSION: u8 = 0;

#[derive(Debug, Clone)]
pub struct VlessClient {
    node: ProxyNode,
    uuid: Vec<u8>,
}

pub enum VlessStream {
    Tls(tokio_rustls::client::TlsStream<tokio::net::TcpStream>),
    Grpc(transport::GrpcTransport),
}

impl AsyncRead for VlessStream {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match &mut *self {
            VlessStream::Tls(s) => std::pin::Pin::new(s).poll_read(cx, buf),
            VlessStream::Grpc(s) => std::pin::Pin::new(s).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for VlessStream {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        match &mut *self {
            VlessStream::Tls(s) => std::pin::Pin::new(s).poll_write(cx, buf),
            VlessStream::Grpc(s) => std::pin::Pin::new(s).poll_write(cx, buf),
        }
    }

    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match &mut *self {
            VlessStream::Tls(s) => std::pin::Pin::new(s).poll_flush(cx),
            VlessStream::Grpc(s) => std::pin::Pin::new(s).poll_flush(cx),
        }
    }

    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match &mut *self {
            VlessStream::Tls(s) => std::pin::Pin::new(s).poll_shutdown(cx),
            VlessStream::Grpc(s) => std::pin::Pin::new(s).poll_shutdown(cx),
        }
    }
}

impl VlessClient {
    pub fn new(node: ProxyNode) -> Result<Self> {
        let uuid_str = node.uuid.as_ref()
            .ok_or_else(|| Error::Parse("VLESS requires UUID".into()))?;

        let uuid = parse_uuid(uuid_str)?;

        Ok(Self { node, uuid })
    }

    pub async fn connect(&self) -> Result<VlessStream> {
        let addr = format!("{}:{}", self.node.server, self.node.port);
        tracing::debug!("Connecting to {}", addr);

        let tcp = tokio::net::TcpStream::connect(&addr).await?;

        match self.node.transport {
            Transport::Grpc => {
                let service_name = self.node.service_name.as_deref()
                    .unwrap_or("VLESS");
                let path = format!("/{}/Tunnel", service_name);
                let sni = self.node.sni.as_deref()
                    .unwrap_or(&self.node.server)
                    .to_string();
                let grpc_stream = transport::connect_grpc(tcp, &sni, &path).await?;
                Ok(VlessStream::Grpc(grpc_stream))
            }
            _ => {
                match self.node.security {
                    ironpass_core::models::Security::Reality => {
                        let stream = self.connect_reality(tcp).await?;
                        Ok(VlessStream::Tls(stream))
                    }
                    ironpass_core::models::Security::Tls => {
                        let stream = self.connect_tls(tcp).await?;
                        Ok(VlessStream::Tls(stream))
                    }
                    ironpass_core::models::Security::None => {
                        Err(Error::UnsupportedProtocol("Plaintext VLESS not supported".into()))
                    }
                    _ => Err(Error::UnsupportedProtocol(format!("{:?}", self.node.security))),
                }
            }
        }
    }

    async fn connect_tls(&self, tcp: tokio::net::TcpStream) -> Result<tokio_rustls::client::TlsStream<tokio::net::TcpStream>> {
        tls::connect_tls(tcp, &self.node).await
    }

    async fn connect_reality(&self, tcp: tokio::net::TcpStream) -> Result<tokio_rustls::client::TlsStream<tokio::net::TcpStream>> {
        reality::connect_reality(tcp, &self.node).await
    }

    pub fn encode_connect_request(&self, target_host: &str, target_port: u16) -> BytesMut {
        header::encode_vless_request(
            &self.uuid,
            target_host,
            target_port,
            &self.node,
        )
    }
}

fn parse_uuid(uuid_str: &str) -> Result<Vec<u8>> {
    let parts: Vec<&str> = uuid_str.split('-').collect();
    if parts.len() != 5 {
        return Err(Error::Parse(format!("Invalid UUID format: {}", uuid_str)));
    }

    let mut bytes = Vec::with_capacity(16);
    for part in &parts {
        let b = hex::decode(part)
            .map_err(|e| Error::Parse(format!("Invalid UUID hex: {}", e)))?;
        bytes.extend_from_slice(&b);
    }

    Ok(bytes)
}

pub fn generate_random_bytes(len: usize) -> Vec<u8> {
    let mut buf = vec![0u8; len];
    rand::thread_rng().fill_bytes(&mut buf);
    buf
}
