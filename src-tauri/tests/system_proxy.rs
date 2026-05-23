use app_lib::system_proxy::{ProxyMode, SystemProxy};

#[test]
fn system_proxy_keeps_proxy_endpoint_configuration() {
    let proxy = SystemProxy::new("127.0.0.1", 7890);

    assert_eq!(proxy.endpoint(), "127.0.0.1:7890");
}

#[test]
fn proxy_mode_matches_mihomo_mode_names() {
    assert_eq!(ProxyMode::Rule.as_mihomo_mode(), "rule");
    assert_eq!(ProxyMode::Global.as_mihomo_mode(), "global");
    assert_eq!(ProxyMode::Direct.as_mihomo_mode(), "direct");
}
