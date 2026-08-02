//! Generate sing-box JSON configuration from a `ProxyNode`.

use ironpass_core::models::{
    Protocol, ProxyNode, Security, SplitTunnelAction, SplitTunnelRule, SplitTunnelTarget, Transport,
};
use serde::Serialize;
use serde_json::{Map, Value};

#[cfg(test)]
use ironpass_core::models::XhttpExtra;

const DEFAULT_MIXED_PORT: u16 = 11080;

/// Generated sing-box config along with exposed local ports.
#[derive(Debug, Clone)]
pub struct SingBoxConfig {
    pub json: String,
    pub socks_port: Option<u16>,
    pub http_port: Option<u16>,
    pub mixed_port: Option<u16>,
}

/// Port selection for proxy inbounds.
#[derive(Debug, Clone, Copy, Default)]
pub struct InboundPorts {
    pub socks_port: Option<u16>,
    pub http_port: Option<u16>,
    pub mixed_port: Option<u16>,
}

#[derive(Serialize)]
struct SingBoxRoot {
    log: Log,
    #[serde(skip_serializing_if = "Option::is_none")]
    dns: Option<Value>,
    inbounds: Vec<Value>,
    outbounds: Vec<Value>,
    route: Route,
}

#[derive(Serialize)]
struct Log {
    level: &'static str,
}

#[derive(Serialize)]
struct Route {
    #[serde(skip_serializing_if = "Option::is_none")]
    geoip: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    geosite: Option<Value>,
    rules: Vec<Value>,
    auto_detect_interface: bool,
    #[serde(rename = "final")]
    final_outbound: &'static str,
}

impl Default for Route {
    fn default() -> Self {
        Self {
            geoip: None,
            geosite: None,
            rules: Vec::new(),
            auto_detect_interface: true,
            final_outbound: "proxy",
        }
    }
}

/// Returns true if sing-box is required for this node (advanced transports / Reality).
#[allow(dead_code)]
pub fn requires_singbox(node: &ProxyNode) -> bool {
    crate::backend::supports_singbox(node)
}

/// Sing-box backend marker type.
#[derive(Debug, Clone, Copy, Default)]
#[allow(dead_code)]
pub struct SingBoxBackend;

impl SingBoxBackend {
    /// Create a new sing-box backend instance.
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self
    }
}

/// Generate a sing-box JSON config for `node` with the requested inbound ports.
pub fn generate_config(
    node: &ProxyNode,
    ports: InboundPorts,
    rules: &[SplitTunnelRule],
) -> anyhow::Result<SingBoxConfig> {
    let outbound = build_outbound(node)?;

    let mut inbounds = Vec::new();
    let mut socks_port = None;
    let mut http_port = None;
    let mut mixed_port = None;

    if let Some(port) = ports.mixed_port {
        mixed_port = Some(port);
        inbounds.push(mixed_inbound(port));
    } else {
        if let Some(port) = ports.socks_port {
            socks_port = Some(port);
            inbounds.push(socks_inbound(port));
        }
        if let Some(port) = ports.http_port {
            http_port = Some(port);
            inbounds.push(http_inbound(port));
        }
    }

    // Ensure at least one inbound exists.
    if inbounds.is_empty() {
        mixed_port = Some(DEFAULT_MIXED_PORT);
        inbounds.push(mixed_inbound(DEFAULT_MIXED_PORT));
    }

    let root = SingBoxRoot {
        log: Log { level: "info" },
        dns: Some(dns()),
        inbounds,
        outbounds: vec![outbound, direct_outbound(), block_outbound()],
        route: build_route(rules),
    };

    let json = serde_json::to_string_pretty(&root)?;
    Ok(SingBoxConfig {
        json,
        socks_port,
        http_port,
        mixed_port,
    })
}

fn build_outbound(node: &ProxyNode) -> anyhow::Result<Value> {
    let mut outbound = Map::new();
    outbound.insert("type".into(), protocol_type(node).into());
    outbound.insert("tag".into(), "proxy".into());
    outbound.insert("server".into(), node.server.clone().into());
    outbound.insert("server_port".into(), (node.port as u64).into());

    match node.protocol {
        Protocol::Vless => build_vless_outbound(node, &mut outbound),
        Protocol::Trojan => build_trojan_outbound(node, &mut outbound),
        Protocol::Vmess => build_vmess_outbound(node, &mut outbound),
        Protocol::Shadowsocks => build_shadowsocks_outbound(node, &mut outbound)?,
        _ => anyhow::bail!(
            "Protocol {:?} not supported by sing-box generator",
            node.protocol
        ),
    }

    build_tls(node, &mut outbound)?;
    build_transport(node, &mut outbound)?;

    Ok(Value::Object(outbound))
}

