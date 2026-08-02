use ironpass_core::models::ProxyNode;
use ironpass_subscription::PlaceholderPolicy;

fn real_vless_node() -> ProxyNode {
    ProxyNode {
        protocol: ironpass_core::models::Protocol::Vless,
        name: "Real".into(),
        server: "example.org".into(),
        port: 443,
        uuid: Some("550e8400-e29b-41d4-a716-446655440000".into()),
        password: None,
        alter_id: None,
        encryption: None,
        transport: ironpass_core::models::Transport::Tcp,
        security: ironpass_core::models::Security::None,
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
        raw_uri: String::new(),
    }
}

fn node_with(fields: impl FnOnce(&mut ProxyNode)) -> ProxyNode {
    let mut node = real_vless_node();
    fields(&mut node);
    node
}

#[test]
fn default_policy_matches_zero_uuid() {
    let node = node_with(|n| {
        n.uuid = Some("00000000-0000-0000-0000-000000000000".into());
    });
    assert!(PlaceholderPolicy::default().is_placeholder(&node));
}

#[test]
fn default_policy_matches_zero_address_and_port() {
    let node = node_with(|n| {
        n.server = "0.0.0.0".into();
        n.port = 1;
        n.uuid = None;
    });
    assert!(PlaceholderPolicy::default().is_placeholder(&node));
}

#[test]
fn default_policy_allows_real_node() {
    assert!(!PlaceholderPolicy::default().is_placeholder(&real_vless_node()));
}

#[test]
fn strict_policy_catches_loopback_and_low_port() {
    let node = node_with(|n| {
        n.server = "127.0.0.1".into();
        n.port = 1;
        n.uuid = None;
    });
    assert!(PlaceholderPolicy::strict().is_placeholder(&node));
}

#[test]
fn strict_policy_catches_sentinel_domain_and_dummy_uuid() {
    let node = node_with(|n| {
        n.server = "test.com".into();
        n.uuid = Some("550e8400-e29b-41d4-a716-446655440000".into());
    });
    assert!(PlaceholderPolicy::strict().is_placeholder(&node));
}

#[test]
fn scoring_avoids_false_positive_for_localhost_with_real_uuid() {
    let node = node_with(|n| {
        n.server = "127.0.0.1".into();
        n.port = 8080;
    });
    assert!(!PlaceholderPolicy::default().is_placeholder(&node));
}

#[test]
fn custom_address_addition_is_detected() {
    let mut policy = PlaceholderPolicy::default();
    policy.add_dummy_address("placeholder.invalid");

    let node = node_with(|n| {
        n.server = "placeholder.invalid".into();
    });
    assert!(policy.is_placeholder(&node));
    assert!(!PlaceholderPolicy::default().is_placeholder(&node));
}

#[test]
fn custom_uuid_addition_is_detected() {
    let mut policy = PlaceholderPolicy::default();
    let sentinel = uuid::Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
    policy.add_dummy_uuid(sentinel);

    let node = node_with(|n| {
        n.uuid = Some("11111111-1111-1111-1111-111111111111".into());
    });
    assert!(policy.is_placeholder(&node));
    assert!(!PlaceholderPolicy::default().is_placeholder(&node));
}

#[test]
fn zero_uuid_and_zero_address_are_always_placeholders() {
    let zero_uuid = node_with(|n| {
        n.server = "real.example.org".into();
        n.port = 443;
        n.uuid = Some("00000000-0000-0000-0000-000000000000".into());
    });
    let zero_addr = node_with(|n| {
        n.server = "0.0.0.0".into();
        n.port = 443;
        n.uuid = Some("550e8400-e29b-41d4-a716-446655440000".into());
    });

    assert!(PlaceholderPolicy::default().is_placeholder(&zero_uuid));
    assert!(PlaceholderPolicy::default().is_placeholder(&zero_addr));
    assert!(PlaceholderPolicy::strict().is_placeholder(&zero_uuid));
    assert!(PlaceholderPolicy::strict().is_placeholder(&zero_addr));
}

#[test]
fn single_non_hard_criterion_is_not_placeholder_by_default() {
    let node = node_with(|n| {
        n.server = "example.com".into();
        n.uuid = Some("00000000-0000-0000-0000-000000000001".into());
    });
    assert!(!PlaceholderPolicy::default().is_placeholder(&node));
}

#[test]
fn ports_zero_and_one_are_always_placeholders() {
    for port in [0u16, 1] {
        let node = node_with(|n| {
            n.server = "example.org".into();
            n.port = port;
        });
        assert!(PlaceholderPolicy::default().is_placeholder(&node));
        assert!(PlaceholderPolicy::strict().is_placeholder(&node));
    }
}
