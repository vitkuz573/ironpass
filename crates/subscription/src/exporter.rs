use ironpass_core::{Result, models::*};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;

pub struct NodeExporterImpl;

impl NodeExporterImpl {
    pub fn new() -> Self {
        Self
    }
}

impl Default for NodeExporterImpl {
    fn default() -> Self {
        Self::new()
    }
}

impl ironpass_core::traits::NodeExporter for NodeExporterImpl {
    fn export(&self, nodes: &[ProxyNode], format: &OutputFormat) -> Result<String> {
        match format {
            OutputFormat::Clash => self.to_clash(nodes),
            OutputFormat::SingBox => self.to_singbox(nodes),
            OutputFormat::V2Ray => self.to_v2ray(nodes),
            OutputFormat::Raw => self.to_raw_uris(nodes),
            _ => Err(ironpass_core::Error::Custom(
                format!("Format {:?} not yet implemented", format),
            )),
        }
    }
}

impl NodeExporterImpl {
    fn to_clash(&self, nodes: &[ProxyNode]) -> Result<String> {
        let proxies: Vec<serde_json::Value> = nodes.iter().map(|n| {
            let mut proxy = serde_json::json!({
                "name": n.name,
                "type": match n.protocol {
                    Protocol::Vless => "vless",
                    Protocol::Vmess => "vmess",
                    Protocol::Trojan => "trojan",
                    Protocol::Shadowsocks => "ss",
                    Protocol::Hysteria2 => "hysteria2",
                    Protocol::Tuic => "tuic",
                    _ => "unknown",
                },
                "server": n.server,
                "port": n.port,
            });

            if let Some(ref uuid) = n.uuid {
                proxy["uuid"] = serde_json::Value::String(uuid.clone());
            }
            if let Some(ref password) = n.password {
                proxy["password"] = serde_json::Value::String(password.clone());
            }
            if let Some(ref alter_id) = n.alter_id {
                proxy["aid"] = serde_json::json!(alter_id);
            }
            if let Some(ref encryption) = n.encryption {
                proxy["cipher"] = serde_json::Value::String(encryption.clone());
            }
            if let Transport::Ws = n.transport {
                proxy["network"] = serde_json::json!("ws");
                if let Some(ref path) = n.path {
                    proxy["ws-path"] = serde_json::Value::String(path.clone());
                }
                if let Some(ref host) = n.host {
                    proxy["ws-opts"] = serde_json::json!({
                        "headers": { "Host": host }
                    });
                }
            }
            if let Transport::Grpc = n.transport {
                proxy["network"] = serde_json::json!("grpc");
                if let Some(ref sn) = n.service_name {
                    proxy["grpc-opts"] = serde_json::json!({
                        "grpc-service-name": sn
                    });
                }
            }
            if n.security != Security::None {
                proxy["tls"] = serde_json::json!(true);
                if let Some(ref sni) = n.sni {
                    proxy["sni"] = serde_json::Value::String(sni.clone());
                }
                if let Some(ref fp) = n.fingerprint {
                    proxy["client-fingerprint"] = serde_json::Value::String(fp.clone());
                }
            }

            proxy
        }).collect();

        let config = serde_json::json!({
            "proxies": proxies,
            "proxy-groups": [],
            "rules": [],
        });

        serde_yaml::to_string(&config)
            .map_err(|e| ironpass_core::Error::Custom(format!("YAML serialization: {}", e)))
    }

    fn to_singbox(&self, nodes: &[ProxyNode]) -> Result<String> {
        let outbounds: Vec<serde_json::Value> = nodes.iter().map(|n| {
            let mut ob = serde_json::json!({
                "type": match n.protocol {
                    Protocol::Vless => "vless",
                    Protocol::Vmess => "vmess",
                    Protocol::Trojan => "trojan",
                    Protocol::Shadowsocks => "shadowsocks",
                    Protocol::Hysteria2 => "hysteria2",
                    Protocol::Tuic => "tuic",
                    _ => "unknown",
                },
                "tag": n.name,
                "server": n.server,
                "server_port": n.port,
            });

            if let Some(ref uuid) = n.uuid {
                ob["uuid"] = serde_json::Value::String(uuid.clone());
            }
            if let Some(ref password) = n.password {
                ob["password"] = serde_json::Value::String(password.clone());
            }

            ob
        }).collect();

        let config = serde_json::json!({
            "outbounds": outbounds,
        });

        serde_json::to_string_pretty(&config)
            .map_err(|e| ironpass_core::Error::Custom(format!("JSON serialization: {}", e)))
    }

