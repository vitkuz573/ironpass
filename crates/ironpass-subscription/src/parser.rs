use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use ironpass_core::{Error, Result, models::*};
use regex::Regex;

fn decode_name(raw: &str) -> String {
    percent_decode(raw)
}

fn percent_decode(input: &str) -> String {
    let decoded: String = url::form_urlencoded::parse(input.as_bytes())
        .map(|(k, _)| k.into_owned())
        .collect();
    if decoded.is_empty() && !input.is_empty() {
        input.to_string()
    } else {
        decoded
    }
}

pub struct SubscriptionParser;

impl SubscriptionParser {
    pub fn new() -> Self {
        Self
    }

    pub fn detect_format(&self, input: &str) -> SubscriptionFormat {
        let trimmed = input.trim();

        if (trimmed.starts_with("{") || trimmed.starts_with("["))
            && (trimmed.contains("\"outbounds\"") || trimmed.contains("\"inbounds\""))
        {
            return SubscriptionFormat::SingBoxJson;
        }

        if trimmed.contains("proxies:") && trimmed.contains("proxy-groups:") {
            return SubscriptionFormat::ClashYaml;
        }

        if let Ok(decoded) = STANDARD.decode(trimmed)
            && let Ok(text) = String::from_utf8(decoded)
            && text.lines().any(|l| {
                l.starts_with("vless://")
                    || l.starts_with("vmess://")
                    || l.starts_with("trojan://")
                    || l.starts_with("ss://")
                    || l.starts_with("hysteria2://")
                    || l.starts_with("tuic://")
            })
        {
            return SubscriptionFormat::Base64VlessList;
        }

        if trimmed.lines().any(|l| {
            l.starts_with("vless://")
                || l.starts_with("vmess://")
                || l.starts_with("trojan://")
                || l.starts_with("ss://")
        }) {
            return SubscriptionFormat::RawUriList;
        }

        SubscriptionFormat::Unknown
    }

    pub fn parse(&self, input: &str) -> Result<Vec<ProxyNode>> {
        let format = self.detect_format(input);

        match format {
            SubscriptionFormat::Base64VlessList => self.parse_base64_list(input),
            SubscriptionFormat::ClashYaml => self.parse_clash_yaml(input),
            SubscriptionFormat::SingBoxJson => self.parse_singbox_json(input),
            SubscriptionFormat::RawUriList => self.parse_raw_list(input),
            SubscriptionFormat::Unknown => {
                Err(Error::Parse("Unable to detect subscription format".into()))
            }
        }
    }

    fn parse_base64_list(&self, input: &str) -> Result<Vec<ProxyNode>> {
        let decoded = STANDARD.decode(input.trim())?;
        let text = String::from_utf8(decoded)
            .map_err(|e| Error::Parse(format!("Invalid UTF-8 in base64: {}", e)))?;
        self.parse_raw_list(&text)
    }

    fn parse_raw_list(&self, input: &str) -> Result<Vec<ProxyNode>> {
        let mut nodes = Vec::new();

        for line in input.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            if let Ok(node) = self.parse_uri(line) {
                nodes.push(node);
            }
        }

        if nodes.is_empty() {
            return Err(Error::Parse("No valid proxy nodes found".into()));
        }

