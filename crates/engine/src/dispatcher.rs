use ironpass_core::{Error, Result, models::{Protocol, ProxyNode}};
use tokio::io::{AsyncRead, AsyncWrite, AsyncReadExt, AsyncWriteExt, ReadBuf};
use std::pin::Pin;
use std::task::{Context, Poll};

use crate::vless;
use crate::trojan;

#[allow(clippy::large_enum_variant)]
pub enum RemoteStream {
    Vless(vless::VlessStream),
    Trojan(trojan::TrojanStream),
}

impl AsyncRead for RemoteStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        match &mut *self {
            RemoteStream::Vless(s) => Pin::new(s).poll_read(cx, buf),
            RemoteStream::Trojan(s) => Pin::new(s).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for RemoteStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        match &mut *self {
            RemoteStream::Vless(s) => Pin::new(s).poll_write(cx, buf),
            RemoteStream::Trojan(s) => Pin::new(s).poll_write(cx, buf),
        }
    }

    fn poll_flush(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<std::io::Result<()>> {
        match &mut *self {
            RemoteStream::Vless(s) => Pin::new(s).poll_flush(cx),
            RemoteStream::Trojan(s) => Pin::new(s).poll_flush(cx),
        }
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<std::io::Result<()>> {
        match &mut *self {
            RemoteStream::Vless(s) => Pin::new(s).poll_shutdown(cx),
            RemoteStream::Trojan(s) => Pin::new(s).poll_shutdown(cx),
        }
    }
}

pub async fn connect_remote(node: &ProxyNode, host: &str, port: u16) -> Result<RemoteStream> {
    match node.protocol {
        Protocol::Trojan => connect_through_trojan(node, host, port).await,
        Protocol::Vless => connect_through_vless(node, host, port).await,
        _ => Err(Error::UnsupportedProtocol(format!("{:?}", node.protocol))),
    }
}

async fn connect_through_trojan(
    node: &ProxyNode,
    target_host: &str,
    target_port: u16,
) -> Result<RemoteStream> {
    let client = trojan::TrojanClient::new(node.clone())?;
    let mut stream = client.connect().await?;

    // For gRPC transport, send gRPC-length-prefixed Trojan connect request
    let connect_req = match node.transport {
        ironpass_core::models::Transport::Grpc => client.encode_connect_request_grpc(target_host, target_port),
        _ => client.encode_connect_request(target_host, target_port),
    };
    stream.write_all(&connect_req).await?;

    tracing::debug!("Trojan connect request sent for {}:{}", target_host, target_port);

    let mut response = [0u8; 1];
    stream.read_exact(&mut response).await?;
    if response[0] != 0x00 {
        return Err(Error::Custom(format!(
            "Trojan server rejected connection: status={}",
            response[0]
        )));
    }
    tracing::debug!("Trojan connection established to {}:{}", target_host, target_port);

    Ok(RemoteStream::Trojan(stream))
}

async fn connect_through_vless(
    node: &ProxyNode,
    target_host: &str,
    target_port: u16,
) -> Result<RemoteStream> {
    let client = vless::VlessClient::new(node.clone())?;
    let mut stream = client.connect().await?;

    let connect_req = client.encode_connect_request(target_host, target_port);
    stream.write_all(&connect_req).await?;

    tracing::debug!("VLESS connect request sent for {}:{}", target_host, target_port);

    let mut response = [0u8; 2];
    match stream.read_exact(&mut response).await {
        Ok(_) => {
            // VLESS response: version (1) + result (1). Result 0 means success.
            if response[1] != 0x00 {
                return Err(Error::Custom(format!(
                    "VLESS server rejected connection: version={} status={}",
                    response[0], response[1]
                )));
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
            // Some implementations close the stream immediately on rejection or only
            // send a single-byte result. Fall back to the legacy single-byte check.
            if response[0] != 0x00 {
                return Err(Error::Custom(format!(
                    "VLESS server rejected connection: status={}",
                    response[0]
                )));
            }
        }
        Err(e) => return Err(e.into()),
    }
    tracing::debug!("VLESS connection established to {}:{}", target_host, target_port);

    Ok(RemoteStream::Vless(stream))
}
