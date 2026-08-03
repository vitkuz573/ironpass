//! Generate Xray-core JSON configuration from a `ProxyNode`.

use crate::assets::GeoAssetStatus;
use ironpass_core::models::{
    Protocol, ProxyNode, Security, SplitTunnelAction, SplitTunnelRule, SplitTunnelTarget,
    Transport, XhttpExtra,
};
use serde::Serialize;
use serde_json::{Map, Value};

/// CIDR ranges that are private/local per RFC1918 / RFC4193 / RFC4291.
const PRIVATE_CIDRS: &[&str] = &[
    "10.0.0.0/8",
    "172.16.0.0/12",
    "192.168.0.0/16",
    "127.0.0.0/8",
    "fc00::/7",
    "fe80::/10",
];

/// Localhost-style domains that should bypass the proxy.
const LOCALHOST_DOMAINS: &[&str] = &["localhost", "localhost.localdomain", "regexp:.*\\.local$"];

/// Generate an Xray-core JSON config for `node` with the requested inbound ports.
///
/// `geo_status` controls whether geoip/geosite rules are emitted or a safe
/// RFC1918/local fallback is used.
pub fn generate_config(
    node: &ProxyNode,
    ports: InboundPorts,
    rules: &[SplitTunnelRule],
    geo_status: GeoAssetStatus,
) -> anyhow::Result<XrayConfig> {
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

    // Use port 0 (let Xray pick) for the stats API; policy enables level=0 stats.
    let api_port = 0;
    let api = api_service(api_port);
    let policy = stats_policy();

    let root = XrayRoot {
        log: Log {
            access: None,
            error: None,
            loglevel: "warning",
        },
        api: Some(api),
        inbounds,
        outbounds: vec![outbound, direct_outbound(), block_outbound()],
        routing: routing(rules, geo_status),
        policy: Some(policy),
    };

    let json = serde_json::to_string_pretty(&root)?;
    Ok(XrayConfig {
        json,
        socks_port,
        http_port,
        mixed_port,
    })
}

const DEFAULT_MIXED_PORT: u16 = 11080;

/// Generated Xray config along with exposed local ports.
#[derive(Debug, Clone)]
pub struct XrayConfig {
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
struct XrayRoot {
    log: Log,
    #[serde(skip_serializing_if = "Option::is_none")]
    api: Option<Value>,
    inbounds: Vec<Value>,
    outbounds: Vec<Value>,
    routing: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    policy: Option<Value>,
}

#[derive(Serialize)]
struct Log {
    #[serde(skip_serializing_if = "Option::is_none")]
    access: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    loglevel: &'static str,
}


fn build_outbound(node: &ProxyNode) -> anyhow::Result<Value> {
    match node.protocol {
        Protocol::Vless => build_vless_outbound(node),
        Protocol::Trojan => build_trojan_outbound(node),
        _ => anyhow::bail!(
            "Protocol {:?} not supported by Xray-core generator",
            node.protocol
        ),
    }
}

fn build_vless_outbound(node: &ProxyNode) -> anyhow::Result<Value> {
    let uuid = node
        .uuid
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("VLESS requires a UUID"))?;

    let mut settings = Map::new();
    let mut vnext = Map::new();
    vnext.insert("address".into(), node.server.clone().into());
    vnext.insert("port".into(), (node.port as u64).into());

    let mut user = Map::new();
    user.insert("id".into(), uuid.into());
    user.insert(
        "encryption".into(),
        node.encryption
            .clone()
            .unwrap_or_else(|| "none".into())
            .into(),
    );
    if let Some(ref flow) = node.flow {
        user.insert("flow".into(), flow.clone().into());
    }
    if let Some(ref fp) = node.fingerprint {
        user.insert("fingerprint".into(), fp.clone().into());
    }
    vnext.insert("users".into(), Value::Array(vec![Value::Object(user)]));
    settings.insert("vnext".into(), Value::Array(vec![Value::Object(vnext)]));

