use ironpass_core::{Error, Result, models::ProxyNode, models::Transport};
use sha2::{Sha224, Digest};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf, AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use std::pin::Pin;
use std::task::{Context, Poll};

use crate::transport;

const CMD_TCP: u8 = 0x01;
const ATYP_IPV4: u8 = 0x01;
const ATYP_DOMAIN: u8 = 0x03;
const ATYP_IPV6: u8 = 0x04;

pub enum TrojanStream {
    Grpc(transport::GrpcTransport),
}

impl AsyncRead for TrojanStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        match &mut *self {
            TrojanStream::Grpc(s) => Pin::new(s).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for TrojanStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        match &mut *self {
            TrojanStream::Grpc(s) => Pin::new(s).poll_write(cx, buf),
        }
    }

    fn poll_flush(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<std::io::Result<()>> {
        match &mut *self {
            TrojanStream::Grpc(s) => Pin::new(s).poll_flush(cx),
        }
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<std::io::Result<()>> {
        match &mut *self {
            TrojanStream::Grpc(s) => Pin::new(s).poll_shutdown(cx),
        }
    }
}

pub struct TrojanClient {
    node: ProxyNode,
    password_hash: String,
}

impl TrojanClient {
    pub fn new(node: ProxyNode) -> Result<Self> {
        let password = node.password.as_ref()
            .ok_or_else(|| Error::Parse("Trojan requires password".into()))?;

        let mut hasher = Sha224::new();
        hasher.update(password.as_bytes());
        let hash = hasher.finalize();
        let password_hash = hex::encode(hash);

        Ok(Self { node, password_hash })
    }

    pub async fn connect(&self) -> Result<TrojanStream> {
        let addr = format!("{}:{}", self.node.server, self.node.port);
        tracing::debug!("Connecting to {}", addr);

        let tcp = TcpStream::connect(&addr).await?;

        let sni = self.node.sni.as_deref()
            .unwrap_or(&self.node.server)
            .to_string();

        match self.node.transport {
            Transport::Grpc => {
                let service_name = self.node.service_name.as_deref()
                    .unwrap_or("TrojanGRPC");
                let path = format!("/{}/Tunnel", service_name);
                let grpc_stream = transport::connect_grpc(tcp, &sni, &path).await?;
                Ok(TrojanStream::Grpc(grpc_stream))
            }
            _ => {
                Err(Error::UnsupportedProtocol(format!(
                    "Trojan transport {:?} not yet supported",
                    self.node.transport
                )))
            }
        }
    }

    pub fn encode_connect_request(&self, target_host: &str, target_port: u16) -> Vec<u8> {
        let mut buf = Vec::with_capacity(256);

        buf.extend_from_slice(self.password_hash.as_bytes());
        buf.extend_from_slice(b"\r\n");

        buf.push(CMD_TCP);

        if let Ok(ip) = target_host.parse::<std::net::Ipv4Addr>() {
            buf.push(ATYP_IPV4);
            buf.extend_from_slice(&ip.octets());
        } else if let Ok(ip) = target_host.parse::<std::net::Ipv6Addr>() {
            buf.push(ATYP_IPV6);
            buf.extend_from_slice(&ip.octets());
        } else {
            buf.push(ATYP_DOMAIN);
            buf.push(target_host.len() as u8);
            buf.extend_from_slice(target_host.as_bytes());
        }

        buf.extend_from_slice(&target_port.to_be_bytes());
        buf.extend_from_slice(b"\r\n");

        buf
    }

    pub fn encode_connect_request_grpc(&self, target_host: &str, target_port: u16) -> Vec<u8> {
        let payload = self.encode_connect_request(target_host, target_port);
        // gRPC length-prefix: compression flag (0) + 4-byte big-endian length
        let mut frame = Vec::with_capacity(5 + payload.len());
        frame.push(0); // no compression
        frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        frame.extend_from_slice(&payload);
        frame
    }
}
