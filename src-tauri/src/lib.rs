pub mod config_service;
pub mod core_manager;
pub mod mihomo_api;
pub mod system_proxy;

use config_service::{
    save_subscription as save_subscription_file, write_runtime_config, SubscriptionSummary,
};
use core_manager::{CoreManager, CoreStatus};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use system_proxy::{ProxyMode, SystemProxy};
use tauri::{Manager, State};

struct AppState {
    subscription: Mutex<Option<String>>,
    mode: Mutex<ProxyMode>,
    core: CoreManager,
    proxy: SystemProxy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AppStatus {
    core: CoreStatus,
    mode: ProxyMode,
    system_proxy: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ImportedSubscription {
    nodes: Vec<String>,
    format: String,
    content: String,
}

#[tauri::command]
fn core_status(state: State<'_, AppState>) -> Result<AppStatus, String> {
    let mode = *state
        .mode
        .lock()
        .map_err(|_| "读取模式状态失败".to_string())?;

    Ok(AppStatus {
        core: state.core.status(),
        mode,
        system_proxy: state.proxy.endpoint(),
    })
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
    write_runtime_config(
        &data_dir.join("mihomo.yaml"),
        &content,
        mode.as_mihomo_mode(),
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
    let content = reqwest::get(url)
        .await
        .map_err(|error| format!("请求订阅失败: {error}"))?
        .text()
        .await
        .map_err(|error| format!("读取订阅内容失败: {error}"))?;
    let summary = save_subscription(app, state, content.clone())?;
    Ok(ImportedSubscription {
        nodes: summary.nodes,
        format: summary.format,
        content,
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
        write_runtime_config(
            &app_data_dir(&app)?.join("mihomo.yaml"),
            subscription,
            mode.as_mihomo_mode(),
        )?;
    }

    core_status(state)
}

#[tauri::command]
fn start_core(state: State<'_, AppState>) -> Result<AppStatus, String> {
    state.core.start()?;
    let _ = state.proxy.enable();
    core_status(state)
}

#[tauri::command]
fn stop_core(state: State<'_, AppState>) -> Result<AppStatus, String> {
    let _ = state.proxy.disable();
    state.core.stop()?;
    core_status(state)
}

#[tauri::command]
fn select_proxy_node(node: String) -> Result<(), String> {
    mihomo_api::select_proxy_node(&node)
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let core = default_core_manager(app.handle())?;
            app.manage(AppState {
                subscription: Mutex::new(None),
                mode: Mutex::new(ProxyMode::Rule),
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
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            core_status,
            save_subscription,
            refresh_subscription,
            select_proxy_node,
            set_proxy_mode,
            start_core,
            stop_core
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
