use ironpass_core::{Error, Result, models::ProxyNode, models::Protocol};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf, AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use bytes::{BufMut, BytesMut};
use std::pin::Pin;
use std::task::{Context, Poll};

const SOCKS5_VERSION: u8 = 0x05;
const AUTH_NONE: u8 = 0x00;
const CMD_CONNECT: u8 = 0x01;
const ATYP_IPV4: u8 = 0x01;
const ATYP_DOMAIN: u8 = 0x03;
const ATYP_IPV6: u8 = 0x04;

#[allow(clippy::large_enum_variant)]
enum RemoteStream {
    Vless(crate::vless::VlessStream),
    Trojan(crate::trojan::TrojanStream),
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

pub async fn run_socks_server(
    node: ProxyNode,
    port: u16,
    mut shutdown: tokio::sync::broadcast::Receiver<()>,
) -> Result<()> {
    let listener = TcpListener::bind(format!("127.0.0.1:{}", port)).await?;
    tracing::info!("SOCKS5 listening on 127.0.0.1:{}", port);

    loop {
        tokio::select! {
            accept = listener.accept() => {
                match accept {
                    Ok((stream, addr)) => {
                        tracing::debug!("SOCKS5 connection from {}", addr);
                        let node = node.clone();
                        tokio::spawn(async move {
                            if let Err(e) = handle_socks_client(stream, node).await {
                                tracing::debug!("SOCKS5 client error: {}", e);
                            }
                        });
                    }
                    Err(e) => {
                        tracing::error!("SOCKS5 accept error: {}", e);
                    }
                }
            }
            _ = shutdown.recv() => {
                tracing::info!("SOCKS5 shutting down");
                break;
            }
        }
    }

    Ok(())
}

async fn handle_socks_client(mut client: TcpStream, node: ProxyNode) -> Result<()> {
    let mut buf = [0u8; 256];

    let n = client.read(&mut buf).await?;
    if n < 3 || buf[0] != SOCKS5_VERSION {
        return Err(Error::Parse("Invalid SOCKS5 auth negotiation".into()));
    }

    client.write_all(&[SOCKS5_VERSION, AUTH_NONE]).await?;

    let n = client.read(&mut buf).await?;
    if n < 4 || buf[0] != SOCKS5_VERSION || buf[1] != CMD_CONNECT {
        return Err(Error::Parse("Invalid SOCKS5 connect request".into()));
    }

    let (target_host, target_port, _) = match buf[3] {
        ATYP_IPV4 => {
            if n < 10 { return Err(Error::Parse("Too short for IPv4".into())); }
            let ip = std::net::Ipv4Addr::new(buf[4], buf[5], buf[6], buf[7]);
            let port = u16::from_be_bytes([buf[8], buf[9]]);
            (ip.to_string(), port, 10)
        }
        ATYP_IPV6 => {
            if n < 22 { return Err(Error::Parse("Too short for IPv6".into())); }
            let mut ip_buf = [0u8; 16];
            ip_buf.copy_from_slice(&buf[4..20]);
            let ip = std::net::Ipv6Addr::from(ip_buf);
            let port = u16::from_be_bytes([buf[20], buf[21]]);
            (ip.to_string(), port, 22)
        }
        ATYP_DOMAIN => {
            let len = buf[4] as usize;
            if n < 5 + len + 2 { return Err(Error::Parse("Too short for domain".into())); }
            let domain = String::from_utf8_lossy(&buf[5..5+len]).to_string();
            let port = u16::from_be_bytes([buf[5+len], buf[5+len+1]]);
            (domain, port, 5 + len + 2)
        }
        _ => return Err(Error::Parse(format!("Unknown address type: {}", buf[3]))),
    };

    tracing::debug!("SOCKS5 CONNECT → {}:{}", target_host, target_port);

    let connect_result = match node.protocol {
        Protocol::Trojan => connect_through_trojan(&node, &target_host, target_port).await,
        Protocol::Vless => connect_through_vless(&node, &target_host, target_port).await,
        _ => Err(Error::UnsupportedProtocol(format!("{:?}", node.protocol))),
    };

    match connect_result {
        Ok(remote) => {
            let mut resp = BytesMut::with_capacity(10);
            resp.put_u8(SOCKS5_VERSION);
            resp.put_u8(0x00);
            resp.put_u8(0x00);
            resp.put_u8(ATYP_IPV4);
            resp.put_slice(&[0, 0, 0, 0]);
            resp.put_u16(0);
            client.write_all(&resp).await?;

            let (client_read, mut client_write) = client.into_split();
            let mut client_read = tokio::io::BufReader::new(client_read);

            let (mut remote_read, mut remote_write) = tokio::io::split(remote);

            let client_to_remote = tokio::io::copy(&mut client_read, &mut remote_write);
            let remote_to_client = tokio::io::copy(&mut remote_read, &mut client_write);

            tokio::select! {
                result = client_to_remote => {
                    if let Err(e) = result { tracing::debug!("client→remote error: {}", e); }
                }
                result = remote_to_client => {
                    if let Err(e) = result { tracing::debug!("remote→client error: {}", e); }
                }
            }
        }
        Err(e) => {
            tracing::debug!("SOCKS5 connect failed: {}", e);
            let mut resp = BytesMut::with_capacity(10);
            resp.put_u8(SOCKS5_VERSION);
            resp.put_u8(0x05);
            resp.put_u8(0x00);
            resp.put_u8(ATYP_IPV4);
            resp.put_slice(&[0, 0, 0, 0]);
            resp.put_u16(0);
            client.write_all(&resp).await?;
        }
    }

    Ok(())
}

async fn connect_through_trojan(
    node: &ProxyNode,
    target_host: &str,
    target_port: u16,
) -> Result<RemoteStream> {
    let client = super::trojan::TrojanClient::new(node.clone())?;
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
    let client = super::vless::VlessClient::new(node.clone())?;
    let mut stream = client.connect().await?;

    let connect_req = client.encode_connect_request(target_host, target_port);
    stream.write_all(&connect_req).await?;

    tracing::debug!("VLESS connect request sent for {}:{}", target_host, target_port);

    let mut response = [0u8; 1];
    stream.read_exact(&mut response).await?;
    if response[0] != 0x00 {
        return Err(Error::Custom(format!(
            "VLESS server rejected connection: status={}",
            response[0]
        )));
    }
    tracing::debug!("VLESS connection established to {}:{}", target_host, target_port);

    Ok(RemoteStream::Vless(stream))
}
