# macOS TUN 模式设计

## 概述

为 EasyProxy 添加 macOS TUN 模式支持。TUN 模式通过创建虚拟网卡 (utun) 实现全系统流量代理，覆盖范围远大于系统代理（仅 HTTP/HTTPS/SOCKS）。

## 架构

### TUN 模式 vs 系统代理

两者正交独立：
- **系统代理**：应用层拦截，通过 `networksetup` 设置。仅影响尊重系统代理的应用。
- **TUN 模式**：网络层拦截，通过虚拟网卡 + 路由表接管所有 IP 流量。

TUN 开启时不依赖系统代理。两者可共存但不必。

### 进程架构

```
App (用户态, Tauri)  ──IPC (Unix Socket)──>  Service Daemon (root)
                                                └── Mihomo 进程 (root, utun)
```

Service Daemon 作为 macOS LaunchDaemon 以 root 运行，负责实际启动/停止 Mihomo 内核。App 通过带 HMAC 签名的 JSON IPC 与 Daemon 通信。

### 文件布局

| 文件 | 作用 |
|------|------|
| `src-tauri/src/service_main.rs` | 新增 - Daemon 二进制入口 |
| `src-tauri/src/ipc.rs` | 新增 - IPC 消息定义与签名逻辑 |
| `src-tauri/src/service_manager.rs` | 新增 - LaunchDaemon 安装/卸载 |
| `src-tauri/Cargo.toml` | 修改 - 新增 [[bin]] |
| `src-tauri/src/lib.rs` | 修改 - TUN 状态、新命令 |
| `src-tauri/src/core_manager.rs` | 修改 - 支持 remote 模式 |
| `src-tauri/src/config_service.rs` | 修改 - 注入 tun: 配置 |
| `src/App.tsx` | 修改 - TUN 开关对接后端 |

## IPC 协议

### 传输

Unix Domain Socket: `/var/run/easyproxy.sock`

### 消息格式

长度前缀 (4 bytes BE) + JSON payload。JSON 内容：

```json
{
  "id": "nanoid",
  "ts": 1716768000,
  "cmd": "start|stop|status",
  "payload": null,
  "sig": "hex-encoded-hmac-sha256"
}
```

### 命令

| 命令 | 说明 |
|------|------|
| `start` | 启动 Mihomo 内核 |
| `stop` | 停止 Mihomo 内核 |
| `status` | 查询内核运行状态 |

签名用固定的 app secret 做 HMAC-SHA256，防非授权连接。

## 配置

### Mihomo TUN 段

当 TUN 开启时，`apply_runtime_settings` 注入：

```yaml
tun:
  enable: true
  stack: system
  dns-hijack:
    - any:53
  auto-route: true
  auto-detect-interface: true
```

`stack: system` 是 macOS 最佳选择，性能和兼容性最好。

### ProxyMode 不变

ProxyMode (Rule/Global/Direct) 控制流量路由策略，与 TUN 模式无关。TUN 只是改变流量如何进入 Mihomo，不影响路由决策。

## 安装流程

1. 用户点击 TUN 开关
2. App 检查 service 是否已安装
3. 若未安装，调用 `osascript -e 'do shell script "..." with administrator privileges'`
4. 复制 `easyproxy-service` 二进制到 `/usr/local/lib/easyproxy/`
5. 写入 `/Library/LaunchDaemons/com.easyproxy.service.plist`
6. `launchctl load` 启动 daemon
7. Daemon 启动后监听 `/var/run/easyproxy.sock`

## 运行时流程

### 开启 TUN
1. App 发送 `start` IPC 命令
2. Daemon 以 root 启动 Mihomo（配置含 `tun.enable: true`）
3. Mihomo 创建 utun 网卡并修改路由表
4. 全系统流量进入 Mihomo

### 关闭 TUN
1. App 发送 `stop` IPC 命令
2. Daemon 停止 Mihomo 进程
3. utun 被释放，路由恢复默认

### 切换代理模式
1. 重新生成 Mihomo 配置（更新 mode 字段）
2. 发送 SIGHUP（或重启）使 Mihomo 重载配置

## TUN 状态独立于 CoreStatus

- CoreStatus: `Stopped | Running` — 内核状态
- TUN 是否开启取决于 Mihomo 配置中 `tun.enable` 的值
- 状态管理在 App 端维护 `tun_enabled: bool`

## 测试计划

- 单元测试：IPC 消息签名/验证
- 单元测试：TUN 配置生成
- 集成测试：service 安装/卸载流程
- 手动测试：实际 TUN 模式代理验证
