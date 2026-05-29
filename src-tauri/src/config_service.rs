use base64::{engine::general_purpose, Engine};
use serde::{Deserialize, Serialize};
use serde_yaml::{Mapping, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProxyGroupSummary {
    pub name: String,
    #[serde(rename = "type")]
    pub group_type: String,
    pub nodes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsOverride {
    pub enabled: bool,
    pub config: serde_yaml::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubscriptionSummary {
    pub nodes: Vec<String>,
    pub format: String,
    pub rules: Vec<String>,
    pub groups: Vec<ProxyGroupSummary>,
    pub node_types: BTreeMap<String, String>,
}

pub fn parse_subscription(content: &str) -> Result<SubscriptionSummary, String> {
    match parse_document(content)? {
        SubscriptionDocument::Clash(document) => {
            let nodes = node_names_from_yaml(&document)?;
            let node_types = node_types_from_yaml(&document);
            let rules = rules_from_yaml(&document);
            let groups = groups_from_yaml(&document);
            Ok(SubscriptionSummary {
                nodes,
                format: "clash-yaml".to_string(),
                rules,
                groups,
                node_types,
            })
        }
        SubscriptionDocument::UriList(nodes) => {
            let all_names: Vec<String> = nodes.iter().map(|n| n.name.clone()).collect();
            let node_types: BTreeMap<String, String> = nodes
                .iter()
                .filter_map(|n| {
                    n.proxy
                        .get("type")?
                        .as_str()
                        .map(|t| (n.name.clone(), t.to_string()))
                })
                .collect();
            let groups = build_groups_for_uri_nodes(&nodes);
            Ok(SubscriptionSummary {
                nodes: all_names,
                format: "uri-list".to_string(),
                rules: default_rules(),
                groups,
                node_types,
            })
        }
    }
}

pub fn build_mihomo_config(
    content: &str,
    mode: &str,
    tun_enabled: bool,
    dns_override: Option<&DnsOverride>,
    custom_rules: &[String],
) -> Result<String, String> {
    let mut document = match parse_document(content)? {
        SubscriptionDocument::Clash(document) => document,
        SubscriptionDocument::UriList(nodes) => build_document_from_uri_nodes(nodes),
    };

    apply_runtime_settings(&mut document, mode, tun_enabled, dns_override, custom_rules)?;

    serde_yaml::to_string(&document).map_err(|error| format!("生成 Mihomo 配置失败: {error}"))
}

pub fn save_subscription(path: &Path, content: &str) -> Result<SubscriptionSummary, String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("创建配置目录失败: {error}"))?;
    }

    let summary = parse_subscription(content)?;
    fs::write(path, content).map_err(|error| format!("保存订阅失败: {error}"))?;
    Ok(summary)
}

