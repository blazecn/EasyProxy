pub mod config_service;
pub mod core_manager;
pub mod ipc;
pub mod mihomo_api;
pub mod service_manager;
pub mod system_proxy;

use config_service::{
    merge_subscription, save_subscription as save_subscription_file, write_runtime_config,
    SubscriptionSummary,
};
use core_manager::{CoreManager, CoreStatus};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use system_proxy::{ProxyMode, SystemProxy};
use tauri::{Manager, State};

struct AppState {
    subscription: Mutex<Option<String>>,
    mode: Mutex<ProxyMode>,
    tun_enabled: Mutex<bool>,
    dns_override: Mutex<Option<config_service::DnsOverride>>,
    subscriptions: Mutex<Vec<SavedSubscription>>,
    selected_nodes: Mutex<HashMap<String, String>>,
    custom_rules: Mutex<Vec<String>>,
    data_dir: PathBuf,
    core: CoreManager,
    proxy: SystemProxy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AppStatus {
    core: CoreStatus,
    mode: ProxyMode,
    system_proxy: String,
    tun_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ImportedSubscription {
    nodes: Vec<String>,
    format: String,
    content: String,
    rules: Vec<String>,
    groups: Vec<config_service::ProxyGroupSummary>,
    node_types: std::collections::BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SavedSubscription {
    name: String,
    url: String,
    content: String,
    nodes: Vec<String>,
    format: String,
    rules: Vec<String>,
    groups: Vec<config_service::ProxyGroupSummary>,
    node_types: std::collections::BTreeMap<String, String>,
}

fn core_status_inner(state: &AppState) -> Result<AppStatus, String> {
    let mode = *state
        .mode
        .lock()
        .map_err(|_| "读取模式状态失败".to_string())?;
    let tun_enabled = *state
        .tun_enabled
        .lock()
        .map_err(|_| "读取 TUN 状态失败".to_string())?;

    let system_proxy = if state.proxy.is_enabled() {
        state.proxy.endpoint()
    } else {
        String::new()
    };
    Ok(AppStatus {
        core: state.core.status(),
        mode,
        system_proxy,
        tun_enabled,
    })
}

#[tauri::command]
fn core_status(state: State<'_, AppState>) -> Result<AppStatus, String> {
    core_status_inner(&state)
}

#[tauri::command]
fn save_subscription(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    content: String,
) -> Result<SubscriptionSummary, String> {
    let data_dir = app_data_dir(&app)?;
    let subscription_path = data_dir.join("subscription.yaml");
    let summary = save_subscription_file(&subscription_path, &content)?;
    let mode = *state
        .mode
        .lock()
        .map_err(|_| "读取模式状态失败".to_string())?;
    let dns_override = state
        .dns_override
        .lock()
        .map_err(|_| "读取 DNS 覆写失败".to_string())?
        .clone();
    write_runtime_config(
        &data_dir.join("mihomo.yaml"),
        &content,
        mode.as_mihomo_mode(),
        false,
        dns_override.as_ref(),
    )?;
    *state
        .subscription
        .lock()
        .map_err(|_| "保存订阅状态失败".to_string())? = Some(content);
    Ok(summary)
}

#[tauri::command]
async fn refresh_subscription(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    url: String,
) -> Result<ImportedSubscription, String> {
    let client = reqwest::Client::new();
    let yaml_text = client
        .get(&url)
        .header("User-Agent", "clash-verge/2.0")
        .send()
        .await
        .map_err(|e| format!("请求订阅失败: {e}"))?
        .text()
        .await
        .map_err(|e| format!("读取订阅内容失败: {e}"))?;

    let merged = match client
        .get(&url)
        .header("User-Agent", "mihomo")
        .send()
        .await
    {
        Ok(resp) => match resp.text().await {
            Ok(uri_text) => merge_subscription(&yaml_text, &uri_text).unwrap_or(yaml_text),
            Err(_) => yaml_text,
        },
        Err(_) => yaml_text,
    };

    let summary = save_subscription(app, state, merged.clone())?;
    Ok(ImportedSubscription {
        nodes: summary.nodes,
        format: summary.format,
        content: merged,
        rules: summary.rules,
        groups: summary.groups,
        node_types: summary.node_types,
    })
}

#[tauri::command]
fn set_proxy_mode(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    mode: ProxyMode,
) -> Result<AppStatus, String> {
    *state
        .mode
        .lock()
        .map_err(|_| "保存模式状态失败".to_string())? = mode;

    if let Some(subscription) = state
        .subscription
        .lock()
        .map_err(|_| "读取订阅状态失败".to_string())?
        .as_ref()
    {
        let tun_enabled = *state
            .tun_enabled
            .lock()
            .map_err(|_| "读取 TUN 状态失败".to_string())?;
        let dns_override = state
            .dns_override
            .lock()
            .map_err(|_| "读取 DNS 覆写失败".to_string())?
            .clone();
        write_runtime_config(
            &app_data_dir(&app)?.join("mihomo.yaml"),
            subscription,
            mode.as_mihomo_mode(),
            tun_enabled,
            dns_override.as_ref(),
        )?;
    }

    core_status(state)
}

#[tauri::command]
async fn start_core(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<AppStatus, String> {
    let tun_enabled = *state.tun_enabled.lock().map_err(|_| "读取 TUN 状态失败".to_string())?;
    start_core_inner_async(
        tun_enabled,
        state.core.clone(),
        state.data_dir.clone(),
        app.path().resource_dir().map_err(|e| format!("获取资源目录失败: {e}"))?,
    )
    .await?;
    core_status_inner(&state)
}

#[tauri::command]
async fn stop_core(state: State<'_, AppState>) -> Result<AppStatus, String> {
    let tun_enabled = *state.tun_enabled.lock().map_err(|_| "读取 TUN 状态失败".to_string())?;
    stop_core_inner_async(tun_enabled, state.core.clone()).await?;
    core_status_inner(&state)
}

#[tauri::command]
async fn set_system_proxy(state: State<'_, AppState>, enable: bool) -> Result<AppStatus, String> {
    let proxy = state.proxy.clone();
    tauri::async_runtime::spawn_blocking(move || {
        if enable {
            proxy.enable()
        } else {
            proxy.disable()
        }
    })
    .await
    .map_err(|e| format!("系统代理操作失败: {e}"))??;
    core_status_inner(&state)
}

#[tauri::command]
fn select_proxy_node(group: String, node: String) -> Result<(), String> {
    mihomo_api::select_proxy_node(&group, &node)
}

#[tauri::command]
async fn test_delays(nodes: Vec<String>) -> Result<std::collections::BTreeMap<String, mihomo_api::DelayResult>, String> {
    tauri::async_runtime::spawn_blocking(move || mihomo_api::test_proxy_delays(&nodes))
        .await
        .map_err(|e| format!("测速失败: {e}"))
}

#[tauri::command]
fn get_dns_override(state: State<'_, AppState>) -> Result<Option<config_service::DnsOverride>, String> {
    Ok(state
        .dns_override
        .lock()
        .map_err(|_| "读取 DNS 覆写状态失败".to_string())?
        .clone())
}

#[tauri::command]
fn set_dns_override(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    dns_override: config_service::DnsOverride,
) -> Result<AppStatus, String> {
    let data_dir = app_data_dir(&app)?;
    let yaml = serde_yaml::to_string(&dns_override)
        .map_err(|e| format!("序列化 DNS 覆写配置失败: {e}"))?;
    std::fs::write(data_dir.join("dns_override.yaml"), yaml)
        .map_err(|e| format!("保存 DNS 覆写配置失败: {e}"))?;

    *state
        .dns_override
        .lock()
        .map_err(|_| "保存 DNS 覆写状态失败".to_string())? = Some(dns_override);

    let subscription = state
        .subscription
        .lock()
        .map_err(|_| "读取订阅失败".to_string())?
        .clone();
    let mode = *state.mode.lock().map_err(|_| "读取模式失败".to_string())?;
    let tun_enabled = *state
        .tun_enabled
        .lock()
        .map_err(|_| "读取 TUN 状态失败".to_string())?;

    if let Some(ref sub) = subscription {
        let dns_ref = state
            .dns_override
            .lock()
            .map_err(|_| "读取 DNS 覆写失败".to_string())?
            .clone();
        write_runtime_config(
            &data_dir.join("mihomo.yaml"),
            sub,
            mode.as_mihomo_mode(),
            tun_enabled,
            dns_ref.as_ref(),
        )?;
    }

    core_status_inner(&state)
}

fn app_data_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("获取应用数据目录失败: {error}"))?;
    fs::create_dir_all(&dir).map_err(|error| format!("创建应用数据目录失败: {error}"))?;
    Ok(dir)
}

fn default_core_manager(app: &tauri::AppHandle) -> Result<CoreManager, String> {
    let data_dir = app_data_dir(app)?;
    let binary_name = if cfg!(target_os = "windows") {
        "mihomo.exe"
    } else {
        "mihomo"
    };
    let binary_path = app
        .path()
        .resource_dir()
        .map_err(|error| format!("获取资源目录失败: {error}"))?
        .join("binaries")
        .join(binary_name);

    Ok(CoreManager::new(binary_path, data_dir.join("mihomo.yaml")))
}

fn ensure_mmdb(data_dir: &std::path::Path) -> Result<(), String> {
    let mmdb_path = data_dir.join("geoip.metadb");
    if mmdb_path.exists() {
        return Ok(());
    }
    if let Some(home) = std::env::var_os("HOME") {
        let src = std::path::PathBuf::from(home).join(".config/mihomo/geoip.metadb");
        if src.exists() {
            std::fs::copy(&src, &mmdb_path)
                .map_err(|e| format!("复制 MMDB 文件失败: {e}"))?;
            return Ok(());
        }
    }
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {e}"))?;
    let resp = client
        .get("https://github.com/MetaCubeX/meta-rules-dat/releases/download/latest/geoip.metadb")
        .send()
        .map_err(|e| format!("下载 MMDB 失败: {e}"))?;
    let bytes = resp.bytes().map_err(|e| format!("读取 MMDB 响应失败: {e}"))?;
    std::fs::write(&mmdb_path, &bytes).map_err(|e| format!("写入 MMDB 文件失败: {e}"))?;
    Ok(())
}

async fn stop_core_inner_async(tun_enabled: bool, core: CoreManager) -> Result<(), String> {
    if tun_enabled {
        tauri::async_runtime::spawn_blocking(|| {
            let mut stream = ipc::connect()?;
            let msg = ipc::IpcMessage::new("stop", None);
            ipc::send_message(&mut stream, &msg)?;
            ipc::recv_message(&mut stream)?;
            Ok::<_, String>(())
        })
        .await
        .map_err(|e| format!("停止 TUN 内核失败: {e}"))??;
    } else {
        core.stop()?;
    }
    Ok(())
}

async fn start_core_inner_async(
    tun_enabled: bool,
    core: CoreManager,
    data_dir: PathBuf,
    resource_dir: PathBuf,
) -> Result<(), String> {
    if tun_enabled {
        let data_dir_clone = data_dir.clone();
        tauri::async_runtime::spawn_blocking(move || {
            ensure_mmdb(&data_dir_clone)?;
            let binary_path = resource_dir.join("binaries").join("mihomo");
            let config_path = data_dir_clone.join("mihomo.yaml");

            let mut stream = ipc::connect()?;
            let msg = ipc::IpcMessage::new(
                "start",
                Some(serde_json::json!({
                    "binary_path": binary_path.to_str().unwrap_or(""),
                    "config_path": config_path.to_str().unwrap_or(""),
                    "working_dir": data_dir_clone.to_str().unwrap_or(""),
                })),
            );
            ipc::send_message(&mut stream, &msg)?;
            ipc::recv_message(&mut stream)?;
            Ok::<_, String>(())
        })
        .await
        .map_err(|e| format!("启动 TUN 内核失败: {e}"))??;
    } else {
        core.start()?;
    }
    Ok(())
}

#[tauri::command]
async fn set_tun_mode(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<AppStatus, String> {
    if !cfg!(target_os = "macos") {
        return Err("TUN 模式当前仅支持 macOS".to_string());
    }

    if enabled {
        let service_bin = app
            .path()
            .resource_dir()
            .map_err(|e| format!("获取资源目录失败: {e}"))?
            .join("binaries")
            .join("easyproxy-service");
        let bin_path = service_bin.to_str().unwrap_or("").to_string();
        if service_manager::needs_update(&bin_path) {
            tauri::async_runtime::spawn_blocking(move || service_manager::install(&bin_path))
                .await
                .map_err(|e| format!("安装服务失败: {e}"))??;
        }
    }

    let tun_was_running = *state
        .tun_enabled
        .lock()
        .map_err(|_| "读取 TUN 状态失败".to_string())?;
    let direct_was_running = state.core.status() == CoreStatus::Running;
    let was_running = tun_was_running || direct_was_running;
    if was_running {
        stop_core_inner_async(tun_was_running, state.core.clone()).await?;
    }

    *state
        .tun_enabled
        .lock()
        .map_err(|_| "保存 TUN 状态失败".to_string())? = enabled;

    let data_dir = state.data_dir.clone();
    let subscription = state
        .subscription
        .lock()
        .map_err(|_| "读取订阅失败".to_string())?
        .clone();
    let mode = *state
        .mode
        .lock()
        .map_err(|_| "读取模式失败".to_string())?;

    if let Some(ref sub) = subscription {
        let config_path = data_dir.join("mihomo.yaml");
        let sub = sub.clone();
        let dns = state
            .dns_override
            .lock()
            .map_err(|_| "读取 DNS 覆写失败".to_string())?
            .clone();
        tauri::async_runtime::spawn_blocking(move || {
            write_runtime_config(
                &config_path,
                &sub,
                mode.as_mihomo_mode(),
                enabled,
                dns.as_ref(),
            )
        })
        .await
        .map_err(|e| format!("写入配置失败: {e}"))??;
    }

    if enabled || direct_was_running {
        start_core_inner_async(
            enabled,
            state.core.clone(),
            state.data_dir.clone(),
            app.path()
                .resource_dir()
                .map_err(|e| format!("获取资源目录失败: {e}"))?,
        )
        .await?;
    }

    core_status_inner(&state)
}

#[tauri::command]
fn load_subscriptions(state: State<'_, AppState>) -> Result<Vec<SavedSubscription>, String> {
    Ok(state
        .subscriptions
        .lock()
        .map_err(|_| "读取订阅列表失败".to_string())?
        .clone())
}

#[tauri::command]
fn save_subscriptions(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    subscriptions: Vec<SavedSubscription>,
) -> Result<(), String> {
    let data_dir = app_data_dir(&app)?;
    let yaml = serde_yaml::to_string(&subscriptions)
        .map_err(|e| format!("序列化订阅列表失败: {e}"))?;
    std::fs::write(data_dir.join("subscriptions.yaml"), yaml)
        .map_err(|e| format!("保存订阅列表失败: {e}"))?;
    *state
        .subscriptions
        .lock()
        .map_err(|_| "更新订阅列表状态失败".to_string())? = subscriptions;
    Ok(())
}

#[tauri::command]
fn load_selected_nodes(state: State<'_, AppState>) -> Result<HashMap<String, String>, String> {
    Ok(state
        .selected_nodes
        .lock()
        .map_err(|_| "读取节点选择失败".to_string())?
        .clone())
}

#[tauri::command]
fn save_selected_nodes(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    nodes: HashMap<String, String>,
) -> Result<(), String> {
    let data_dir = app_data_dir(&app)?;
    let yaml = serde_yaml::to_string(&nodes)
        .map_err(|e| format!("序列化节点选择失败: {e}"))?;
    std::fs::write(data_dir.join("selected_nodes.yaml"), yaml)
        .map_err(|e| format!("保存节点选择失败: {e}"))?;
    *state
        .selected_nodes
        .lock()
        .map_err(|_| "更新节点选择状态失败".to_string())? = nodes;
    Ok(())
}

#[tauri::command]
fn load_custom_rules(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    Ok(state
        .custom_rules
        .lock()
        .map_err(|_| "读取自定义规则失败".to_string())?
        .clone())
}

#[tauri::command]
fn save_custom_rules(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    rules: Vec<String>,
) -> Result<(), String> {
    let data_dir = app_data_dir(&app)?;
    let yaml = serde_yaml::to_string(&rules)
        .map_err(|e| format!("序列化自定义规则失败: {e}"))?;
    std::fs::write(data_dir.join("custom_rules.yaml"), yaml)
        .map_err(|e| format!("保存自定义规则失败: {e}"))?;
    *state
        .custom_rules
        .lock()
        .map_err(|_| "更新自定义规则状态失败".to_string())? = rules;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let data_dir = app_data_dir(app.handle())?;
            let core = default_core_manager(app.handle())?;
            let subscription = std::fs::read_to_string(data_dir.join("subscription.yaml")).ok();
            let dns_override = std::fs::read_to_string(data_dir.join("dns_override.yaml"))
                .ok()
                .and_then(|s| serde_yaml::from_str(&s).ok());
            let subscriptions: Vec<SavedSubscription> =
                std::fs::read_to_string(data_dir.join("subscriptions.yaml"))
                    .ok()
                    .and_then(|s| serde_yaml::from_str(&s).ok())
                    .unwrap_or_default();
            let selected_nodes: HashMap<String, String> =
                std::fs::read_to_string(data_dir.join("selected_nodes.yaml"))
                    .ok()
                    .and_then(|s| serde_yaml::from_str(&s).ok())
                    .unwrap_or_default();
            let custom_rules: Vec<String> =
                std::fs::read_to_string(data_dir.join("custom_rules.yaml"))
                    .ok()
                    .and_then(|s| serde_yaml::from_str(&s).ok())
                    .unwrap_or_default();
            app.manage(AppState {
                subscription: Mutex::new(subscription),
                mode: Mutex::new(ProxyMode::Rule),
                tun_enabled: Mutex::new(false),
                dns_override: Mutex::new(dns_override),
                subscriptions: Mutex::new(subscriptions),
                selected_nodes: Mutex::new(selected_nodes),
                custom_rules: Mutex::new(custom_rules),
                data_dir,
                core,
                proxy: SystemProxy::new("127.0.0.1", 7890),
            });
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            // Auto-start core on launch (non-TUN mode)
            let state = app.state::<AppState>();
            if let Err(e) = state.core.start() {
                log::warn!("自动启动 Mihomo 内核失败: {e}");
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            core_status,
            save_subscription,
            refresh_subscription,
            select_proxy_node,
            set_proxy_mode,
            set_system_proxy,
            set_tun_mode,
            start_core,
            stop_core,
            test_delays,
            get_dns_override,
            set_dns_override,
            load_subscriptions,
            save_subscriptions,
            load_selected_nodes,
            save_selected_nodes,
            load_custom_rules,
            save_custom_rules,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
