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

// --- ss:// tests ---

const SS_URI_SUBSCRIPTION: &str =
    "ss://YWVzLTI1Ni1nY206c2VjcmV0@us.example.com:8388#US%2001";

#[test]
fn parse_subscription_accepts_ss_uri() {
    let parsed = parse_subscription(SS_URI_SUBSCRIPTION).expect("ss subscription should parse");
    assert_eq!(parsed.nodes, vec!["US 01"]);
}

#[test]
fn build_mihomo_config_converts_ss_uri_to_proxy_yaml() {
    let config =
        build_mihomo_config(SS_URI_SUBSCRIPTION, "rule", false, None, &[]).expect("config should build");

    assert!(config.contains("type: ss"));
    assert!(config.contains("name: US 01"));
    assert!(config.contains("server: us.example.com"));
    assert!(config.contains("port: 8388"));
    assert!(config.contains("cipher: aes-256-gcm"));
    assert!(config.contains("password: secret"));
}

const SS_URI_WITH_TLS: &str =
    "ss://YWVzLTI1Ni1nY206c2VjcmV0@us.example.com:443?tls=1&sni=us.example.com#TLS%20SS";

#[test]
fn parse_ss_uri_with_tls_options() {
    let config =
        build_mihomo_config(SS_URI_WITH_TLS, "rule", false, None, &[]).expect("config should build");

    assert!(config.contains("tls: true"));
    assert!(config.contains("sni: us.example.com"));
}

// --- vmess:// tests ---

fn make_vmess_uri(ps: &str, add: &str, port: &str, id: &str, net: &str, tls: &str) -> String {
    let json = serde_json::json!({
        "v": "2",
        "ps": ps,
        "add": add,
        "port": port,
        "id": id,
        "aid": "0",
        "net": net,
        "type": "none",
        "host": "",
        "path": "",
        "tls": tls,
    });
    let encoded = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        json.to_string().as_bytes(),
    );
    format!("vmess://{encoded}")
}

#[test]
fn parse_subscription_accepts_vmess_uri() {
    let uri = make_vmess_uri("JP 01", "jp.example.com", "443", "uuid-here", "tcp", "tls");
    let parsed = parse_subscription(&uri).expect("vmess subscription should parse");
    assert_eq!(parsed.nodes, vec!["JP 01"]);
}

#[test]
fn build_mihomo_config_converts_vmess_uri_to_proxy_yaml() {
    let uri = make_vmess_uri("JP 01", "jp.example.com", "443", "uuid-here", "tcp", "tls");
    let config =
        build_mihomo_config(&uri, "rule", false, None, &[]).expect("config should build");

    assert!(config.contains("type: vmess"));
    assert!(config.contains("name: JP 01"));
    assert!(config.contains("server: jp.example.com"));
    assert!(config.contains("port: 443"));
    assert!(config.contains("uuid: uuid-here"));
    assert!(config.contains("tls: true"));
}

#[test]
fn build_mihomo_config_converts_vmess_ws_uri() {
    let uri = make_vmess_uri("WS 01", "ws.example.com", "443", "uuid-here", "ws", "tls");
    let config =
        build_mihomo_config(&uri, "rule", false, None, &[]).expect("config should build");

    assert!(config.contains("network: ws"));
    assert!(config.contains("ws-opts:"));
}

// --- vless:// tests ---

const VLESS_URI_SUBSCRIPTION: &str =
    "vless://uuid-here@de.example.com:443?security=tls&sni=de.example.com&fp=chrome#DE%2001";

#[test]
fn parse_subscription_accepts_vless_uri() {
    let parsed =
        parse_subscription(VLESS_URI_SUBSCRIPTION).expect("vless subscription should parse");
    assert_eq!(parsed.nodes, vec!["DE 01"]);
}

#[test]
fn build_mihomo_config_converts_vless_uri_to_proxy_yaml() {
    let config = build_mihomo_config(VLESS_URI_SUBSCRIPTION, "rule", false, None, &[])
        .expect("config should build");

    assert!(config.contains("type: vless"));
    assert!(config.contains("name: DE 01"));
    assert!(config.contains("server: de.example.com"));
    assert!(config.contains("port: 443"));
    assert!(config.contains("uuid: uuid-here"));
    assert!(config.contains("tls: true"));
    assert!(config.contains("sni: de.example.com"));
    assert!(config.contains("client-fingerprint: chrome"));
}

const VLESS_WS_URI: &str =
    "vless://uuid-here@ws.example.com:443?security=tls&type=ws&host=ws.example.com&path=/ws#VLESS%20WS";

#[test]
fn build_mihomo_config_converts_vless_ws_uri() {
    let config =
        build_mihomo_config(VLESS_WS_URI, "rule", false, None, &[]).expect("config should build");

    assert!(config.contains("network: ws"));
    assert!(config.contains("ws-opts:"));
}

// --- trojan:// tests ---

