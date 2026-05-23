use base64::{engine::general_purpose, Engine};
use serde::{Deserialize, Serialize};
use serde_yaml::{Mapping, Value};
use std::fs;
use std::path::Path;
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubscriptionSummary {
    pub nodes: Vec<String>,
    pub format: String,
}

pub fn parse_subscription(content: &str) -> Result<SubscriptionSummary, String> {
    match parse_document(content)? {
        SubscriptionDocument::Clash(document) => {
            let nodes = node_names_from_yaml(&document)?;
            Ok(SubscriptionSummary {
                nodes,
                format: "clash-yaml".to_string(),
            })
        }
        SubscriptionDocument::UriList(nodes) => Ok(SubscriptionSummary {
            nodes: nodes.into_iter().map(|node| node.name).collect(),
            format: "uri-list".to_string(),
        }),
    }
}

pub fn build_mihomo_config(content: &str, mode: &str) -> Result<String, String> {
    let mut document = match parse_document(content)? {
        SubscriptionDocument::Clash(document) => document,
        SubscriptionDocument::UriList(nodes) => build_document_from_uri_nodes(nodes),
    };

    apply_runtime_settings(&mut document, mode)?;

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

pub fn write_runtime_config(path: &Path, content: &str, mode: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("创建运行目录失败: {error}"))?;
    }

    let config = build_mihomo_config(content, mode)?;
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

fn build_document_from_uri_nodes(nodes: Vec<UriProxyNode>) -> Value {
    let node_names = nodes
        .iter()
        .map(|node| Value::String(node.name.clone()))
        .collect::<Vec<_>>();
    let proxies = nodes
        .into_iter()
        .map(|node| Value::Mapping(node.proxy))
        .collect::<Vec<_>>();

    let mut group = Mapping::new();
    insert_scalar(&mut group, "name", Value::String("Proxy".to_string()));
    insert_scalar(&mut group, "type", Value::String("select".to_string()));
    group.insert(
        Value::String("proxies".to_string()),
        Value::Sequence(node_names),
    );

    let mut root = Mapping::new();
    root.insert(
        Value::String("proxies".to_string()),
        Value::Sequence(proxies),
    );
    root.insert(
        Value::String("proxy-groups".to_string()),
        Value::Sequence(vec![Value::Mapping(group)]),
    );
    root.insert(
        Value::String("rules".to_string()),
        Value::Sequence(vec![Value::String("MATCH,Proxy".to_string())]),
    );

    Value::Mapping(root)
}

fn apply_runtime_settings(document: &mut Value, mode: &str) -> Result<(), String> {
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
