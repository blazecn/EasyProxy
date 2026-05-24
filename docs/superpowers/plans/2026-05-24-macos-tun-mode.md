# macOS TUN Mode Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement macOS TUN mode via a privileged LaunchDaemon service that runs the Mihomo core with root, communicating with the main app over Unix socket IPC.

**Architecture:** The main Tauri app communicates with a root-privileged service daemon over a Unix socket (`/var/run/easyproxy.sock`). The daemon is installed as a macOS LaunchDaemon via `osascript` admin prompt. HMAC-SHA256 signed JSON messages secure the IPC channel.

**Tech Stack:** Rust (std Unix sockets, serde_json, hmac/sha2), Tauri v2, React/TypeScript frontend

---

## IPC wire format

All messages are length-prefixed JSON over Unix stream. Shared secret is `easyproxy-ipc-key-2026`.

```
Write: 4 bytes BE length + JSON bytes
Read: 4 bytes BE length → read exact bytes → deserialize IpcMessage
```

```rust
struct IpcMessage {
    id: String,    // random 16-char hex (xorshift64 seeded by nanos)
    ts: u64,       // unix timestamp seconds
    cmd: String,   // "start" | "stop" | "status"
    payload: Option<Value>,
    sig: String,   // hex HMAC-SHA256(id|ts_le|cmd|payload_json)
}
```

---

### Task 1: Add IPC dependencies and binary target

**Files:**
- Modify: `src-tauri/Cargo.toml`

- [ ] **Step 1: Add hmac, sha2, hex to [dependencies]**

Read `src-tauri/Cargo.toml`, add after `urlencoding` line:

```toml
hmac = "0.12"
sha2 = "0.10"
hex = "0.4"
```

- [ ] **Step 2: Add [[bin]] target at end of Cargo.toml**

```toml
[[bin]]
name = "easyproxy-service"
path = "src/service_main.rs"
```

- [ ] **Step 3: Verify Cargo.toml is valid**

Run: `cargo metadata --manifest-path src-tauri/Cargo.toml --no-deps --format-version 1 2>&1 | head -5`

Expected: no parse errors

- [ ] **Step 4: Commit**

```bash
git add src-tauri/Cargo.toml
git commit -m "chore: add IPC dependencies and service binary target"
```

---

### Task 2: Create IPC protocol module

**Files:**
- Create: `src-tauri/src/ipc.rs`

- [ ] **Step 1: Write ipc.rs**

```rust
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::Sha256;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::time::{SystemTime, UNIX_EPOCH};

const APP_SECRET: &[u8] = b"easyproxy-ipc-key-2026";
pub const SOCKET_PATH: &str = "/var/run/easyproxy.sock";

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcMessage {
    pub id: String,
    pub ts: u64,
    pub cmd: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<Value>,
    pub sig: String,
}

impl IpcMessage {
    pub fn new(cmd: &str, payload: Option<Value>) -> Self {
        let id = random_hex(16);
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let sig = sign(&id, ts, cmd, &payload);
        Self { id, ts, cmd: cmd.to_string(), payload, sig }
    }

    pub fn verify(&self) -> bool {
        let expected = sign(&self.id, self.ts, &self.cmd, &self.payload);
        expected == self.sig
    }
}

fn sign(id: &str, ts: u64, cmd: &str, payload: &Option<Value>) -> String {
    let mut mac = HmacSha256::new_from_slice(APP_SECRET).expect("HMAC key creation");
    mac.update(id.as_bytes());
    mac.update(&ts.to_le_bytes());
    mac.update(cmd.as_bytes());
    if let Some(ref p) = payload {
        mac.update(serde_json::to_string(p).unwrap_or_default().as_bytes());
    }
    hex::encode(mac.finalize().into_bytes())
}

fn random_hex(len: usize) -> String {
    let mut buf = vec![0u8; len / 2];
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let mut state = seed;
    for byte in buf.iter_mut() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        *byte = (state >> 56) as u8;
    }
    hex::encode(buf)
}

pub fn send_message(stream: &mut UnixStream, msg: &IpcMessage) -> Result<(), String> {
    let json = serde_json::to_vec(msg).map_err(|e| format!("序列化 IPC 消息失败: {e}"))?;
    let len = json.len() as u32;
    stream.write_all(&len.to_be_bytes()).map_err(|e| format!("写入 IPC 长度失败: {e}"))?;
    stream.write_all(&json).map_err(|e| format!("写入 IPC 消息失败: {e}"))?;
    stream.flush().map_err(|e| format!("刷新 IPC 流失败: {e}"))?;
    Ok(())
}

pub fn recv_message(stream: &mut UnixStream) -> Result<IpcMessage, String> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).map_err(|e| format!("读取 IPC 长度失败: {e}"))?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > 65536 {
        return Err("IPC 消息过大".to_string());
    }
    let mut buf = vec![0u8; len];
    stream.read_exact(&mut buf).map_err(|e| format!("读取 IPC 消息数据失败: {e}"))?;
    let msg: IpcMessage = serde_json::from_slice(&buf).map_err(|e| format!("解析 IPC 消息失败: {e}"))?;
    if !msg.verify() {
        return Err("IPC 签名验证失败".to_string());
    }
    Ok(msg)
}

pub fn connect() -> Result<UnixStream, String> {
    UnixStream::connect(SOCKET_PATH)
        .map_err(|e| format!("连接 easyproxy 服务失败 (TUN 服务未安装?): {e}"))
}
```