    let mut outbound = Map::new();
    outbound.insert("protocol".into(), "vless".into());
    outbound.insert("settings".into(), Value::Object(settings));
    build_tls(node, &mut outbound)?;
    build_transport(node, &mut outbound)?;
    outbound.insert("tag".into(), "proxy".into());

    Ok(Value::Object(outbound))
}

fn build_trojan_outbound(node: &ProxyNode) -> anyhow::Result<Value> {
    let password = node
        .password
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("Trojan requires a password"))?;

    let mut settings = Map::new();
    let mut server = Map::new();
    server.insert("address".into(), node.server.clone().into());
    server.insert("port".into(), (node.port as u64).into());
    server.insert("password".into(), password.into());
    if let Some(ref sni) = node.sni {
        server.insert("sni".into(), sni.clone().into());
    }
    if let Some(ref fp) = node.fingerprint {
        server.insert("fingerprint".into(), fp.clone().into());
    }
    settings.insert("servers".into(), Value::Array(vec![Value::Object(server)]));

    let mut outbound = Map::new();
    outbound.insert("protocol".into(), "trojan".into());
    outbound.insert("settings".into(), Value::Object(settings));
    build_tls(node, &mut outbound)?;
    build_transport(node, &mut outbound)?;
    outbound.insert("tag".into(), "proxy".into());

    Ok(Value::Object(outbound))
}

fn build_tls(node: &ProxyNode, outbound: &mut Map<String, Value>) -> anyhow::Result<()> {
    match node.security {
        Security::None => Ok(()),
        Security::Tls => {
            let mut tls = Map::new();
            tls.insert(
                "serverName".into(),
                node.sni
                    .clone()
                    .unwrap_or_else(|| node.server.clone())
                    .into(),
            );
            if let Some(ref alpn) = node.alpn {
                tls.insert("alpn".into(), alpn.clone().into());
            }
            if let Some(ref fp) = node.fingerprint {
                tls.insert("fingerprint".into(), fp.clone().into());
            }
            outbound.insert(
                "streamSettings".into(),
                serde_json::json!({ "security": "tls", "tlsSettings": tls }),
            );
            Ok(())
        }
        Security::Reality | Security::RealityPsk => {
            let mut reality = Map::new();
            reality.insert(
                "serverName".into(),
                node.sni
                    .clone()
                    .unwrap_or_else(|| node.server.clone())
                    .into(),
            );
            if let Some(ref pbk) = node.public_key {
                reality.insert("publicKey".into(), pbk.clone().into());
            }
            if let Some(ref sid) = node.short_id {
                reality.insert("shortId".into(), sid.clone().into());
            } else {
                reality.insert("shortId".into(), "".into());
            }
            if let Some(ref spider_x) = node.spider_x {
                reality.insert("spiderX".into(), spider_x.clone().into());
            }
            if let Some(ref fp) = node.fingerprint {
                reality.insert("fingerprint".into(), fp.clone().into());
            }

            let mut stream = Map::new();
            stream.insert("security".into(), "reality".into());
            stream.insert("realitySettings".into(), Value::Object(reality));
            outbound.insert("streamSettings".into(), Value::Object(stream));
            Ok(())
        }
    }
}

