# EasyProxy 功能模块分析与迭代方向

## 一、现有功能模块总览

| 模块 | 后端 | 前端 | 完整度 | 备注 |
|------|------|------|--------|------|
| 订阅管理 | config_service.rs | App.tsx (subscription state) | **85%** | 缺少手动刷新、批量操作 |
| 代理核心 | core_manager.rs + service_main.rs | 状态指示 + TUN toggle | **80%** | 缺少优雅关闭、健康检查 |
| 系统代理 | system_proxy.rs | toggle 开关 | **70%** | macOS 完整，Windows 有缺陷，Linux 无 |
| 节点切换 | mihomo_api.rs | nodes 页面 | **75%** | 缺少搜索、收藏、分组排序 |
| 连接监控 | mihomo_api.rs | WebSocket 实时展示 | **80%** | 速率计算粗糙，无流量统计历史 |
| 代理规则 | lib.rs (custom_rules) | rules 页面 | **70%** | 无法编辑已有规则，缺少规则模板 |
| DNS 覆写 | config_service.rs + lib.rs | dns 页面 (form + yaml) | **65%** | YAML 解析器脆弱，缺少验证 |
| 日志查看 | lib.rs (read_logs) | logs 页面 | **60%** | 无日志轮转、过滤、搜索 |
| 设置 | autostart.rs + lib.rs | settings 页面 | **65%** | 仅自启+绕过，缺少大量配置项 |
| IPC 特权服务 | ipc.rs + service_main.rs + service_manager.rs | 无暴露 UI | **85%** | 仅 macOS，无 Windows/Linux |
| 系统托盘 | lib.rs (tray) | — | **90%** | 基本完整 |

---

## 二、各模块详细分析与迭代方向

### 1. 订阅管理 (config_service)

**现状：**
- 支持 Clash YAML、Base64、URI 列表、anytls:// 四种格式
- 支持多订阅保存、切换、删除、重命名
- 订阅刷新走双重 UA 请求 + 合并策略

**缺失：**
- 无「单独刷新当前订阅」按钮（重新导入等同刷新，但 UX 不明确）
- 无订阅自动更新（定时刷新）
- 无订阅导入校验预览（导入前展示节点数、格式等）
- 不支持订阅分组/文件夹管理

**迭代方向：**
- **订阅自动刷新**：后台定时拉取（可配置间隔），变更时通知用户
- **导入预览**：解析后展示节点数、地区分布、格式识别结果，确认后再保存
- **订阅分组**：支持文件夹分类（如「主力」「备用」「测试」）
- **订阅健康度**：记录每次刷新的成功/失败历史，标记失效订阅

---

### 2. URI 协议支持 (config_service)

**现状：**
- 仅支持 `anytls://` URI 转换
- `vmess://`、`ss://`、`trojan://`、`vless://`、`hysteria2://` 等全部返回错误

**这是当前最大的功能缺口。** 绝大多数代理订阅使用这些协议。

**迭代方向（按优先级）：**
1. `ss://` (Shadowsocks) — 最广泛使用
2. `vmess://` (V2Ray) — 国内主流
3. `trojan://` — 简洁高效
4. `vless://` + XTLS — V2Ray 新一代
5. `hysteria2://` — UDP 加速
6. `tuic://` — QUIC 代理
7. `wireguard://` — VPN 隧道

实现建议：抽象 `UriParser` trait，各协议实现独立模块，统一返回 `serde_yaml::Value` 代理描述。

---

### 3. 代理核心管理 (core_manager + service_main)

**现状：**
- 用户空间：CoreManager 通过 spawn 管理子进程，停止时直接 SIGKILL
- 特权空间：service_main 通过 INT→TERM→KILL 优雅关闭
- 无重启逻辑、无健康检查、无崩溃自动恢复

**缺失：**
- CoreManager 停止始终 SIGKILL，可能导致连接中断、配置未保存
- 无 mihomo 崩溃自动重启
- 无 API 控制器健康探针
- 日志无轮转，mihomo.log 无限增长

**迭代方向：**
- **优雅关闭**：CoreManager 统一使用 INT→TERM→KILL 升级策略
- **崩溃恢复**：监控子进程退出码，非用户主动停止时自动重启（带退避）
- **健康检查**：周期性 GET `http://127.0.0.1:9090/`，超时触发重启
- **日志轮转**：按大小（如 10MB）或行数截断，保留最近 N 个文件
- **进程状态细化**：区分 Running / Crashed / Starting / Stopped

---

### 4. 系统代理 (system_proxy)

**现状：**
- macOS：完整实现（networksetup，所有网络服务）
- Windows：可设置代理，但无法检测代理是否已开启
- Linux：全部 stub，返回错误

**缺失：**
- Windows `is_platform_proxy_enabled()` 未实现
- macOS `is_platform_proxy_enabled()` 仅检查 HTTP，未检查 HTTPS/SOCKS
- Linux 完全空白