        Ok(nodes)
    }

    fn parse_uri(&self, uri: &str) -> Result<ProxyNode> {
        if uri.starts_with("vless://") {
            self.parse_vless(uri)
        } else if uri.starts_with("vmess://") {
            self.parse_vmess(uri)
        } else if uri.starts_with("trojan://") {
            self.parse_trojan(uri)
        } else if uri.starts_with("ss://") {
            self.parse_shadowsocks(uri)
        } else {
            Err(Error::UnsupportedProtocol(
                uri[..uri.find("://").unwrap_or(0)].to_string(),
            ))
        }
    }

    fn parse_vless(&self, uri: &str) -> Result<ProxyNode> {
        let re = Regex::new(
            r"^vless://(?P<uuid>[^@]+)@(?P<server>[^:]+):(?P<port>\d+)(?:\?(?P<params>[^#]*))?(?:#(?P<name>.+))?$"
        ).map_err(|e| Error::Parse(e.to_string()))?;

        let caps = re
            .captures(uri)
            .ok_or_else(|| Error::Parse("Invalid VLESS URI".into()))?;

        let params = url::form_urlencoded::parse(
            caps.name("params")
                .map(|m| m.as_str())
                .unwrap_or("")
                .as_bytes(),
        )
        .collect::<Vec<_>>();

        let get_param = |name: &str| -> Option<String> {
            params
                .iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.to_string())
        };

        Ok(ProxyNode {
            protocol: Protocol::Vless,
            name: decode_name(caps.name("name").map(|m| m.as_str()).unwrap_or("unnamed")),
            server: caps.name("server").unwrap().as_str().to_string(),
            port: caps
                .name("port")
                .unwrap()
                .as_str()
                .parse::<u16>()
                .map_err(|e| Error::Parse(e.to_string()))?,
            uuid: Some(caps.name("uuid").unwrap().as_str().to_string()),
            password: None,
            alter_id: None,
            encryption: get_param("encryption").or_else(|| Some("none".into())),
            transport: parse_transport(&get_param("type").unwrap_or_default()),
            security: parse_security(&get_param("security").unwrap_or_default()),
            flow: get_param("flow"),
            sni: get_param("sni"),
            fingerprint: get_param("fp"),
            public_key: get_param("pbk"),
            short_id: get_param("sid"),
            spider_x: get_param("spx"),
            path: get_param("path"),
            host: get_param("host"),
            service_name: get_param("serviceName"),
            alpn: get_param("alpn").map(|a| a.split(',').map(String::from).collect()),
            extra: None,
            tags: Vec::new(),
            raw_uri: uri.to_string(),
        })
    }

    fn parse_vmess(&self, uri: &str) -> Result<ProxyNode> {
        let encoded = uri
            .strip_prefix("vmess://")
            .ok_or_else(|| Error::Parse("Invalid VMess URI".into()))?;

        let decoded = STANDARD.decode(encoded)?;
        let json: serde_json::Value = serde_json::from_slice(&decoded)?;

        Ok(ProxyNode {
            protocol: Protocol::Vmess,
            name: decode_name(json["ps"].as_str().unwrap_or("unnamed")),
            server: json["add"].as_str().unwrap_or("").to_string(),
            port: json["port"]
                .as_str()
                .and_then(|s| s.parse().ok())
                .or_else(|| json["port"].as_u64().map(|p| p as u16))
                .unwrap_or(443),
            uuid: json["id"].as_str().map(String::from),
            password: None,
            alter_id: json["aid"]
                .as_str()
                .and_then(|s| s.parse().ok())
                .or_else(|| json["aid"].as_u64().map(|a| a as u32)),
            encryption: json["scy"].as_str().map(String::from),
            transport: parse_transport(json["net"].as_str().unwrap_or("tcp")),
            security: parse_security(json["tls"].as_str().unwrap_or("none")),
            flow: json["flow"].as_str().map(String::from),
            sni: json["sni"].as_str().map(String::from),
            fingerprint: json["fp"].as_str().map(String::from),
            public_key: None,
            short_id: None,
            spider_x: json["spx"].as_str().map(String::from),
            path: json["path"].as_str().map(String::from),
            host: json["host"].as_str().map(String::from),
            service_name: json["path"].as_str().map(String::from),
            alpn: json["alpn"]
                .as_str()
                .map(|a| a.split(',').map(String::from).collect()),
            extra: None,
            tags: Vec::new(),
            raw_uri: uri.to_string(),
        })
    }

    fn parse_trojan(&self, uri: &str) -> Result<ProxyNode> {
        let re = Regex::new(
            r"^trojan://(?P<password>[^@]+)@(?P<server>[^:]+):(?P<port>\d+)(?:\?(?P<params>[^#]*))?(?:#(?P<name>.+))?$"
        ).map_err(|e| Error::Parse(e.to_string()))?;

        let caps = re
            .captures(uri)
            .ok_or_else(|| Error::Parse("Invalid Trojan URI".into()))?;

        let params = url::form_urlencoded::parse(
            caps.name("params")
                .map(|m| m.as_str())
                .unwrap_or("")
                .as_bytes(),
        )
        .collect::<Vec<_>>();

        let get_param = |name: &str| -> Option<String> {
            params
                .iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.to_string())
        };

        Ok(ProxyNode {
            protocol: Protocol::Trojan,
            name: decode_name(caps.name("name").map(|m| m.as_str()).unwrap_or("unnamed")),
            server: caps.name("server").unwrap().as_str().to_string(),
            port: caps
                .name("port")
                .unwrap()
                .as_str()
                .parse::<u16>()
                .map_err(|e| Error::Parse(e.to_string()))?,
            uuid: None,
            password: Some(caps.name("password").unwrap().as_str().to_string()),
            alter_id: None,
            encryption: None,
            transport: parse_transport(&get_param("type").unwrap_or_default()),
            security: parse_security(&get_param("security").unwrap_or("tls".to_string())),
            flow: None,
            sni: get_param("sni"),
            fingerprint: get_param("fp"),
            public_key: None,
            short_id: None,
            spider_x: None,
            path: get_param("path"),
            host: get_param("host"),
            service_name: get_param("serviceName"),
            alpn: get_param("alpn").map(|a| a.split(',').map(String::from).collect()),
            extra: None,
            tags: Vec::new(),
            raw_uri: uri.to_string(),
        })
    }

    fn parse_shadowsocks(&self, uri: &str) -> Result<ProxyNode> {
        let encoded = uri
            .strip_prefix("ss://")
            .ok_or_else(|| Error::Parse("Invalid SS URI".into()))?;

        let parts: Vec<&str> = encoded.split('#').collect();
        let name = parts.get(1).unwrap_or(&"unnamed");
        let rest = parts[0];

        let decoded = STANDARD.decode(rest).or_else(|_| {
            let pad_len = (4 - rest.len() % 4) % 4;
            let padded = format!("{}{}", rest, "=".repeat(pad_len));
            STANDARD.decode(padded)
        })?;

        let text = String::from_utf8(decoded)
            .map_err(|e| Error::Parse(format!("Invalid UTF-8: {}", e)))?;

        let (auth, server_part) = if text.contains('@') {
            let mut parts = text.splitn(2, '@');
            (
                Some(parts.next().unwrap().to_string()),
                parts.next().unwrap(),
            )
        } else {
            (None, text.as_str())
        };

        let re = Regex::new(r"^(?P<server>[^:]+):(?P<port>\d+)$")
            .map_err(|e| Error::Parse(e.to_string()))?;

        let caps = re
            .captures(server_part)
            .ok_or_else(|| Error::Parse("Invalid SS server format".into()))?;

        Ok(ProxyNode {
            protocol: Protocol::Shadowsocks,
            name: decode_name(name),
            server: caps.name("server").unwrap().as_str().to_string(),
            port: caps
                .name("port")
                .unwrap()
                .as_str()
                .parse::<u16>()
                .map_err(|e| Error::Parse(e.to_string()))?,
            uuid: auth.clone(),
            password: auth,
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
            tags: Vec::new(),
            raw_uri: uri.to_string(),
        })
    }

    fn parse_clash_yaml(&self, input: &str) -> Result<Vec<ProxyNode>> {
        let yaml: serde_yaml::Value = serde_yaml::from_str(input)
            .map_err(|e| Error::Parse(format!("Invalid Clash YAML: {}", e)))?;

        let proxies = yaml["proxies"]
            .as_sequence()
            .ok_or_else(|| Error::Parse("No 'proxies' in Clash config".into()))?;

        let mut nodes = Vec::new();

        for proxy in proxies {
            if let Some(node) = self.clash_proxy_to_node(proxy) {
                nodes.push(node);
            }
        }

        Ok(nodes)
    }

    fn clash_proxy_to_node(&self, proxy: &serde_yaml::Value) -> Option<ProxyNode> {
        let name = proxy["name"].as_str()?.to_string();
        let proxy_type = proxy["type"].as_str()?;
        let server = proxy["server"].as_str()?.to_string();
        let port = proxy["port"].as_u64()? as u16;

        let protocol = match proxy_type {
            "ss" => Protocol::Shadowsocks,
            "vmess" => Protocol::Vmess,
            "vless" => Protocol::Vless,
            "trojan" => Protocol::Trojan,
            "hysteria2" | "hy2" => Protocol::Hysteria2,
            "tuic" => Protocol::Tuic,
            _ => return None,
        };

        let uuid = proxy["uuid"]
            .as_str()
            .map(String::from)
            .or_else(|| proxy["password"].as_str().map(String::from));

        let transport = match proxy["network"].as_str().unwrap_or("tcp") {
            "ws" => Transport::Ws,
            "grpc" => Transport::Grpc,
            "h2" => Transport::H2,
            "tcp" => Transport::Tcp,
            _ => Transport::Tcp,
        };

        let security = if proxy["tls"].as_bool().unwrap_or(false) {
            Security::Tls
        } else {
            Security::None
        };

        Some(ProxyNode {
            protocol,
            name,
            server,
            port,
            uuid,
            password: proxy["password"].as_str().map(String::from),
            alter_id: proxy["aid"].as_u64().map(|a| a as u32),
            encryption: proxy["cipher"].as_str().map(String::from),
            transport,
            security,
            flow: proxy["flow"].as_str().map(String::from),
            sni: proxy["sni"].as_str().map(String::from),
            fingerprint: proxy["client-fingerprint"].as_str().map(String::from),
            public_key: proxy["public-key"].as_str().map(String::from),
            short_id: proxy["short-id"].as_str().map(String::from),
            spider_x: None,
            path: proxy["ws-path"]
                .as_str()
                .or(proxy["path"].as_str())
                .map(String::from),
            host: proxy["ws-opts"]["headers"]["Host"]
                .as_str()
                .or(proxy["server-name"].as_str())
                .map(String::from),
            service_name: proxy["grpc-opts"]["grpc-service-name"]
                .as_str()
                .map(String::from),
            alpn: proxy["alpn"].as_sequence().map(|seq| {
                seq.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            }),
            extra: None,
            tags: Vec::new(),
            raw_uri: String::new(),
        })
    }

    fn parse_singbox_json(&self, input: &str) -> Result<Vec<ProxyNode>> {
        let json: serde_json::Value = serde_json::from_str(input)
            .map_err(|e| Error::Parse(format!("Invalid sing-box JSON: {}", e)))?;

        let outbounds = json["outbounds"]
            .as_array()
            .ok_or_else(|| Error::Parse("No 'outbounds' in sing-box config".into()))?;

        let mut nodes = Vec::new();

        for outbound in outbounds {
            if let Some(node) = self.singbox_outbound_to_node(outbound) {
                nodes.push(node);
            }
        }

        Ok(nodes)
    }

    fn singbox_outbound_to_node(&self, outbound: &serde_json::Value) -> Option<ProxyNode> {
        let tag = outbound["tag"].as_str()?.to_string();
        let protocol = outbound["type"].as_str()?;

        let (server, port) = match (
            outbound["server"].as_str(),
            outbound["server_port"].as_u64(),
        ) {
            (Some(s), Some(p)) => (s.to_string(), p as u16),
            _ => return None,
        };

        let transport = match outbound["transport"]["type"].as_str().unwrap_or("tcp") {
            "ws" => Transport::Ws,
            "grpc" => Transport::Grpc,
            "http" => Transport::H2,
            "splithttp" | "xhttp" => Transport::Xhttp,
            _ => Transport::Tcp,
        };

        let security = match outbound["tls"]["enabled"].as_bool().unwrap_or(false) {
            true if outbound["tls"]["reality"].as_object().is_some() => Security::Reality,
            true => Security::Tls,
            false => Security::None,
        };

        Some(ProxyNode {
            protocol: match protocol {
                "vless" => Protocol::Vless,
                "vmess" => Protocol::Vmess,
                "trojan" => Protocol::Trojan,
                "shadowsocks" => Protocol::Shadowsocks,
                "hysteria2" => Protocol::Hysteria2,
                "tuic" => Protocol::Tuic,
                _ => return None,
            },
            name: tag,
            server,
            port,
            uuid: outbound["uuid"].as_str().map(String::from),
            password: outbound["password"].as_str().map(String::from),
            alter_id: outbound["alter_id"].as_u64().map(|a| a as u32),
            encryption: outbound["method"].as_str().map(String::from),
            transport,
            security,
            flow: outbound["flow"].as_str().map(String::from),
            sni: outbound["tls"]["server_name"].as_str().map(String::from),
            fingerprint: outbound["tls"]["utls"]["enabled"]
                .as_bool()
                .and_then(|_| outbound["tls"]["utls"]["fingerprint"].as_str())
                .map(String::from),
            public_key: outbound["tls"]["reality"]["public_key"]
                .as_str()
                .map(String::from),
            short_id: outbound["tls"]["reality"]["short_id"]
                .as_str()
                .map(String::from),
            spider_x: None,
            path: outbound["transport"]["path"].as_str().map(String::from),
            host: outbound["transport"]["headers"]["Host"]
                .as_str()
                .map(String::from),
            service_name: outbound["transport"]["service_name"]
                .as_str()
                .map(String::from),
            alpn: outbound["tls"]["alpn"].as_array().map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            }),
            extra: None,
            tags: Vec::new(),
            raw_uri: String::new(),
        })
    }
}