- [ ] **Step 2: Verify compile**

Run: `cargo check -p easyproxy 2>&1`

Expected: compile success (both lib and easyproxy-service)

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/ipc.rs
git commit -m "feat: add IPC protocol module with HMAC-signed messages"
```

---

### Task 3: Create service daemon binary

**Files:**
- Create: `src-tauri/src/service_main.rs`

- [ ] **Step 1: Write service_main.rs**

```rust
use app_lib::ipc;
use std::os::unix::net::UnixListener;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

struct ServiceState {
    child: Mutex<Option<Child>>,
}

fn main() {
    let _ = std::fs::remove_file(ipc::SOCKET_PATH);

    let listener = UnixListener::bind(ipc::SOCKET_PATH)
        .expect("绑定 IPC socket 失败 (需要 root)");

    let state = ServiceState {
        child: Mutex::new(None),
    };

    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                if let Err(e) = handle_client(&state, &mut stream) {
                    eprintln!("IPC 错误: {e}");
                }
            }
            Err(e) => eprintln!("连接错误: {e}"),
        }
    }
}

fn handle_client(state: &ServiceState, stream: &mut std::os::unix::net::UnixStream) -> Result<(), String> {
    let msg = ipc::recv_message(stream)?;

    let response = match msg.cmd.as_str() {
        "start" => {
            let payload = msg.payload.as_ref().ok_or("start 命令缺少参数")?;
            let binary_path = payload["binary_path"].as_str().ok_or("缺少 binary_path")?;
            let config_path = payload["config_path"].as_str().ok_or("缺少 config_path")?;

            let mut guard = state.child.lock().map_err(|_| "锁定失败")?;
            if guard.is_some() {
                serde_json::json!({"ok": true, "msg": "已在运行"})
            } else {
                let child = Command::new(binary_path)
                    .arg("-f")
                    .arg(config_path)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .map_err(|e| format!("启动 Mihomo 失败: {e}"))?;
                *guard = Some(child);
                serde_json::json!({"ok": true, "msg": "已启动"})
            }
        }
        "stop" => {
            let mut guard = state.child.lock().map_err(|_| "锁定失败")?;
            if let Some(mut child) = guard.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
            serde_json::json!({"ok": true, "msg": "已停止"})
        }
        "status" => {
            let guard = state.child.lock().map_err(|_| "锁定失败")?;
            serde_json::json!({"running": guard.is_some()})
        }
        _ => serde_json::json!({"ok": false, "msg": "未知命令"}),
    };

    let resp = ipc::IpcMessage::new("response", Some(response));
    ipc::send_message(stream, &resp)?;
    Ok(())
}
```

- [ ] **Step 2: Verify compile**

Run: `cargo check -p easyproxy 2>&1`

Expected: compile success

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/service_main.rs
git commit -m "feat: add service daemon binary listening on Unix socket"
```

---

### Task 4: Create service manager (LaunchDaemon install/uninstall)

**Files:**
- Create: `src-tauri/src/service_manager.rs`

- [ ] **Step 1: Write service_manager.rs**

