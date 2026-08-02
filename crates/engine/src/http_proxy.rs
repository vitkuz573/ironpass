use ironpass_core::{Error, Result, models::ProxyNode};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

use crate::dispatcher;

pub async fn run_http_server(
    node: ProxyNode,
    port: u16,
    mut shutdown: tokio::sync::broadcast::Receiver<()>,
) -> Result<()> {
    let listener = TcpListener::bind(format!("127.0.0.1:{}", port)).await?;
    tracing::info!("HTTP proxy listening on 127.0.0.1:{}", port);

    loop {
        tokio::select! {
            accept = listener.accept() => {
                match accept {
                    Ok((stream, addr)) => {
                        tracing::debug!("HTTP proxy connection from {}", addr);
                        let node = node.clone();
                        tokio::spawn(async move {
                            if let Err(e) = handle_http_client(stream, node).await {
                                tracing::debug!("HTTP proxy client error: {}", e);
                            }
                        });
                    }
                    Err(e) => {
                        tracing::error!("HTTP proxy accept error: {}", e);
                    }
                }
            }
            _ = shutdown.recv() => {
                tracing::info!("HTTP proxy shutting down");
                break;
            }
        }
    }

    Ok(())
}

async fn handle_http_client(stream: TcpStream, node: ProxyNode) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);

    let mut request_line = String::new();
    let n = reader.read_line(&mut request_line).await?;
    if n == 0 {
        return Ok(());
    }

    // Drain remaining request headers; CONNECT requests rarely have a body,
    // but we must at least read up to the end of the headers.
    loop {
        let mut line = String::new();
        let n = reader.read_line(&mut line).await?;
        if n == 0 || line == "\r\n" || line == "\n" {
            break;
        }
    }

    let parts: Vec<&str> = request_line.split_whitespace().collect();
    if parts.len() < 3 {
        let _ = writer.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n").await;
        return Err(Error::Parse("Invalid HTTP request line".into()));
    }

    if parts[0].to_uppercase() != "CONNECT" {
        let _ = writer.write_all(b"HTTP/1.1 405 Method Not Allowed\r\n\r\n").await;
        return Err(Error::UnsupportedProtocol(format!(
            "HTTP proxy only supports CONNECT, got {}",
            parts[0]
        )));
    }

    let authority = parts[1];
    let (target_host, target_port) = parse_authority(authority)?;

    tracing::debug!("HTTP proxy CONNECT -> {}:{}", target_host, target_port);

    match dispatcher::connect_remote(&node, &target_host, target_port).await {
        Ok(remote) => {
            writer.write_all(b"HTTP/1.1 200 Connection established\r\n\r\n").await?;

            // Reassemble the full stream from its split halves.
            let client = writer.reunite(reader.into_inner())
                .map_err(|_| Error::Custom("Failed to reunite HTTP proxy stream".into()))?;

            crate::socks5::relay(client, remote).await;
        }
        Err(e) => {
            tracing::debug!("HTTP proxy connect failed: {}", e);
            let msg = "HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\n\r\n";
            let _ = writer.write_all(msg.as_bytes()).await;
            return Err(e);
        }
    }

    Ok(())
}

pub(crate) fn parse_authority(authority: &str) -> Result<(String, u16)> {
    // IPv6 literals are bracketed, e.g. [::1]:80
    if let Some(end) = authority.rfind(':') {
        if authority.starts_with('[') {
            let bracket_end = authority.rfind(']')
                .ok_or_else(|| Error::Parse(format!("Invalid IPv6 authority: {}", authority)))?;
            if end <= bracket_end {
                // No port present; use default 80.
                return Ok((authority.to_string(), 80));
            }
            let host = authority[..=bracket_end].to_string();
            let port: u16 = authority[end + 1..]
                .parse()
                .map_err(|_| Error::Parse(format!("Invalid port in authority: {}", authority)))?;
            return Ok((host, port));
        }

        let host = authority[..end].to_string();
        let port: u16 = authority[end + 1..]
            .parse()
            .map_err(|_| Error::Parse(format!("Invalid port in authority: {}", authority)))?;
        return Ok((host, port));
    }

    Ok((authority.to_string(), 80))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_authority_ipv4_with_port() {
        assert_eq!(parse_authority("127.0.0.1:8080").unwrap(), ("127.0.0.1".to_string(), 8080));
    }

    #[test]
    fn parse_authority_domain_with_port() {
        assert_eq!(parse_authority("example.com:443").unwrap(), ("example.com".to_string(), 443));
    }

    #[test]
    fn parse_authority_no_port_defaults_to_80() {
        assert_eq!(parse_authority("example.com").unwrap(), ("example.com".to_string(), 80));
    }

    #[test]
    fn parse_authority_ipv6_with_port() {
        assert_eq!(parse_authority("[::1]:1080").unwrap(), ("[::1]".to_string(), 1080));
    }

    #[test]
    fn parse_authority_ipv6_no_port_defaults_to_80() {
        assert_eq!(parse_authority("[::1]").unwrap(), ("[::1]".to_string(), 80));
    }
}
