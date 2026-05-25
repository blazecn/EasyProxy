use app_lib::config_service::{build_mihomo_config, parse_subscription, DnsOverride};

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

const SUBSCRIPTION_WITH_TUN: &str = r#"
mixed-port: 7890
tun:
  enable: true
  stack: system
  auto-route: true
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
    let config = build_mihomo_config(SAMPLE_SUBSCRIPTION, "rule", false, None, &[])
        .expect("config should build");

    assert!(config.contains("mixed-port: 7897"));
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
    let config = build_mihomo_config(ANYTLS_URI_SUBSCRIPTION, "rule", false, None, &[])
        .expect("config should build");

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
    let config = build_mihomo_config(ANYTLS_WITH_INFO_LINES, "rule", false, None, &[])
        .expect("config should build");

    assert!(config.contains("剩余流量"));
    assert!(config.contains("套餐到期"));
    assert!(config.contains("永久官网"));
    assert!(config.contains("🇭🇰 香港1"));
}

#[test]
fn build_mihomo_config_with_tun_adds_tun_section() {
    let config = build_mihomo_config(SAMPLE_SUBSCRIPTION, "rule", true, None, &[])
        .expect("config should build");

    assert!(config.contains("tun:"));
    assert!(config.contains("enable: true"));
    assert!(config.contains("stack: system"));
    assert!(config.contains("dns-hijack:"));
    assert!(config.contains("any:53"));
    assert!(config.contains("auto-route: true"));
    assert!(config.contains("auto-detect-interface: true"));
}

#[test]
fn build_mihomo_config_without_tun_omits_tun_section() {
    let config = build_mihomo_config(SAMPLE_SUBSCRIPTION, "rule", false, None, &[])
        .expect("config should build");

    assert!(!config.contains("tun:"));
    assert!(!config.contains("enable: true"));
    assert!(!config.contains("stack: system"));
    assert!(!config.contains("auto-route: true"));
    assert!(!config.contains("auto-detect-interface: true"));
}

#[test]
fn build_mihomo_config_without_tun_removes_subscription_tun_section() {
    let config = build_mihomo_config(SUBSCRIPTION_WITH_TUN, "rule", false, None, &[])
        .expect("config should build");

    assert!(!config.contains("tun:"));
    assert!(!config.contains("auto-route: true"));
    assert!(config.contains("mixed-port: 7897"));
}

#[test]
fn build_mihomo_config_tun_with_anytls_uri() {
    let config = build_mihomo_config(ANYTLS_URI_SUBSCRIPTION, "global", true, None, &[])
        .expect("config should build");

    assert!(config.contains("type: anytls"));
    assert!(config.contains("tun:"));
    assert!(config.contains("stack: system"));
    assert!(config.contains("mode: global"));
}

#[test]
fn dns_override_enabled_injects_dns_section() {
    let content = "proxies:\n  - name: Test\n    type: ss\n    server: 1.2.3.4\n    port: 8388\n    password: pwd\n    cipher: aes-256-gcm\n";
    let dns_config: serde_yaml::Value =
        serde_yaml::from_str("enable: true\nlisten: 0.0.0.0:53\nnameserver:\n  - 223.5.5.5\n")
            .unwrap();
    let dns_override = DnsOverride {
        enabled: true,
        config: dns_config,
    };

    let result = build_mihomo_config(content, "rule", false, Some(&dns_override), &[]).unwrap();
    let doc: serde_yaml::Value = serde_yaml::from_str(&result).unwrap();

    let dns = doc.get("dns").unwrap();
    assert_eq!(dns.get("enable").unwrap().as_bool().unwrap(), true);
    assert_eq!(dns.get("listen").unwrap().as_str().unwrap(), "0.0.0.0:53");
}

#[test]
fn dns_override_disabled_does_not_inject_dns_section() {
    let content = "proxies:\n  - name: Test\n    type: ss\n    server: 1.2.3.4\n    port: 8388\n    password: pwd\n    cipher: aes-256-gcm\n";
    let dns_config: serde_yaml::Value =
        serde_yaml::from_str("enable: true\nlisten: 0.0.0.0:53\n").unwrap();
    let dns_override = DnsOverride {
        enabled: false,
        config: dns_config,
    };

    let result = build_mihomo_config(content, "rule", false, Some(&dns_override), &[]).unwrap();
    let doc: serde_yaml::Value = serde_yaml::from_str(&result).unwrap();

    assert!(doc.get("dns").is_none());
}

#[test]
fn dns_override_none_does_not_inject_dns_section() {
    let content = "proxies:\n  - name: Test\n    type: ss\n    server: 1.2.3.4\n    port: 8388\n    password: pwd\n    cipher: aes-256-gcm\n";

    let result = build_mihomo_config(content, "rule", false, None, &[]).unwrap();
    let doc: serde_yaml::Value = serde_yaml::from_str(&result).unwrap();

    assert!(doc.get("dns").is_none());
}