fn parse_transport(s: &str) -> Transport {
    match s {
        "ws" => Transport::Ws,
        "grpc" => Transport::Grpc,
        "h2" => Transport::H2,
        "xhttp" => Transport::Xhttp,
        "splithttp" => Transport::Splithttp,
        "kcp" => Transport::Kcp,
        _ => Transport::Tcp,
    }
}

fn parse_security(s: &str) -> Security {
    match s {
        "tls" => Security::Tls,
        "reality" => Security::Reality,
        "reality_psk" => Security::RealityPsk,
        _ => Security::None,
    }
}

impl Default for SubscriptionParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ironpass_core::models::{Protocol, Security, SubscriptionFormat, Transport};

    fn parser() -> SubscriptionParser {
        SubscriptionParser::new()
    }

    #[test]
    fn detect_format_base64_vless_list() {
        let raw = "vless://uuid@example.com:443?encryption=none#Test";
        let encoded = STANDARD.encode(raw.as_bytes());
        assert_eq!(
            parser().detect_format(&encoded),
            SubscriptionFormat::Base64VlessList
        );
    }

    #[test]
    fn detect_format_raw_uri_list() {
        let input = "vless://uuid@example.com:443?encryption=none#Test\nss://method:pass@example.com:1080#SS";
        assert_eq!(
            parser().detect_format(input),
            SubscriptionFormat::RawUriList
        );
    }

    #[test]
    fn detect_format_clash_yaml() {
        let input = "proxies:\n  - name: p\n    type: ss\nproxy-groups:\n  - name: g\n";
        assert_eq!(parser().detect_format(input), SubscriptionFormat::ClashYaml);
    }

    #[test]
    fn detect_format_singbox_json() {
        let input = r#"{"outbounds": [], "inbounds": []}"#;
        assert_eq!(
            parser().detect_format(input),
            SubscriptionFormat::SingBoxJson
        );
    }

    #[test]
    fn detect_format_unknown() {
        let input = "just some random text";
        assert_eq!(parser().detect_format(input), SubscriptionFormat::Unknown);
    }

    #[test]
    fn detect_format_unknown_empty() {
        assert_eq!(parser().detect_format(""), SubscriptionFormat::Unknown);
    }

    #[test]
    fn parse_base64_list_valid() {
        let raw = "vless://550e8400-e29b-41d4-a716-446655440000@example.com:443?encryption=none&security=tls&sni=example.com&fp=chrome&type=ws&path=%2Fchat&host=example.com#Test";
        let encoded = STANDARD.encode(raw.as_bytes());
        let nodes = parser().parse_base64_list(&encoded).unwrap();
        assert_eq!(nodes.len(), 1);
        let n = &nodes[0];
        assert_eq!(n.protocol, Protocol::Vless);
        assert_eq!(n.server, "example.com");
        assert_eq!(n.port, 443);
        assert_eq!(n.name, "Test");
    }

    #[test]
    fn parse_base64_list_invalid_base64() {
        let result = parser().parse_base64_list("not-valid-base64!!!");
        assert!(matches!(result, Err(Error::Base64(_))));
    }

    #[test]
    fn parse_raw_list_vless_vmess_trojan_ss() {
        // Note: current parser expects SS URI base64 payload to include server:port and no '@'.
        let input = r#"
vless://550e8400-e29b-41d4-a716-446655440000@example.com:443?encryption=none#Vless
vmess://eyJhZGQiOiJleGFtcGxlLmNvbSIsInBvcnQiOiI0NDMiLCJpZCI6IjU1MGU4NDAwLWUyOWItNDFkNC1hNzE2LTQ0NjY1NTQ0MDAwMCIsImFpZCI6IjAiLCJwcyI6IlZtZXNzIn0=
trojan://password@example.com:443?sni=example.com#Trojan
ss://ZXhhbXBsZS5jb206MTA4MA==#SS
"#;
        let nodes = parser().parse_raw_list(input).unwrap();
        assert_eq!(nodes.len(), 4);
        assert_eq!(nodes[0].protocol, Protocol::Vless);
        assert_eq!(nodes[1].protocol, Protocol::Vmess);
        assert_eq!(nodes[2].protocol, Protocol::Trojan);
        assert_eq!(nodes[3].protocol, Protocol::Shadowsocks);
    }

    #[test]
    fn parse_raw_list_unsupported_protocol_is_skipped() {
        let input =
            "hysteria2://pass@example.com:443#H\nvless://uuid@example.com:443?encryption=none#V";
        let nodes = parser().parse_raw_list(input).unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].protocol, Protocol::Vless);
    }

    #[test]
    fn parse_raw_list_empty() {
        let result = parser().parse_raw_list("");
        assert!(matches!(result, Err(Error::Parse(_))));
    }

    #[test]
    fn parse_raw_list_no_valid_uris() {
        let result = parser().parse_raw_list("hello world\nnothing here");
        assert!(matches!(result, Err(Error::Parse(_))));
    }

    #[test]
    fn parse_vless_full_params() {
        let uri = "vless://550e8400-e29b-41d4-a716-446655440000@example.com:443?encryption=none&security=reality&sni=example.com&fp=chrome&pbk=FakePublicKey&sid=FakeShortID&flow=xtls-rprx-vision&type=ws&path=%2Fchat&host=cdn.example.com&alpn=h2%2Chttp%2F1.1#My%20Node";
        let n = parser().parse_vless(uri).unwrap();
        assert_eq!(n.protocol, Protocol::Vless);
        assert_eq!(n.server, "example.com");
        assert_eq!(n.port, 443);
        assert_eq!(
            n.uuid.as_deref().unwrap(),
            "550e8400-e29b-41d4-a716-446655440000"
        );
        assert_eq!(n.encryption.as_deref().unwrap(), "none");
        assert_eq!(n.security, Security::Reality);
        assert_eq!(n.sni.as_deref().unwrap(), "example.com");
        assert_eq!(n.fingerprint.as_deref().unwrap(), "chrome");
        assert_eq!(n.public_key.as_deref().unwrap(), "FakePublicKey");
        assert_eq!(n.short_id.as_deref().unwrap(), "FakeShortID");
        assert_eq!(n.flow.as_deref().unwrap(), "xtls-rprx-vision");
        assert_eq!(n.transport, Transport::Ws);
        assert_eq!(n.path.as_deref().unwrap(), "/chat");
        assert_eq!(n.host.as_deref().unwrap(), "cdn.example.com");
        assert_eq!(n.alpn.as_ref().unwrap(), &vec!["h2", "http/1.1"]);
        assert_eq!(n.name, "My Node");
    }

    #[test]
    fn parse_vless_defaults() {
        let uri = "vless://uuid@example.com:443?encryption=auto#Default";
        let n = parser().parse_vless(uri).unwrap();
        assert_eq!(n.encryption.as_deref().unwrap(), "auto");
        assert_eq!(n.security, Security::None);
        assert_eq!(n.transport, Transport::Tcp);
    }

    #[test]
    fn parse_vless_invalid_uri() {
        let result = parser().parse_vless("vless://nope");
        assert!(matches!(result, Err(Error::Parse(_))));
    }

    #[test]
    fn parse_vmess_base64_json() {
        let json = r#"{"add":"example.com","port":"443","id":"550e8400-e29b-41d4-a716-446655440000","aid":"0","scy":"auto","net":"ws","tls":"tls","host":"cdn.example.com","path":"/chat","sni":"example.com","fp":"chrome","ps":"Vmess%20Node"}"#;
        let encoded = STANDARD.encode(json.as_bytes());
        let uri = format!("vmess://{}", encoded);
        let n = parser().parse_vmess(&uri).unwrap();
        assert_eq!(n.protocol, Protocol::Vmess);
        assert_eq!(n.server, "example.com");
        assert_eq!(n.port, 443);
        assert_eq!(
            n.uuid.as_deref().unwrap(),
            "550e8400-e29b-41d4-a716-446655440000"
        );
        assert_eq!(n.transport, Transport::Ws);
        assert_eq!(n.security, Security::Tls);
        assert_eq!(n.host.as_deref().unwrap(), "cdn.example.com");
        assert_eq!(n.path.as_deref().unwrap(), "/chat");
        assert_eq!(n.sni.as_deref().unwrap(), "example.com");
        assert_eq!(n.fingerprint.as_deref().unwrap(), "chrome");
        assert_eq!(n.name, "Vmess Node");
    }

    #[test]
    fn parse_vmess_invalid_base64() {
        let result = parser().parse_vmess("vmess://not-base64!!!");
        assert!(matches!(result, Err(Error::Base64(_))));
    }

    #[test]
    fn parse_vmess_missing_required_defaults() {
        let json = r#"{"ps":"Minimal"}"#;
        let uri = format!("vmess://{}", STANDARD.encode(json.as_bytes()));
        let n = parser().parse_vmess(&uri).unwrap();
        assert_eq!(n.server, "");
        assert_eq!(n.port, 443);
        assert_eq!(n.uuid, None);
    }

    #[test]
    fn parse_trojan_full_params() {
        let uri = "trojan://password@example.com:443?sni=example.com&fp=firefox&type=ws&path=%2Fchat&host=cdn.example.com&alpn=h2#Trojan%20Node";
        let n = parser().parse_trojan(uri).unwrap();
        assert_eq!(n.protocol, Protocol::Trojan);
        assert_eq!(n.server, "example.com");
        assert_eq!(n.port, 443);
        assert_eq!(n.password.as_deref().unwrap(), "password");
        assert_eq!(n.sni.as_deref().unwrap(), "example.com");
        assert_eq!(n.fingerprint.as_deref().unwrap(), "firefox");
        assert_eq!(n.transport, Transport::Ws);
        assert_eq!(n.path.as_deref().unwrap(), "/chat");
        assert_eq!(n.host.as_deref().unwrap(), "cdn.example.com");
        assert_eq!(n.alpn.as_ref().unwrap(), &vec!["h2"]);
        assert_eq!(n.name, "Trojan Node");
        assert_eq!(n.security, Security::Tls);
    }

    #[test]
    fn parse_trojan_default_security_tls() {
        let uri = "trojan://password@example.com:443#Default";
        let n = parser().parse_trojan(uri).unwrap();
        assert_eq!(n.security, Security::Tls);
    }

    #[test]
    fn parse_trojan_invalid_uri() {
        let result = parser().parse_trojan("trojan://bad");
        assert!(matches!(result, Err(Error::Parse(_))));
    }

    #[test]
    fn parse_shadowsocks_with_auth() {
        // Current parser expects the base64 payload to contain auth@server:port with no
        // separate '@' separator before the server in the URI.
        let auth = STANDARD.encode("chacha20-ietf-poly1305:password@example.com:1080".as_bytes());
        let uri = format!("ss://{}#SS%20Node", auth);
        let n = parser().parse_shadowsocks(&uri).unwrap();
        assert_eq!(n.protocol, Protocol::Shadowsocks);
        assert_eq!(n.server, "example.com");
        assert_eq!(n.port, 1080);
        assert_eq!(
            n.uuid.as_deref().unwrap(),
            "chacha20-ietf-poly1305:password"
        );
        assert_eq!(
            n.password.as_deref().unwrap(),
            "chacha20-ietf-poly1305:password"
        );
        assert_eq!(n.name, "SS Node");
    }

    #[test]
    fn parse_shadowsocks_without_auth() {
        let payload = STANDARD.encode("example.com:1080".as_bytes());
        let uri = format!("ss://{}", payload);
        let n = parser().parse_shadowsocks(&uri).unwrap();
        assert_eq!(n.protocol, Protocol::Shadowsocks);
        assert_eq!(n.server, "example.com");
        assert_eq!(n.port, 1080);
        assert!(n.uuid.is_none());
    }

    #[test]
    fn parse_shadowsocks_invalid_server() {
        let payload = STANDARD.encode("nope".as_bytes());
        let uri = format!("ss://{}#Name", payload);
        let result = parser().parse_shadowsocks(&uri);
        assert!(matches!(result, Err(Error::Parse(_))));
    }

    #[test]
    fn parse_shadowsocks_sip002_format_with_base64_only_userinfo() {
        // SIP002 style: ss://base64(user:pass)@server:port. Current parser base64-decodes
        // the whole segment before '#', so '@' before the server makes base64 invalid.
        let uri = "ss://Y2hhY2hhMjAtaWV0Zi1wb2x5MTMwNTpwYXNzd29yZA==@example.com:1080#Name";
        let result = parser().parse_shadowsocks(uri);
        // Current implementation cannot parse SIP002 style; it decodes entire prefix.
        assert!(result.is_err());
    }

    #[test]
    fn parse_clash_yaml_sample() {
        let input = r#"
proxies:
  - name: "ss-proxy"
    type: ss
    server: example.com
    port: 1080
    cipher: chacha20-ietf-poly1305
    password: password
  - name: "vmess-proxy"
    type: vmess
    server: example.com
    port: 443
    uuid: 550e8400-e29b-41d4-a716-446655440000
    alterId: 0
    cipher: auto
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
        let nodes = parser().parse_clash_yaml(input).unwrap();
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0].protocol, Protocol::Shadowsocks);
        assert_eq!(nodes[0].name, "ss-proxy");
        assert_eq!(
            nodes[0].encryption.as_deref().unwrap(),
            "chacha20-ietf-poly1305"
        );
        assert_eq!(nodes[1].protocol, Protocol::Vmess);
        assert_eq!(nodes[1].transport, Transport::Ws);
        assert_eq!(nodes[1].security, Security::Tls);
        assert_eq!(nodes[1].path.as_deref().unwrap(), "/chat");
        assert_eq!(nodes[1].host.as_deref().unwrap(), "cdn.example.com");
    }

    #[test]
    fn parse_clash_yaml_no_proxies() {
        let input = "proxies:\nproxy-groups:\n";
        let result = parser().parse_clash_yaml(input);
        assert!(matches!(result, Err(Error::Parse(_))));
    }

    #[test]
    fn parse_singbox_json_sample() {
        let input = r#"
{
  "outbounds": [
    {
      "type": "vless",
      "tag": "vless-out",
      "server": "example.com",
      "server_port": 443,
      "uuid": "550e8400-e29b-41d4-a716-446655440000",
      "flow": "xtls-rprx-vision",
      "transport": {
        "type": "ws",
        "path": "/chat",
        "headers": {
          "Host": "cdn.example.com"
        }
      },
      "tls": {
        "enabled": true,
        "server_name": "example.com",
        "utls": {
          "enabled": true,
          "fingerprint": "chrome"
        }
      }
    },
    {
      "type": "shadowsocks",
      "tag": "ss-out",
      "server": "example.com",
      "server_port": 1080,
      "method": "chacha20-ietf-poly1305",
      "password": "password"
    }
  ]
}
"#;
        let nodes = parser().parse_singbox_json(input).unwrap();
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0].protocol, Protocol::Vless);
        assert_eq!(nodes[0].name, "vless-out");
        assert_eq!(nodes[0].transport, Transport::Ws);
        assert_eq!(nodes[0].security, Security::Tls);
        assert_eq!(nodes[0].sni.as_deref().unwrap(), "example.com");
        assert_eq!(nodes[0].fingerprint.as_deref().unwrap(), "chrome");
        assert_eq!(nodes[0].host.as_deref().unwrap(), "cdn.example.com");
        assert_eq!(nodes[1].protocol, Protocol::Shadowsocks);
        assert_eq!(nodes[1].name, "ss-out");
        assert_eq!(
            nodes[1].encryption.as_deref().unwrap(),
            "chacha20-ietf-poly1305"
        );
    }

    #[test]
    fn parse_singbox_json_reality() {
        let input = r#"{"outbounds":[{"type":"vless","tag":"r","server":"example.com","server_port":443,"uuid":"uuid","tls":{"enabled":true,"reality":{"public_key":"pbk","short_id":"sid"}}}]}"#;
        let nodes = parser().parse_singbox_json(input).unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].security, Security::Reality);
        assert_eq!(nodes[0].public_key.as_deref().unwrap(), "pbk");
        assert_eq!(nodes[0].short_id.as_deref().unwrap(), "sid");
    }

    #[test]
    fn parse_singbox_json_no_outbounds() {
        let input = r#"{"inbounds": []}"#;
        let result = parser().parse_singbox_json(input);
        assert!(matches!(result, Err(Error::Parse(_))));
    }

    #[test]
    fn parse_ipv6_server_not_supported_by_current_regex() {
        // Current VLESS/Trojan regexes use `[^:]+` for server, so brackets break parsing.
        let uri = "vless://uuid@[2001:db8::1]:443?encryption=none#IPv6";
        let result = parser().parse_vless(uri);
        // Documenting current behavior: IPv6 literal addresses fail.
        assert!(result.is_err());
    }

    #[test]
    fn parse_transport_and_security_helpers() {
        assert_eq!(parse_transport("ws"), Transport::Ws);
        assert_eq!(parse_transport("grpc"), Transport::Grpc);
        assert_eq!(parse_transport("h2"), Transport::H2);
        assert_eq!(parse_transport("xhttp"), Transport::Xhttp);
        assert_eq!(parse_transport("splithttp"), Transport::Splithttp);
        assert_eq!(parse_transport("kcp"), Transport::Kcp);
        assert_eq!(parse_transport("anything"), Transport::Tcp);

        assert_eq!(parse_security("tls"), Security::Tls);
        assert_eq!(parse_security("reality"), Security::Reality);
        assert_eq!(parse_security("reality_psk"), Security::RealityPsk);
        assert_eq!(parse_security("none"), Security::None);
    }
}
