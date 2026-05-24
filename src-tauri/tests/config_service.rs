use app_lib::config_service::{build_mihomo_config, parse_subscription};

const ANYTLS_URI_SUBSCRIPTION: &str =
    "anytls://secret@example.com:8443?security=tls&sni=edge.example.com&insecure=1#Edge%2001";

const BASE64_ANYTLS_SUBSCRIPTION: &str =
  "YW55dGxzOi8vc2VjcmV0QGV4YW1wbGUuY29tOjg0NDM/c2VjdXJpdHk9dGxzJnNuaT1lZGdlLmV4YW1wbGUuY29tJmluc2VjdXJlPTEjRWRnZSUyMDAx";

const ANYTLS_WITH_INFO_LINES: &str = r#"
anytls://secret@example.com:8443?security=tls&sni=edge.example.com#剩余流量：198.72%20GB
anytls://secret@example.com:8443?security=tls&sni=edge.example.com#套餐到期：2027-02-22
anytls://secret@example.com:8443?security=tls&sni=edge.example.com#永久官网:666.boostqz.com
anytls://secret@example.com:8443?security=tls&sni=edge.example.com#🇭🇰%20香港1
"#;

const SAMPLE_SUBSCRIPTION: &str = r#"
proxies:
  - name: HK 01
    type: ss
    server: hk.example.com
    port: 8388
    cipher: aes-128-gcm
    password: secret
proxy-groups:
  - name: Proxy
    type: select
    proxies:
      - HK 01
rules:
  - MATCH,Proxy
"#;

#[test]
fn parse_subscription_returns_node_names() {
    let parsed = parse_subscription(SAMPLE_SUBSCRIPTION).expect("subscription should parse");

    assert_eq!(parsed.nodes, vec!["HK 01"]);
}

#[test]
fn build_mihomo_config_sets_ports_mode_and_controller() {
    let config = build_mihomo_config(SAMPLE_SUBSCRIPTION, "rule", false).expect("config should build");

    assert!(config.contains("mixed-port: 7890"));
    assert!(config.contains("external-controller: 127.0.0.1:9090"));
    assert!(config.contains("mode: rule"));
    assert!(config.contains("HK 01"));
}

#[test]
fn parse_subscription_accepts_base64_anytls_uri_lists() {
    let parsed = parse_subscription(BASE64_ANYTLS_SUBSCRIPTION)
        .expect("base64 anytls subscription should parse");

    assert_eq!(parsed.nodes, vec!["Edge 01"]);
}

#[test]
fn build_mihomo_config_converts_anytls_uri_to_proxy_yaml() {
    let config = build_mihomo_config(ANYTLS_URI_SUBSCRIPTION, "rule", false).expect("config should build");

    assert!(config.contains("type: anytls"));
    assert!(config.contains("name: Edge 01"));
    assert!(config.contains("server: example.com"));
    assert!(config.contains("port: 8443"));
    assert!(config.contains("password: secret"));
    assert!(config.contains("sni: edge.example.com"));
    assert!(config.contains("skip-cert-verify: true"));
    assert!(config.contains("proxy-groups:"));
    assert!(config.contains("rules:"));
}

#[test]
fn uri_subscription_preserves_provider_info_nodes() {
    let parsed = parse_subscription(ANYTLS_WITH_INFO_LINES).expect("subscription should parse");

    assert_eq!(
        parsed.nodes,
        vec![
            "剩余流量：198.72 GB",
            "套餐到期：2027-02-22",
            "永久官网:666.boostqz.com",
            "🇭🇰 香港1"
        ]
    );
}

#[test]
fn generated_proxy_group_preserves_provider_info_nodes() {
    let config = build_mihomo_config(ANYTLS_WITH_INFO_LINES, "rule", false).expect("config should build");

    assert!(config.contains("剩余流量"));
    assert!(config.contains("套餐到期"));
    assert!(config.contains("永久官网"));
    assert!(config.contains("🇭🇰 香港1"));
}