fn build_transport(node: &ProxyNode, outbound: &mut Map<String, Value>) -> anyhow::Result<()> {
    let transport_type = match node.transport {
        Transport::Tcp => {
            // TCP may still carry streamSettings from TLS; do nothing here.
            return Ok(());
        }
        Transport::Ws => "ws",
        Transport::Grpc => "grpc",
        Transport::H2 => "http",
        Transport::Xhttp => "xhttp",
        Transport::Splithttp => "splithttp",
        Transport::Kcp => "kcp",
    };

    let mut t = Map::new();

    match node.transport {
        Transport::Ws | Transport::H2 | Transport::Xhttp | Transport::Splithttp => {
            // Xray XHTTP with REALITY works reliably with path="/" and no explicit mode.
            // The subscription extra blob is kept in `extra` for advanced tuning but the
            // default path is set to "/" when missing to avoid routing mismatches.
            if node.transport == Transport::Xhttp || node.transport == Transport::Splithttp {
                t.insert(
                    "path".into(),
                    node.path.clone().unwrap_or_else(|| "/".into()).into(),
                );
            } else if let Some(ref path) = node.path {
                t.insert("path".into(), path.clone().into());
            }
            if let Some(ref host) = node.host {
                t.insert("host".into(), host.clone().into());
            }
            if let Some(ref extra) = node.extra {
                apply_xhttp_extra(&mut t, extra);
            }
            // Merge extra headers if provided.
            if let Some(ref extra) = node.extra
                && !extra.headers.is_empty()
            {
                let headers: Map<String, Value> = extra
                    .headers
                    .iter()
                    .map(|(k, v)| (k.clone(), Value::String(v.clone())))
                    .collect();
                t.insert("headers".into(), Value::Object(headers));
            }
        }
        Transport::Grpc => {
            if let Some(ref service) = node.service_name {
                t.insert("serviceName".into(), service.clone().into());
            }
        }
        _ => {}
    }

    // Xray uses streamSettings.network + streamSettings.xhttpSettings etc.
    let stream_settings = outbound
        .entry("streamSettings")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("streamSettings is not an object"))?;

    // For transports we set the network key and the specific settings key.
    let settings_key = format!("{transport_type}Settings");
    stream_settings.insert("network".into(), transport_type.into());

    if node.transport == Transport::Xhttp || node.transport == Transport::Splithttp {
        // XHTTP-specific settings are stored under xhttpSettings/splithttpSettings.
        stream_settings.insert(settings_key.clone(), Value::Object(t));
    } else {
        stream_settings.insert(settings_key, Value::Object(t));
    }

    Ok(())
}

fn apply_xhttp_extra(t: &mut Map<String, Value>, extra: &XhttpExtra) {
    // Forward any fields from the raw `extra` blob that are not modelled explicitly.
    for (k, v) in &extra.other {
        t.insert(k.clone(), v.clone());
    }
    if let Some(ref mode) = extra.mode {
        t.insert("mode".into(), mode.clone().into());
    }
    if let Some(max) = extra.max_connections {
        t.insert("maxConnections".into(), max.into());
    }
    if let Some(max) = extra.max_concurrent_uploads {
        t.insert("maxConcurrentUploads".into(), max.into());
    }
    if let Some(no_grpc_header) = extra.no_grpc_header {
        t.insert("noGRPCHeader".into(), no_grpc_header.into());
    }
    if let Some(ref padding) = extra.x_padding_bytes {
        t.insert("xPaddingBytes".into(), padding.clone().into());
    }
}

fn mixed_inbound(port: u16) -> Value {
    serde_json::json!({
        "tag": "mixed-in",
        "port": port,
        "listen": "127.0.0.1",
        "protocol": "socks",
        "settings": {
            "auth": "noauth",
            "udp": true,
            "ip": "127.0.0.1"
        },
        "sniffing": {
            "enabled": true,
            "destOverride": ["http", "tls", "quic"]
        }
    })
}

fn socks_inbound(port: u16) -> Value {
    serde_json::json!({
        "tag": "socks-in",
        "port": port,
        "listen": "127.0.0.1",
        "protocol": "socks",
        "settings": {
            "auth": "noauth",
            "udp": true,
            "ip": "127.0.0.1"
        },
        "sniffing": {
            "enabled": true,
            "destOverride": ["http", "tls", "quic"]
        }
    })
}

fn http_inbound(port: u16) -> Value {
    serde_json::json!({
        "tag": "http-in",
        "port": port,
        "listen": "127.0.0.1",
        "protocol": "http",
        "settings": {},
        "sniffing": {
            "enabled": true,
            "destOverride": ["http", "tls", "quic"]
        }
    })
}

fn direct_outbound() -> Value {
    serde_json::json!({
        "protocol": "freedom",
        "tag": "direct"
    })
}

fn block_outbound() -> Value {
    serde_json::json!({
        "protocol": "blackhole",
        "tag": "block"
    })
}