pub fn merge_subscription(yaml_content: &str, uri_content: &str) -> Result<String, String> {
    let mut yaml_doc: Value =
        parse_yaml_or_base64(yaml_content).ok_or_else(|| "无法解析 YAML 订阅内容".to_string())?;

    let uri_nodes = match parse_uri_or_base64(uri_content) {
        Some(nodes) => nodes,
        None => return Err("URI 订阅内容解析失败".to_string()),
    };

    let (info_nodes, proxy_nodes): (Vec<_>, Vec<_>) = uri_nodes
        .into_iter()
        .partition(|n| is_info_node_name(&n.name));

    let all_proxies: Vec<Value> = proxy_nodes
        .iter()
        .map(|n| Value::Mapping(n.proxy.clone()))
        .chain(info_nodes.iter().map(|n| Value::Mapping(n.proxy.clone())))
        .collect();

    let proxy_names: Vec<Value> = proxy_nodes
        .iter()
        .map(|n| Value::String(n.name.clone()))
        .collect();

    // Build name map: stripped name → full URI name, for resolving YAML group references
    let name_map: BTreeMap<String, String> = proxy_nodes
        .iter()
        .map(|n| (strip_region_flags(&n.name), n.name.clone()))
        .collect();

    let root = yaml_doc
        .as_mapping_mut()
        .ok_or_else(|| "YAML 格式无效".to_string())?;

    root.insert(
        Value::String("proxies".to_string()),
        Value::Sequence(all_proxies),
    );

    // Collect group names for select-type groups to reference each other
    let group_names: Vec<String> = root
        .get("proxy-groups")
        .and_then(|v| v.as_sequence())
        .map(|groups| {
            groups
                .iter()
                .filter_map(|g| g.get("name")?.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    if let Some(groups) = root
        .get_mut("proxy-groups")
        .and_then(|v| v.as_sequence_mut())
    {
        for group in groups {
            if let Some(group_map) = group.as_mapping_mut() {
                let group_type = group_map.get("type").and_then(|v| v.as_str()).unwrap_or("");

                let this_group_name = group_map.get("name").and_then(|v| v.as_str()).unwrap_or("");

                // Filter placeholders, resolve old names → URI names via stripped-name matching
                let mut kept: Vec<Value> = group_map
                    .get("proxies")
                    .and_then(|v| v.as_sequence())
                    .map(|seq| {
                        seq.iter()
                            .filter_map(|v| v.as_str())
                            .filter(|name| {
                                !name.contains("客户端太旧")
                                    && !name.contains("升级客户端")
                                    && !name.contains("官网教程")
                            })
                            .filter_map(|n| {
                                let stripped = strip_region_flags(n);
                                if let Some(resolved) = name_map.get(&stripped) {
                                    Some(Value::String(resolved.clone()))
                                } else {
                                    Some(Value::String(n.to_string()))
                                }
                            })
                            .collect()
                    })
                    .unwrap_or_default();

                if group_type == "select" {
                    // Select groups: add all sub-groups + all proxy nodes (standard Clash behavior)
                    for gn in &group_names {
                        if gn != this_group_name {
                            let gn_str = Value::String(gn.clone());
                            if !kept.contains(&gn_str) {
                                kept.push(gn_str);
                            }
                        }
                    }
                    for pn in &proxy_names {
                        if !kept.contains(pn) {
                            kept.push(pn.clone());
                        }
                    }
                } else if kept.is_empty() {
                    // Non-select groups with no real entries: fill with all proxy nodes
                    for pn in &proxy_names {
                        kept.push(pn.clone());
                    }
                }

                group_map.insert(Value::String("proxies".to_string()), Value::Sequence(kept));
            }
        }
    }

    serde_yaml::to_string(&yaml_doc).map_err(|e| format!("生成合并配置失败: {e}"))
}

fn parse_yaml_or_base64(content: &str) -> Option<Value> {
    if let Ok(doc) = serde_yaml::from_str(content) {
        return Some(doc);
    }
    let decoded = decode_base64_subscription(content)?;
    serde_yaml::from_str(&decoded).ok()
}

fn parse_uri_or_base64(content: &str) -> Option<Vec<UriProxyNode>> {
    if let Ok(nodes) = parse_uri_subscription(content) {
        return Some(nodes);
    }
    let decoded = decode_base64_subscription(content)?;
    parse_uri_subscription(&decoded).ok()
}

pub fn write_runtime_config(
    path: &Path,
    content: &str,
    mode: &str,
    tun_enabled: bool,
    dns_override: Option<&DnsOverride>,
    custom_rules: &[String],
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("创建运行目录失败: {error}"))?;
    }

    let config = build_mihomo_config(content, mode, tun_enabled, dns_override, custom_rules)?;
    fs::write(path, config).map_err(|error| format!("写入 Mihomo 配置失败: {error}"))
}

enum SubscriptionDocument {
    Clash(Value),
    UriList(Vec<UriProxyNode>),
}

#[derive(Debug, Clone)]
struct UriProxyNode {
    name: String,
    proxy: Mapping,
}

fn parse_document(content: &str) -> Result<SubscriptionDocument, String> {
    if let Ok(document) = parse_yaml_subscription(content) {
        return Ok(SubscriptionDocument::Clash(document));
    }

    if let Ok(nodes) = parse_uri_subscription(content) {
        return Ok(SubscriptionDocument::UriList(nodes));
    }

    if let Some(decoded) = decode_base64_subscription(content) {
        if let Ok(document) = parse_yaml_subscription(&decoded) {
            return Ok(SubscriptionDocument::Clash(document));
        }

        if let Ok(nodes) = parse_uri_subscription(&decoded) {
            return Ok(SubscriptionDocument::UriList(nodes));
        }
    }

    Err("订阅格式不支持，请使用 Clash/Mihomo YAML 或 Base64/明文 URI 订阅".to_string())
}

fn parse_yaml_subscription(content: &str) -> Result<Value, String> {
    let document = parse_yaml(content)?;
    node_names_from_yaml(&document)?;
    Ok(document)
}

fn parse_yaml(content: &str) -> Result<Value, String> {
    serde_yaml::from_str(content).map_err(|error| format!("订阅 YAML 解析失败: {error}"))
}

fn node_names_from_yaml(document: &Value) -> Result<Vec<String>, String> {
    let nodes = document
        .get("proxies")
        .and_then(Value::as_sequence)
        .ok_or_else(|| "订阅中没有找到 proxies 节点".to_string())?
        .iter()
        .filter_map(|proxy| {
            proxy
                .get("name")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .collect::<Vec<_>>();

    if nodes.is_empty() {
        return Err("订阅中没有可用节点".to_string());
    }

    Ok(nodes)
}

fn node_types_from_yaml(document: &Value) -> BTreeMap<String, String> {
    document
        .get("proxies")
        .and_then(Value::as_sequence)
        .map(|proxies| {
            proxies
                .iter()
                .filter_map(|proxy| {
                    let name = proxy.get("name")?.as_str()?;
                    let ptype = proxy.get("type")?.as_str()?;
                    Some((name.to_string(), ptype.to_string()))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn rules_from_yaml(document: &Value) -> Vec<String> {
    document
        .get("rules")
        .and_then(Value::as_sequence)
        .map(|rules| {
            rules
                .iter()
                .filter_map(|rule| rule.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

fn groups_from_yaml(document: &Value) -> Vec<ProxyGroupSummary> {
    document
        .get("proxy-groups")
        .and_then(Value::as_sequence)
        .map(|groups| {
            groups
                .iter()
                .filter_map(|g| {
                    let name = g.get("name")?.as_str()?.to_string();
                    let group_type = g.get("type")?.as_str()?.to_string();
                    let nodes = g
                        .get("proxies")
                        .and_then(Value::as_sequence)
                        .map(|proxies| {
                            proxies
                                .iter()
                                .filter_map(|p| p.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default();
                    Some(ProxyGroupSummary {
                        name,
                        group_type,
                        nodes,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn default_rules() -> Vec<String> {
    vec![
        "DOMAIN-SUFFIX,cn,DIRECT".to_string(),
        "GEOIP,CN,DIRECT".to_string(),
        "MATCH,Proxy".to_string(),
    ]
}

fn build_groups_for_uri_nodes(nodes: &[UriProxyNode]) -> Vec<ProxyGroupSummary> {
    let proxy_nodes: Vec<&UriProxyNode> = nodes
        .iter()
        .filter(|n| !is_info_node_name(&n.name))
        .collect();

    let mut regions: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for node in &proxy_nodes {
        let region = extract_region(&node.name);
        if !region.is_empty() {
            regions.entry(region).or_default().push(node.name.clone());
        }
    }

    let mut region_entries: Vec<(String, Vec<String>)> = regions.into_iter().collect();
    region_entries.sort_by(|a, b| b.1.len().cmp(&a.1.len()));

    region_entries
        .into_iter()
        .map(|(region, node_names)| {
            let group_name = format!("{} - {} 条", region, node_names.len());
            ProxyGroupSummary {
                name: group_name,
                group_type: "url-test".to_string(),
                nodes: node_names,
            }
        })
        .collect()
}

fn parse_uri_subscription(content: &str) -> Result<Vec<UriProxyNode>, String> {
    let mut seen = std::collections::HashSet::new();
    let nodes: Vec<UriProxyNode> = content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| line.contains("://"))
        .map(parse_proxy_uri)
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|node| seen.insert(node.name.clone()))
        .collect();

    if nodes.is_empty() {
        return Err("订阅中没有可用 URI 节点".to_string());
    }

    Ok(nodes)
}

fn parse_proxy_uri(line: &str) -> Result<UriProxyNode, String> {
    let url = Url::parse(line).map_err(|error| format!("节点 URI 解析失败: {error}"))?;
    match url.scheme() {
        "anytls" => parse_anytls_uri(url),
        "ss" => parse_ss_uri(url),
        "vmess" => parse_vmess_uri(url),
        "vless" => parse_vless_uri(url),
        "trojan" => parse_trojan_uri(url),
        "hysteria2" | "hy2" => parse_hysteria2_uri(url),
        "tuic" => parse_tuic_uri(url),
        scheme => Err(format!("暂不支持 {scheme}:// 节点转换")),
    }
}

fn parse_anytls_uri(url: Url) -> Result<UriProxyNode, String> {
    let server = url
        .host_str()
        .ok_or_else(|| "anytls 节点缺少服务器地址".to_string())?
        .to_string();
    let port = url
        .port()
        .ok_or_else(|| "anytls 节点缺少端口".to_string())?;
    let name = decode_url_component(url.fragment().unwrap_or("AnyTLS"));
    let password = if let Some(password) = url.password() {
        password.to_string()
    } else {
        url.username().to_string()
    };

    if password.is_empty() {
        return Err("anytls 节点缺少密码".to_string());
    }

    let mut proxy = Mapping::new();
    insert_scalar(&mut proxy, "name", Value::String(name.clone()));
    insert_scalar(&mut proxy, "type", Value::String("anytls".to_string()));
    insert_scalar(&mut proxy, "server", Value::String(server));
    insert_scalar(&mut proxy, "port", Value::Number(port.into()));
    insert_scalar(
        &mut proxy,
        "password",
        Value::String(decode_url_component(&password)),
    );

    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "sni" | "servername" => {
                insert_scalar(&mut proxy, "sni", Value::String(value.to_string()));
            }
            "insecure" | "allowInsecure" | "skip-cert-verify" => {
                insert_scalar(
                    &mut proxy,
                    "skip-cert-verify",
                    Value::Bool(matches!(value.as_ref(), "1" | "true" | "TRUE")),
                );
            }
            "udp" => {
                insert_scalar(
                    &mut proxy,
                    "udp",
                    Value::Bool(matches!(value.as_ref(), "1" | "true" | "TRUE")),
                );
            }
            _ => {}
        }
    }

    Ok(UriProxyNode { name, proxy })
}

fn parse_ss_uri(url: Url) -> Result<UriProxyNode, String> {
    let server = url
        .host_str()
        .ok_or_else(|| "ss 节点缺少服务器地址".to_string())?
        .to_string();
    let port = url
        .port()
        .ok_or_else(|| "ss 节点缺少端口".to_string())?;
    let name = decode_url_component(url.fragment().unwrap_or("Shadowsocks"));

    // Decode userinfo: base64(method:password)
    let userinfo = url.username();
    let decoded_userinfo = general_purpose::STANDARD
        .decode(userinfo)
        .or_else(|_| general_purpose::URL_SAFE.decode(userinfo))
        .map_err(|_| "ss 节点 userinfo 解码失败".to_string())?;
    let userinfo_str =
        String::from_utf8(decoded_userinfo).map_err(|_| "ss 节点 userinfo 非 UTF-8".to_string())?;
    let (method, password) = userinfo_str
        .split_once(':')
        .ok_or_else(|| "ss 节点 userinfo 格式无效，应为 method:password".to_string())?;

    let mut proxy = Mapping::new();
    insert_scalar(&mut proxy, "name", Value::String(name.clone()));
    insert_scalar(&mut proxy, "type", Value::String("ss".to_string()));
    insert_scalar(&mut proxy, "server", Value::String(server));
    insert_scalar(&mut proxy, "port", Value::Number(port.into()));
    insert_scalar(
        &mut proxy,
        "cipher",
        Value::String(method.to_string()),
    );
    insert_scalar(
        &mut proxy,
        "password",
        Value::String(decode_url_component(password)),
    );

    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "obfs" => {
                insert_scalar(&mut proxy, "plugin", Value::String("obfs".to_string()));
                insert_scalar(&mut proxy, "plugin-opts", Value::String(value.to_string()));
            }
            "obfs-opts" => {
                insert_scalar(
                    &mut proxy,
                    "plugin-opts",
                    Value::String(value.to_string()),
                );
            }
            "v2ray-plugin" | "v2ray-opts" => {
                insert_scalar(
                    &mut proxy,
                    "plugin",
                    Value::String("v2ray-plugin".to_string()),
                );
                insert_scalar(
                    &mut proxy,
                    "plugin-opts",
                    Value::String(value.to_string()),
                );
            }
            "tls" | "ssl" => {
                if matches!(value.as_ref(), "1" | "true" | "TRUE") {
                    insert_scalar(&mut proxy, "tls", Value::Bool(true));
                }
            }
            "sni" => {
                insert_scalar(&mut proxy, "sni", Value::String(value.to_string()));
            }
            "skip-cert-verify" => {
                insert_scalar(
                    &mut proxy,
                    "skip-cert-verify",
                    Value::Bool(matches!(value.as_ref(), "1" | "true" | "TRUE")),
                );
            }
            _ => {}
        }
    }

    Ok(UriProxyNode { name, proxy })
}

fn parse_vmess_uri(url: Url) -> Result<UriProxyNode, String> {
    let encoded = url
        .host_str()
        .ok_or_else(|| "vmess 节点缺少编码数据".to_string())?;

    // vmess://base64(json) — the host part after scheme is the Base64-encoded JSON
    // Reconstruct the full encoded string from host + path (Url::parse may split on /)
    let full_encoded = format!(
        "{}{}",
        encoded,
        url.path().trim_start_matches('/')
    );
    let decoded = general_purpose::STANDARD
        .decode(&full_encoded)
        .or_else(|_| general_purpose::URL_SAFE.decode(&full_encoded))
        .map_err(|_| "vmess 节点 Base64 解码失败".to_string())?;
    let json_str =
        String::from_utf8(decoded).map_err(|_| "vmess 节点 JSON 非 UTF-8".to_string())?;
    let json: serde_json::Value = serde_json::from_str(&json_str)
        .map_err(|error| format!("vmess 节点 JSON 解析失败: {error}"))?;

    let server = json
        .get("add")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "vmess 节点缺少服务器地址 (add)".to_string())?
        .to_string();
    let port: u32 = json
        .get("port")
        .and_then(|v| v.as_str().or_else(|| v.as_u64().map(|_| "")))
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| "vmess 节点缺少有效端口 (port)".to_string())?;
    let uuid = json
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "vmess 节点缺少 UUID (id)".to_string())?
        .to_string();
    let alter_id = json
        .get("aid")
        .and_then(|v| v.as_str().and_then(|s| s.parse::<u32>().ok()).or_else(|| v.as_u64().map(|n| n as u32)))
        .unwrap_or(0);
    let name = json
        .get("ps")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("VMess")
        .to_string();

    let mut proxy = Mapping::new();
    insert_scalar(&mut proxy, "name", Value::String(name.clone()));
    insert_scalar(&mut proxy, "type", Value::String("vmess".to_string()));
    insert_scalar(&mut proxy, "server", Value::String(server));
    insert_scalar(&mut proxy, "port", Value::Number(port.into()));
    insert_scalar(&mut proxy, "uuid", Value::String(uuid));
    insert_scalar(&mut proxy, "alterId", Value::Number(alter_id.into()));
    insert_scalar(
        &mut proxy,
        "cipher",
        Value::String(
            json.get("scy")
                .and_then(|v| v.as_str())
                .unwrap_or("auto")
                .to_string(),
        ),
    );

    // Transport type
    let net = json
        .get("net")
        .and_then(|v| v.as_str())
        .unwrap_or("tcp");
    if net != "tcp" {
        insert_scalar(&mut proxy, "network", Value::String(net.to_string()));
    }

    // TLS
    if let Some(tls) = json.get("tls").and_then(|v| v.as_str()) {
        if tls == "tls" {
            insert_scalar(&mut proxy, "tls", Value::Bool(true));
        }
    }

    // Transport-specific options
    match net {
        "ws" => {
            let mut ws_opts = Mapping::new();
            if let Some(path) = json.get("path").and_then(|v| v.as_str()) {
                insert_scalar(&mut ws_opts, "path", Value::String(path.to_string()));
            }
            if let Some(host) = json.get("host").and_then(|v| v.as_str()) {
                insert_scalar(&mut ws_opts, "headers", {
                    let mut headers = Mapping::new();
                    insert_scalar(&mut headers, "Host", Value::String(host.to_string()));
                    Value::Mapping(headers)
                });
            }
            if !ws_opts.is_empty() {
                root_proxy_insert(&mut proxy, "ws-opts", Value::Mapping(ws_opts));
            }
        }
        "grpc" => {
            let mut grpc_opts = Mapping::new();
            if let Some(service_name) = json.get("path").and_then(|v| v.as_str()) {
                insert_scalar(
                    &mut grpc_opts,
                    "grpc-service-name",
                    Value::String(service_name.to_string()),
                );
            }
            if !grpc_opts.is_empty() {
                root_proxy_insert(&mut proxy, "grpc-opts", Value::Mapping(grpc_opts));
            }
        }
        "h2" => {
            let mut h2_opts = Mapping::new();
            if let Some(path) = json.get("path").and_then(|v| v.as_str()) {
                insert_scalar(&mut h2_opts, "path", Value::String(path.to_string()));
            }
            if let Some(host) = json.get("host").and_then(|v| v.as_str()) {
                insert_scalar(
                    &mut h2_opts,
                    "host",
                    Value::Sequence(vec![Value::String(host.to_string())]),
                );
            }
            if !h2_opts.is_empty() {
                root_proxy_insert(&mut proxy, "h2-opts", Value::Mapping(h2_opts));
            }
        }
        _ => {}
    }

    // TLS options
    if proxy.get("tls").and_then(|v| v.as_bool()) == Some(true) {
        if let Some(sni) = json.get("sni").and_then(|v| v.as_str()) {
            insert_scalar(&mut proxy, "sni", Value::String(sni.to_string()));
        }
        if let Some(fp) = json.get("fp").and_then(|v| v.as_str()) {
            insert_scalar(&mut proxy, "client-fingerprint", Value::String(fp.to_string()));
        }
        if let Some(alpn) = json.get("alpn").and_then(|v| v.as_str()) {
            insert_scalar(
                &mut proxy,
                "alpn",
                Value::Sequence(
                    alpn.split(',')
                        .map(|s| Value::String(s.trim().to_string()))
                        .collect(),
                ),
            );
        }
    }

    Ok(UriProxyNode { name, proxy })
}

fn parse_vless_uri(url: Url) -> Result<UriProxyNode, String> {
    let uuid = decode_url_component(url.username());
    let server = url
        .host_str()
        .ok_or_else(|| "vless 节点缺少服务器地址".to_string())?
        .to_string();
    let port = url
        .port()
        .ok_or_else(|| "vless 节点缺少端口".to_string())?;
    let name = decode_url_component(url.fragment().unwrap_or("VLESS"));

    let mut proxy = Mapping::new();
    insert_scalar(&mut proxy, "name", Value::String(name.clone()));
    insert_scalar(&mut proxy, "type", Value::String("vless".to_string()));
    insert_scalar(&mut proxy, "server", Value::String(server));
    insert_scalar(&mut proxy, "port", Value::Number(port.into()));
    insert_scalar(&mut proxy, "uuid", Value::String(uuid));
    insert_scalar(
        &mut proxy,
        "udp",
        Value::Bool(true),
    );

    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "encryption" => {} // usually "none", skip
            "security" => {
                if value != "none" {
                    insert_scalar(&mut proxy, "tls", Value::Bool(true));
                    if value == "reality" {
                        insert_scalar(
                            &mut proxy,
                            "reality-opts",
                            Value::Mapping(Mapping::new()),
                        );
                    }
                }
            }
            "sni" => {
                insert_scalar(&mut proxy, "sni", Value::String(value.to_string()));
            }
            "fp" => {
                insert_scalar(
                    &mut proxy,
                    "client-fingerprint",
                    Value::String(value.to_string()),
                );
            }
            "pbk" => {
                insert_scalar(&mut proxy, "public-key", Value::String(value.to_string()));
            }
            "sid" => {
                insert_scalar(&mut proxy, "short-id", Value::String(value.to_string()));
            }
            "flow" => {
                insert_scalar(&mut proxy, "flow", Value::String(value.to_string()));
            }
            "type" | "net" => {
                if value != "tcp" {
                    insert_scalar(&mut proxy, "network", Value::String(value.to_string()));
                }
            }
            "host" => {
                insert_scalar(
                    &mut proxy,
                    "ws-opts",
                    {
                        let mut ws_opts = Mapping::new();
                        insert_scalar(
                            &mut ws_opts,
                            "headers",
                            {
                                let mut headers = Mapping::new();
                                insert_scalar(&mut headers, "Host", Value::String(value.to_string()));
                                Value::Mapping(headers)
                            },
                        );
                        Value::Mapping(ws_opts)
                    },
                );
            }
            "path" => {
                let network = proxy
                    .get("network")
                    .and_then(|v| v.as_str())
                    .unwrap_or("tcp");
                let opts_key = match network {
                    "ws" => "ws-opts",
                    "h2" => "h2-opts",
                    "grpc" => "grpc-opts",
                    _ => "ws-opts",
                };
                let existing = proxy
                    .get(opts_key)
                    .and_then(|v| v.as_mapping())
                    .cloned()
                    .unwrap_or_default();
                let mut opts = existing;
                if network == "grpc" {
                    insert_scalar(
                        &mut opts,
                        "grpc-service-name",
                        Value::String(value.to_string()),
                    );
                } else {
                    insert_scalar(&mut opts, "path", Value::String(value.to_string()));
                }
                root_proxy_insert(&mut proxy, opts_key, Value::Mapping(opts));
            }
            "alpn" => {
                insert_scalar(
                    &mut proxy,
                    "alpn",
                    Value::Sequence(
                        value
                            .split(',')
                            .map(|s| Value::String(s.trim().to_string()))
                            .collect(),
                    ),
                );
            }
            "skip-cert-verify" => {
                insert_scalar(
                    &mut proxy,
                    "skip-cert-verify",
                    Value::Bool(matches!(value.as_ref(), "1" | "true" | "TRUE")),
                );
            }
            _ => {}
        }
    }

    Ok(UriProxyNode { name, proxy })
}

fn parse_trojan_uri(url: Url) -> Result<UriProxyNode, String> {
    let password = decode_url_component(url.username());
    let server = url
        .host_str()
        .ok_or_else(|| "trojan 节点缺少服务器地址".to_string())?
        .to_string();
    let port = url
        .port()
        .ok_or_else(|| "trojan 节点缺少端口".to_string())?;
    let name = decode_url_component(url.fragment().unwrap_or("Trojan"));

    let mut proxy = Mapping::new();
    insert_scalar(&mut proxy, "name", Value::String(name.clone()));
    insert_scalar(&mut proxy, "type", Value::String("trojan".to_string()));
    insert_scalar(&mut proxy, "server", Value::String(server));
    insert_scalar(&mut proxy, "port", Value::Number(port.into()));
    insert_scalar(&mut proxy, "password", Value::String(password));
    insert_scalar(&mut proxy, "udp", Value::Bool(true));

    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "security" | "type" => {
                if value == "tls" {
                    insert_scalar(&mut proxy, "tls", Value::Bool(true));
                } else if value != "none" {
                    insert_scalar(&mut proxy, "network", Value::String(value.to_string()));
                }
            }
            "sni" | "peer" => {
                insert_scalar(&mut proxy, "sni", Value::String(value.to_string()));
            }
            "fp" => {
                insert_scalar(
                    &mut proxy,
                    "client-fingerprint",
                    Value::String(value.to_string()),
                );
            }
            "alpn" => {
                insert_scalar(
                    &mut proxy,
                    "alpn",
                    Value::Sequence(
                        value
                            .split(',')
                            .map(|s| Value::String(s.trim().to_string()))
                            .collect(),
                    ),
                );
            }
            "host" => {
                insert_scalar(&mut proxy, "sni", Value::String(value.to_string()));
            }
            "path" => {
                let network = proxy
                    .get("network")
                    .and_then(|v| v.as_str())
                    .unwrap_or("tcp");
                let opts_key = match network {
                    "ws" => "ws-opts",
                    "h2" => "h2-opts",
                    "grpc" => "grpc-opts",
                    _ => "ws-opts",
                };
                let existing = proxy
                    .get(opts_key)
                    .and_then(|v| v.as_mapping())
                    .cloned()
                    .unwrap_or_default();
                let mut opts = existing;
                if network == "grpc" {
                    insert_scalar(
                        &mut opts,
                        "grpc-service-name",
                        Value::String(value.to_string()),
                    );
                } else {
                    insert_scalar(&mut opts, "path", Value::String(value.to_string()));
                }
                root_proxy_insert(&mut proxy, opts_key, Value::Mapping(opts));
            }
            "skip-cert-verify" => {
                insert_scalar(
                    &mut proxy,
                    "skip-cert-verify",
                    Value::Bool(matches!(value.as_ref(), "1" | "true" | "TRUE")),
                );
            }
            _ => {}
        }
    }

    Ok(UriProxyNode { name, proxy })
}

fn parse_hysteria2_uri(url: Url) -> Result<UriProxyNode, String> {
    let password = decode_url_component(url.username());
    let server = url
        .host_str()
        .ok_or_else(|| "hysteria2 节点缺少服务器地址".to_string())?
        .to_string();
    let port = url
        .port()
        .ok_or_else(|| "hysteria2 节点缺少端口".to_string())?;
    let name = decode_url_component(url.fragment().unwrap_or("Hysteria2"));

    let mut proxy = Mapping::new();
    insert_scalar(&mut proxy, "name", Value::String(name.clone()));
    insert_scalar(
        &mut proxy,
        "type",
        Value::String("hysteria2".to_string()),
    );
    insert_scalar(&mut proxy, "server", Value::String(server));
    insert_scalar(&mut proxy, "port", Value::Number(port.into()));
    if !password.is_empty() {
        insert_scalar(&mut proxy, "password", Value::String(password));
    }

    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "sni" => {
                insert_scalar(&mut proxy, "sni", Value::String(value.to_string()));
            }
            "insecure" => {
                insert_scalar(
                    &mut proxy,
                    "skip-cert-verify",
                    Value::Bool(matches!(value.as_ref(), "1" | "true" | "TRUE")),
                );
            }
            "obfs" => {
                insert_scalar(&mut proxy, "obfs", Value::String(value.to_string()));
            }
            "obfs-password" => {
                insert_scalar(
                    &mut proxy,
                    "obfs-password",
                    Value::String(value.to_string()),
                );
            }
            "pinSHA256" => {
                insert_scalar(
                    &mut proxy,
                    "pinSHA256",
                    Value::String(value.to_string()),
                );
            }
            _ => {}
        }
    }

    Ok(UriProxyNode { name, proxy })
}

fn parse_tuic_uri(url: Url) -> Result<UriProxyNode, String> {
    let uuid = decode_url_component(url.username());
    let password = decode_url_component(url.password().unwrap_or(""));
    let server = url
        .host_str()
        .ok_or_else(|| "tuic 节点缺少服务器地址".to_string())?
        .to_string();
    let port = url
        .port()
        .ok_or_else(|| "tuic 节点缺少端口".to_string())?;
    let name = decode_url_component(url.fragment().unwrap_or("TUIC"));

    let mut proxy = Mapping::new();
    insert_scalar(&mut proxy, "name", Value::String(name.clone()));
    insert_scalar(&mut proxy, "type", Value::String("tuic".to_string()));
    insert_scalar(&mut proxy, "server", Value::String(server));
    insert_scalar(&mut proxy, "port", Value::Number(port.into()));
    insert_scalar(&mut proxy, "uuid", Value::String(uuid));
    insert_scalar(&mut proxy, "password", Value::String(password));

    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "congestion_control" | "congestion-control" => {
                insert_scalar(
                    &mut proxy,
                    "congestion-control",
                    Value::String(value.to_string()),
                );
            }
            "udp_relay_mode" | "udp-relay-mode" => {
                insert_scalar(
                    &mut proxy,
                    "udp-relay-mode",
                    Value::String(value.to_string()),
                );
            }
            "allow_insecure" | "allow-insecure" => {
                insert_scalar(
                    &mut proxy,
                    "skip-cert-verify",
                    Value::Bool(matches!(value.as_ref(), "1" | "true" | "TRUE")),
                );
            }
            "sni" => {
                insert_scalar(&mut proxy, "sni", Value::String(value.to_string()));
            }
            "alpn" => {
                insert_scalar(
                    &mut proxy,
                    "alpn",
                    Value::Sequence(
                        value
                            .split(',')
                            .map(|s| Value::String(s.trim().to_string()))
                            .collect(),
                    ),
                );
            }
            _ => {}
        }
    }

    Ok(UriProxyNode { name, proxy })
}

fn root_proxy_insert(proxy: &mut Mapping, key: &str, value: Value) {
    proxy.insert(Value::String(key.to_string()), value);
}

fn strip_region_flags(name: &str) -> String {
    name.chars()
        .skip_while(|c| ('\u{1F1E6}'..='\u{1F1FF}').contains(c))
        .collect::<String>()
        .trim()
        .to_string()
}

fn is_info_node_name(name: &str) -> bool {
    name.contains("流量")
        || name.contains("重置")
        || name.contains("到期")
        || name.contains("官网")
        || name.contains("套餐")
        || name.contains("剩余")
        || name.contains("用量")
        || name.contains("公告")
        || name.contains("通知")
        || name.contains("续费")
        || name.contains("购买")
        || name.contains("过期")
        || name.contains("更新")
        || name.contains("升级")
        || name.contains("教程")
}

fn extract_region(name: &str) -> String {
    let no_flags: String = name
        .chars()
        .filter(|c| !('\u{1F1E6}'..='\u{1F1FF}').contains(c))
        .collect();
    let name = no_flags.trim();

    // Handle "美国直连-0.5倍率" → "美国"
    let name = name.replace("直连", "");

    // Handle "英国-3倍率" → "英国"
    if let Some(idx) = name.find('-') {
        let after_dash = &name[idx + 1..];
        if after_dash
            .chars()
            .next()
            .map_or(false, |c| c.is_ascii_digit())
        {
            return name[..idx].trim().to_string();
        }
    }

    name.trim_end_matches(|c: char| c.is_ascii_digit())
        .trim()
        .to_string()
}

fn build_document_from_uri_nodes(nodes: Vec<UriProxyNode>) -> Value {
    // Separate info nodes from actual proxy nodes
    let (info_nodes, proxy_nodes): (Vec<_>, Vec<_>) = nodes
        .into_iter()
        .partition(|node| is_info_node_name(&node.name));

    let proxies: Vec<Value> = proxy_nodes
        .iter()
        .map(|node| Value::Mapping(node.proxy.clone()))
        .chain(
            info_nodes
                .iter()
                .map(|node| Value::Mapping(node.proxy.clone())),
        )
        .collect();

    // Group proxy nodes by region
    let mut regions: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for node in &proxy_nodes {
        let region = extract_region(&node.name);
        if !region.is_empty() {
            regions.entry(region).or_default().push(node.name.clone());
        }
    }

    let mut groups: Vec<Value> = Vec::new();
    let mut region_group_names: Vec<String> = Vec::new();

    // Sort regions by node count descending
    let mut region_entries: Vec<(String, Vec<String>)> = regions.into_iter().collect();
    region_entries.sort_by(|a, b| b.1.len().cmp(&a.1.len()));

    for (region, node_names) in &region_entries {
        let group_name = format!("{} - {} 条", region, node_names.len());
        let group_proxies: Vec<Value> = node_names
            .iter()
            .map(|n| Value::String(n.clone()))
            .collect();

        let mut group = Mapping::new();
        insert_scalar(&mut group, "name", Value::String(group_name.clone()));
        insert_scalar(&mut group, "type", Value::String("url-test".to_string()));
        insert_scalar(
            &mut group,
            "url",
            Value::String("http://www.gstatic.com/generate_204".to_string()),
        );
        insert_scalar(&mut group, "interval", Value::Number(300.into()));
        group.insert(
            Value::String("proxies".to_string()),
            Value::Sequence(group_proxies),
        );

        groups.push(Value::Mapping(group));
        region_group_names.push(group_name);
    }

    // Main select group: includes region groups + all individual nodes
    let mut main_proxies: Vec<Value> = region_group_names
        .iter()
        .map(|n| Value::String(n.clone()))
        .collect();
    for node in &proxy_nodes {
        main_proxies.push(Value::String(node.name.clone()));
    }

    let mut main_group = Mapping::new();
    insert_scalar(&mut main_group, "name", Value::String("Proxy".to_string()));
    insert_scalar(&mut main_group, "type", Value::String("select".to_string()));
    main_group.insert(
        Value::String("proxies".to_string()),
        Value::Sequence(main_proxies),
    );

    groups.insert(0, Value::Mapping(main_group));

    // Build document
    let mut root = Mapping::new();
    root.insert(
        Value::String("proxies".to_string()),
        Value::Sequence(proxies),
    );
    root.insert(
        Value::String("proxy-groups".to_string()),
        Value::Sequence(groups),
    );

    let rules: Vec<Value> = default_rules().into_iter().map(Value::String).collect();
    root.insert(Value::String("rules".to_string()), Value::Sequence(rules));

    Value::Mapping(root)
}

fn apply_runtime_settings(
    document: &mut Value,
    mode: &str,
    tun_enabled: bool,
    dns_override: Option<&DnsOverride>,
    custom_rules: &[String],
) -> Result<(), String> {
    let root = document
        .as_mapping_mut()
        .ok_or_else(|| "订阅配置格式无效".to_string())?;

    insert_scalar(root, "mixed-port", Value::Number(7897.into()));
    insert_scalar(root, "allow-lan", Value::Bool(false));
    insert_scalar(root, "mode", Value::String(mode.to_string()));
    insert_scalar(
        root,
        "external-controller",
        Value::String("127.0.0.1:9090".to_string()),
    );
    insert_scalar(root, "secret", Value::String(String::new()));

    // Prepend custom rules before subscription rules (custom rules take priority)
    if !custom_rules.is_empty() {
        let custom_entries: Vec<Value> = custom_rules
            .iter()
            .map(|r| Value::String(r.clone()))
            .collect();
        let existing_rules: Vec<Value> = root
            .get("rules")
            .and_then(|v| v.as_sequence())
            .map(|seq| seq.iter().cloned().collect())
            .unwrap_or_default();
        let merged: Vec<Value> = custom_entries.into_iter().chain(existing_rules).collect();
        root.insert(Value::String("rules".to_string()), Value::Sequence(merged));
    }

    root.remove(&Value::String("tun".to_string()));
    if tun_enabled {
        let mut tun_section = Mapping::new();
        insert_scalar(&mut tun_section, "enable", Value::Bool(true));
        insert_scalar(
            &mut tun_section,
            "stack",
            Value::String("system".to_string()),
        );
        tun_section.insert(
            Value::String("dns-hijack".to_string()),
            Value::Sequence(vec![Value::String("any:53".to_string())]),
        );
        insert_scalar(&mut tun_section, "auto-route", Value::Bool(true));
        insert_scalar(&mut tun_section, "auto-detect-interface", Value::Bool(true));
        root.insert(
            Value::String("tun".to_string()),
            Value::Mapping(tun_section),
        );
    }

    if let Some(dns) = dns_override {
        if dns.enabled {
            root.insert(Value::String("dns".to_string()), dns.config.clone());
        } else {
            // DNS override explicitly disabled: remove subscription DNS, let mihomo use system DNS
            root.remove(&Value::String("dns".to_string()));
        }
    } else {
        // No DNS override configured: remove subscription DNS to avoid fake-ip breaking
        // internal domains that require system DNS resolution
        root.remove(&Value::String("dns".to_string()));
    }

    Ok(())
}

fn decode_base64_subscription(content: &str) -> Option<String> {
    let compact = content.split_whitespace().collect::<String>();
    if compact.is_empty() {
        return None;
    }

    let padded = format!("{}{}", compact, "=".repeat((4 - compact.len() % 4) % 4));
    general_purpose::STANDARD
        .decode(padded.as_bytes())
        .or_else(|_| general_purpose::URL_SAFE.decode(padded.as_bytes()))
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())
}

fn decode_url_component(value: &str) -> String {
    urlencoding::decode(value)
        .map(|decoded| decoded.into_owned())
        .unwrap_or_else(|_| value.to_string())
}

fn insert_scalar(root: &mut Mapping, key: &str, value: Value) {
    root.insert(Value::String(key.to_string()), value);
}
