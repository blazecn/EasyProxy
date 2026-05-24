use serde_json::json;
use std::collections::BTreeMap;
use std::time::Duration;

const CONTROLLER: &str = "http://127.0.0.1:9090";

pub fn select_proxy_node(group: &str, node: &str) -> Result<(), String> {
    let client = reqwest::blocking::Client::new();
    let response = client
        .put(format!("{CONTROLLER}/proxies/{group}"))
        .json(&json!({ "name": node }))
        .send()
        .map_err(|error| format!("切换节点失败: {error}"))?;

    if response.status().is_success() {
        Ok(())
    } else {
        Err(format!("切换节点失败: Mihomo 返回 {}", response.status()))
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DelayResult {
    pub delay: Option<u64>,
    pub error: Option<String>,
}

pub fn test_proxy_delays(nodes: &[String]) -> BTreeMap<String, DelayResult> {
    use std::sync::mpsc;
    use std::thread;

    let (tx, rx) = mpsc::channel();
    let total = nodes.len();

    for name in nodes.iter().cloned() {
        let tx = tx.clone();
        thread::spawn(move || {
            let result = test_single_proxy(&name);
            let _ = tx.send((name, result));
        });
    }
    drop(tx);

    let mut results = BTreeMap::new();
    for _ in 0..total {
        if let Ok((name, result)) = rx.recv() {
            results.insert(name, result);
        }
    }
    results
}

fn encode_uri_component(s: &str) -> String {
    s.bytes().map(|b| {
        if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b'!' || b == b'~' || b == b'*' || b == b'\'' || b == b'(' || b == b')' {
            (b as char).to_string()
        } else {
            format!("%{:02X}", b)
        }
    }).collect()
}

fn test_single_proxy(name: &str) -> DelayResult {
    let delay_url = format!(
        "{}/proxies/{}/delay?url={}&timeout=5000",
        CONTROLLER,
        encode_uri_component(name),
        encode_uri_component("http://www.gstatic.com/generate_204"),
    );

    let client = match reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(8))
        .build()
    {
        Ok(c) => c,
        Err(e) => return DelayResult { delay: None, error: Some(format!("client build: {e}")) },
    };

    let resp = match client.get(&delay_url).send() {
        Ok(r) => r,
        Err(e) => return DelayResult { delay: None, error: Some(format!("{e}")) },
    };

    let status = resp.status();
    let json: serde_json::Value = match resp.json() {
        Ok(j) => j,
        Err(e) => return DelayResult { delay: None, error: Some(format!("json parse: {e}")) },
    };

    if status.is_success() {
        match json.get("delay").and_then(|v| v.as_u64()) {
            Some(d) => DelayResult { delay: Some(d), error: None },
            None => DelayResult { delay: None, error: Some(format!("missing delay field: {json}")) },
        }
    } else {
        DelayResult { delay: None, error: Some(format!("HTTP {status}: {json}")) }
    }
}
