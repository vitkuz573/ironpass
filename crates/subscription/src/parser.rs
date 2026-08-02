use ironpass_core::{Error, Result, models::*};
use regex::Regex;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;

fn decode_percent(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect::<String>()
        .replace("%20", " ")
        .replace("%3A", ":")
        .replace("%2C", ",")
        .replace("%23", "#")
        .replace("%26", "&")
        .replace("%3F", "?")
        .replace("%3D", "=")
        .replace("%2F", "/")
}

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

        if trimmed.starts_with("{") || trimmed.starts_with("[") {
            if trimmed.contains("\"outbounds\"") || trimmed.contains("\"inbounds\"") {
                return SubscriptionFormat::SingBoxJson;
            }
        }

        if trimmed.contains("proxies:") && trimmed.contains("proxy-groups:") {
            return SubscriptionFormat::ClashYaml;
        }

        if let Ok(decoded) = STANDARD.decode(trimmed) {
            if let Ok(text) = String::from_utf8(decoded) {
                if text.lines().any(|l| {
                    l.starts_with("vless://")
                        || l.starts_with("vmess://")
                        || l.starts_with("trojan://")
                        || l.starts_with("ss://")
                        || l.starts_with("hysteria2://")
                        || l.starts_with("tuic://")
                }) {
                    return SubscriptionFormat::Base64VlessList;
                }
            }
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
            SubscriptionFormat::Unknown => Err(Error::Parse("Unable to detect subscription format".into())),
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
            Err(Error::UnsupportedProtocol(uri[..uri.find("://").unwrap_or(0)].to_string()))
        }
    }

    fn parse_vless(&self, uri: &str) -> Result<ProxyNode> {
        let re = Regex::new(
            r"^vless://(?P<uuid>[^@]+)@(?P<server>[^:]+):(?P<port>\d+)(?:\?(?P<params>[^#]*))?(?:#(?P<name>.+))?$"
        ).map_err(|e| Error::Parse(e.to_string()))?;

        let caps = re.captures(uri).ok_or_else(|| Error::Parse("Invalid VLESS URI".into()))?;

        let params = url::form_urlencoded::parse(
            caps.name("params").map(|m| m.as_str()).unwrap_or("").as_bytes(),
        ).collect::<Vec<_>>();

        let get_param = |name: &str| -> Option<String> {
            params.iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.to_string())
        };

        Ok(ProxyNode {
            protocol: Protocol::Vless,
            name: decode_name(caps.name("name").map(|m| m.as_str()).unwrap_or("unnamed")),
            server: caps.name("server").unwrap().as_str().to_string(),
            port: caps.name("port").unwrap().as_str().parse::<u16>().map_err(|e| Error::Parse(e.to_string()))?,
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
            tags: Vec::new(),
            raw_uri: uri.to_string(),
        })
    }

    fn parse_vmess(&self, uri: &str) -> Result<ProxyNode> {
        let encoded = uri.strip_prefix("vmess://")
            .ok_or_else(|| Error::Parse("Invalid VMess URI".into()))?;

        let decoded = STANDARD.decode(encoded)?;
        let json: serde_json::Value = serde_json::from_slice(&decoded)?;

        Ok(ProxyNode {
            protocol: Protocol::Vmess,
            name: decode_name(json["ps"].as_str().unwrap_or("unnamed")),
            server: json["add"].as_str().unwrap_or("").to_string(),
            port: json["port"].as_str()
                .and_then(|s| s.parse().ok())
                .or_else(|| json["port"].as_u64().map(|p| p as u16))
                .unwrap_or(443),
            uuid: json["id"].as_str().map(String::from),
            password: None,
            alter_id: json["aid"].as_str()
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
            alpn: json["alpn"].as_str().map(|a| a.split(',').map(String::from).collect()),
            tags: Vec::new(),
            raw_uri: uri.to_string(),
        })
    }

    fn parse_trojan(&self, uri: &str) -> Result<ProxyNode> {
        let re = Regex::new(
            r"^trojan://(?P<password>[^@]+)@(?P<server>[^:]+):(?P<port>\d+)(?:\?(?P<params>[^#]*))?(?:#(?P<name>.+))?$"
        ).map_err(|e| Error::Parse(e.to_string()))?;

        let caps = re.captures(uri).ok_or_else(|| Error::Parse("Invalid Trojan URI".into()))?;

        let params = url::form_urlencoded::parse(
            caps.name("params").map(|m| m.as_str()).unwrap_or("").as_bytes(),
        ).collect::<Vec<_>>();

        let get_param = |name: &str| -> Option<String> {
            params.iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.to_string())
        };

        Ok(ProxyNode {
            protocol: Protocol::Trojan,
            name: decode_name(caps.name("name").map(|m| m.as_str()).unwrap_or("unnamed")),
            server: caps.name("server").unwrap().as_str().to_string(),
            port: caps.name("port").unwrap().as_str().parse::<u16>().map_err(|e| Error::Parse(e.to_string()))?,
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
            tags: Vec::new(),
            raw_uri: uri.to_string(),
        })
    }

    fn parse_shadowsocks(&self, uri: &str) -> Result<ProxyNode> {
        let encoded = uri.strip_prefix("ss://")
            .ok_or_else(|| Error::Parse("Invalid SS URI".into()))?;

        let parts: Vec<&str> = encoded.split('#').collect();
        let name = parts.get(1).unwrap_or(&"unnamed");
        let rest = parts[0];

        let decoded = STANDARD.decode(rest)
            .or_else(|_| {
                let pad_len = (4 - rest.len() % 4) % 4;
                let padded = format!("{}{}", rest, "=".repeat(pad_len));
                STANDARD.decode(padded)
            })?;

        let text = String::from_utf8(decoded)
            .map_err(|e| Error::Parse(format!("Invalid UTF-8: {}", e)))?;

        let (auth, server_part) = if text.contains('@') {
            let mut parts = text.splitn(2, '@');
            (Some(parts.next().unwrap().to_string()), parts.next().unwrap())
        } else {
            (None, text.as_str())
        };

        let re = Regex::new(r"^(?P<server>[^:]+):(?P<port>\d+)$")
            .map_err(|e| Error::Parse(e.to_string()))?;

        let caps = re.captures(server_part)
            .ok_or_else(|| Error::Parse("Invalid SS server format".into()))?;

        Ok(ProxyNode {
            protocol: Protocol::Shadowsocks,
            name: decode_name(name),
            server: caps.name("server").unwrap().as_str().to_string(),
            port: caps.name("port").unwrap().as_str().parse::<u16>().map_err(|e| Error::Parse(e.to_string()))?,
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

        let uuid = proxy["uuid"].as_str().map(String::from)
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
            path: proxy["ws-path"].as_str().or(proxy["path"].as_str()).map(String::from),
            host: proxy["ws-opts"]["headers"]["Host"].as_str()
                .or(proxy["server-name"].as_str())
                .map(String::from),
            service_name: proxy["grpc-opts"]["grpc-service-name"].as_str().map(String::from),
            alpn: proxy["alpn"].as_sequence()
                .map(|seq| seq.iter().filter_map(|v| v.as_str().map(String::from)).collect()),
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

        let (server, port) = match (outbound["server"].as_str(), outbound["server_port"].as_u64()) {
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
            fingerprint: outbound["tls"]["utls"]["enabled"].as_bool()
                .and_then(|_| outbound["tls"]["utls"]["fingerprint"].as_str())
                .map(String::from),
            public_key: outbound["tls"]["reality"]["public_key"].as_str().map(String::from),
            short_id: outbound["tls"]["reality"]["short_id"].as_str().map(String::from),
            spider_x: None,
            path: outbound["transport"]["path"].as_str().map(String::from),
            host: outbound["transport"]["headers"]["Host"].as_str().map(String::from),
            service_name: outbound["transport"]["service_name"].as_str().map(String::from),
            alpn: outbound["tls"]["alpn"].as_array()
                .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()),
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
