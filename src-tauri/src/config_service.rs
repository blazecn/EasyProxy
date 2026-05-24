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
                .filter_map(|n| n.proxy.get("type")?.as_str().map(|t| (n.name.clone(), t.to_string())))
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

pub fn build_mihomo_config(content: &str, mode: &str, tun_enabled: bool) -> Result<String, String> {
    let mut document = match parse_document(content)? {
        SubscriptionDocument::Clash(document) => document,
        SubscriptionDocument::UriList(nodes) => build_document_from_uri_nodes(nodes),
    };

    apply_runtime_settings(&mut document, mode, tun_enabled)?;

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
    let mut yaml_doc: Value = parse_yaml_or_base64(yaml_content)
        .ok_or_else(|| "无法解析 YAML 订阅内容".to_string())?;

    let uri_nodes = match parse_uri_or_base64(uri_content) {
        Some(nodes) => nodes,
        None => return Ok(yaml_content.to_string()),
    };

    let (info_nodes, proxy_nodes): (Vec<_>, Vec<_>) =
        uri_nodes.into_iter().partition(|n| is_info_node_name(&n.name));

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
                let group_type = group_map
                    .get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                let this_group_name = group_map
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

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

                group_map.insert(
                    Value::String("proxies".to_string()),
                    Value::Sequence(kept),
                );
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

pub fn write_runtime_config(path: &Path, content: &str, mode: &str, tun_enabled: bool) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("创建运行目录失败: {error}"))?;
    }

    let config = build_mihomo_config(content, mode, tun_enabled)?;
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
    let nodes = content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| line.contains("://"))
        .map(parse_proxy_uri)
        .collect::<Result<Vec<_>, _>>()?;

    if nodes.is_empty() {
        return Err("订阅中没有可用 URI 节点".to_string());
    }

    Ok(nodes)
}

fn parse_proxy_uri(line: &str) -> Result<UriProxyNode, String> {
    let url = Url::parse(line).map_err(|error| format!("节点 URI 解析失败: {error}"))?;
    match url.scheme() {
        "anytls" => parse_anytls_uri(url),
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
    let (info_nodes, proxy_nodes): (Vec<_>, Vec<_>) =
        nodes.into_iter().partition(|node| is_info_node_name(&node.name));

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

    let rules: Vec<Value> = default_rules()
        .into_iter()
        .map(Value::String)
        .collect();
    root.insert(Value::String("rules".to_string()), Value::Sequence(rules));

    Value::Mapping(root)
}

fn apply_runtime_settings(document: &mut Value, mode: &str, tun_enabled: bool) -> Result<(), String> {
    let root = document
        .as_mapping_mut()
        .ok_or_else(|| "订阅配置格式无效".to_string())?;

    insert_scalar(root, "mixed-port", Value::Number(7890.into()));
    insert_scalar(root, "allow-lan", Value::Bool(false));
    insert_scalar(root, "mode", Value::String(mode.to_string()));
    insert_scalar(
        root,
        "external-controller",
        Value::String("127.0.0.1:9090".to_string()),
    );
    insert_scalar(root, "secret", Value::String(String::new()));

    if tun_enabled {
        let mut tun_section = Mapping::new();
        insert_scalar(&mut tun_section, "enable", Value::Bool(true));
        insert_scalar(&mut tun_section, "stack", Value::String("system".to_string()));
        tun_section.insert(
            Value::String("dns-hijack".to_string()),
            Value::Sequence(vec![Value::String("any:53".to_string())]),
        );
        insert_scalar(&mut tun_section, "auto-route", Value::Bool(true));
        insert_scalar(&mut tun_section, "auto-detect-interface", Value::Bool(true));
        root.insert(Value::String("tun".to_string()), Value::Mapping(tun_section));
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