fn protocol_type(node: &ProxyNode) -> &'static str {
    match node.protocol {
        Protocol::Vless => "vless",
        Protocol::Trojan => "trojan",
        Protocol::Vmess => "vmess",
        Protocol::Shadowsocks => "shadowsocks",
        _ => "block",
    }
}

fn build_vless_outbound(node: &ProxyNode, outbound: &mut Map<String, Value>) {
    if let Some(ref uuid) = node.uuid {
        outbound.insert("uuid".into(), uuid.clone().into());
    }
    if let Some(ref flow) = node.flow {
        outbound.insert("flow".into(), flow.clone().into());
    }
    if node.encryption.as_deref() == Some("none") {
        outbound.insert("packet_encoding".into(), "xudp".into());
    }
}

fn build_trojan_outbound(node: &ProxyNode, outbound: &mut Map<String, Value>) {
    if let Some(ref password) = node.password {
        outbound.insert("password".into(), password.clone().into());
    }
}

fn build_vmess_outbound(node: &ProxyNode, outbound: &mut Map<String, Value>) {
    if let Some(ref uuid) = node.uuid {
        outbound.insert("uuid".into(), uuid.clone().into());
    }
    if let Some(alter_id) = node.alter_id {
        outbound.insert("alter_id".into(), (alter_id as u64).into());
    }
    if let Some(ref security) = node.encryption {
        outbound.insert("security".into(), security.clone().into());
    } else {
        outbound.insert("security".into(), "auto".into());
    }
}

fn build_shadowsocks_outbound(
    node: &ProxyNode,
    outbound: &mut Map<String, Value>,
) -> anyhow::Result<()> {
    // The core parser stores method:password in the uuid/password fields depending on source.
    let (method, password) = parse_shadowsocks_auth(node)?;
    outbound.insert("method".into(), method.into());
    outbound.insert("password".into(), password.into());
    Ok(())
}

fn parse_shadowsocks_auth(node: &ProxyNode) -> anyhow::Result<(&str, &str)> {
    let candidate = node
        .uuid
        .as_deref()
        .or(node.password.as_deref())
        .unwrap_or("");
    if let Some((method, password)) = candidate.split_once(':') {
        Ok((method, password))
    } else {
        anyhow::bail!("Invalid Shadowsocks credentials")
    }
}

fn build_tls(node: &ProxyNode, outbound: &mut Map<String, Value>) -> anyhow::Result<()> {
    match node.security {
        Security::None => Ok(()),
        Security::Tls => {
            let mut tls = Map::new();
            tls.insert("enabled".into(), true.into());
            if let Some(ref sni) = node.sni {
                tls.insert("server_name".into(), sni.clone().into());
            }
            if let Some(ref fp) = node.fingerprint {
                tls.insert(
                    "utls".into(),
                    serde_json::json!({ "enabled": true, "fingerprint": fp }),
                );
            }
            if let Some(ref alpn) = node.alpn {
                tls.insert("alpn".into(), alpn.clone().into());
            }
            outbound.insert("tls".into(), Value::Object(tls));
            Ok(())
        }
        Security::Reality | Security::RealityPsk => {
            let mut tls = Map::new();
            tls.insert("enabled".into(), true.into());
            if let Some(ref sni) = node.sni {
                tls.insert("server_name".into(), sni.clone().into());
            }
            if let Some(ref fp) = node.fingerprint {
                tls.insert(
                    "utls".into(),
                    serde_json::json!({ "enabled": true, "fingerprint": fp }),
                );
            }
            let mut reality = Map::new();
            reality.insert("enabled".into(), true.into());
            if let Some(ref pbk) = node.public_key {
                reality.insert("public_key".into(), pbk.clone().into());
            }
            if let Some(ref sid) = node.short_id {
                reality.insert("short_id".into(), sid.clone().into());
            }
            if let Some(ref spider_x) = node.spider_x {
                reality.insert("spiderX".into(), spider_x.clone().into());
            }
            tls.insert("reality".into(), Value::Object(reality));
            outbound.insert("tls".into(), Value::Object(tls));
            Ok(())
        }
    }
}

