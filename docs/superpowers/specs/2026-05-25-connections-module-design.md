# 连接模块设计

## 目标

为 EasyProxy 增加「连接」模块，展示 mihomo 当前活动连接，帮助用户理解哪些应用和域名正在走代理，并在需要时断开单条或全部连接。

第一版采用「默认可读列表 + 详细模式」：

- 默认面向普通用户，优先展示目标、进程、规则、节点、流量和持续时间。
- 详细模式用于排障，在每条连接中展开源地址、目标地址、规则 payload、链路、进程路径和连接 ID。
- 数据实时更新，关闭连接等有副作用操作由 Rust 后端封装。

## 导航与页面位置

侧边栏新增一级入口「连接」，放在「线路切换」之后、「代理规则」之前。

页面仍使用当前 macOS 风格：

- 灰白背景、白色列表块、细边框、6-8px 圆角。
- 不增加新的设计系统或依赖。
- 移动端和窄窗口保留可读列表形态，不切换为宽表格。

## 架构

### 实时数据

前端进入连接页时打开 mihomo WebSocket：

```text
ws://127.0.0.1:9090/connections?interval=1000
```

mihomo 每秒推送 snapshot：

- `downloadTotal`
- `uploadTotal`
- `memory`
- `connections[]`

前端维护最近两次 snapshot，用于计算：

- 活动连接数
- 总上传 / 总下载
- 近 1 秒上传速率 / 下载速率
- 每条连接持续时间

### 关闭操作

关闭连接属于有副作用操作，由 Rust 后端封装，前端通过 Tauri command 调用：

- `close_connection(id: String)`
- `close_all_connections()`

Rust 在 `mihomo_api.rs` 中实现：

- `close_connection(id: &str) -> Result<(), String>`
- `close_all_connections() -> Result<(), String>`

对应 mihomo 控制器接口：

```text
DELETE http://127.0.0.1:9090/connections/{id}
DELETE http://127.0.0.1:9090/connections
```

不在 Rust 里解析实时连接列表。读实时数据走 WebSocket，写操作走 Tauri command，边界保持清晰。

## 前端数据模型

```ts
interface ConnectionSnapshot {
  downloadTotal: number
  uploadTotal: number
  memory: number
  connections: ConnectionItem[]
}

interface ConnectionItem {
  id: string
  metadata?: {
    network?: string
    type?: string
    sourceIP?: string
    sourcePort?: string
    destinationIP?: string
    destinationPort?: string
    host?: string
    process?: string
    processPath?: string
  }
  upload: number
  download: number
  start: string
  chains?: string[]
  providerChains?: string[]
  rule?: string
  rulePayload?: string
}
```

字段缺失时前端按空值处理，不能因为单条连接数据不完整导致整页崩溃。

展示优先级：

- 目标：优先 `metadata.host`，否则 `destinationIP:destinationPort`。
- 进程：优先 `metadata.process`，否则显示网络类型。
- 节点：优先从 `chains` 取最后一个可读节点。
- 规则：显示 `rule`，展开后显示 `rulePayload`。

## UI 结构

### 顶部工具栏

顶部显示当前状态和操作：

- 左侧：`24 个活动连接 · 实时更新`
- 中间：搜索框，搜索目标域名/IP、进程名、规则、节点。
- 右侧：
  - 视图切换：`列表 / 详细`
  - 刷新按钮
  - `全部断开`危险按钮

刷新按钮用于立即重建 WebSocket 连接，不改变连接数据。

### 概览指标

工具栏下方展示四个小型指标块：

- 活动连接
- 下载速率
- 上传速率
- 总流量

指标块保持紧凑，不使用大面积营销式卡片。

### 筛选条

筛选只作用于前端列表，不修改 mihomo 配置：

- `全部`
- `TCP`
- `UDP`
- `Proxy`
- `DIRECT`
- `REJECT`
- `有流量`

`Proxy`、`DIRECT`、`REJECT` 根据 `chains` 或规则结果推断；无法判断时保留在 `全部` 中。

### 可展开连接列表

默认行展示：

- 目标：域名或 IP:端口
- 副信息：进程 · 协议 · 规则
- 节点：当前链路中的主要节点
- 流量：下载 / 上传 / 持续时间
- 操作：断开

点击行展开详情：

- 源地址
- 目标地址
- 规则 payload
- `chains`
- `providerChains`
- `processPath`
- connection id

详细模式不切换为传统表格，而是让每条连接默认展开更多字段，延续可展开列表布局。

## 状态与错误处理

### 核心未运行

显示空态：

```text
核心未运行，开启系统代理或 TUN 后查看连接
```

不弹错误弹窗。

### 核心运行但无连接

显示空态：

```text
暂无活动连接
```

### WebSocket 断开

处理方式：

- 保留最后一次 snapshot。
- 状态显示为「正在重连」。
- 每 2 秒自动重连。
- 用户可点击刷新按钮立即重连。

### 关闭单条连接失败

只在对应行显示短错误，列表数据不清空。

### 全部断开失败

在顶部状态区域显示错误提示，不清空列表。

### 浏览器预览模式

没有 Tauri 或 mihomo 时使用 mock snapshot，便于本地浏览器调试和测试现有预览模式。

## 实现边界

### 前端

在 `src/App.tsx` 中增加连接页状态和渲染：

- `Page` 增加 `connections`
- 侧边栏增加连接入口
- 新增连接 snapshot 状态、WebSocket 生命周期、搜索、筛选、展开行、视图模式
- 复用现有 `displayError` / 浏览器预览思路

CSS 在 `src/App.css` 中新增连接页样式，保持当前命名风格和响应式策略。

### 后端

在 `src-tauri/src/mihomo_api.rs` 中增加关闭连接请求。

在 `src-tauri/src/lib.rs` 中注册 Tauri commands：

- `close_connection`
- `close_all_connections`

不修改 `src-tauri/vendor/mihomo`。

## 测试

### React

更新 `src/App.test.tsx`，覆盖：

- 侧边栏连接入口存在。
- 核心未运行空态。
- mock snapshot 渲染连接行。
- 搜索目标域名、进程、规则、节点。
- 筛选 TCP / UDP / 有流量。
- 展开连接详情。
- 点击断开调用 `close_connection`。

### Rust

为 `mihomo_api` 增加轻量测试：

- URI component 编码对连接 ID 和特殊字符稳定。
- 关闭连接请求使用预期路径。

如果当前测试结构不适合 mock HTTP，则至少覆盖 URI 编码，并在实现中保持关闭接口足够薄。

### 手动验证

实现完成后运行：

```sh
npm test
npm run build
cd src-tauri && cargo test
```

在真实运行环境中验证：

- 打开连接页后实时出现连接。
- 访问网页时连接数和流量变化。
- 单条断开后该连接消失。
- 全部断开后列表清空或快速减少。
- WebSocket 断开后显示重连状态。

## 不做范围

- 不做历史连接记录。
- 不做流量图表。
- 不做按连接限速。
- 不做规则编辑联动。
- 不修改 mihomo vendor 代码。
- 不引入新前端依赖。
