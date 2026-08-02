use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use ironpass_core::models::{OutputFormat, Protocol, SubscriptionFormat};
use ironpass_subscription::SubscriptionService;

fn raw_vless_uri() -> &'static str {
    "vless://550e8400-e29b-41d4-a716-446655440000@example.com:443?encryption=none&security=tls&sni=example.com&fp=chrome&type=ws&path=%2Fchat&host=cdn.example.com#TestNode"
}

fn raw_trojan_uri() -> &'static str {
    "trojan://password@example.com:443?sni=example.com&type=ws&path=%2Fchat&host=cdn.example.com#TrojanNode"
}

fn raw_shadowsocks_uri() -> String {
    let payload = base64::engine::general_purpose::STANDARD.encode("example.com:1080".as_bytes());
    format!("ss://{}#SSNode", payload)
}

fn raw_vmess_uri() -> String {
    let json = r#"{"v":"2","ps":"VmessNode","add":"example.com","port":"443","id":"550e8400-e29b-41d4-a716-446655440000","aid":"0","scy":"auto","net":"ws","type":"none","host":"cdn.example.com","path":"/chat","tls":"tls","sni":"example.com","fp":"chrome"}"#;
    format!("vmess://{}", STANDARD.encode(json.as_bytes()))
}

#[test]
fn raw_uri_list_to_raw_round_trip() {
    let svc = SubscriptionService::new();
    let input = raw_vless_uri();
    let nodes = svc.parse_raw(input).unwrap();
    assert_eq!(nodes.len(), 1);

    let exported = svc.export(&nodes, &OutputFormat::Raw).unwrap();
    assert!(exported.contains("vless://"));
    assert!(exported.contains("example.com:443"));
}

#[test]
fn raw_uri_list_to_v2ray_round_trip() {
    let svc = SubscriptionService::new();
    let input = format!("{}\n{}", raw_vless_uri(), raw_trojan_uri());
    let nodes = svc.parse_raw(&input).unwrap();
    assert_eq!(nodes.len(), 2);

    let exported = svc.export(&nodes, &OutputFormat::V2Ray).unwrap();
    let decoded = STANDARD.decode(&exported).unwrap();
    let text = String::from_utf8(decoded).unwrap();
    assert!(text.contains("vless://"));
    assert!(text.contains("trojan://"));
}

#[test]
fn base64_list_parses_and_exports_to_clash() {
    let svc = SubscriptionService::new();
    let raw = format!("{}\n{}", raw_vless_uri(), raw_shadowsocks_uri());
    let encoded = STANDARD.encode(raw.as_bytes());

    assert_eq!(
        svc.detect_format(&encoded),
        SubscriptionFormat::Base64VlessList
    );

    let nodes = svc.parse_raw(&encoded).unwrap();
    assert_eq!(nodes.len(), 2);
    assert!(nodes.iter().any(|n| n.protocol == Protocol::Vless));
    assert!(nodes.iter().any(|n| n.protocol == Protocol::Shadowsocks));

    let yaml = svc.export(&nodes, &OutputFormat::Clash).unwrap();
    assert!(yaml.contains("proxies:"));
    assert!(yaml.contains("TestNode"));
    assert!(yaml.contains("SSNode"));
}

#[test]
fn base64_list_parses_and_exports_to_singbox() {
    let svc = SubscriptionService::new();
    let raw = format!("{}\n{}", raw_trojan_uri(), raw_vmess_uri());
    let encoded = STANDARD.encode(raw.as_bytes());

    let nodes = svc.parse_raw(&encoded).unwrap();
    assert_eq!(nodes.len(), 2);

    let json = svc.export(&nodes, &OutputFormat::SingBox).unwrap();
    assert!(json.contains("\"outbounds\""));
    assert!(json.contains("TrojanNode"));
    assert!(json.contains("VmessNode"));
}

#[test]
fn clash_yaml_to_raw_round_trip() {
    let input = r#"
proxies:
  - name: "clash-vless"
    type: vless
    server: example.com
    port: 443
    uuid: 550e8400-e29b-41d4-a716-446655440000
    cipher: none
    tls: true
    network: ws
    ws-path: /chat
    ws-opts:
      headers:
        Host: cdn.example.com
    sni: example.com
proxy-groups:
  - name: "Auto"
"#;

    let svc = SubscriptionService::new();
    assert_eq!(svc.detect_format(input), SubscriptionFormat::ClashYaml);

    let nodes = svc.parse_raw(input).unwrap();
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0].protocol, Protocol::Vless);

    let raw = svc.export(&nodes, &OutputFormat::Raw).unwrap();
    assert!(raw.starts_with("vless://"));
}

#[test]
fn singbox_json_to_raw_round_trip() {
    let input = r#"
{
  "outbounds": [
    {
      "type": "trojan",
      "tag": "singbox-trojan",
      "server": "example.com",
      "server_port": 443,
      "password": "password",
      "tls": {
        "enabled": true,
        "server_name": "example.com"
      }
    }
  ]
}
"#;

    let svc = SubscriptionService::new();
    assert_eq!(svc.detect_format(input), SubscriptionFormat::SingBoxJson);

    let nodes = svc.parse_raw(input).unwrap();
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0].protocol, Protocol::Trojan);

    let raw = svc.export(&nodes, &OutputFormat::Raw).unwrap();
    assert!(raw.starts_with("trojan://"));
}

#[test]
fn export_empty_nodes_is_valid() {
    let svc = SubscriptionService::new();
    assert_eq!(svc.export(&[], &OutputFormat::Raw).unwrap(), "");
    assert!(
        svc.export(&[], &OutputFormat::Clash)
            .unwrap()
            .contains("proxies: []")
    );
    assert!(
        svc.export(&[], &OutputFormat::SingBox)
            .unwrap()
            .contains("\"outbounds\": []")
    );
}