fn build_transport(node: &ProxyNode, outbound: &mut Map<String, Value>) -> anyhow::Result<()> {
    let transport = match node.transport {
        Transport::Tcp => return Ok(()),
        Transport::Ws => "ws",
        Transport::Grpc => "grpc",
        Transport::H2 => "http",
        // sing-box 1.13.x renamed "splithttp" to "httpupgrade". "xhttp" nodes
        // from VLESS subscriptions use HTTP upgrade semantics, so map both here.
        Transport::Xhttp => "httpupgrade",
        Transport::Splithttp => "httpupgrade",
        Transport::Kcp => "kcp",
    };

    let mut t = Map::new();
    t.insert("type".into(), transport.into());

    match node.transport {
        Transport::Ws | Transport::H2 | Transport::Xhttp | Transport::Splithttp => {
            if let Some(ref path) = node.path {
                t.insert("path".into(), path.clone().into());
            }
            if let Some(ref host) = node.host {
                let mut headers = Map::new();
                headers.insert("Host".into(), host.clone().into());
                t.insert("headers".into(), Value::Object(headers));
            }
        }
        Transport::Grpc => {
            if let Some(ref service) = node.service_name {
                t.insert("service_name".into(), service.clone().into());
            }
        }
        _ => {}
    }

    // Merge XHTTP extra settings if present.
    if (node.transport == Transport::Xhttp || node.transport == Transport::Splithttp)
        && let Some(ref extra) = node.extra
    {
        if let Some(ref mode) = extra.mode {
            t.insert("mode".into(), mode.clone().into());
        }
        if let Some(max) = extra.max_connections {
            t.insert("max_connections".into(), max.into());
        }
        if let Some(max) = extra.max_concurrent_uploads {
            t.insert("max_concurrent_uploads".into(), max.into());
        }
        if let Some(no_grpc_header) = extra.no_grpc_header {
            t.insert("no_grpc_header".into(), no_grpc_header.into());
        }
        if let Some(ref padding) = extra.x_padding_bytes {
            t.insert("padding_bytes".into(), padding.clone().into());
        }
        if !extra.headers.is_empty() {
            let headers: Map<String, Value> = extra
                .headers
                .iter()
                .map(|(k, v)| (k.clone(), Value::String(v.clone())))
                .collect();
            if let Some(existing) = t
                .entry("headers")
                .or_insert_with(|| Value::Object(Map::new()))
                .as_object_mut()
            {
                for (k, v) in headers {
                    existing.insert(k, v);
                }
            }
        }
    }

    outbound.insert("transport".into(), Value::Object(t));
    Ok(())
}

fn mixed_inbound(port: u16) -> Value {
    serde_json::json!({
        "type": "mixed",
        "tag": "mixed-in",
        "listen": "127.0.0.1",
        "listen_port": port,
    })
}

fn socks_inbound(port: u16) -> Value {
    serde_json::json!({
        "type": "socks",
        "tag": "socks-in",
        "listen": "127.0.0.1",
        "listen_port": port,
    })
}

fn http_inbound(port: u16) -> Value {
    serde_json::json!({
        "type": "http",
        "tag": "http-in",
        "listen": "127.0.0.1",
        "listen_port": port,
    })
}

fn direct_outbound() -> Value {
    serde_json::json!({
        "type": "direct",
        "tag": "direct",
    })
}

fn block_outbound() -> Value {
    serde_json::json!({
        "type": "block",
        "tag": "block",
    })
}

fn build_route(rules: &[SplitTunnelRule]) -> Route {
    let mut route_rules = Vec::with_capacity(rules.len());
    for rule in rules {
        let Some(value) = build_route_rule(rule) else {
            tracing::warn!("Skipping unsupported split tunnel rule: {:?}", rule);
            continue;
        };
        route_rules.push(value);
    }
    Route {
        geoip: None,
        geosite: None,
        rules: route_rules,
        auto_detect_interface: true,
        final_outbound: "proxy",
    }
}

fn build_route_rule(rule: &SplitTunnelRule) -> Option<Value> {
    let outbound = match rule.action {
        SplitTunnelAction::Direct => "direct",
        SplitTunnelAction::Proxy => "proxy",
    };
    let mut map = Map::new();
    map.insert("outbound".into(), outbound.into());

    match rule.target {
        SplitTunnelTarget::Domain => {
            let value = rule.value.trim_start_matches('*').trim_start_matches('.');
            if rule.value.starts_with('*') || rule.value.starts_with('.') {
                map.insert("domain_suffix".into(), value.into());
            } else {
                map.insert("domain".into(), serde_json::json!([rule.value.clone()]));
            }
        }
        SplitTunnelTarget::Ip => {
            map.insert("ip_cidr".into(), serde_json::json!([rule.value.clone()]));
        }
        SplitTunnelTarget::Cidr => {
            map.insert("ip_cidr".into(), serde_json::json!([rule.value.clone()]));
        }
        SplitTunnelTarget::App => {
            return None;
        }
    }

    Some(Value::Object(map))
}