    fn to_v2ray(&self, nodes: &[ProxyNode]) -> Result<String> {
        let uris: Vec<String> = nodes.iter().map(|n| {
            if n.raw_uri.is_empty() {
                self.node_to_uri(n)
            } else {
                n.raw_uri.clone()
            }
        }).collect();

        let combined = uris.join("\n");
        Ok(STANDARD.encode(combined.as_bytes()))
    }

    fn to_raw_uris(&self, nodes: &[ProxyNode]) -> Result<String> {
        let uris: Vec<String> = nodes.iter().map(|n| {
            if n.raw_uri.is_empty() {
                self.node_to_uri(n)
            } else {
                n.raw_uri.clone()
            }
        }).collect();

        Ok(uris.join("\n"))
    }

    fn node_to_uri(&self, n: &ProxyNode) -> String {
        match n.protocol {
            Protocol::Vless => {
                let mut params = vec![format!("encryption={}", n.encryption.as_deref().unwrap_or("none"))];
                if n.security != Security::None {
                    params.push(format!("security={}", match n.security {
                        Security::Tls => "tls",
                        Security::Reality => "reality",
                        _ => "none",
                    }));
                }
                if let Transport::Ws = n.transport {
                    params.push("type=ws".into());
                    if let Some(ref path) = n.path {
                        params.push(format!("path={}", path));
                    }
                    if let Some(ref host) = n.host {
                        params.push(format!("host={}", host));
                    }
                }
                if let Some(ref sni) = n.sni {
                    params.push(format!("sni={}", sni));
                }
                if let Some(ref fp) = n.fingerprint {
                    params.push(format!("fp={}", fp));
                }
                if let Some(ref pbk) = n.public_key {
                    params.push(format!("pbk={}", pbk));
                }
                if let Some(ref sid) = n.short_id {
                    params.push(format!("sid={}", sid));
                }
                if let Some(ref flow) = n.flow {
                    params.push(format!("flow={}", flow));
                }
                format!("vless://{}@{}:{}?{}#{}", n.uuid.as_deref().unwrap_or(""), n.server, n.port, params.join("&"), n.name)
            }
            Protocol::Vmess => {
                let json = serde_json::json!({
                    "v": "2",
                    "ps": n.name,
                    "add": n.server,
                    "port": n.port.to_string(),
                    "id": n.uuid.as_deref().unwrap_or(""),
                    "aid": n.alter_id.unwrap_or(0).to_string(),
                    "scy": n.encryption.as_deref().unwrap_or("auto"),
                    "net": match n.transport {
                        Transport::Ws => "ws",
                        Transport::Grpc => "grpc",
                        _ => "tcp",
                    },
                    "type": "none",
                    "host": n.host.as_deref().unwrap_or(""),
                    "path": n.path.as_deref().unwrap_or(""),
                    "tls": if n.security == Security::Tls { "tls" } else { "" },
                    "sni": n.sni.as_deref().unwrap_or(""),
                    "fp": n.fingerprint.as_deref().unwrap_or(""),
                });
                format!("vmess://{}", STANDARD.encode(serde_json::to_string(&json).unwrap().as_bytes()))
            }
            Protocol::Trojan => {
                let mut params = vec![];
                if let Some(ref sni) = n.sni {
                    params.push(format!("sni={}", sni));
                }
                if let Transport::Ws = n.transport {
                    params.push("type=ws".into());
                }
                let query = if params.is_empty() { String::new() } else { format!("?{}", params.join("&")) };
                format!("trojan://{}@{}:{}{}#{}", n.password.as_deref().unwrap_or(""), n.server, n.port, query, n.name)
            }
            Protocol::Shadowsocks => {
                let encoded = STANDARD.encode(format!("{}:{}", n.uuid.as_deref().unwrap_or(""), n.password.as_deref().unwrap_or("")).as_bytes());
                format!("ss://{}@{}:{}#{}", encoded, n.server, n.port, n.name)
            }
            _ => format!("unsupported://{}", n.server),
        }
    }
}
