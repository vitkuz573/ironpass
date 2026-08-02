use bytes::{BufMut, BytesMut};
use ironpass_core::models::ProxyNode;

const CMD_TCP: u8 = 0x01;
const ATYP_IPV4: u8 = 0x01;
const ATYP_DOMAIN: u8 = 0x03;
const ATYP_IPV6: u8 = 0x04;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VlessFlow {
    None,
    XtlsRprxVision,
}

impl VlessFlow {
    pub fn parse(s: &str) -> Self {
        match s {
            "xtls-rprx-vision" => Self::XtlsRprxVision,
            _ => Self::None,
        }
    }
}

pub fn encode_vless_request(
    uuid: &[u8],
    target_host: &str,
    target_port: u16,
    node: &ProxyNode,
) -> BytesMut {
    let flow = node.flow.as_deref()
        .map(VlessFlow::parse)
        .unwrap_or(VlessFlow::None);

    let mut buf = BytesMut::with_capacity(256);

    buf.put_u8(0); // version

    buf.put_slice(uuid);

    buf.put_u8(CMD_TCP); // command: TCP

    if flow == VlessFlow::XtlsRprxVision {
        buf.put_u8(0x00); // RSV + Proto: xtls-rprx-vision
    } else {
        buf.put_u8(0x00); // RSV + Proto
    }

    // Port (big endian)
    buf.put_u16(target_port);

    // Address type + address
    if let Ok(ip) = target_host.parse::<std::net::Ipv4Addr>() {
        buf.put_u8(ATYP_IPV4);
        buf.put_slice(&ip.octets());
    } else if let Ok(ip) = target_host.parse::<std::net::Ipv6Addr>() {
        buf.put_u8(ATYP_IPV6);
        buf.put_slice(&ip.octets());
    } else {
        buf.put_u8(ATYP_DOMAIN);
        buf.put_u8(target_host.len() as u8);
        buf.put_slice(target_host.as_bytes());
    }

    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_domain() {
        let uuid = vec![0u8; 16];
        let node = ProxyNode {
            protocol: ironpass_core::models::Protocol::Vless,
            name: "test".into(),
            server: "example.com".into(),
            port: 443,
            uuid: Some("00000000-0000-0000-0000-000000000000".into()),
            password: None,
            alter_id: None,
            encryption: None,
            transport: ironpass_core::models::Transport::Tcp,
            security: ironpass_core::models::Security::Reality,
            flow: Some("xtls-rprx-vision".into()),
            sni: None,
            fingerprint: None,
            public_key: None,
            short_id: None,
            spider_x: None,
            path: None,
            host: None,
            service_name: None,
            alpn: None,
            tags: vec![],
            raw_uri: String::new(),
        };

        let buf = encode_vless_request(&uuid, "example.com", 443, &node);

        // VLESS request layout: ver(1) + uuid(16) + cmd(1) + rsv/proto(1) + port(2) + atyp(1) + addr
        assert_eq!(buf[0], 0); // version
        assert_eq!(buf[17], CMD_TCP); // command
        assert_eq!(buf[18], 0); // flow / rsv + proto
        assert_eq!(buf[19], 1); // port high byte
        assert_eq!(buf[20], 187); // port low byte (443)
        assert_eq!(buf[21], ATYP_DOMAIN); // address type
        assert_eq!(buf[22], 11); // domain length
        assert_eq!(&buf[23..34], b"example.com");
    }
}