// Minimal DNS configuration compatible with sing-box 1.11.x through 1.13.x.
fn dns() -> Value {
    serde_json::json!({
        "servers": [
            { "tag": "local", "type": "local" }
        ]
    })
}

/// Generate a minimal outbound only (used for testing serialization).
#[cfg(test)]
mod tests {
    use super::*;
    use ironpass_core::models::{Protocol, Security, Transport};
    use std::collections::HashMap;

    fn sample_vless_reality() -> ProxyNode {
        ProxyNode {
            protocol: Protocol::Vless,
            name: "vless-reality".into(),
            server: "example.com".into(),
            port: 443,
            uuid: Some("550e8400-e29b-41d4-a716-446655440000".into()),
            password: None,
            alter_id: None,
            encryption: Some("none".into()),
            transport: Transport::Tcp,
            security: Security::Reality,
            flow: Some("xtls-rprx-vision".into()),
            sni: Some("sni.example.com".into()),
            fingerprint: Some("chrome".into()),
            public_key: Some("pbk-example".into()),
            short_id: Some("0123456789abcdef".into()),
            spider_x: Some("/spider".into()),
            path: None,
            host: None,
            service_name: None,
            alpn: None,
            extra: None,
            tags: Vec::new(),
            raw_uri: String::new(),
        }
    }

    #[test]
    fn vless_reality_config_contains_outbound() {
        let node = sample_vless_reality();
        let cfg = generate_config(&node, InboundPorts::default(), &[]).unwrap();
        let value: Value = serde_json::from_str(&cfg.json).unwrap();
        let outbounds = value.get("outbounds").unwrap().as_array().unwrap();
        let proxy = outbounds
            .iter()
            .find(|o| o.get("tag").unwrap() == "proxy")
            .unwrap();
        assert_eq!(proxy.get("type").unwrap(), "vless");
        assert!(proxy.get("tls").is_some());
        assert!(cfg.mixed_port.is_some());
    }

    #[test]
    fn vless_xhttp_maps_to_http_transport() {
        let mut node = sample_vless_reality();
        node.transport = Transport::Xhttp;
        node.security = Security::Tls;
        node.path = Some("/xhttp".into());
        node.host = Some("host.example.com".into());
        node.extra = Some(XhttpExtra {
            mode: Some("stream-up".into()),
            max_connections: Some(4),
            max_concurrent_uploads: Some(8),
            no_grpc_header: Some(false),
            x_padding_bytes: Some("100-200".into()),
            headers: HashMap::new(),
        });

        let cfg = generate_config(
            &node,
            InboundPorts {
                socks_port: Some(1080),
                http_port: Some(8080),
                ..Default::default()
            },
            &[],
        )
        .unwrap();
        let value: Value = serde_json::from_str(&cfg.json).unwrap();
        let outbounds = value.get("outbounds").unwrap().as_array().unwrap();
        let proxy = outbounds
            .iter()
            .find(|o| o.get("tag").unwrap() == "proxy")
            .unwrap();
        let transport = proxy.get("transport").unwrap();
        assert_eq!(transport.get("type").unwrap(), "httpupgrade");
        assert_eq!(transport.get("path").unwrap(), "/xhttp");
        assert!(cfg.socks_port.is_some());
        assert!(cfg.http_port.is_some());
    }

    #[test]
    fn vless_xhttp_config_contains_transport() {
        let mut node = sample_vless_reality();
        node.transport = Transport::Xhttp;
        node.security = Security::Tls;
        node.path = Some("/xhttp".into());
        node.host = Some("host.example.com".into());
        node.extra = Some(XhttpExtra {
            mode: Some("stream-up".into()),
            max_connections: Some(4),
            max_concurrent_uploads: Some(8),
            no_grpc_header: Some(false),
            x_padding_bytes: Some("100-200".into()),
            headers: HashMap::new(),
        });

        let cfg = generate_config(
            &node,
            InboundPorts {
                socks_port: Some(1080),
                http_port: Some(8080),
                mixed_port: None,
            },
            &[],
        )
        .unwrap();
        let value: Value = serde_json::from_str(&cfg.json).unwrap();
        let outbounds = value.get("outbounds").unwrap().as_array().unwrap();
        let proxy = outbounds
            .iter()
            .find(|o| o.get("tag").unwrap() == "proxy")
            .unwrap();
        let transport = proxy.get("transport").unwrap();
        assert_eq!(transport.get("type").unwrap(), "httpupgrade");
        assert_eq!(transport.get("mode").unwrap(), "stream-up");
        assert!(value.get("inbounds").unwrap().as_array().unwrap().len() == 2);
    }

