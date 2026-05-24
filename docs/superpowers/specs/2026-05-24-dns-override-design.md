# DNS 覆写功能设计

## 概述

在 EasyProxy 中增加 DNS 覆写功能，允许用户用自定义 DNS 配置覆盖机场订阅自带的 DNS 设置。提供可视化表单和 YAML 编辑器两种配置方式，由后端统一管理 DNS 覆写数据并在生成 mihomo.yaml 时注入。

## 交互设计

### 导航

侧边栏新增第 4 个导航按钮「DNS」，点击进入独立的 DNS 配置页面。

```
总览 / 线路切换 / 代理规则 / DNS
```

### 页面结构

DNS 页面包含以下层级：

- **顶栏**：标题「DNS 覆写」+ 覆写开关（控制 DNS 覆写是否生效）
- **模式切换 Tab**：「表单模式」（默认）|「YAML 高级编辑」
- **表单模式**：分组表单，包含 5 组配置项
- **YAML 模式**：文本编辑器，显示完整的 `dns:` 配置片段

### 覆写开关行为

| 开关状态 | 行为 |
|---------|------|
| 开启 | 将用户自定义的 DNS 配置注入 mihomo.yaml 的 `dns:` 段落 |
| 关闭 | 不写入 `dns:` 段落，mihomo 使用内核默认 DNS 或订阅原始 DNS |

开关关闭时表单数据保留，用户可以继续编辑。开关状态影响的是「是否将覆写配置注入配置文件」，而非「是否显示配置表单」。

### 表单模式 — 配置分组

#### 基础设置

| 字段 | 控件类型 | 说明 |
|------|---------|------|
| DNS 功能 | 开关 | `enable`：dns 模块总开关 |
| 监听地址 | 文本输入 | `listen`：默认 `0.0.0.0:53` |
| 解析模式 | 下拉选择 | `enhanced-mode`：`fake-ip` / `redir-host` |
| IPv6 | 开关 | `ipv6`：是否启用 IPv6 DNS 解析 |

#### DNS 服务器

| 字段 | 控件类型 | 说明 |
|------|---------|------|
| 引导 DNS | 列表编辑器（文本数组） | `default-nameserver`：用于解析 DoH/DoT 域名 |
| 主 DNS | 列表编辑器（文本数组） | `nameserver`：主 DNS 服务器 |
| 回退 DNS | 列表编辑器（文本数组） | `fallback`：被规则判定为国际流量的走这里 |

列表编辑器：展示文本列表，每行一个，可增加/删除条目。

#### 域名策略 (nameserver-policy)

| 字段 | 控件类型 | 说明 |
|------|---------|------|
| 域名匹配 | 文本输入 | 如 `+.company.internal` |
| DNS 服务器 | 文本输入 | 如 `10.0.0.53` |

表格形式展示所有策略条目，每行可删除。底部有「+ 添加策略」按钮。

#### Hosts 映射 (hosts)

| 字段 | 控件类型 | 说明 |
|------|---------|------|
| 域名 | 文本输入 | 如 `example.local` |
| IP 地址 | 文本输入 | 如 `192.168.1.100` |

表格形式，与域名策略类似。

#### Fallback 过滤 (fallback-filter)

| 字段 | 控件类型 | 说明 |
|------|---------|------|
| GeoIP 过滤 | 开关 | `geoip`：是否启用 |
| GeoIP 代码 | 文本输入 | `geoip-code`：默认 `CN` |
| 域名列表 | 列表编辑器 | `domain`：如 `+.google.com` |

### YAML 编辑器模式

- 提供 `<textarea>` 文本编辑器，展示 `dns:` 段落的 YAML
- 与表单模式双向同步：
  - 表单修改后切换到 YAML → 看到更新后的 YAML
  - YAML 修改后切换到表单 → 解析回填各字段
  - YAML 解析失败 → 提示用户并保留在 YAML 模式
- 保存时做 YAML 语法校验

## 数据模型

### Rust 后端

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsOverride {
    pub enabled: bool,
    pub config: serde_yaml::Value,  // 自由格式的 DNS 配置片段
}
```

AppState 新增字段：
```rust
struct AppState {
    // ... 现有字段
    dns_override: Mutex<Option<DnsOverride>>,
}
```

DNS 覆写持久化到文件：`<app-data-dir>/dns_override.yaml`

### TypeScript 前端

```typescript
interface DnsOverride {
  enabled: boolean
  config: DnsOverrideConfig
}

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
```

## Tauri 命令

新增两个命令：

| 命令 | 参数 | 返回 | 说明 |
|------|------|------|------|
| `get_dns_override` | 无 | `Option<DnsOverride>` | 读取当前保存的 DNS 覆写 |
| `set_dns_override` | `DnsOverride` | `AppStatus` | 保存并重写 mihomo.yaml |

### 数据流

```
用户编辑 DNS 配置
  → 表单修改 → React state 更新
  → 保存时 invoke('set_dns_override', dnsOverride)
  → Rust: dns_override 存入 AppState + 持久化到文件
  → config_service::apply_runtime_settings()
     → 如果 dns_override.enabled:
         将 dns_override.config 合并到 mihomo.yaml 的 dns: 段落
     → 如果 !dns_override.enabled:
         不写 dns: 段，mihomo 使用内核默认
  → mihomo.yaml 写入磁盘 → mihomo 内核读取生效
```

## config_service 修改

`apply_runtime_settings` 函数签名变更：
```rust
fn apply_runtime_settings(
    document: &mut Value, 
    mode: &str, 
    tun_enabled: bool,
    dns_override: Option<&DnsOverride>,  // 新增参数
) -> Result<(), String>
```

当 `dns_override` 为 `Some` 且 `enabled` 为 `true` 时，将 `dns_override.config` 中的键值对作为完整的 `dns:` 段落替换到 document 中（整体替换，非深度合并）。

## 影响范围

### Rust 后端

- `src-tauri/src/config_service.rs` — 新增 `DnsOverride` struct，修改 `apply_runtime_settings` 签名
- `src-tauri/src/lib.rs` — 新增 `get_dns_override` / `set_dns_override` 命令，AppState 新增字段，setup 时恢复持久化数据

所有调用 `apply_runtime_settings` 的地方（`build_mihomo_config`、`save_subscription` handler、`set_proxy_mode` handler、`set_tun_mode` handler）需要传入 `dns_override` 参数。

### 前端

- `src/App.tsx` — 新增 DNS 页面渲染、表单状态管理、YAML 编辑、表单↔YAML 双向同步、新增导航按钮
- `src/App.css` — 新增 DNS 页面样式

## 边界情况

- **无订阅时**：DNS 页面可访问但覆写无法生效（没有活跃订阅则没有 mihomo.yaml 可生成）
- **开关关闭时**：表单保留编辑内容，mihomo.yaml 不写入 dns 段落
- **首次使用**：`dns_override` 为 None，前端加载后显示空表单
- **表单→YAML 切换**：自动生成 YAML 文本，展示给用户
- **YAML→表单切换**：解析 YAML 回填表单。解析失败则显示错误并保留 YAML 模式
- **YAML 校验**：保存时做 YAML 语法校验，不合法则拒绝保存
- **订阅切换**：DNS 覆写是全局配置，不随订阅切换而变化
- **配置冲突**：listen 地址若与 mixed-port 冲突，mihomo 自身会在启动时报错，前端不做预检
