pub mod autostart;
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
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex, MutexGuard,
};
use system_proxy::{ProxyMode, SystemProxy};
use tauri::{
    image::Image,
    menu::{CheckMenuItemBuilder, MenuBuilder, MenuEvent, MenuItemBuilder, SubmenuBuilder},
    tray::TrayIconBuilder,
    ActivationPolicy, Emitter, Manager, RunEvent, State,
};

struct AppState {
    subscription: Mutex<Option<String>>,
    mode: Mutex<ProxyMode>,
    tun_enabled: Mutex<bool>,
    dns_override: Mutex<Option<config_service::DnsOverride>>,
    subscriptions: Mutex<Vec<SavedSubscription>>,
    selected_nodes: Mutex<HashMap<String, String>>,
    custom_rules: Mutex<Vec<String>>,
    proxy_bypass: Mutex<Vec<String>>,
    autostart_enabled: Mutex<bool>,
    data_dir: PathBuf,
    core: CoreManager,
    proxy: SystemProxy,
    is_quitting: AtomicBool,
}

const MIXED_PORT: u16 = 7897;

fn default_bypass_domains() -> Vec<String> {
    vec![
        "localhost".to_string(),
        "127.0.0.1".to_string(),
        "::1".to_string(),
        "*.local".to_string(),
        "10.0.0.0/8".to_string(),
        "172.16.0.0/12".to_string(),
        "192.168.0.0/16".to_string(),
        "169.254.0.0/16".to_string(),
    ]
}

fn show_fatal_error(message: &str) {
    eprintln!("EasyProxy 启动失败: {message}");
    #[cfg(target_os = "macos")]
    {
        let script = format!(
            "display alert \"EasyProxy 无法启动\" message \"{}\" as critical buttons {{\"退出\"}}",
            message.replace('\\', "\\\\").replace('"', "\\\""),
        );
        let _ = std::process::Command::new("osascript")
            .arg("-e")
            .arg(&script)
            .status();
    }
}