    #[test]
    fn requires_singbox_for_reality() {
        let node = sample_vless_reality();
        assert!(requires_singbox(&node));
    }

    #[test]
    fn requires_singbox_for_xhttp() {
        let mut node = sample_vless_reality();
        node.security = Security::Tls;
        node.transport = Transport::Xhttp;
        assert!(requires_singbox(&node));
    }

    #[test]
    fn domain_rule_appears_in_route() {
        use ironpass_core::models::{SplitTunnelAction, SplitTunnelTarget};
        let node = sample_vless_reality();
        let rules = vec![SplitTunnelRule::new(
            SplitTunnelTarget::Domain,
            "example.com",
            SplitTunnelAction::Direct,
            None,
        )];
        let cfg = generate_config(&node, InboundPorts::default(), &rules).unwrap();
        let value: Value = serde_json::from_str(&cfg.json).unwrap();
        let route = value.get("route").unwrap();
        let route_rules = route.get("rules").unwrap().as_array().unwrap();
        assert_eq!(route_rules.len(), 1);
        assert_eq!(route_rules[0].get("outbound").unwrap(), "direct");
        let domains = route_rules[0].get("domain").unwrap().as_array().unwrap();
        assert!(domains.iter().any(|d| d == "example.com"));
    }

    #[test]
    fn wildcard_domain_uses_domain_suffix() {
        use ironpass_core::models::{SplitTunnelAction, SplitTunnelTarget};
        let node = sample_vless_reality();
        let rules = vec![SplitTunnelRule::new(
            SplitTunnelTarget::Domain,
            "*.example.com",
            SplitTunnelAction::Proxy,
            None,
        )];
        let cfg = generate_config(&node, InboundPorts::default(), &rules).unwrap();
        let value: Value = serde_json::from_str(&cfg.json).unwrap();
        let route_rules = value["route"]["rules"].as_array().unwrap();
        assert_eq!(route_rules[0].get("outbound").unwrap(), "proxy");
        assert_eq!(route_rules[0].get("domain_suffix").unwrap(), "example.com");
    }

    #[test]
    fn ip_and_cidr_rules_use_ip_cidr() {
        use ironpass_core::models::{SplitTunnelAction, SplitTunnelTarget};
        let node = sample_vless_reality();
        let rules = vec![
            SplitTunnelRule::new(
                SplitTunnelTarget::Ip,
                "1.2.3.4",
                SplitTunnelAction::Direct,
                None,
            ),
            SplitTunnelRule::new(
                SplitTunnelTarget::Cidr,
                "10.0.0.0/8",
                SplitTunnelAction::Proxy,
                None,
            ),
        ];
        let cfg = generate_config(&node, InboundPorts::default(), &rules).unwrap();
        let value: Value = serde_json::from_str(&cfg.json).unwrap();
        let route_rules = value["route"]["rules"].as_array().unwrap();
        assert_eq!(route_rules.len(), 2);
        let first = route_rules[0]["ip_cidr"].as_array().unwrap();
        assert!(first.iter().any(|v| v == "1.2.3.4"));
        let second = route_rules[1]["ip_cidr"].as_array().unwrap();
        assert!(second.iter().any(|v| v == "10.0.0.0/8"));
    }

    #[test]
    fn app_rules_are_skipped() {
        use ironpass_core::models::{SplitTunnelAction, SplitTunnelTarget};
        let node = sample_vless_reality();
        let rules = vec![SplitTunnelRule::new(
            SplitTunnelTarget::App,
            "curl",
            SplitTunnelAction::Direct,
            None,
        )];
        let cfg = generate_config(&node, InboundPorts::default(), &rules).unwrap();
        let value: Value = serde_json::from_str(&cfg.json).unwrap();
        let route_rules = value["route"]["rules"].as_array().unwrap();
        assert!(route_rules.is_empty());
    }
}