```rust
use std::path::PathBuf;
use std::process::Command;

const SERVICE_NAME: &str = "com.easyproxy.service";
const PLIST_PATH: &str = "/Library/LaunchDaemons/com.easyproxy.service.plist";
const SERVICE_BIN_NAME: &str = "easyproxy-service";
const INSTALL_DIR: &str = "/usr/local/lib/easyproxy";

pub fn is_installed() -> bool {
    std::path::Path::new(PLIST_PATH).exists()
}

pub fn install(binary_path: &str) -> Result<(), String> {
    let install_dir = PathBuf::from(INSTALL_DIR);
    std::fs::create_dir_all(&install_dir)
        .map_err(|e| format!("创建安装目录失败: {e}"))?;

    let dest = install_dir.join(SERVICE_BIN_NAME);
    std::fs::copy(binary_path, &dest)
        .map_err(|e| format!("复制 service 二进制失败: {e}"))?;

    Command::new("chmod")
        .args(["755", dest.to_str().unwrap_or("")])
        .status()
        .map_err(|e| format!("设置权限失败: {e}"))?;

    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{name}</string>
    <key>Program</key>
    <string>{bin}</string>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardErrorPath</key>
    <string>/var/log/easyproxy-service.log</string>
    <key>StandardOutPath</key>
    <string>/var/log/easyproxy-service.log</string>
</dict>
</plist>"#,
        name = SERVICE_NAME,
        bin = dest.display(),
    );

    let script = format!(
        r#"do shell script "cat > {plist} << 'PLIST_EOF'
{content}
PLIST_EOF
launchctl load {plist}
mkdir -p /var/log && touch /var/log/easyproxy-service.log
" with administrator privileges"#,
        plist = PLIST_PATH,
        content = plist.replace("\\", "\\\\").replace("\"", "\\\""),
    );

    let output = Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .output()
        .map_err(|e| format!("执行安装脚本失败: {e}"))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("cancel") || stderr.contains("User cancelled") {
            Err("用户取消了授权".to_string())
        } else {
            Err(format!("安装服务失败: {}", stderr.trim()))
        }
    }
}

pub fn uninstall() -> Result<(), String> {
    let script = format!(
        r#"do shell script "launchctl unload {plist} 2>/dev/null; rm -f {plist}" with administrator privileges"#,
        plist = PLIST_PATH,
    );

    let output = Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .output()
        .map_err(|e| format!("执行卸载脚本失败: {e}"))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("卸载服务失败: {}", stderr.trim()))
    }
}
```

- [ ] **Step 2: Verify compile**

Run: `cargo check -p easyproxy 2>&1`

Expected: compile success

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/service_manager.rs
git commit -m "feat: add LaunchDaemon service manager for install/uninstall"
```

---

### Task 5: Register new modules in lib.rs

**Files:**
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Add module declarations at top**

Replace the existing `pub mod` block at line 1-4:

```rust
pub mod config_service;
pub mod core_manager;
pub mod ipc;
pub mod mihomo_api;
pub mod service_manager;
pub mod system_proxy;
```

- [ ] **Step 2: Verify compile**

Run: `cargo check -p easyproxy 2>&1`

Expected: compile success

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat: register ipc and service_manager modules"
```

---

### Task 6: Add TUN config generation

**Files:**
- Modify: `src-tauri/src/config_service.rs`
- Modify: `src-tauri/src/lib.rs` (update callers)

- [ ] **Step 1: Add tun_enabled parameter to apply_runtime_settings**

Change function signature (line ~608):

```rust
pub fn apply_runtime_settings(document: &mut Value, mode: &str, tun_enabled: bool) -> Result<(), String> {
```

- [ ] **Step 2: Add TUN config section at end of apply_runtime_settings**

After the `secret` insertion, before `Ok(())`:

```rust
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
```

- [ ] **Step 3: Update build_mihomo_config signature**

```rust
pub fn build_mihomo_config(content: &str, mode: &str, tun_enabled: bool) -> Result<String, String> {
    let mut document = match parse_document(content)? {
        SubscriptionDocument::Clash(document) => document,
        SubscriptionDocument::UriList(nodes) => build_document_from_uri_nodes(nodes),
    };
    apply_runtime_settings(&mut document, mode, tun_enabled)?;
    serde_yaml::to_string(&document).map_err(|error| format!("生成 Mihomo 配置失败: {error}"))
}
```

- [ ] **Step 4: Update write_runtime_config signature**

```rust
pub fn write_runtime_config(path: &Path, content: &str, mode: &str, tun_enabled: bool) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("创建运行目录失败: {error}"))?;
    }
    let config = build_mihomo_config(content, mode, tun_enabled)?;
    fs::write(path, config).map_err(|error| format!("写入 Mihomo 配置失败: {error}"))
}
```

- [ ] **Step 5: Update callers in lib.rs**

In `save_subscription` (pass `false` for tun during import):
```rust
write_runtime_config(&data_dir.join("mihomo.yaml"), &content, mode.as_mihomo_mode(), false)?;
```

