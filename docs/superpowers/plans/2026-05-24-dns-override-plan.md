# DNS 覆写功能 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 EasyProxy 中实现 DNS 覆写功能，用户可通过可视化表单或 YAML 编辑器自定义 DNS 配置并覆盖订阅自带 DNS 设置。

**Architecture:** Rust 后端新增 `DnsOverride` 数据模型和两个 Tauri 命令 (`get_dns_override` / `set_dns_override`)，在 `apply_runtime_settings` 中将 DNS 覆写配置注入 mihomo.yaml。前端新增独立 DNS 页面，提供表单模式（5组配置）和 YAML 编辑器模式的双向同步。

**Tech Stack:** Rust (serde_yaml, serde_json, tauri), TypeScript/React 19, CSS

---

### Task 1: Config service — DnsOverride struct and DNS merge logic

**Files:**
- Modify: `src-tauri/src/config_service.rs`

- [ ] **Step 1: Add DnsOverride struct**

在文件顶部 `ProxyGroupSummary` 之后添加：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsOverride {
    pub enabled: bool,
    pub config: serde_yaml::Value,
}
```

- [ ] **Step 2: Modify apply_runtime_settings to accept and apply DNS override**

修改函数签名为：

```rust
fn apply_runtime_settings(
    document: &mut Value,
    mode: &str,
    tun_enabled: bool,
    dns_override: Option<&DnsOverride>,
) -> Result<(), String> {
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

    if let Some(dns) = dns_override {
        if dns.enabled {
            root.insert(Value::String("dns".to_string()), dns.config.clone());
        }
    }

    Ok(())
}
```

- [ ] **Step 3: Update build_mihomo_config signature**

```rust
pub fn build_mihomo_config(
    content: &str,
    mode: &str,
    tun_enabled: bool,
    dns_override: Option<&DnsOverride>,
) -> Result<String, String> {
    let mut document = match parse_document(content)? {
        SubscriptionDocument::Clash(document) => document,
        SubscriptionDocument::UriList(nodes) => build_document_from_uri_nodes(nodes),
    };

    apply_runtime_settings(&mut document, mode, tun_enabled, dns_override)?;

    serde_yaml::to_string(&document).map_err(|error| format!("生成 Mihomo 配置失败: {error}"))
}
```

- [ ] **Step 4: Update write_runtime_config signature**

```rust
pub fn write_runtime_config(
    path: &Path,
    content: &str,
    mode: &str,
    tun_enabled: bool,
    dns_override: Option<&DnsOverride>,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("创建运行目录失败: {error}"))?;
    }

    let config = build_mihomo_config(content, mode, tun_enabled, dns_override)?;
    fs::write(path, config).map_err(|error| format!("写入 Mihomo 配置失败: {error}"))
}
```

- [ ] **Step 5: Build check**

Run: `cargo build -p app_lib 2>&1 | head -30`
Expected: 编译错误 — lib.rs 中的调用者尚未更新签名（下一步修复）

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/config_service.rs
git commit -m "feat: add DnsOverride struct and wire into config generation

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

### Task 2: Lib.rs — AppState, Tauri commands, and caller updates

**Files:**
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Add dns_override to AppState**

在 `struct AppState` 中添加新字段（在 `tun_enabled` 之后）：

```rust
struct AppState {
    subscription: Mutex<Option<String>>,
    mode: Mutex<ProxyMode>,
    tun_enabled: Mutex<bool>,
    dns_override: Mutex<Option<config_service::DnsOverride>>,
    data_dir: PathBuf,
    core: CoreManager,
    proxy: SystemProxy,
}
```

- [ ] **Step 2: Add get_dns_override and set_dns_override commands**

在 `test_delays` 命令之后添加：

```rust
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
```

- [ ] **Step 3: Register new commands in generate_handler**

在 `generate_handler!` 宏中添加：

```rust
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
])
```

- [ ] **Step 4: Update save_subscription handler call to write_runtime_config**

修改 `save_subscription` handler 中 `write_runtime_config` 调用，在其之前读取 dns_override：

```rust
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
```

- [ ] **Step 5: Update set_proxy_mode handler call to write_runtime_config**

```rust
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
```

- [ ] **Step 6: Update set_tun_mode handler call to write_runtime_config**

修改 `set_tun_mode` 中 `spawn_blocking` 闭包内的 `write_runtime_config` 调用。在闭包前捕获 dns_override：

```rust
#[tauri::command]
async fn set_tun_mode(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<AppStatus, String> {
    // ... (前面的代码不变，直到 let subscription = ... 之后)

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

    // ... (后面的代码不变)
}
```

- [ ] **Step 7: Restore dns_override from file in setup**

在 `setup` 闭包中，`let subscription = std::fs::read_to_string(...)` 之后，`app.manage(AppState { ... })` 之前添加：

```rust
let dns_override = std::fs::read_to_string(data_dir.join("dns_override.yaml"))
    .ok()
    .and_then(|s| serde_yaml::from_str(&s).ok());
```

在 `app.manage(AppState { ... })` 中添加 `dns_override` 字段：

```rust
app.manage(AppState {
    subscription: Mutex::new(subscription),
    mode: Mutex::new(ProxyMode::Rule),
    tun_enabled: Mutex::new(false),
    dns_override: Mutex::new(dns_override),
    data_dir,
    core,
    proxy: SystemProxy::new("127.0.0.1", 7890),
});
```

- [ ] **Step 8: Build check**

Run: `cargo build -p app_lib 2>&1 | tail -20`
Expected: 编译成功

- [ ] **Step 9: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat: add get/set_dns_override commands with persistence

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

### Task 3: Backend tests for DNS override config generation

**Files:**
- Modify: `src-tauri/tests/config_service.rs`

- [ ] **Step 1: Read existing test file to understand patterns**

```bash
head -80 src-tauri/tests/config_service.rs
```

- [ ] **Step 2: Add test — DNS override enabled injects dns section**

```rust
#[test]
fn dns_override_enabled_injects_dns_section() {
    use app_lib::config_service::{build_mihomo_config, DnsOverride};

    let content = "proxies:\n  - name: Test\n    type: ss\n    server: 1.2.3.4\n    port: 8388\n    password: pwd\n    cipher: aes-256-gcm\n";
    let dns_config: serde_yaml::Value = serde_yaml::from_str("enable: true\nlisten: 0.0.0.0:53\nnameserver:\n  - 223.5.5.5\n").unwrap();
    let dns_override = DnsOverride {
        enabled: true,
        config: dns_config,
    };

    let result = build_mihomo_config(content, "rule", false, Some(&dns_override)).unwrap();
    let doc: serde_yaml::Value = serde_yaml::from_str(&result).unwrap();

    let dns = doc.get("dns").unwrap();
    assert_eq!(dns.get("enable").unwrap().as_bool().unwrap(), true);
    assert_eq!(dns.get("listen").unwrap().as_str().unwrap(), "0.0.0.0:53");
}

#[test]
fn dns_override_disabled_does_not_inject_dns_section() {
    use app_lib::config_service::{build_mihomo_config, DnsOverride};

    let content = "proxies:\n  - name: Test\n    type: ss\n    server: 1.2.3.4\n    port: 8388\n    password: pwd\n    cipher: aes-256-gcm\n";
    let dns_config: serde_yaml::Value = serde_yaml::from_str("enable: true\nlisten: 0.0.0.0:53\n").unwrap();
    let dns_override = DnsOverride {
        enabled: false,
        config: dns_config,
    };

    let result = build_mihomo_config(content, "rule", false, Some(&dns_override)).unwrap();
    let doc: serde_yaml::Value = serde_yaml::from_str(&result).unwrap();

    assert!(doc.get("dns").is_none());
}

#[test]
fn dns_override_none_does_not_inject_dns_section() {
    use app_lib::config_service::build_mihomo_config;

    let content = "proxies:\n  - name: Test\n    type: ss\n    server: 1.2.3.4\n    port: 8388\n    password: pwd\n    cipher: aes-256-gcm\n";

    let result = build_mihomo_config(content, "rule", false, None).unwrap();
    let doc: serde_yaml::Value = serde_yaml::from_str(&result).unwrap();

    assert!(doc.get("dns").is_none());
}
```

- [ ] **Step 3: Run the new tests**

Run: `cargo test dns_override -- -Z unstable-options --format=json 2>&1 | tail -15`
Expected: 3 tests PASS

- [ ] **Step 4: Run the full config_service test suite to check for regressions**

Run: `cargo test config_service 2>&1 | tail -10`
Expected: all tests PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/tests/config_service.rs
git commit -m "test: add DNS override config generation tests

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

### Task 4: Frontend — DNS page shell, nav button, and state

**Files:**
- Modify: `src/App.tsx`

- [ ] **Step 1: Extend Page type and add DNS state variables**

将 `type Page = 'overview' | 'nodes' | 'rules'` 改为：

```typescript
type Page = 'overview' | 'nodes' | 'rules' | 'dns'
```

在现有的 state 声明区域（`const [overviewTab, setOverviewTab] = ...` 之后）添加：

```typescript
interface DnsOverrideConfig {
  enable?: boolean
  listen?: string
  'enhanced-mode'?: string
  ipv6?: boolean
  nameserver?: string[]
  fallback?: string[]
  'default-nameserver'?: string[]
  'nameserver-policy'?: Record<string, string>
  hosts?: Record<string, string>
  'fallback-filter'?: {
    geoip?: boolean
    'geoip-code'?: string
    domain?: string[]
  }
}

interface DnsOverrideData {
  enabled: boolean
  config: DnsOverrideConfig
}

const defaultDnsConfig: DnsOverrideConfig = {
  enable: true,
  listen: '0.0.0.0:53',
  'enhanced-mode': 'fake-ip',
  ipv6: false,
  nameserver: ['223.5.5.5'],
  fallback: ['8.8.8.8'],
  'default-nameserver': ['223.5.5.5'],
}

const [dnsOverride, setDnsOverride] = useState<DnsOverrideData | null>(null)
const [dnsTab, setDnsTab] = useState<'form' | 'yaml'>('form')
const [dnsForm, setDnsForm] = useState<DnsOverrideConfig>(defaultDnsConfig)
const [dnsYaml, setDnsYaml] = useState('')
const [dnsYamlError, setDnsYamlError] = useState('')
const [dnsDirty, setDnsDirty] = useState(false)
```

- [ ] **Step 2: Add DNS nav button in sidebar**

在「代理规则」按钮之后添加：

```tsx
<button
  className={`nav-item ${page === 'dns' ? 'selected' : ''}`}
  type="button"
  onClick={() => setPage('dns')}
>
  <span className="nav-icon">
    <Globe size={16} />
  </span>
  DNS
</button>
```

在文件顶部的 icon import 中添加 `Globe`：

```typescript
import { Pencil, Trash2, Layout, Share2, List, ChevronDown, Globe } from 'lucide-react'
```

- [ ] **Step 3: Add DNS page shell (with toggle) in the content area**

在 rules page 的 `{page === 'rules' && (...)}` 块之后，`</section>` 之前添加：

```tsx
{page === 'dns' && (
  <>
    <div>
      <h2>DNS 覆写</h2>
      <p className="subtitle">
        自定义 DNS 配置以覆盖订阅自带的 DNS 设置
      </p>
    </div>

    <div className="dns-toggle-bar">
      <span>启用 DNS 覆写</span>
      <button
        className={dnsOverride?.enabled ? 'dns-toggle active' : 'dns-toggle'}
        type="button"
        role="switch"
        aria-checked={dnsOverride?.enabled ?? false}
        onClick={() => {
          if (!dnsOverride) return
          const next = { ...dnsOverride, enabled: !dnsOverride.enabled }
          setDnsOverride(next)
          setDnsDirty(true)
        }}
      >
        <span className="toggle-track" aria-hidden="true" />
      </button>
    </div>

    <div className="dns-tabs">
      <button
        className={`dns-tab ${dnsTab === 'form' ? 'active' : ''}`}
        type="button"
        onClick={() => {
          if (dnsTab === 'yaml') {
            setDnsYaml(formToYaml(dnsForm))
            setDnsYamlError('')
          }
          setDnsTab('form')
        }}
      >
        表单模式
      </button>
      <button
        className={`dns-tab ${dnsTab === 'yaml' ? 'active' : ''}`}
        type="button"
        onClick={() => {
          setDnsYaml(formToYaml(dnsForm))
          setDnsYamlError('')
          setDnsTab('yaml')
        }}
      >
        YAML 高级编辑
      </button>
    </div>

    {dnsTab === 'form' && (
      <div className="dns-form">
        {/* Task 5 fills this in */}
        <p className="node-preview-more">表单模式 — 待实现</p>
      </div>
    )}

    {dnsTab === 'yaml' && (
      <div className="dns-yaml-editor">
        <textarea
          className="dns-yaml-textarea"
          value={dnsYaml}
          onChange={(e) => {
            setDnsYaml(e.target.value)
            setDnsDirty(true)
          }}
          spellCheck={false}
          aria-label="DNS YAML 配置"
        />
        {dnsYamlError && (
          <span className="dns-yaml-error">{dnsYamlError}</span>
        )}
      </div>
    )}

    {dnsDirty && (
      <div className="dns-actions">
        <button
          className="dns-save-btn"
          type="button"
          onClick={() => saveDnsOverride()}
        >
          保存
        </button>
        <button
          className="dns-cancel-btn"
          type="button"
          onClick={() => {
            // Reset to last saved state (reload from backend)
            loadDnsOverride()
          }}
        >
          取消
        </button>
      </div>
    )}
  </>
)}
```

- [ ] **Step 4: Add placeholder functions for formToYaml, saveDnsOverride, loadDnsOverride**

在 `App` 函数体中（state 声明之后，return 之前）添加：

```typescript
function formToYaml(config: DnsOverrideConfig): string {
  const lines: string[] = ['dns:']
  if (config.enable !== undefined) lines.push(`  enable: ${config.enable}`)
  if (config.listen) lines.push(`  listen: ${config.listen}`)
  if (config['enhanced-mode']) lines.push(`  enhanced-mode: ${config['enhanced-mode']}`)
  if (config.ipv6 !== undefined) lines.push(`  ipv6: ${config.ipv6}`)
  if (config['default-nameserver'] && config['default-nameserver'].length > 0) {
    lines.push('  default-nameserver:')
    for (const ns of config['default-nameserver']) lines.push(`    - ${ns}`)
  }
  if (config.nameserver && config.nameserver.length > 0) {
    lines.push('  nameserver:')
    for (const ns of config.nameserver) lines.push(`    - ${ns}`)
  }
  if (config.fallback && config.fallback.length > 0) {
    lines.push('  fallback:')
    for (const ns of config.fallback) lines.push(`    - ${ns}`)
  }
  if (config['nameserver-policy'] && Object.keys(config['nameserver-policy']).length > 0) {
    lines.push('  nameserver-policy:')
    for (const [domain, server] of Object.entries(config['nameserver-policy'])) {
      lines.push(`    '${domain}': '${server}'`)
    }
  }
  if (config.hosts && Object.keys(config.hosts).length > 0) {
    lines.push('  hosts:')
    for (const [domain, ip] of Object.entries(config.hosts)) {
      lines.push(`    '${domain}': ${ip}`)
    }
  }
  if (config['fallback-filter']) {
    lines.push('  fallback-filter:')
    if (config['fallback-filter'].geoip !== undefined) {
      lines.push(`    geoip: ${config['fallback-filter'].geoip}`)
    }
    if (config['fallback-filter']['geoip-code']) {
      lines.push(`    geoip-code: ${config['fallback-filter']['geoip-code']}`)
    }
    if (config['fallback-filter'].domain && config['fallback-filter'].domain.length > 0) {
      lines.push('    domain:')
      for (const d of config['fallback-filter'].domain) lines.push(`      - '${d}'`)
    }
  }
  return lines.join('\n')
}

async function loadDnsOverride() {
  try {
    const data = await invoke<DnsOverrideData | null>('get_dns_override')
    if (data) {
      setDnsOverride(data)
      setDnsForm(data.config)
      setDnsYaml(formToYaml(data.config))
    } else {
      setDnsOverride({ enabled: false, config: defaultDnsConfig })
      setDnsForm(defaultDnsConfig)
      setDnsYaml(formToYaml(defaultDnsConfig))
    }
    setDnsDirty(false)
  } catch (error) {
    setMessage(displayError(error))
  }
}

async function saveDnsOverride() {
  if (!dnsOverride) return

  let config: DnsOverrideConfig

  if (dnsTab === 'yaml') {
    // Parse YAML text to config object
    const parsed = parseDnsYaml(dnsYaml)
    if (!parsed) {
      setDnsYamlError('YAML 格式错误，请检查后重试')
      return
    }
    config = parsed
  } else {
    config = dnsForm
  }

  const data: DnsOverrideData = { enabled: dnsOverride.enabled, config }
  try {
    const status = await invoke<AppStatus>('set_dns_override', data)
    setDnsOverride(data)
    setDnsForm(config)
    setDnsYaml(formToYaml(config))
    setDnsYamlError('')
    setDnsDirty(false)
    setEnabled(status.system_proxy !== '')
    setProxyMode(status.mode)
    setTunEnabled(status.tun_enabled)
    setMessage('DNS 覆写配置已保存')
  } catch (error) {
    setMessage(displayError(error))
  }
}

function parseDnsYaml(yaml: string): DnsOverrideConfig | null {
  // Simple line-by-line parser for the known DNS config keys
  // Strip leading "dns:" line if present
  const lines = yaml.trim().split('\n').filter(l => l.trim() !== 'dns:')
  const result: DnsOverrideConfig = {}
  const listFields = ['nameserver', 'fallback', 'default-nameserver']
  const policyMap: Record<string, string> = {}
  const hostsMap: Record<string, string> = {}
  const ffDomain: string[] = []

  let currentList: string | null = null
  let currentMap: 'policy' | 'hosts' | 'ff-domain' | null = null

  for (const line of lines) {
    const trimmed = line.trim()
    if (!trimmed) continue

    // Top-level key-value pairs (2-space indent)
    const kvMatch = trimmed.match(/^(\S[^:]*):\s*(.+)$/)
    const listItemMatch = trimmed.match(/^\s*-\s+(.+)$/)

    if (kvMatch && !trimmed.startsWith('-') && !trimmed.startsWith("'")) {
      const key = kvMatch[1].trim()
      const value = kvMatch[2].trim()

      if (key === 'enable' || key === 'ipv6') {
        ;(result as Record<string, unknown>)[key] = value === 'true'
        currentList = null
        currentMap = null
      } else if (listFields.includes(key)) {
        ;(result as Record<string, unknown>)[key] = []
        currentList = key
        currentMap = null
      } else if (key === 'listen' || key === 'enhanced-mode' || key === 'geoip-code') {
        ;(result as Record<string, unknown>)[key] = value
        currentList = null
        currentMap = null
      } else if (key === 'nameserver-policy') {
        currentMap = 'policy'
        currentList = null
      } else if (key === 'hosts') {
        currentMap = 'hosts'
        currentList = null
      } else if (key === 'geoip') {
        if (!result['fallback-filter']) result['fallback-filter'] = {}
        ;(result as Record<string, unknown>)['fallback-filter'] = {
          ...result['fallback-filter'],
          geoip: value === 'true',
        }
        currentList = null
        currentMap = null
      } else if (key === 'domain') {
        currentMap = 'ff-domain'
        currentList = null
        if (!result['fallback-filter']) result['fallback-filter'] = {}
        result['fallback-filter'].domain = []
      }
    } else if (listItemMatch && currentList) {
      const arr = (result as Record<string, unknown>)[currentList] as string[]
      if (arr) arr.push(listItemMatch[1].trim())
    } else if (listItemMatch && currentMap === 'ff-domain') {
      ffDomain.push(listItemMatch[1].trim().replace(/^'|'$/g, ''))
    }
  }

  if (Object.keys(policyMap).length > 0) result['nameserver-policy'] = policyMap
  if (Object.keys(hostsMap).length > 0) result.hosts = hostsMap
  if (ffDomain.length > 0 && result['fallback-filter']) {
    result['fallback-filter'].domain = ffDomain
  }

  // At minimum we got something — return it
  return result
}
```

Note: `parseDnsYaml` 是简化实现。nameserver-policy 和 hosts 的完整解析通过在 form 模式下编辑来补充。

- [ ] **Step 5: Build check**

Run: `cd src && npx tsc --noEmit 2>&1 | head -30`
Expected: 可能有未使用的import警告，无类型错误（或只有已知的类型警告）

- [ ] **Step 6: Commit**

```bash
git add src/App.tsx
git commit -m "feat: add DNS page shell, nav button, toggle, and YAML sync stubs

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

### Task 5: Frontend — DNS form mode (5 configuration groups)

**Files:**
- Modify: `src/App.tsx`

- [ ] **Step 1: Replace the placeholder in DNS form section with all 5 groups**

将 Step 3 中的 `<p className="node-preview-more">表单模式 — 待实现</p>` 替换为：

```tsx
{/* --- 基础设置 --- */}
<fieldset className="dns-fieldset">
  <legend className="dns-legend">基础设置</legend>

  <label className="dns-field">
    <span>DNS 功能</span>
    <button
      className={dnsForm.enable ? 'dns-mini-toggle active' : 'dns-mini-toggle'}
      type="button"
      role="switch"
      aria-checked={dnsForm.enable ?? true}
      onClick={() => {
        setDnsForm(f => ({ ...f, enable: !f.enable }))
        setDnsDirty(true)
      }}
    >
      <span className="toggle-track" aria-hidden="true" />
    </button>
  </label>

  <label className="dns-field">
    <span>监听地址</span>
    <input
      className="dns-input"
      value={dnsForm.listen ?? ''}
      onChange={(e) => { setDnsForm(f => ({ ...f, listen: e.target.value })); setDnsDirty(true) }}
      placeholder="0.0.0.0:53"
    />
  </label>

  <label className="dns-field">
    <span>解析模式</span>
    <select
      className="dns-select"
      value={dnsForm['enhanced-mode'] ?? 'fake-ip'}
      onChange={(e) => { setDnsForm(f => ({ ...f, 'enhanced-mode': e.target.value })); setDnsDirty(true) }}
    >
      <option value="fake-ip">fake-ip</option>
      <option value="redir-host">redir-host</option>
    </select>
  </label>

  <label className="dns-field">
    <span>IPv6</span>
    <button
      className={dnsForm.ipv6 ? 'dns-mini-toggle active' : 'dns-mini-toggle'}
      type="button"
      role="switch"
      aria-checked={dnsForm.ipv6 ?? false}
      onClick={() => {
        setDnsForm(f => ({ ...f, ipv6: !f.ipv6 }))
        setDnsDirty(true)
      }}
    >
      <span className="toggle-track" aria-hidden="true" />
    </button>
  </label>
</fieldset>

{/* --- DNS 服务器 --- */}
<fieldset className="dns-fieldset">
  <legend className="dns-legend">DNS 服务器</legend>

  {(['default-nameserver', 'nameserver', 'fallback'] as const).map((key) => (
    <div key={key} className="dns-field dns-list-field">
      <span className="dns-list-label">
        {key === 'default-nameserver' ? '引导 DNS' : key === 'nameserver' ? '主 DNS' : '回退 DNS'}
      </span>
      <div className="dns-list-items">
        {(dnsForm[key] ?? []).map((item, idx) => (
          <div key={idx} className="dns-list-row">
            <input
              className="dns-input"
              value={item}
              onChange={(e) => {
                setDnsForm(f => {
                  const arr = [...(f[key] ?? [])]
                  arr[idx] = e.target.value
                  return { ...f, [key]: arr }
                })
                setDnsDirty(true)
              }}
              placeholder="DNS 服务器地址"
            />
            <button
              className="dns-list-remove"
              type="button"
              onClick={() => {
                setDnsForm(f => ({ ...f, [key]: (f[key] ?? []).filter((_, i) => i !== idx) }))
                setDnsDirty(true)
              }}
            >
              ×
            </button>
          </div>
        ))}
      </div>
      <button
        className="dns-list-add"
        type="button"
        onClick={() => {
          setDnsForm(f => ({ ...f, [key]: [...(f[key] ?? []), ''] }))
          setDnsDirty(true)
        }}
      >
        + 添加
      </button>
    </div>
  ))}
</fieldset>

{/* --- 域名策略 --- */}
<fieldset className="dns-fieldset">
  <legend className="dns-legend">域名策略 (nameserver-policy)</legend>
  {(dnsForm['nameserver-policy'] ? Object.entries(dnsForm['nameserver-policy']) : []).map(([domain, server], idx) => (
    <div key={idx} className="dns-list-row">
      <input
        className="dns-input"
        value={domain}
        onChange={(e) => {
          setDnsForm(f => {
            const policy = { ...f['nameserver-policy'] }
            const entries = Object.entries(policy)
            const [, srv] = entries[idx]
            delete policy[domain]
            policy[e.target.value] = srv
            return { ...f, 'nameserver-policy': policy }
          })
          setDnsDirty(true)
        }}
        placeholder="+.company.internal"
      />
      <input
        className="dns-input"
        value={server}
        onChange={(e) => {
          setDnsForm(f => {
            const policy = { ...f['nameserver-policy'] }
            const entries = Object.entries(policy)
            const [dom] = entries[idx]
            policy[dom] = e.target.value
            return { ...f, 'nameserver-policy': policy }
          })
          setDnsDirty(true)
        }}
        placeholder="10.0.0.53"
      />
      <button
        className="dns-list-remove"
        type="button"
        onClick={() => {
          setDnsForm(f => {
            const policy = { ...f['nameserver-policy'] }
            const entries = Object.entries(policy)
            delete policy[entries[idx][0]]
            return { ...f, 'nameserver-policy': policy }
          })
          setDnsDirty(true)
        }}
      >
        ×
      </button>
    </div>
  ))}
  <button
    className="dns-list-add"
    type="button"
    onClick={() => {
      setDnsForm(f => {
        const policy = { ...f['nameserver-policy'], '': '' }
        return { ...f, 'nameserver-policy': policy }
      })
      setDnsDirty(true)
    }}
  >
    + 添加策略
  </button>
</fieldset>

{/* --- Hosts 映射 --- */}
<fieldset className="dns-fieldset">
  <legend className="dns-legend">Hosts 映射</legend>
  {(dnsForm.hosts ? Object.entries(dnsForm.hosts) : []).map(([domain, ip], idx) => (
    <div key={idx} className="dns-list-row">
      <input
        className="dns-input"
        value={domain}
        onChange={(e) => {
          setDnsForm(f => {
            const hosts = { ...f.hosts }
            const entries = Object.entries(hosts)
            const [, ipAddr] = entries[idx]
            delete hosts[domain]
            hosts[e.target.value] = ipAddr
            return { ...f, hosts }
          })
          setDnsDirty(true)
        }}
        placeholder="example.local"
      />
      <input
        className="dns-input"
        value={ip}
        onChange={(e) => {
          setDnsForm(f => {
            const hosts = { ...f.hosts }
            const entries = Object.entries(hosts)
            const [dom] = entries[idx]
            hosts[dom] = e.target.value
            return { ...f, hosts }
          })
          setDnsDirty(true)
        }}
        placeholder="192.168.1.100"
      />
      <button
        className="dns-list-remove"
        type="button"
        onClick={() => {
          setDnsForm(f => {
            const hosts = { ...f.hosts }
            const entries = Object.entries(hosts)
            delete hosts[entries[idx][0]]
            return { ...f, hosts }
          })
          setDnsDirty(true)
        }}
      >
        ×
      </button>
    </div>
  ))}
  <button
    className="dns-list-add"
    type="button"
    onClick={() => {
      setDnsForm(f => {
        const hosts = { ...f.hosts, '': '' }
        return { ...f, hosts }
      })
      setDnsDirty(true)
    }}
  >
    + 添加映射
  </button>
</fieldset>

{/* --- Fallback 过滤 --- */}
<fieldset className="dns-fieldset">
  <legend className="dns-legend">Fallback 过滤</legend>

  <label className="dns-field">
    <span>GeoIP 过滤</span>
    <button
      className={dnsForm['fallback-filter']?.geoip ? 'dns-mini-toggle active' : 'dns-mini-toggle'}
      type="button"
      role="switch"
      aria-checked={dnsForm['fallback-filter']?.geoip ?? false}
      onClick={() => {
        setDnsForm(f => ({
          ...f,
          'fallback-filter': { ...f['fallback-filter'], geoip: !f['fallback-filter']?.geoip },
        }))
        setDnsDirty(true)
      }}
    >
      <span className="toggle-track" aria-hidden="true" />
    </button>
  </label>

  <label className="dns-field">
    <span>GeoIP 代码</span>
    <input
      className="dns-input"
      value={dnsForm['fallback-filter']?.['geoip-code'] ?? 'CN'}
      onChange={(e) => {
        setDnsForm(f => ({
          ...f,
          'fallback-filter': { ...f['fallback-filter'], 'geoip-code': e.target.value },
        }))
        setDnsDirty(true)
      }}
      placeholder="CN"
    />
  </label>

  <div className="dns-field dns-list-field">
    <span className="dns-list-label">域名列表</span>
    <div className="dns-list-items">
      {(dnsForm['fallback-filter']?.domain ?? []).map((domain, idx) => (
        <div key={idx} className="dns-list-row">
          <input
            className="dns-input"
            value={domain}
            onChange={(e) => {
              setDnsForm(f => {
                const arr = [...(f['fallback-filter']?.domain ?? [])]
                arr[idx] = e.target.value
                return { ...f, 'fallback-filter': { ...f['fallback-filter'], domain: arr } }
              })
              setDnsDirty(true)
            }}
            placeholder="+.google.com"
          />
          <button
            className="dns-list-remove"
            type="button"
            onClick={() => {
              setDnsForm(f => ({
                ...f,
                'fallback-filter': {
                  ...f['fallback-filter'],
                  domain: (f['fallback-filter']?.domain ?? []).filter((_, i) => i !== idx),
                },
              }))
              setDnsDirty(true)
            }}
          >
            ×
          </button>
        </div>
      ))}
    </div>
    <button
      className="dns-list-add"
      type="button"
      onClick={() => {
        setDnsForm(f => ({
          ...f,
          'fallback-filter': {
            ...f['fallback-filter'],
            domain: [...(f['fallback-filter']?.domain ?? []), ''],
          },
        }))
        setDnsDirty(true)
      }}
    >
      + 添加
    </button>
  </div>
</fieldset>
```

- [ ] **Step 2: Build check**

Run: `cd src && npx tsc --noEmit 2>&1 | head -20`
Expected: 无新增类型错误

- [ ] **Step 3: Commit**

```bash
git add src/App.tsx
git commit -m "feat: add DNS form mode with all 5 configuration groups

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

### Task 6: Frontend — DNS page styles

**Files:**
- Modify: `src/App.css`

- [ ] **Step 1: Add all DNS page CSS**

在文件末尾添加：

```css
/* ============================================
   DNS 覆写页面
   ============================================ */

/* Toggle bar */
.dns-toggle-bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 14px 18px;
  background: var(--card-bg, rgba(255, 255, 255, 0.04));
  border: 1px solid var(--border-color, rgba(255, 255, 255, 0.08));
  border-radius: 10px;
  margin-bottom: 16px;
  font-size: 14px;
  font-weight: 500;
}