fn ensure_mixed_port_free(port: u16) -> Result<(), String> {
    match std::net::TcpListener::bind(("127.0.0.1", port)) {
        Ok(listener) => {
            drop(listener);
            Ok(())
        }
        Err(error) => Err(format!(
            "本地端口 127.0.0.1:{port} 已被其他程序占用（{error}），\nEasyProxy 需要该端口提供 HTTP/SOCKS 代理。请关闭占用该端口的程序后重试。"
        )),
    }
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

fn reload_running_core(state: &AppState, tun_enabled: bool) {
    let direct_running = state.core.status() == CoreStatus::Running;
    if !direct_running && !tun_enabled {
        return;
    }
    let config_path = state.data_dir.join("mihomo.yaml");
    if let Err(error) = mihomo_api::reload_config(&config_path) {
        log::warn!("热重载 Mihomo 配置失败: {error}");
    }
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

fn build_tray_menu(
    app: &tauri::AppHandle,
    mode: ProxyMode,
    system_proxy_enabled: bool,
    tun_enabled: bool,
) -> Result<tauri::menu::Menu<tauri::Wry>, tauri::Error> {
    let mode_rule = CheckMenuItemBuilder::new("规则模式")
        .id("mode_rule")
        .checked(mode == ProxyMode::Rule)
        .build(app)?;
    let mode_global = CheckMenuItemBuilder::new("全局模式")
        .id("mode_global")
        .checked(mode == ProxyMode::Global)
        .build(app)?;
    let mode_direct = CheckMenuItemBuilder::new("直连模式")
        .id("mode_direct")
        .checked(mode == ProxyMode::Direct)
        .build(app)?;

    let mode_submenu = SubmenuBuilder::new(app, "代理模式")
        .item(&mode_rule)
        .item(&mode_global)
        .item(&mode_direct)
        .build()?;

    let system_proxy_item = CheckMenuItemBuilder::new("系统代理")
        .id("system_proxy")
        .checked(system_proxy_enabled)
        .build(app)?;
    let tun_item = CheckMenuItemBuilder::new("TUN 模式")
        .id("tun_mode")
        .checked(tun_enabled)
        .build(app)?;
    let show_item = MenuItemBuilder::new("显示主窗口")
        .id("show_window")
        .build(app)?;
    let quit_item = MenuItemBuilder::new("退出 EasyProxy")
        .id("quit")
        .build(app)?;

    MenuBuilder::new(app)
        .item(&mode_submenu)
        .separator()
        .item(&system_proxy_item)
        .item(&tun_item)
        .separator()
        .item(&show_item)
        .item(&quit_item)
        .build()
}

fn update_tray_menu(app: &tauri::AppHandle, state: &AppState) {
    let mode = state
        .mode
        .lock()
        .ok()
        .map(|m| *m)
        .unwrap_or(ProxyMode::Rule);
    let system_proxy_enabled = state.proxy.is_enabled();
    let tun_enabled = state.tun_enabled.lock().ok().map(|t| *t).unwrap_or(false);
    if let Ok(menu) = build_tray_menu(app, mode, system_proxy_enabled, tun_enabled) {
        if let Some(tray) = app.tray_by_id("main-tray") {
            let _ = tray.set_menu(Some(menu));
        }
    }
}

fn show_main_window(app: &tauri::AppHandle) {
    #[cfg(target_os = "macos")]
    let _ = app.set_activation_policy(ActivationPolicy::Regular);

    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn hide_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }

    #[cfg(target_os = "macos")]
    let _ = app.set_activation_policy(ActivationPolicy::Accessory);
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
    let custom_rules = state
        .custom_rules
        .lock()
        .map_err(|_| "读取自定义规则失败".to_string())?
        .clone();
    let tun_enabled = *state
        .tun_enabled
        .lock()
        .map_err(|_| "读取 TUN 状态失败".to_string())?;
    write_runtime_config(
        &data_dir.join("mihomo.yaml"),
        &content,
        mode.as_mihomo_mode(),
        tun_enabled,
        dns_override.as_ref(),
        &custom_rules,
    )?;
    *state
        .subscription
        .lock()
        .map_err(|_| "保存订阅状态失败".to_string())? = Some(content);
    reload_running_core(&state, tun_enabled);
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

    let merged = match client.get(&url).header("User-Agent", "mihomo").send().await {
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

    let mut reload_after = false;
    let mut reload_tun = false;
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
        let custom_rules = state
            .custom_rules
            .lock()
            .map_err(|_| "读取自定义规则失败".to_string())?
            .clone();
        write_runtime_config(
            &app_data_dir(&app)?.join("mihomo.yaml"),
            subscription,
            mode.as_mihomo_mode(),
            tun_enabled,
            dns_override.as_ref(),
            &custom_rules,
        )?;
        reload_after = true;
        reload_tun = tun_enabled;
    }

    if reload_after {
        reload_running_core(&state, reload_tun);
    }
    update_tray_menu(&app, &state);
    core_status(state)
}

#[tauri::command]
async fn start_core(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<AppStatus, String> {
    let tun_enabled = *state
        .tun_enabled
        .lock()
        .map_err(|_| "读取 TUN 状态失败".to_string())?;
    start_core_inner_async(
        tun_enabled,
        state.core.clone(),
        state.data_dir.clone(),
        app.path()
            .resource_dir()
            .map_err(|e| format!("获取资源目录失败: {e}"))?,
    )
    .await?;
    core_status_inner(&state)
}

#[tauri::command]
async fn stop_core(state: State<'_, AppState>) -> Result<AppStatus, String> {
    let tun_enabled = *state
        .tun_enabled
        .lock()
        .map_err(|_| "读取 TUN 状态失败".to_string())?;
    stop_core_inner_async(tun_enabled, state.core.clone()).await?;
    core_status_inner(&state)
}

#[tauri::command]
async fn set_system_proxy(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    enable: bool,
) -> Result<AppStatus, String> {
    let bypass = state
        .proxy_bypass
        .lock()
        .map_err(|_| "读取绕过域名失败".to_string())?
        .clone();
    let proxy = state.proxy.clone();
    tauri::async_runtime::spawn_blocking(move || {
        if enable {
            proxy.enable(&bypass)
        } else {
            proxy.disable()
        }
    })
    .await
    .map_err(|e| format!("系统代理操作失败: {e}"))??;
    update_tray_menu(&app, &state);
    core_status_inner(&state)
}

#[tauri::command]
fn select_proxy_node(group: String, node: String) -> Result<(), String> {
    mihomo_api::select_proxy_node(&group, &node)
}

#[tauri::command]
async fn test_delays(
    nodes: Vec<String>,
) -> Result<std::collections::BTreeMap<String, mihomo_api::DelayResult>, String> {
    tauri::async_runtime::spawn_blocking(move || mihomo_api::test_proxy_delays(&nodes))
        .await
        .map_err(|e| format!("测速失败: {e}"))
}

#[tauri::command]
fn get_dns_override(
    state: State<'_, AppState>,
) -> Result<Option<config_service::DnsOverride>, String> {
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
        let custom_rules = state
            .custom_rules
            .lock()
            .map_err(|_| "读取自定义规则失败".to_string())?
            .clone();
        write_runtime_config(
            &data_dir.join("mihomo.yaml"),
            sub,
            mode.as_mihomo_mode(),
            tun_enabled,
            dns_ref.as_ref(),
            &custom_rules,
        )?;
        reload_running_core(&state, tun_enabled);
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
            std::fs::copy(&src, &mmdb_path).map_err(|e| format!("复制 MMDB 文件失败: {e}"))?;
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
    let bytes = resp
        .bytes()
        .map_err(|e| format!("读取 MMDB 响应失败: {e}"))?;
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
    let mode = *state.mode.lock().map_err(|_| "读取模式失败".to_string())?;

    if let Some(ref sub) = subscription {
        let config_path = data_dir.join("mihomo.yaml");
        let sub = sub.clone();
        let dns = state
            .dns_override
            .lock()
            .map_err(|_| "读取 DNS 覆写失败".to_string())?
            .clone();
        let custom_rules = state
            .custom_rules
            .lock()
            .map_err(|_| "读取自定义规则失败".to_string())?
            .clone();
        tauri::async_runtime::spawn_blocking(move || {
            write_runtime_config(
                &config_path,
                &sub,
                mode.as_mihomo_mode(),
                enabled,
                dns.as_ref(),
                &custom_rules,
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

    update_tray_menu(&app, &state);
    core_status_inner(&state)
}

#[tauri::command]
fn get_autostart(state: State<'_, AppState>) -> Result<bool, String> {
    Ok(*state
        .autostart_enabled
        .lock()
        .map_err(|_| "读取自启动状态失败".to_string())?)
}

#[tauri::command]
fn set_autostart(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<(), String> {
    if enabled {
        autostart::enable()?;
    } else {
        autostart::disable()?;
    }
    *state
        .autostart_enabled
        .lock()
        .map_err(|_| "保存自启动状态失败".to_string())? = enabled;
    Ok(())
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
    let yaml =
        serde_yaml::to_string(&subscriptions).map_err(|e| format!("序列化订阅列表失败: {e}"))?;
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
    let yaml = serde_yaml::to_string(&nodes).map_err(|e| format!("序列化节点选择失败: {e}"))?;
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
    let yaml = serde_yaml::to_string(&rules).map_err(|e| format!("序列化自定义规则失败: {e}"))?;
    std::fs::write(data_dir.join("custom_rules.yaml"), yaml)
        .map_err(|e| format!("保存自定义规则失败: {e}"))?;
    *state
        .custom_rules
        .lock()
        .map_err(|_| "更新自定义规则状态失败".to_string())? = rules.clone();

    // Rewrite mihomo config so custom rules take effect immediately
    if let Some(ref subscription) = *state
        .subscription
        .lock()
        .map_err(|_| "读取订阅状态失败".to_string())?
    {
        let mode = *state
            .mode
            .lock()
            .map_err(|_| "读取模式状态失败".to_string())?;
        let tun_enabled = *state
            .tun_enabled
            .lock()
            .map_err(|_| "读取 TUN 状态失败".to_string())?;
        let dns_override = state
            .dns_override
            .lock()
            .map_err(|_| "读取 DNS 覆写失败".to_string())?
            .clone();
        let config_path = data_dir.join("mihomo.yaml");
        write_runtime_config(
            &config_path,
            subscription,
            mode.as_mihomo_mode(),
            tun_enabled,
            dns_override.as_ref(),
            &rules,
        )?;
        reload_running_core(&state, tun_enabled);
    }

    Ok(())
}

#[tauri::command]
fn load_proxy_bypass(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    Ok(state
        .proxy_bypass
        .lock()
        .map_err(|_| "读取绕过域名失败".to_string())?
        .clone())
}

#[tauri::command]
fn save_proxy_bypass(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    domains: Vec<String>,
) -> Result<(), String> {
    let data_dir = app_data_dir(&app)?;
    let yaml = serde_yaml::to_string(&domains).map_err(|e| format!("序列化绕过域名失败: {e}"))?;
    std::fs::write(data_dir.join("proxy_bypass.yaml"), yaml)
        .map_err(|e| format!("保存绕过域名失败: {e}"))?;
    *state
        .proxy_bypass
        .lock()
        .map_err(|_| "更新绕过域名状态失败".to_string())? = domains.clone();

    if state.proxy.is_enabled() {
        let _ = state.proxy.apply_bypass_domains(&domains);
    }

    Ok(())
}

fn sub_content(state: &AppState) -> Option<String> {
    let guard: MutexGuard<'_, Option<String>> = state.subscription.lock().ok()?;
    (*guard).clone()
}

fn tun_enabled_val(state: &AppState) -> bool {
    state
        .tun_enabled
        .lock()
        .ok()
        .map(|g: MutexGuard<'_, bool>| *g)
        .unwrap_or(false)
}

fn apply_proxy_mode(app: &tauri::AppHandle, state: &AppState, mode: ProxyMode) {
    if let Ok(mut mode_state) = state.mode.lock() {
        *mode_state = mode;
    }
    if let Some(ref content) = sub_content(state) {
        let tun = tun_enabled_val(state);
        let dns = state
            .dns_override
            .lock()
            .ok()
            .and_then(|g: MutexGuard<'_, Option<config_service::DnsOverride>>| (*g).clone());
        let custom_rules = state
            .custom_rules
            .lock()
            .ok()
            .map(|g: MutexGuard<'_, Vec<String>>| (*g).clone())
            .unwrap_or_default();
        let _ = write_runtime_config(
            &app_data_dir(app).unwrap_or_default().join("mihomo.yaml"),
            content,
            mode.as_mihomo_mode(),
            tun,
            dns.as_ref(),
            &custom_rules,
        );
        reload_running_core(state, tun);
    }
    update_tray_menu(app, state);
    let _ = app.emit("proxy-mode-changed", mode);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                #[cfg(target_os = "macos")]
                let _ = window
                    .app_handle()
                    .set_activation_policy(ActivationPolicy::Accessory);
                api.prevent_close();
            }
        })
        .setup(|app| {
            if let Err(message) = ensure_mixed_port_free(MIXED_PORT) {
                show_fatal_error(&message);
                app.handle().exit(1);
                return Ok(());
            }
            let start_hidden = std::env::args().any(|arg| arg == "--hidden");
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
            let bypass_path = data_dir.join("proxy_bypass.yaml");
            let proxy_bypass: Vec<String> =
                if bypass_path.exists() {
                    std::fs::read_to_string(&bypass_path)
                        .ok()
                        .and_then(|s| serde_yaml::from_str(&s).ok())
                        .unwrap_or_else(default_bypass_domains)
                } else {
                    default_bypass_domains()
                };
            let autostart_enabled = autostart::is_enabled();
            app.manage(AppState {
                subscription: Mutex::new(subscription),
                mode: Mutex::new(ProxyMode::Rule),
                tun_enabled: Mutex::new(false),
                dns_override: Mutex::new(dns_override),
                subscriptions: Mutex::new(subscriptions),
                selected_nodes: Mutex::new(selected_nodes),
                custom_rules: Mutex::new(custom_rules),
                proxy_bypass: Mutex::new(proxy_bypass),
                autostart_enabled: Mutex::new(autostart_enabled),
                data_dir: data_dir.clone(),
                core,
                proxy: SystemProxy::new("127.0.0.1", MIXED_PORT),
                is_quitting: AtomicBool::new(false),
            });

            // When launched via auto-start, hide the main window immediately
            if start_hidden {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
                #[cfg(target_os = "macos")]
                let _ = app.set_activation_policy(ActivationPolicy::Accessory);
            }
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }

            // Build tray menu
            let handle = app.handle().clone();
            let state = app.state::<AppState>();
            let mode = state
                .mode
                .lock()
                .ok()
                .map(|m| *m)
                .unwrap_or(ProxyMode::Rule);
            let system_proxy_enabled = state.proxy.is_enabled();
            let tun_enabled = tun_enabled_val(&state);
            let tray_menu = build_tray_menu(&handle, mode, system_proxy_enabled, tun_enabled)
                .map_err(|e| format!("创建托盘菜单失败: {e}"))?;

            // Load tray icon
            let icon_bytes = std::fs::read(
                std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("icons")
                    .join("tray-icon.png"),
            )
            .or_else(|_| {
                app.path()
                    .resource_dir()
                    .map(|p| p.join("icons").join("tray-icon.png"))
                    .ok()
                    .and_then(|p| std::fs::read(p).ok())
                    .ok_or_else(|| {
                        std::io::Error::new(std::io::ErrorKind::NotFound, "icon not found")
                    })
            })
            .map_err(|e| format!("读取托盘图标失败: {e}"))?;
            let icon =
                Image::from_bytes(&icon_bytes).map_err(|e| format!("解析托盘图标失败: {e}"))?;

            let _tray = TrayIconBuilder::with_id("main-tray")
                .icon(icon)
                .icon_as_template(true)
                .menu(&tray_menu)
                .show_menu_on_left_click(true)
                .tooltip("EasyProxy")
                .on_menu_event(|app: &tauri::AppHandle, event: MenuEvent| {
                    let id = event.id().as_ref();
                    let state = app.state::<AppState>();
                    match id {
                        "mode_rule" => apply_proxy_mode(&app, &state, ProxyMode::Rule),
                        "mode_global" => apply_proxy_mode(&app, &state, ProxyMode::Global),
                        "mode_direct" => apply_proxy_mode(&app, &state, ProxyMode::Direct),
                        "system_proxy" => {
                            let currently = state.proxy.is_enabled();
                            if currently {
                                let _ = state.proxy.disable();
                            } else {
                                let bypass = state
                                    .proxy_bypass
                                    .lock()
                                    .ok()
                                    .map(|g| g.clone())
                                    .unwrap_or_default();
                                let _ = state.proxy.enable(&bypass);
                            }
                            update_tray_menu(&app, &state);
                            let _ = app.emit("system-proxy-changed", !currently);
                        }
                        "tun_mode" => {
                            let currently = tun_enabled_val(&state);
                            let enabled = !currently;
                            let handle = app.clone();
                            tauri::async_runtime::spawn(async move {
                                let state = handle.state::<AppState>();
                                let tun_was_running = tun_enabled_val(&state);
                                let direct_was_running = state.core.status() == CoreStatus::Running;
                                let was_running = tun_was_running || direct_was_running;
                                if was_running {
                                    let _ =
                                        stop_core_inner_async(tun_was_running, state.core.clone())
                                            .await;
                                }
                                *state.tun_enabled.lock().unwrap() = enabled;
                                if let Some(ref content) = sub_content(&state) {
                                    let mode = state
                                        .mode
                                        .lock()
                                        .ok()
                                        .map(|m| *m)
                                        .unwrap_or(ProxyMode::Rule);
                                    let dns: Option<config_service::DnsOverride> =
                                        state.dns_override.lock().ok().and_then(
                                            |g: MutexGuard<
                                                '_,
                                                Option<config_service::DnsOverride>,
                                            >| {
                                                (*g).clone()
                                            },
                                        );
                                    let custom_rules: Vec<String> = state
                                        .custom_rules
                                        .lock()
                                        .ok()
                                        .map(|g: MutexGuard<'_, Vec<String>>| (*g).clone())
                                        .unwrap_or_default();
                                    let config_path = state.data_dir.join("mihomo.yaml");
                                    let sub_clone = content.clone();
                                    let _ = tauri::async_runtime::spawn_blocking(move || {
                                        write_runtime_config(
                                            &config_path,
                                            &sub_clone,
                                            mode.as_mihomo_mode(),
                                            enabled,
                                            dns.as_ref(),
                                            &custom_rules,
                                        )
                                    })
                                    .await;
                                }
                                if enabled || direct_was_running {
                                    if let Ok(rd) = handle.path().resource_dir() {
                                        let _ = start_core_inner_async(
                                            enabled,
                                            state.core.clone(),
                                            state.data_dir.clone(),
                                            rd,
                                        )
                                        .await;
                                    }
                                }
                                update_tray_menu(&handle, &state);
                            });
                        }
                        "show_window" => {
                            show_main_window(app);
                        }
                        "quit" => {
                            state.is_quitting.store(true, Ordering::SeqCst);
                            app.exit(0);
                        }
                        _ => {}
                    }
                })
                .build(app.handle())?;

            // Auto-start core on launch (non-TUN mode)
            let state = app.state::<AppState>();
            // Regenerate runtime config so it always reflects the current port and settings
            {
                let subscription = state.subscription.lock().ok().and_then(|g| g.clone());
                if let Some(ref sub) = subscription {
                    let mode = state
                        .mode
                        .lock()
                        .ok()
                        .map(|m| *m)
                        .unwrap_or(ProxyMode::Rule);
                    let dns_override = state
                        .dns_override
                        .lock()
                        .ok()
                        .and_then(|g| g.clone());
                    let custom_rules = state
                        .custom_rules
                        .lock()
                        .ok()
                        .map(|g| g.clone())
                        .unwrap_or_default();
                    let config_path = data_dir.join("mihomo.yaml");
                    if let Err(e) = write_runtime_config(
                        &config_path,
                        sub,
                        mode.as_mihomo_mode(),
                        false,
                        dns_override.as_ref(),
                        &custom_rules,
                    ) {
                        log::warn!("生成 Mihomo 运行配置失败: {e}");
                    }
                }
            }
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
            get_autostart,
            set_autostart,
            load_subscriptions,
            save_subscriptions,
            load_selected_nodes,
            save_selected_nodes,
            load_custom_rules,
            save_custom_rules,
            load_proxy_bypass,
            save_proxy_bypass,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if let RunEvent::ExitRequested { api, .. } = event {
                let state = app.state::<AppState>();
                if !state.is_quitting.load(Ordering::SeqCst) {
                    hide_main_window(app);
                    api.prevent_exit();
                } else {
                    let _ = state.proxy.disable();
                    let _ = state.core.stop();
                }
            }
        });
}
