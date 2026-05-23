use serde_json::json;

const CONTROLLER: &str = "http://127.0.0.1:9090";

pub fn select_proxy_node(node: &str) -> Result<(), String> {
    let client = reqwest::blocking::Client::new();
    let response = client
        .put(format!("{CONTROLLER}/proxies/Proxy"))
        .json(&json!({ "name": node }))
        .send()
        .map_err(|error| format!("切换节点失败: {error}"))?;

    if response.status().is_success() {
        Ok(())
    } else {
        Err(format!("切换节点失败: Mihomo 返回 {}", response.status()))
    }
}