fn routing(rules: &[SplitTunnelRule], geo_status: GeoAssetStatus) -> Value {
    let mut rule_values = Vec::new();
    if geo_status.available {
        rule_values.push(serde_json::json!({
            "type": "field",
            "outboundTag": "direct",
            "ip": ["geoip:private"]
        }));
        rule_values.push(serde_json::json!({
            "type": "field",
            "outboundTag": "block",
            "domain": ["geosite:category-ads-all"]
        }));
    } else {
        rule_values.push(serde_json::json!({
            "type": "field",
            "outboundTag": "direct",
            "ip": PRIVATE_CIDRS
        }));
        rule_values.push(serde_json::json!({
            "type": "field",
            "outboundTag": "direct",
            "domain": LOCALHOST_DOMAINS
        }));
    }
    for rule in rules {
        let Some(value) = build_routing_rule(rule) else {
            tracing::warn!(
                "Skipping unsupported split tunnel rule for Xray: {:?}",
                rule
            );
            continue;
        };
        rule_values.push(value);
    }

    serde_json::json!({
        "domainStrategy": "IPIfNonMatch",
        "rules": rule_values
    })
}

fn build_routing_rule(rule: &SplitTunnelRule) -> Option<Value> {
    let outbound = match rule.action {
        SplitTunnelAction::Direct => "direct",
        SplitTunnelAction::Proxy => "proxy",
    };
    let mut map = Map::new();
    map.insert("type".into(), "field".into());
    map.insert("outboundTag".into(), outbound.into());

    match rule.target {
        SplitTunnelTarget::Domain => {
            let value = rule.value.trim_start_matches('*').trim_start_matches('.');
            if rule.value.starts_with('*') || rule.value.starts_with('.') {
                map.insert(
                    "domain".into(),
                    serde_json::json!([format!("domain:{value}")]),
                );
            } else {
                map.insert("domain".into(), serde_json::json!([rule.value.clone()]));
            }
        }
        SplitTunnelTarget::Ip | SplitTunnelTarget::Cidr => {
            map.insert("ip".into(), serde_json::json!([rule.value.clone()]));
        }
        SplitTunnelTarget::App => {
            return None;
        }
    }

    Some(Value::Object(map))
}

fn api_service(api_port: u16) -> Value {
    serde_json::json!({
        "tag": "api",
        "services": ["StatsService"],
        "port": api_port
    })
}