const TROJAN_URI_SUBSCRIPTION: &str =
    "trojan://password@tw.example.com:443?security=tls&sni=tw.example.com#TW%2001";

#[test]
fn parse_subscription_accepts_trojan_uri() {
    let parsed =
        parse_subscription(TROJAN_URI_SUBSCRIPTION).expect("trojan subscription should parse");
    assert_eq!(parsed.nodes, vec!["TW 01"]);
}

#[test]
fn build_mihomo_config_converts_trojan_uri_to_proxy_yaml() {
    let config = build_mihomo_config(TROJAN_URI_SUBSCRIPTION, "rule", false, None, &[])
        .expect("config should build");

    assert!(config.contains("type: trojan"));
    assert!(config.contains("name: TW 01"));
    assert!(config.contains("server: tw.example.com"));
    assert!(config.contains("port: 443"));
    assert!(config.contains("password: password"));
    assert!(config.contains("tls: true"));
    assert!(config.contains("sni: tw.example.com"));
}

// --- hysteria2:// tests ---

const HYSTERIA2_URI_SUBSCRIPTION: &str =
    "hysteria2://auth-password@sg.example.com:443?sni=sg.example.com&insecure=1#SG%2001";

#[test]
fn parse_subscription_accepts_hysteria2_uri() {
    let parsed = parse_subscription(HYSTERIA2_URI_SUBSCRIPTION)
        .expect("hysteria2 subscription should parse");
    assert_eq!(parsed.nodes, vec!["SG 01"]);
}

#[test]
fn build_mihomo_config_converts_hysteria2_uri_to_proxy_yaml() {
    let config = build_mihomo_config(HYSTERIA2_URI_SUBSCRIPTION, "rule", false, None, &[])
        .expect("config should build");

    assert!(config.contains("type: hysteria2"));
    assert!(config.contains("name: SG 01"));
    assert!(config.contains("server: sg.example.com"));
    assert!(config.contains("port: 443"));
    assert!(config.contains("password: auth-password"));
    assert!(config.contains("sni: sg.example.com"));
    assert!(config.contains("skip-cert-verify: true"));
}

// --- tuic:// tests ---

const TUIC_URI_SUBSCRIPTION: &str =
    "tuic://uuid:password@jp-tuic.example.com:443?congestion_control=bbr&sni=jp-tuic.example.com#TUIC%20JP";

#[test]
fn parse_subscription_accepts_tuic_uri() {
    let parsed =
        parse_subscription(TUIC_URI_SUBSCRIPTION).expect("tuic subscription should parse");
    assert_eq!(parsed.nodes, vec!["TUIC JP"]);
}

#[test]
fn build_mihomo_config_converts_tuic_uri_to_proxy_yaml() {
    let config = build_mihomo_config(TUIC_URI_SUBSCRIPTION, "rule", false, None, &[])
        .expect("config should build");

    assert!(config.contains("type: tuic"));
    assert!(config.contains("name: TUIC JP"));
    assert!(config.contains("server: jp-tuic.example.com"));
    assert!(config.contains("port: 443"));
    assert!(config.contains("uuid: uuid"));
    assert!(config.contains("password: password"));
    assert!(config.contains("congestion-control: bbr"));
    assert!(config.contains("sni: jp-tuic.example.com"));
}

// --- Dedup tests ---

#[test]
fn uri_subscription_deduplicates_nodes() {
    let sub = format!(
        "{uri}\n{uri}",
        uri = "ss://YWVzLTI1Ni1nY206c2VjcmV0@us.example.com:8388#US%2001"
    );
    let parsed = parse_subscription(&sub).expect("subscription should parse");
    assert_eq!(parsed.nodes, vec!["US 01"]);
}

// --- Mixed protocol subscription ---

#[test]
fn mixed_protocol_subscription_parses_all() {
    let sub = format!(
        "{ss}\n{vless}\n{trojan}",
        ss = SS_URI_SUBSCRIPTION,
        vless = VLESS_URI_SUBSCRIPTION,
        trojan = TROJAN_URI_SUBSCRIPTION,
    );
    let parsed = parse_subscription(&sub).expect("mixed subscription should parse");
    assert_eq!(parsed.nodes, vec!["US 01", "DE 01", "TW 01"]);
}

// --- Info node expansion tests ---

const INFO_NODES_WITH_NEW_PATTERNS: &str = r#"
anytls://secret@example.com:8443?security=tls#套餐信息：Premium
anytls://secret@example.com:8443?security=tls#剩余流量：50GB
anytls://secret@example.com:8443?security=tls#公告：维护通知
anytls://secret@example.com:8443?security=tls#🇭🇰%20香港1
"#;

#[test]
fn is_info_node_catches_new_patterns() {
    let parsed = parse_subscription(INFO_NODES_WITH_NEW_PATTERNS)
        .expect("subscription should parse");
    assert_eq!(parsed.nodes, vec!["套餐信息：Premium", "剩余流量：50GB", "公告：维护通知", "🇭🇰 香港1"]);
}