In `set_proxy_mode` (read current tun_enabled state):
```rust
let tun_enabled = *state.tun_enabled.lock().map_err(|_| "读取 TUN 状态失败".to_string())?;
write_runtime_config(
    &app_data_dir(&app)?.join("mihomo.yaml"),
    subscription,
    mode.as_mihomo_mode(),
    tun_enabled,
)?;
```

- [ ] **Step 6: Verify compile**

Run: `cargo check -p easyproxy 2>&1`

Expected: compile success

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/config_service.rs src-tauri/src/lib.rs
git commit -m "feat: add TUN config section to mihomo config builder"
```

---

### Task 7: Extend AppState and add TUN commands

**Files:**
- Modify: `src-tauri/src/lib.rs`

This is the core integration task. All changes are in lib.rs.

- [ ] **Step 1: Add data_dir and tun_enabled to AppState, update AppStatus**

Replace the existing struct definitions:

```rust
struct AppState {
    subscription: Mutex<Option<String>>,
    mode: Mutex<ProxyMode>,
    tun_enabled: Mutex<bool>,
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
```

- [ ] **Step 2: Update setup() to include new fields**

Replace the `app.manage(AppState { ... })` block:

```rust
let data_dir = app_data_dir(app.handle())?;
let core = default_core_manager(app.handle())?;
app.manage(AppState {
    subscription: Mutex::new(None),
    mode: Mutex::new(ProxyMode::Rule),
    tun_enabled: Mutex::new(false),
    data_dir,
    core,
    proxy: SystemProxy::new("127.0.0.1", 7890),
});
```

- [ ] **Step 3: Update core_status to include tun_enabled**

```rust
#[tauri::command]
fn core_status(state: State<'_, AppState>) -> Result<AppStatus, String> {
    let mode = *state.mode.lock().map_err(|_| "读取模式状态失败".to_string())?;
    let tun_enabled = *state.tun_enabled.lock().map_err(|_| "读取 TUN 状态失败".to_string())?;
    Ok(AppStatus {
        core: state.core.status(),
        mode,
        system_proxy: state.proxy.endpoint(),
        tun_enabled,
    })
}
```

- [ ] **Step 4: Add start_core_inner / stop_core_inner helpers**

Add after the `default_core_manager` function:

```rust
fn stop_core_inner(state: &AppState) -> Result<(), String> {
    let tun_enabled = *state.tun_enabled.lock().map_err(|_| "读取 TUN 状态失败".to_string())?;
    if tun_enabled {
        let mut stream = ipc::connect()?;
        let msg = ipc::IpcMessage::new("stop", None);
        ipc::send_message(&mut stream, &msg)?;
        let _resp = ipc::recv_message(&mut stream)?;
    } else {
        let _ = state.proxy.disable();
        state.core.stop()?;
    }
    Ok(())
}

fn start_core_inner(app: &tauri::AppHandle, state: &AppState) -> Result<(), String> {
    let tun_enabled = *state.tun_enabled.lock().map_err(|_| "读取 TUN 状态失败".to_string())?;
    if tun_enabled {
        let binary_path = app.path()
            .resource_dir()
            .map_err(|e| format!("获取资源目录失败: {e}"))?
            .join("binaries")
            .join("mihomo");
        let config_path = state.data_dir.join("mihomo.yaml");

        let mut stream = ipc::connect()?;
        let msg = ipc::IpcMessage::new("start", Some(serde_json::json!({
            "binary_path": binary_path.to_str().unwrap_or(""),
            "config_path": config_path.to_str().unwrap_or(""),
        })));
        ipc::send_message(&mut stream, &msg)?;
        let _resp = ipc::recv_message(&mut stream)?;
    } else {
        state.core.start()?;
        let _ = state.proxy.enable();
    }
    Ok(())
}
```

- [ ] **Step 5: Replace start_core and stop_core to use the inner helpers**

Replace `start_core`:

```rust
#[tauri::command]
fn start_core(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<AppStatus, String> {
    start_core_inner(&app, &state)?;
    core_status(state)
}
```

Replace `stop_core`:

```rust
#[tauri::command]
fn stop_core(state: State<'_, AppState>) -> Result<AppStatus, String> {
    stop_core_inner(&state)?;
    core_status(state)
}
```

- [ ] **Step 6: Add set_tun_mode command**

Add before the `app_data_dir` function:

```rust
#[tauri::command]
fn set_tun_mode(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<AppStatus, String> {
    if !cfg!(target_os = "macos") {
        return Err("TUN 模式当前仅支持 macOS".to_string());
    }

    if enabled && !service_manager::is_installed() {
        let service_bin = app.path()
            .resource_dir()
            .map_err(|e| format!("获取资源目录失败: {e}"))?
            .join("binaries")
            .join("easyproxy-service");
        service_manager::install(service_bin.to_str().unwrap_or(""))?;
    }

    let was_running = state.core.status() == CoreStatus::Running;
    if was_running {
        stop_core_inner(&state)?;
    }

    *state.tun_enabled.lock().map_err(|_| "保存 TUN 状态失败".to_string())? = enabled;

    // Regenerate config with new TUN setting
    if let Some(sub) = state.subscription.lock().map_err(|_| "读取订阅失败".to_string())?.as_ref() {
        write_runtime_config(
            &state.data_dir.join("mihomo.yaml"),
            sub,
            state.mode.lock().map_err(|_| "读取模式失败".to_string())?.as_mihomo_mode(),
            enabled,
        )?;
    }

    if was_running {
        start_core_inner(&app, &state)?;
    }

    core_status(state)
}
```

- [ ] **Step 7: Register set_tun_mode in invoke_handler**

```rust
.invoke_handler(tauri::generate_handler![
    core_status,
    save_subscription,
    refresh_subscription,
    select_proxy_node,
    set_proxy_mode,
    set_tun_mode,
    start_core,
    stop_core,
    test_delays
])
```

- [ ] **Step 8: Verify compile**

Run: `cargo check -p easyproxy 2>&1`

Expected: compile success

- [ ] **Step 9: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat: add TUN mode commands with IPC routing to service daemon"
```

---

### Task 8: Wire frontend TUN toggle to backend

**Files:**
- Modify: `src/App.tsx`

- [ ] **Step 1: Add tun_enabled to AppStatus interface (line ~20)**

```typescript
interface AppStatus {
  core: CoreStatus
  mode: BackendProxyMode
  system_proxy: string
  tun_enabled: boolean
}
```

- [ ] **Step 2: Update useEffect to read tun_enabled**

In the `useEffect` after `setProxyMode(status.mode)`:

```typescript
setTunEnabled(status.tun_enabled)
```

- [ ] **Step 3: Replace toggleTunMode (line ~636)**

```typescript
async function toggleTunMode() {
    const nextEnabled = !tunEnabled
    setTunEnabled(nextEnabled)
    setMessage(nextEnabled ? '正在安装并开启 TUN 模式...' : '正在关闭 TUN 模式...')
    setBusyAction('tun')

    try {
      const status = await invoke<AppStatus>('set_tun_mode', { enabled: nextEnabled })
      setTunEnabled(status.tun_enabled)
      setEnabled(status.core === 'Running')
      setProxyMode(status.mode)
      setMessage(nextEnabled ? 'TUN 模式已开启' : 'TUN 模式已关闭')
    } catch (error) {
      setTunEnabled(!nextEnabled)
      setMessage(displayError(error))
    } finally {
      setBusyAction(null)
    }
  }
```

- [ ] **Step 4: Update TUN toggle button to disable while busy (line ~745)**

```tsx
<button
  className={tunEnabled ? 'sidebar-toggle active' : 'sidebar-toggle'}
  type="button"
  role="switch"
  aria-checked={tunEnabled}
  aria-label="TUN 模式"
  onClick={toggleTunMode}
  disabled={busyAction === 'tun' || busyAction === 'proxy'}
>
  <span>TUN 模式</span>
  <span className="toggle-track" aria-hidden="true" />
</button>
```

- [ ] **Step 5: Run TypeScript check**

Run: `cd /Users/czh/Projects/EasyProxy && npx tsc --noEmit 2>&1`

Expected: no type errors

- [ ] **Step 6: Commit**

```bash
git add src/App.tsx
git commit -m "feat: wire TUN toggle to backend set_tun_mode command"
```

---

### Task 9: Bundle service binary with macOS app

**Files:**
- Modify: `src-tauri/Cargo.toml` (build hook)
- or: just add a `build.rs` step / npm script

The tauri.conf.json already has `"resources": ["binaries/*"]`, so we just need the service binary in `src-tauri/binaries/` before `tauri build`.

- [ ] **Step 1: Add build script to Cargo.toml for copying**

In `Cargo.toml`, after `[build-dependencies]`:

```toml
[package.metadata.tauri-build]
# Build the service binary then copy to binaries dir
```

Actually, the simplest approach: build the service binary as part of `cargo build` and just document it. For the `beforeBuildCommand`, add a cargo build step.

A simpler approach: add a `cp` step in the npm `build` script. Let's check package.json scripts:

```bash
cat package.json | grep -A5 scripts
```

Then modify accordingly. The key point: `easyproxy-service` binary must exist in `src-tauri/binaries/` before `tauri build` runs.

For now, verify this works manually:

- [ ] **Step 1: Build both targets**

Run: `cargo build --release -p easyproxy 2>&1 | tail -5`

Expected: both binaries built

- [ ] **Step 2: Copy service binary to resources**

Run: `cp src-tauri/target/release/easyproxy-service src-tauri/binaries/easyproxy-service`

- [ ] **Step 3: Add npm build hook**

Add to package.json scripts:

```json
"build:service": "cargo build --release -p easyproxy && cp src-tauri/target/release/easyproxy-service src-tauri/binaries/easyproxy-service"
```

- [ ] **Step 4: Commit**

```bash
git add src-tauri/binaries/easyproxy-service .gitignore package.json
git commit -m "chore: bundle service binary with macOS app build"
```

---

### Task 10: Tests

**Files:**
- Create: `src-tauri/tests/ipc.rs`
- Create: `src-tauri/tests/tun_config.rs`

- [ ] **Step 1: Write IPC tests**

`src-tauri/tests/ipc.rs`:
```rust
#[cfg(test)]
mod ipc_tests {
    use app_lib::ipc;

    #[test]
    fn test_message_sign_and_verify() {
        let msg = ipc::IpcMessage::new("status", None);
        assert!(msg.verify());
    }

    #[test]
    fn test_message_tamper_detection() {
        let mut msg = ipc::IpcMessage::new("start", Some(serde_json::json!({"k": "v"})));
        msg.cmd = "stop".to_string();
        assert!(!msg.verify());
    }

    #[test]
    fn test_message_with_payload() {
        let msg = ipc::IpcMessage::new("start", Some(serde_json::json!({
            "binary_path": "/usr/bin/mihomo",
            "config_path": "/tmp/m.yaml",
        })));
        assert!(msg.verify());
    }
}
```

- [ ] **Step 2: Write TUN config tests**

`src-tauri/tests/tun_config.rs`:
```rust
#[cfg(test)]
mod tun_config_tests {
    use app_lib::config_service::build_mihomo_config;

    #[test]
    fn test_tun_section_when_enabled() {
        let content = "proxies:\n  - name: t\n    type: ss\n    server: 1.2.3.4\n    port: 8388\n";
        let config = build_mihomo_config(content, "rule", true).unwrap();
        let doc: serde_yaml::Value = serde_yaml::from_str(&config).unwrap();
        let tun = doc.get("tun").expect("tun section should exist");
        assert_eq!(tun["enable"].as_bool(), Some(true));
        assert_eq!(tun["stack"].as_str(), Some("system"));
        assert_eq!(tun["auto-route"].as_bool(), Some(true));
    }

    #[test]
    fn test_no_tun_section_when_disabled() {
        let content = "proxies:\n  - name: t\n    type: ss\n    server: 1.2.3.4\n    port: 8388\n";
        let config = build_mihomo_config(content, "rule", false).unwrap();
        let doc: serde_yaml::Value = serde_yaml::from_str(&config).unwrap();
        assert!(doc.get("tun").is_none());
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p easyproxy 2>&1`

Expected: all 5 tests pass (3 existing + 5 new IPC/TUN tests)

- [ ] **Step 4: Commit**

```bash
git add src-tauri/tests/ipc.rs src-tauri/tests/tun_config.rs
git commit -m "test: add IPC signing and TUN config generation tests"
```

---

### Task 11: Final build verification (macOS only)

**Files:**
- None

- [ ] **Step 1: Build release**

```bash
cargo build --release -p easyproxy 2>&1
```

Expected: `target/release/easyproxy-service` exists and is executable

- [ ] **Step 2: Check service binary**

```bash
file src-tauri/target/release/easyproxy-service
ls -la src-tauri/target/release/easyproxy-service
```

Expected: Mach-O binary, executable

- [ ] **Step 3: Tauri dev build (dry run)**

```bash
cd /Users/czh/Projects/EasyProxy && npx tauri build --ci 2>&1 | tail -20
# Or just verify: cargo check passes for all targets
```

Expected: build succeeds or check passes