fn stats_policy() -> Value {
    serde_json::json!({
        "levels": {
            "0": {
                "statsUserUplink": true,
                "statsUserDownlink": true
            }
        },
        "system": {
            "statsInboundUplink": true,
            "statsInboundDownlink": true,
            "statsOutboundUplink": true,
            "statsOutboundDownlink": true
        }
    })
}

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
        let cfg = generate_config(&node, InboundPorts::default(), &[], GeoAssetStatus::new(true)).unwrap();
        let value: Value = serde_json::from_str(&cfg.json).unwrap();
        let outbounds = value.get("outbounds").unwrap().as_array().unwrap();
        let proxy = outbounds
            .iter()
            .find(|o| o.get("tag").unwrap() == "proxy")
            .unwrap();
        assert_eq!(proxy.get("protocol").unwrap(), "vless");
        assert!(proxy.get("streamSettings").is_some());
        assert!(cfg.mixed_port.is_some());
    }

    #[test]
    fn vless_xhttp_extra_is_applied() {
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
            headers: {
                let mut h = HashMap::new();
                h.insert("X-Custom".into(), "value".into());
                h
            },
            ..Default::default()
        });

        let cfg = generate_config(
            &node,
            InboundPorts {
                socks_port: Some(1080),
                http_port: Some(8080),
                ..Default::default()
            },
            &[],
            GeoAssetStatus::new(true),
        )
        .unwrap();

        let value: Value = serde_json::from_str(&cfg.json).unwrap();
        let outbounds = value.get("outbounds").unwrap().as_array().unwrap();
        let proxy = outbounds
            .iter()
            .find(|o| o.get("tag").unwrap() == "proxy")
            .unwrap();
        let stream = proxy.get("streamSettings").unwrap();
        assert_eq!(stream.get("network").unwrap(), "xhttp");
        let xhttp = stream.get("xhttpSettings").unwrap();
        assert_eq!(xhttp.get("path").unwrap(), "/xhttp");
        assert_eq!(xhttp.get("host").unwrap(), "host.example.com");
        assert_eq!(xhttp.get("mode").unwrap(), "stream-up");
        assert_eq!(xhttp.get("maxConnections").unwrap(), 4);
        assert_eq!(xhttp.get("xPaddingBytes").unwrap(), "100-200");
        let headers = xhttp.get("headers").unwrap();
        assert_eq!(headers.get("X-Custom").unwrap(), "value");
        assert!(cfg.socks_port.is_some());
        assert!(cfg.http_port.is_some());
    }

    #[test]
    fn vless_xhttp_raw_extra_fields_are_forwarded() {
        let mut node = sample_vless_reality();
        node.transport = Transport::Xhttp;
        node.security = Security::Tls;
        node.path = Some("/xhttp".into());
        node.host = Some("host.example.com".into());
        node.extra = Some(XhttpExtra {
            mode: Some("stream-up".into()),
            x_padding_bytes: Some("100-200".into()),
            other: {
                let mut m = std::collections::HashMap::new();
                let mut xmux = Map::new();
                xmux.insert("maxConnections".into(), 8.into());
                m.insert("xmux".into(), Value::Object(xmux));
                m
            },
            ..Default::default()
        });

        let cfg = generate_config(
            &node,
            InboundPorts {
                socks_port: Some(1080),
                ..Default::default()
            },
            &[],
            GeoAssetStatus::new(true),
        )
        .unwrap();

        let value: Value = serde_json::from_str(&cfg.json).unwrap();
        let outbounds = value.get("outbounds").unwrap().as_array().unwrap();
        let proxy = outbounds
            .iter()
            .find(|o| o.get("tag").unwrap() == "proxy")
            .unwrap();
        let xhttp = proxy["streamSettings"]["xhttpSettings"].as_object().unwrap();
        assert_eq!(xhttp.get("mode").unwrap(), "stream-up");
        assert_eq!(xhttp.get("xPaddingBytes").unwrap(), "100-200");
        let xmux = xhttp.get("xmux").unwrap().as_object().unwrap();
        assert_eq!(xmux.get("maxConnections").unwrap(), 8);
    }

    #[test]
    fn trojan_grpc_config_is_generated() {
        let mut node = sample_vless_reality();
        node.protocol = Protocol::Trojan;
        node.password = Some("secret".into());
        node.uuid = None;
        node.security = Security::Tls;
        node.transport = Transport::Grpc;
        node.service_name = Some("MyService".into());
        node.sni = Some("sni.example.com".into());

        let cfg = generate_config(&node, InboundPorts::default(), &[], GeoAssetStatus::new(true)).unwrap();
        let value: Value = serde_json::from_str(&cfg.json).unwrap();
        let outbounds = value.get("outbounds").unwrap().as_array().unwrap();
        let proxy = outbounds
            .iter()
            .find(|o| o.get("tag").unwrap() == "proxy")
            .unwrap();
        assert_eq!(proxy.get("protocol").unwrap(), "trojan");
        let stream = proxy.get("streamSettings").unwrap();
        assert_eq!(stream.get("network").unwrap(), "grpc");
        assert_eq!(
            stream
                .get("grpcSettings")
                .unwrap()
                .get("serviceName")
                .unwrap(),
            "MyService"
        );
    }

    #[test]
    fn domain_rule_appears_in_routing() {
        use ironpass_core::models::{SplitTunnelAction, SplitTunnelTarget};
        let node = sample_vless_reality();
        let rules = vec![SplitTunnelRule::new(
            SplitTunnelTarget::Domain,
            "example.com",
            SplitTunnelAction::Direct,
            None,
        )];
        let cfg = generate_config(&node, InboundPorts::default(), &rules, GeoAssetStatus::new(true)).unwrap();
        let value: Value = serde_json::from_str(&cfg.json).unwrap();
        let routing_rules = value["routing"]["rules"].as_array().unwrap();
        // default geoip:private + geosite:ads + 1 custom rule
        assert_eq!(routing_rules.len(), 3);
        let rule = &routing_rules[2];
        assert_eq!(rule.get("outboundTag").unwrap(), "direct");
        let domains = rule.get("domain").unwrap().as_array().unwrap();
        assert!(domains.iter().any(|d| d == "example.com"));
    }

    #[test]
    fn fallback_routing_uses_private_cidrs_and_localhost() {
        let node = sample_vless_reality();
        let cfg = generate_config(
            &node,
            InboundPorts::default(),
            &[],
            GeoAssetStatus::new(false),
        )
        .unwrap();
        let value: Value = serde_json::from_str(&cfg.json).unwrap();
        let routing_rules = value["routing"]["rules"].as_array().unwrap();
        assert_eq!(routing_rules.len(), 2);
        assert_eq!(routing_rules[0].get("outboundTag").unwrap(), "direct");
        let ips = routing_rules[0]["ip"].as_array().unwrap();
        assert!(ips.iter().any(|v| v == "10.0.0.0/8"));
        assert!(ips.iter().any(|v| v == "fc00::/7"));
        let domains = routing_rules[1]["domain"].as_array().unwrap();
        assert!(domains.iter().any(|v| v == "localhost"));
    }

    #[test]
    fn geo_routing_uses_geoip_and_geosite() {
        let node = sample_vless_reality();
        let cfg = generate_config(
            &node,
            InboundPorts::default(),
            &[],
            GeoAssetStatus::new(true),
        )
        .unwrap();
        let value: Value = serde_json::from_str(&cfg.json).unwrap();
        let routing_rules = value["routing"]["rules"].as_array().unwrap();
        assert_eq!(routing_rules.len(), 2);
        let ips = routing_rules[0]["ip"].as_array().unwrap();
        assert!(ips.iter().any(|v| v == "geoip:private"));
        let domains = routing_rules[1]["domain"].as_array().unwrap();
        assert!(domains.iter().any(|v| v == "geosite:category-ads-all"));
    }

    #[test]
    fn wildcard_domain_uses_domain_prefix() {
        use ironpass_core::models::{SplitTunnelAction, SplitTunnelTarget};
        let node = sample_vless_reality();
        let rules = vec![SplitTunnelRule::new(
            SplitTunnelTarget::Domain,
            "*.example.com",
            SplitTunnelAction::Proxy,
            None,
        )];
        let cfg = generate_config(&node, InboundPorts::default(), &rules, GeoAssetStatus::new(true)).unwrap();
        let value: Value = serde_json::from_str(&cfg.json).unwrap();
        let routing_rules = value["routing"]["rules"].as_array().unwrap();
        let rule = &routing_rules[2];
        assert_eq!(rule.get("outboundTag").unwrap(), "proxy");
        let domains = rule.get("domain").unwrap().as_array().unwrap();
        assert!(domains.iter().any(|d| d == "domain:example.com"));
    }

    #[test]
    fn ip_and_cidr_rules_use_ip_field() {
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
        let cfg = generate_config(&node, InboundPorts::default(), &rules, GeoAssetStatus::new(true)).unwrap();
        let value: Value = serde_json::from_str(&cfg.json).unwrap();
        let routing_rules = value["routing"]["rules"].as_array().unwrap();
        let first = &routing_rules[2];
        assert_eq!(first.get("outboundTag").unwrap(), "direct");
        let ips = first.get("ip").unwrap().as_array().unwrap();
        assert!(ips.iter().any(|v| v == "1.2.3.4"));
        let second = &routing_rules[3];
        assert_eq!(second.get("outboundTag").unwrap(), "proxy");
        let ips = second.get("ip").unwrap().as_array().unwrap();
        assert!(ips.iter().any(|v| v == "10.0.0.0/8"));
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
        let cfg = generate_config(&node, InboundPorts::default(), &rules, GeoAssetStatus::new(true)).unwrap();
        let value: Value = serde_json::from_str(&cfg.json).unwrap();
        let routing_rules = value["routing"]["rules"].as_array().unwrap();
        assert_eq!(routing_rules.len(), 2);
    }
}