**迭代方向：**
- **Windows 代理检测**：读取注册表 `ProxyEnable` 值
- **Linux 支持**：
  - GNOME：`gsettings set org.gnome.system.proxy`
  - KDE：修改 `kioslaverc`
  - 通用：`nmcli` 修改网络连接代理设置
- **代理状态一致性**：启动时检测系统代理状态，与 UI 同步
- **代理模式感知**：显示当前系统代理指向的地址是否与 EasyProxy 一致

---

### 5. 节点切换与延迟测试 (mihomo_api)

**现状：**
- 按组展示节点，支持 per-group 节点选择
- 支持 per-group 批量延迟测试
- 选中节点持久化，重启后恢复

**缺失：**
- 节点无搜索/过滤功能（节点多时查找困难）
- 无节点收藏/标记
- 延迟测试无并发限制（一个节点一个线程）
- 节点排序仅按原始顺序，无按延迟排序
- 总览页 mock delay 假数据 `42 + index * 18` 可能误导用户

**迭代方向：**
- **节点搜索**：按名称、协议、地区过滤
- **一键全组测速**：带进度条和并发池（如 10 线程池）
- **按延迟排序**：自动排序展示，低延迟优先
- **节点收藏**：标记常用节点，跨订阅保留映射
- **节点健康历史**：记录过去 N 次测速结果，展示延迟趋势
- **移除假数据**：未测速时不展示延迟值，改为「未测试」

---

### 6. 连接监控 (WebSocket + 前端)

**现状：**
- WebSocket 连接 `ws://127.0.0.1:9090/connections?interval=1000`
- 展示活跃连接数、上下行速率、总流量
- 支持搜索、过滤（all/tcp/udp/proxy/direct/reject/active）
- 可展开查看详情（源、目标、规则链、进程路径）
- 支持单条/批量关闭连接

**缺失：**
- 速率计算用 `delta(total)` 而非时间归一化，间隔不稳时数据不准
- 无流量历史图表（只有实时值）
- 无连接地理信息/IP 归属展示
- 无按流量/持续时间排序

**迭代方向：**
- **速率归一化**：用实际时间差而非假设 1s 间隔
- **流量图表**：引入轻量图表库（如 uPlot），展示 5 分钟/1 小时流量曲线
- **IP 地理信息**：对目标 IP 做 GeoIP 查询，展示国家/城市
- **排序支持**：按流量、持续时间、速率排序
- **连接快照导出**：导出当前连接列表为 CSV/JSON

---

### 7. 代理规则 (custom_rules + subscription rules)

**现状：**
- 自定义规则：增删，支持 IP-CIDR / DOMAIN-SUFFIX / DOMAIN-KEYWORD 自动检测
- 订阅规则：只读展示，可搜索
- 自定义规则优先级高于订阅规则

**缺失：**
- 无法编辑已有自定义规则（只能删除重建）
- 无规则模板/常用规则快捷添加
- 无规则冲突检测
- 无规则生效匹配测试（输入 URL 测试命中规则）
- 无规则导入/导出

**迭代方向：**
- **规则编辑**：行内编辑已有规则
- **规则模板**：预置常用规则集（如「去广告」「国内直连」「AI 服务」）
- **规则测试工具**：输入 URL/IP，展示匹配的规则和最终动作
- **规则导入/导出**：支持 YAML/文本格式导入导出
- **规则冲突检测**：标记重叠或矛盾的规则
- **拖拽排序**：调整规则优先级

---

### 8. DNS 覆写

**现状：**
- 表单模式和 YAML 模式双编辑器
- 支持 nameserver、nameserver-policy、hosts、fallback-filter 配置
- 脏标记 + 保存/取消

**缺失：**
- YAML 解析器是手写的脆弱实现，不能正确处理含冒号的 key、引号值
- `policyMap` 和 `hostsMap` 变量填充后未写入结果（bug）
- 表单模式不能完整覆盖所有 DNS 配置项
- 无配置验证（保存前不检查格式合法性）
- 无 DNS 解析测试

**迭代方向：**
- **引入 YAML 库**：用 `serde_yaml` 替代手写解析器（后端解析，前端展示）
- **修复 bug**：policyMap/hostsMap 未写入结果
- **DNS 解析测试**：输入域名，展示解析结果（A/AAAA/CNAME）
- **配置模板**：预置常用 DNS 配置（如 Google DNS、Cloudflare、国内 DNS）
- **实时预览**：编辑时实时展示生成的 YAML

---

### 9. 日志查看

**现状：**
- 读取 mihomo.log 最后 500 行
- 支持「运行状态」和「核心日志」两个 tab
- 自动刷新（2 秒间隔）+ 手动刷新

**缺失：**
- 日志无级别过滤（ERROR / WARN / INFO / DEBUG）
- 无关键词搜索
- 无日志导出
- 日志量大时（500 行纯文本）渲染性能差
- 无时间戳格式化