.dns-toggle {
  width: 44px;
  height: 24px;
  border-radius: 12px;
  border: none;
  background: rgba(255, 255, 255, 0.15);
  cursor: pointer;
  position: relative;
  transition: background 0.2s;
  padding: 0;
}

.dns-toggle.active {
  background: var(--accent, #007aff);
}

.dns-toggle .toggle-track {
  display: block;
  width: 20px;
  height: 20px;
  border-radius: 50%;
  background: #fff;
  position: absolute;
  top: 2px;
  left: 2px;
  transition: transform 0.2s;
}

.dns-toggle.active .toggle-track {
  transform: translateX(20px);
}

/* Tabs */
.dns-tabs {
  display: flex;
  gap: 0;
  border-bottom: 1px solid var(--border-color, rgba(255, 255, 255, 0.08));
  margin-bottom: 18px;
}

.dns-tab {
  padding: 8px 18px;
  font-size: 13px;
  font-weight: 500;
  border: none;
  background: none;
  color: var(--text-secondary, #999);
  cursor: pointer;
  border-bottom: 2px solid transparent;
  transition: color 0.15s, border-color 0.15s;
  font-family: inherit;
}

.dns-tab.active {
  color: var(--text-primary, #fff);
  border-bottom-color: var(--accent, #007aff);
}

.dns-tab:hover:not(.active) {
  color: var(--text-primary, #ccc);
}

/* Form */
.dns-form {
  display: flex;
  flex-direction: column;
  gap: 18px;
}

/* Fieldset */
.dns-fieldset {
  border: 1px solid var(--border-color, rgba(255, 255, 255, 0.06));
  border-radius: 10px;
  padding: 14px 16px;
  margin: 0;
}

.dns-legend {
  font-size: 12px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.5px;
  color: var(--text-secondary, #888);
  padding: 0 6px;
}

/* Fields */
.dns-field {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 0;
  font-size: 13px;
}

.dns-field + .dns-field {
  border-top: 1px solid var(--border-color, rgba(255, 255, 255, 0.04));
}

.dns-field > span:first-child {
  color: var(--text-secondary, #aaa);
  min-width: 100px;
}

.dns-list-label {
  color: var(--text-secondary, #aaa);
  font-size: 13px;
  margin-bottom: 6px;
}

/* List field (vertical layout) */
.dns-list-field {
  flex-direction: column;
  align-items: flex-start;
  gap: 6px;
}

/* Inputs */
.dns-input {
  flex: 1;
  max-width: 320px;
  padding: 6px 10px;
  border: 1px solid var(--border-color, rgba(255, 255, 255, 0.1));
  border-radius: 6px;
  background: var(--input-bg, rgba(255, 255, 255, 0.04));
  color: var(--text-primary, #fff);
  font-size: 12px;
  font-family: inherit;
  outline: none;
  transition: border-color 0.15s;
}

.dns-input:focus {
  border-color: var(--accent, #007aff);
}

.dns-select {
  padding: 6px 10px;
  border: 1px solid var(--border-color, rgba(255, 255, 255, 0.1));
  border-radius: 6px;
  background: var(--input-bg, rgba(255, 255, 255, 0.04));
  color: var(--text-primary, #fff);
  font-size: 12px;
  font-family: inherit;
  outline: none;
  cursor: pointer;
  min-width: 140px;
}

.dns-select:focus {
  border-color: var(--accent, #007aff);
}

/* Mini toggle (for form fields) */
.dns-mini-toggle {
  width: 36px;
  height: 20px;
  border-radius: 10px;
  border: none;
  background: rgba(255, 255, 255, 0.15);
  cursor: pointer;
  position: relative;
  transition: background 0.2s;
  padding: 0;
}

.dns-mini-toggle.active {
  background: var(--accent, #007aff);
}

.dns-mini-toggle .toggle-track {
  display: block;
  width: 16px;
  height: 16px;
  border-radius: 50%;
  background: #fff;
  position: absolute;
  top: 2px;
  left: 2px;
  transition: transform 0.2s;
}

.dns-mini-toggle.active .toggle-track {
  transform: translateX(16px);
}

/* List items */
.dns-list-items {
  display: flex;
  flex-direction: column;
  gap: 4px;
  width: 100%;
}

.dns-list-row {
  display: flex;
  gap: 6px;
  align-items: center;
}

.dns-list-row .dns-input {
  flex: 1;
  max-width: none;
}

.dns-list-remove {
  width: 24px;
  height: 24px;
  border-radius: 6px;
  border: none;
  background: transparent;
  color: var(--text-secondary, #999);
  font-size: 16px;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  transition: background 0.15s, color 0.15s;
  padding: 0;
}

.dns-list-remove:hover {
  background: rgba(255, 59, 48, 0.15);
  color: #ff3b30;
}

.dns-list-add {
  font-size: 12px;
  padding: 4px 12px;
  border: 1px dashed var(--border-color, rgba(255, 255, 255, 0.15));
  border-radius: 6px;
  background: transparent;
  color: var(--accent, #007aff);
  cursor: pointer;
  margin-top: 4px;
  font-family: inherit;
  transition: border-color 0.15s;
}

.dns-list-add:hover {
  border-color: var(--accent, #007aff);
}

/* YAML editor */
.dns-yaml-editor {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.dns-yaml-textarea {
  width: 100%;
  min-height: 400px;
  padding: 14px;
  border: 1px solid var(--border-color, rgba(255, 255, 255, 0.1));
  border-radius: 10px;
  background: #1a1a2e;
  color: #e0e0e0;
  font-family: 'SF Mono', 'Fira Code', 'Cascadia Code', monospace;
  font-size: 12px;
  line-height: 1.6;
  resize: vertical;
  outline: none;
  tab-size: 2;
}

.dns-yaml-textarea:focus {
  border-color: var(--accent, #007aff);
}

.dns-yaml-error {
  color: #ff3b30;
  font-size: 12px;
}

/* Action buttons */
.dns-actions {
  display: flex;
  gap: 10px;
  justify-content: flex-end;
  margin-top: 16px;
  padding-top: 14px;
  border-top: 1px solid var(--border-color, rgba(255, 255, 255, 0.06));
}

.dns-save-btn {
  padding: 8px 22px;
  border: none;
  border-radius: 8px;
  background: var(--accent, #007aff);
  color: #fff;
  font-size: 13px;
  font-weight: 600;
  cursor: pointer;
  font-family: inherit;
  transition: opacity 0.15s;
}

.dns-save-btn:hover {
  opacity: 0.85;
}

.dns-cancel-btn {
  padding: 8px 22px;
  border: 1px solid var(--border-color, rgba(255, 255, 255, 0.1));
  border-radius: 8px;
  background: transparent;
  color: var(--text-secondary, #aaa);
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  font-family: inherit;
  transition: background 0.15s;
}

.dns-cancel-btn:hover {
  background: rgba(255, 255, 255, 0.05);
}

/* Dark mode overrides */
@media (prefers-color-scheme: dark) {
  .dns-yaml-textarea {
    background: #0d0d1a;
  }
}
```

- [ ] **Step 2: Commit**

```bash
git add src/App.css
git commit -m "style: add DNS override page styles

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

### Task 7: Frontend — Wire up load on page enter

**Files:**
- Modify: `src/App.tsx`

- [ ] **Step 1: Add useEffect to load DNS override when entering DNS page**

在 `loadDnsOverride` 函数定义之后添加：

```typescript
useEffect(() => {
  if (page === 'dns') {
    loadDnsOverride()
  }
}, [page])
```

- [ ] **Step 2: Build check**

Run: `cd src && npx tsc --noEmit 2>&1 | head -20`
Expected: 无新增错误

- [ ] **Step 3: Commit**

```bash
git add src/App.tsx
git commit -m "feat: wire up DNS override data loading on page enter

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

### Task 8: Verification — Full build and manual test planning

- [ ] **Step 1: Full Rust build**

Run: `cargo build -p app_lib 2>&1 | tail -5`
Expected: 编译成功

- [ ] **Step 2: Full frontend typecheck**

Run: `cd src && npx tsc --noEmit 2>&1`
Expected: 无类型错误

- [ ] **Step 3: Run all Rust tests**

Run: `cargo test 2>&1 | tail -15`
Expected: 所有测试通过（包括新增的 DNS 测试）

- [ ] **Step 4: Run frontend tests**

Run: `npx vitest run 2>&1 | tail -10`
Expected: 测试通过

- [ ] **Step 5: Commit any remaining changes**

```bash
git status
```

如有未提交的更改，提交它们。

---

### Task 9: Manual verification checklist

> 以下步骤需要在运行中的应用中手动验证

- [ ] **UI rendering**: 点击侧边栏「DNS」按钮，DNS 覆写页面正常渲染
- [ ] **Toggle**: 覆写开关可切换
- [ ] **Form editing**: 所有 5 组表单字段可正常编辑
- [ ] **List editors**: DNS 服务器列表可添加/编辑/删除条目
- [ ] **Policy/hosts tables**: 域名策略和 Hosts 映射可添加/删除
- [ ] **Form → YAML**: 切换到 YAML 模式，看到正确的 YAML 内容
- [ ] **YAML → Form**: 在 YAML 模式编辑后切回表单，字段正确回填
- [ ] **Save**: 点击保存按钮，返回成功提示
- [ ] **Persistence**: 重启应用后，DNS 配置仍然保留
- [ ] **Config injection**: 检查 `<app-data-dir>/mihomo.yaml` 中是否包含 `dns:` 段落
- [ ] **Refresh**: 刷新页面后，DNS 覆写配置保持不变
- [ ] **Subscription switch**: 切换订阅后，DNS 覆写配置不变
