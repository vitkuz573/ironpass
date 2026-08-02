use ironpass_core::models::{Protocol, ProxyNode, Security, Transport};
use ironpass_engine::{ProxyConfig, ProxyEngine};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::net::TcpStream;

fn fake_vless_node() -> ProxyNode {
    ProxyNode {
        protocol: Protocol::Vless,
        name: "fake-test-node".into(),
        server: "127.0.0.1".into(),
        port: 1, // Unlikely to be reachable; connection will fail.
        uuid: Some("550e8400-e29b-41d4-a716-446655440000".into()),
        password: None,
        alter_id: None,
        encryption: None,
        transport: Transport::Tcp,
        security: Security::None,
        flow: None,
        sni: None,
        fingerprint: None,
        public_key: None,
        short_id: None,
        spider_x: None,
        path: None,
        host: None,
        service_name: None,
        alpn: None,
        extra: None,
        tags: vec![],
        raw_uri: String::new(),
    }
}

async fn pick_free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    listener.local_addr().unwrap().port()
}

async fn spawn_engine() -> (ProxyEngine, u16, u16) {
    let socks_port = pick_free_port().await;
    let http_port = pick_free_port().await;
    let config = ProxyConfig {
        node: fake_vless_node(),
        local_socks_port: socks_port,
        local_http_port: http_port,
        dns_port: 5353,
    };
    let engine = ProxyEngine::new(config);
    (engine, socks_port, http_port)
}

#[tokio::test]
async fn socks5_server_accepts_auth_negotiation() {
    let (engine, socks_port, _http_port) = spawn_engine().await;

    let handle = tokio::spawn(async move {
        let _ = engine.start().await;
    });

    // Give the server a moment to bind.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let mut stream = TcpStream::connect(format!("127.0.0.1:{}", socks_port))
        .await
        .expect("failed to connect to SOCKS5 proxy");

    stream.write_all(&[0x05, 0x01, 0x00]).await.unwrap();

    let mut buf = [0u8; 2];
    stream.read_exact(&mut buf).await.unwrap();

    assert_eq!(buf, [0x05, 0x00]);

    // Since the target is invalid, a CONNECT request should receive a failure reply.
    stream
        .write_all(&[0x05, 0x01, 0x00, 0x03, 7, b'e', b'x', b'a', b'm', b'p', b'l', b'e', 0x01, 0xbb])
        .await
        .unwrap();

    let mut reply = [0u8; 10];
    stream.read_exact(&mut reply).await.unwrap();
    assert_eq!(reply[0], 0x05);
    assert_eq!(reply[1], 0x05); // connection refused / general failure

    handle.abort();
}

#[tokio::test]
async fn http_proxy_server_returns_502_for_unreachable_target() {
    let (engine, _socks_port, http_port) = spawn_engine().await;

    let handle = tokio::spawn(async move {
        let _ = engine.start().await;
    });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let mut stream = TcpStream::connect(format!("127.0.0.1:{}", http_port))
        .await
        .expect("failed to connect to HTTP proxy");

    stream
        .write_all(b"CONNECT example.com:443 HTTP/1.1\r\nHost: example.com:443\r\n\r\n")
        .await
        .unwrap();

    let mut buf = vec![0u8; 256];
    let n = stream.read(&mut buf).await.unwrap();
    let response = String::from_utf8_lossy(&buf[..n]);

    assert!(response.starts_with("HTTP/1.1 502"));

    handle.abort();
}