**迭代方向：**
- **日志级别过滤**：按 ERROR / WARN / INFO / DEBUG 筛选
- **日志搜索**：关键词高亮 + 上下翻页
- **虚拟滚动**：大量日志时只渲染可见区域
- **日志导出**：保存为文件
- **结构化解析**：解析 mihomo 日志格式，提取时间、级别、模块字段
- **日志持久化**：应用自己的操作日志（与 mihomo 日志分开）

---

### 10. 设置

**现状：**
- 开机自启（macOS LaunchAgent）
- 代理绕过域名管理

**缺失：**
- 无语言设置
- 无主题设置（亮色/暗色）
- 无代理端口配置（hardcoded 7897）
- 无 API 控制器地址配置（hardcoded 9090）
- 无日志级别配置
- 无导入/导出配置
- 无关于页面（版本号、更新检查）
- 无更新检查

**迭代方向：**
- **基础设置扩展**：代理端口、允许局域网连接、API 地址
- **外观设置**：暗色模式、字体大小
- **关于与更新**：版本展示、自动更新检查、更新日志
- **配置导入/导出**：备份恢复全部设置
- **高级设置**：mihomo 启动参数、日志级别、延迟测试超时

---

### 11. IPC 与特权服务 (ipc + service_main + service_manager)

**现状：**
- macOS：完整的 LaunchDaemon + Unix socket + HMAC-SHA256 认证
- Windows/Linux：无

**缺失：**
- Windows 需要 SCM 服务 + Named Pipe 替代 Unix socket
- Linux 需要 systemd unit + Unix socket
- HMAC 密钥硬编码，所有安装相同
- 无消息时间戳新鲜度校验（理论可重放）
- `read_exact` 无超时，可能 hang

**迭代方向：**
- **Windows 服务**：SCM 注册 + Named Pipe IPC
- **Linux 服务**：systemd unit + Unix socket
- **安全加固**：安装时生成随机密钥、消息时间戳校验（±30s 窗口）
- **超时保护**：IPC 读写加超时，防止服务异常导致 GUI 卡死
- **服务版本协商**：主程序与服务版本不兼容时提示升级

---

### 12. 前端架构 (App.tsx)

**现状：**
- 单文件 ~2800 行，50 个 useState，7 个页面全部内联
- 无组件拆分、无路由库、无状态管理
- 浏览器预览模式提供丰富 mock 数据

**缺失：**
- 无 React Error Boundary
- 总览页 info card 和 node list 渲染了两次（桌面 grid + carousel），无响应式切换
- DNS page 无加载状态
- 总览页硬编码 `127.0.0.1:7897`
- `groups[0]?.name ?? ''` 在空 groups 时传空字符串

**迭代方向：**
- **组件拆分**：每个页面拆为独立组件文件，侧边栏、模态框等提取公共组件
- **引入路由**：用 hash router 替代 `page` state，支持浏览器前进后退
- **状态管理**：考虑 useReducer + Context 或 Zustand 管理全局状态
- **Error Boundary**：每个页面区域包裹错误边界，防止整页崩溃
- **响应式重构**：carousel 和 grid 通过 CSS media query 切换而非双渲染
- **移除硬编码**：从后端读取实际代理地址

---

## 三、跨模块系统级缺失

| 缺失项 | 影响范围 | 优先级 |
|--------|----------|--------|
| URI 协议支持不足 | 订阅解析 | **P0** — 阻碍正常使用 |
| Linux 完全不支持 | 系统代理/TUN/自启/服务 | P1 |
| Windows 部分功能缺失 | 代理检测/TUN/自启/服务 | P1 |
| 无国际化 (i18n) | 全局 | P2 |
| 无暗色模式 | UI | P2 |
| 无自动更新机制 | 应用分发 | P2 |
| 无结构化错误类型 | 后端全局 | P3 |
| 无配置备份/恢复 | 设置 | P3 |

---

## 四、建议迭代路线图

### v0.2 — 核心体验补齐
1. URI 协议支持（ss/vmess/trojan/vless/hysteria2）
2. CoreManager 优雅关闭
3. 节点搜索与按延迟排序
4. 移除假延迟数据
5. DNS YAML 解析器 bug 修复

### v0.3 — 功能完善
1. 订阅自动刷新
2. 连接流量图表
3. 规则编辑 + 测试工具
4. 日志级别过滤与搜索
5. 设置页面扩展（端口、主题、关于）
6. 前端组件拆分

### v0.4 — 跨平台
1. Windows 代理检测修复
2. Windows 开机自启
3. Windows 特权服务 (SCM + Named Pipe)
4. Linux 系统代理 (gsettings/nmcli)
5. Linux 自启 (xdg autostart)

### v0.5 — 产品化
1. 自动更新
2. 国际化 (i18n)
3. 暗色模式
4. 配置导入/导出
5. 结构化错误处理 (thiserror)
6. 性能优化（前端虚拟滚动、后端异步 API）
