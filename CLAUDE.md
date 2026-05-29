# EasyProxy

EasyProxy 是一个基于 Tauri 2 的跨平台桌面代理客户端，为 Mihomo（原 Clash Meta）代理引擎提供简化的图形界面。

## 功能概览

- 代理订阅导入（Clash YAML / URI 列表 / Base64 编码）
- 一键系统代理开关
- 代理模式切换（Rule / Global / Direct）
- 节点选择与延迟测试
- TUN 模式（macOS，需特权服务）
- WebSocket 实时连接监控
- DNS 覆写与自定义规则
- 开机自启与系统托盘

## 技术栈

| 层级 | 技术 |
|------|------|
| 前端 | React 19 + TypeScript 6 + Vite 8 |
| 后端 | Rust (Tauri 2.11) |
| 代理核心 | Mihomo (Go, git submodule) |
| 测试前端 | Vitest + @testing-library/react |
| 测试后端 | cargo test |
| Lint | ESLint 10 (flat config) |

## 项目结构

```
├── src/                          # React 前端
│   ├── main.tsx                  # 入口
│   ├── App.tsx                   # 单体组件 (~2800 行，7 个页面)
│   ├── App.css                   # 样式 (~2181 行)
│   └── App.test.tsx              # 前端测试 (13 用例)
├── src-tauri/                    # Tauri/Rust 后端
│   ├── src/
│   │   ├── main.rs               # 应用入口 → lib::run()
│   │   ├── lib.rs                # 核心：AppState、26 个 Tauri command、托盘、生命周期 (~1580 行)
│   │   ├── config_service.rs     # 订阅解析 & Mihomo 配置生成 (~715 行)
│   │   ├── core_manager.rs       # Mihomo 进程生命周期管理 (~123 行)
│   │   ├── system_proxy.rs       # 系统代理设置 (macOS/Windows) (~264 行)
│   │   ├── ipc.rs                # Unix socket IPC + HMAC-SHA256 认证 (~123 行)
│   │   ├── mihomo_api.rs         # Mihomo 控制器 HTTP 客户端 (~214 行)
│   │   ├── service_manager.rs    # macOS launchd 服务管理 (~153 行)
│   │   ├── autostart.rs          # macOS 开机自启 (~74 行)
│   │   └── service_main.rs       # 特权服务守护进程 (独立 binary, ~319 行)
│   ├── tests/                    # Rust 集成测试 (22 用例)
│   ├── binaries/                 # 预构建 sidecar 二进制
│   └── vendor/mihomo/            # Mihomo Go 源码 (git submodule)
├── scripts/
│   └── build-mihomo-sidecar.sh   # 构建 Mihomo sidecar
└── package.json
```

## 架构模式

### 双二进制架构

- **easyproxy** (主应用，用户空间)：UI、订阅管理、系统代理切换
- **easyproxy-service** (守护进程，root 权限)：通过 Unix socket IPC 管理 TUN 模式下的 Mihomo 进程

### 后端

- **Tauri Command 模式**：26 个 `#[tauri::command]` 函数，前端通过 `invoke()` 调用
- **Mutex 全局状态**：`AppState` 持有所有应用状态（订阅、模式、TUN、DNS、规则等），变更持久化为 YAML 文件
- **进程管理**：`CoreManager` 管理 Mihomo 子进程生命周期，Drop 实现自动清理
- **平台条件编译**：`system_proxy.rs` 通过 `#[cfg(target_os)]` 区分 macOS/Windows

### 前端

- **单体组件**：所有 UI 在一个 `App.tsx` 中，7 个页面 + 侧边栏
- **无路由库/状态管理库**：全部 `useState` 本地状态
- **浏览器预览模式**：非 Tauri 环境下展示 mock 数据，支持 `npm run dev` 独立开发

### 数据持久化

所有状态以独立 YAML 文件存储在应用数据目录：
订阅列表、选中节点、自定义规则、DNS 覆写、TUN 状态、系统代理状态、代理绕过域名

## 常用命令

```sh
# 开发
npm install
git submodule update --init --recursive
npm run tauri -- dev

# 构建
npm run build                    # 前端构建 (tsc + vite)
npm run build:service            # 构建特权服务
./scripts/build-mihomo-sidecar.sh  # 构建 Mihomo sidecar

# 测试
npm test                         # 前端测试 (Vitest)
cd src-tauri && cargo test       # 后端测试 (22 用例)

# Lint
npm run lint
```

## Mihomo 通信

- **控制 API**：`http://127.0.0.1:9090`（PUT /configs, PUT /proxies/{group}, DELETE /connections, GET /proxies/{name}/delay）
- **WebSocket**：`ws://127.0.0.1:9090/connections`（实时连接监控）
- **代理端口**：`mixed-port: 7897`

## IPC 协议（特权服务）

- Unix socket：`/var/run/easyproxy.sock`
- 长度前缀 JSON 消息 + HMAC-SHA256 签名
- 支持命令：`start` / `stop` / `status`
- 优雅关闭：INT → TERM → KILL 信号升级

## 订阅格式支持

- Clash/Mihomo YAML（`proxies` 字段）
- 原始 URI 列表
- Base64 编码 URI 列表
- `anytls://` URI → Mihomo proxy YAML 转换
