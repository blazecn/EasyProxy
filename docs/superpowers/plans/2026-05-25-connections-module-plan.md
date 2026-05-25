# Connections Module Implementation Plan

> **For agentic workers:** Implement this plan task-by-task. Use checkbox (`- [ ]`) syntax for tracking. The current worktree already contains unrelated source changes; do not revert or include them unless they are directly required by the task.

**Goal:** Add a realtime connections page to EasyProxy. The page shows current mihomo connections in a readable expandable list, supports search/filter/detail mode, and lets users close one connection or all connections.

**Architecture:** The frontend reads realtime snapshots directly from mihomo WebSocket (`ws://127.0.0.1:9090/connections?interval=1000`). Rust only wraps side-effectful close actions through Tauri commands. The UI follows the approved expandable-list layout from `docs/superpowers/specs/2026-05-25-connections-module-design.md`.

**Tech Stack:** React 19, TypeScript, Tauri commands, Rust reqwest blocking client, existing CSS patterns

---

### Task 1: Backend close-connection API

**Files:**
- Modify: `src-tauri/src/mihomo_api.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify or add tests as practical under `src-tauri/tests/` or module tests

- [ ] **Step 1: Add close helpers in `mihomo_api.rs`**

Add:

```rust
pub fn close_connection(id: &str) -> Result<(), String>
pub fn close_all_connections() -> Result<(), String>
```

Use the existing `CONTROLLER` constant. For single close, URL-encode `id` with the existing URI component encoder and send:

```text
DELETE {CONTROLLER}/connections/{encoded_id}
```

For all close, send:

```text
DELETE {CONTROLLER}/connections
```

Return `Ok(())` for 2xx responses. Return a Chinese error message with the HTTP status for non-2xx responses.

- [ ] **Step 2: Expose Tauri commands in `lib.rs`**

Add:

```rust
#[tauri::command]
fn close_connection(id: String) -> Result<(), String> {
    mihomo_api::close_connection(&id)
}

#[tauri::command]
fn close_all_connections() -> Result<(), String> {
    mihomo_api::close_all_connections()
}
```

Register both commands in `tauri::generate_handler!`.

- [ ] **Step 3: Add focused Rust coverage**

If HTTP mocking is not already available, avoid new dependencies. At minimum, add unit coverage for URI component encoding with IDs and special characters used by the close path. If adding request-level tests is low-friction, verify the expected `DELETE /connections/{id}` and `DELETE /connections` paths.

- [ ] **Step 4: Verify backend build/tests**

Run:

```sh
cd src-tauri && cargo test
```

If unrelated dirty worktree changes break tests, record the failure and isolate whether this task introduced it.

---

### Task 2: Frontend connection data model and WebSocket lifecycle

**Files:**
- Modify: `src/App.tsx`

- [ ] **Step 1: Add page and icon wiring**

Extend the `Page` union:

```ts
type Page = 'overview' | 'nodes' | 'connections' | 'rules' | 'dns' | 'logs' | 'settings'
```

Add a sidebar entry named `连接` between `线路切换` and `代理规则`. Use an existing lucide icon that fits the current style, such as `Cable`, `Activity`, or `Network`.

- [ ] **Step 2: Add connection types**

Add TypeScript interfaces for:

```ts
interface ConnectionSnapshot
interface ConnectionItem
type ConnectionStatus = 'idle' | 'connecting' | 'live' | 'reconnecting' | 'error'
type ConnectionFilter = 'all' | 'tcp' | 'udp' | 'proxy' | 'direct' | 'reject' | 'active'
type ConnectionViewMode = 'list' | 'detail'
```

Keep metadata fields optional because mihomo snapshots may omit process, host, or address fields.

- [ ] **Step 3: Add formatting and display helpers**

Add small pure helpers in `App.tsx`:

- `formatBytes(bytes: number): string`
- `formatRate(bytesPerSecond: number): string`
- `formatDuration(start: string, nowMs: number): string`
- `connectionTarget(connection): string`
- `connectionProcess(connection): string`
- `connectionNode(connection): string`
- `connectionRoute(connection): 'Proxy' | 'DIRECT' | 'REJECT' | ''`
- `connectionMatchesSearch(connection, search): boolean`
- `connectionMatchesFilter(connection, filter): boolean`

Prefer readable fallbacks over throwing on missing fields.

- [ ] **Step 4: Add state**

Add state for:

- latest snapshot
- previous snapshot
- WebSocket status
- search string
- selected filter
- view mode
- expanded connection IDs
- row-level close errors
- top-level connection error
- reconnect attempt timer/ref

- [ ] **Step 5: Implement WebSocket lifecycle**

When `page === 'connections'`, connect to:

```text
ws://127.0.0.1:9090/connections?interval=1000
```

On message:

- Parse snapshot defensively.
- Move current snapshot to previous snapshot.
- Store latest snapshot.
- Set status to `live`.

On close/error:

- Preserve the last snapshot.
- Set status to `reconnecting` or `error`.
- Schedule reconnect every 2 seconds while the page remains `connections`.

On page leave:

- Close the WebSocket.
- Clear reconnect timers.

- [ ] **Step 6: Add browser preview fallback**

If WebSocket construction fails in browser preview mode, use a mock snapshot so the page can render during `npm run dev` outside Tauri/mihomo.

---

### Task 3: Connections page UI

**Files:**
- Modify: `src/App.tsx`

- [ ] **Step 1: Add derived data**

Compute:

- `connectionItems`
- `filteredConnections`
- `connectionCount`
- upload/download total
- upload/download rate from latest and previous snapshot
- total traffic
- status label (`实时更新`, `正在连接`, `正在重连`, `连接控制器不可用`)

- [ ] **Step 2: Add toolbar**

Render when `page === 'connections'`:

- left status text: `{count} 个活动连接 · {statusLabel}`
- search input
- segmented view toggle: `列表` / `详细`
- refresh button that rebuilds the WebSocket
- `全部断开` danger button that invokes `close_all_connections`

Disable `全部断开` when there are no connections.

- [ ] **Step 3: Add metrics row**

Render four compact metrics:

- 活动连接
- 下载速率
- 上传速率
- 总流量

- [ ] **Step 4: Add filter chips**

Render filters:

- 全部
- TCP
- UDP
- Proxy
- DIRECT
- REJECT
- 有流量

Use buttons, not links. Keep selection local to the page.

- [ ] **Step 5: Add expandable list rows**

For each filtered connection, render a row with:

- target
- process / network / rule
- node
- download and upload bytes
- duration
- close button

Clicking the row toggles details. Clicking close must stop propagation and invoke `close_connection`.

- [ ] **Step 6: Add details content**

Expanded rows show:

- source address
- destination address
- rule payload
- `chains`
- `providerChains`
- `processPath`
- connection id

In `detail` mode, render details expanded by default while still allowing manual collapse.

- [ ] **Step 7: Add empty and error states**

Handle:

- core not running or controller unavailable
- no active connections
- no results after search/filter
- WebSocket reconnecting while retaining old data
- row-level close failure
- all-close failure

Use inline messages, not modal alerts.

---

### Task 4: Styling and responsive behavior

**Files:**
- Modify: `src/App.css`

- [ ] **Step 1: Add connections page styles**

Add classes for:

- `.connections-toolbar`
- `.connections-search`
- `.connections-view-toggle`
- `.connections-danger-btn`
- `.connections-metrics`
- `.connections-metric`
- `.connections-filters`
- `.connections-filter`
- `.connections-list`
- `.connection-row`
- `.connection-main`
- `.connection-target`
- `.connection-meta`
- `.connection-stats`
- `.connection-detail`
- `.connection-empty`
- `.connection-error`

Follow current color, border, radius, and spacing patterns.

- [ ] **Step 2: Responsive behavior**

For narrow screens:

- toolbar wraps cleanly
- metrics become two columns or one column
- connection row stacks target/meta/stats
- close button remains reachable without overlapping text

- [ ] **Step 3: Dark mode**

Extend the existing `prefers-color-scheme: dark` blocks for the new connection classes.

---

### Task 5: Frontend tests

**Files:**
- Modify: `src/App.test.tsx`
- Modify: `src/test/setup.ts` only if WebSocket mocking is needed globally

- [ ] **Step 1: Mock WebSocket**

Provide a controllable mock WebSocket for tests that can emit a sample snapshot.

- [ ] **Step 2: Cover navigation and empty state**

Assert:

- sidebar contains `连接`
- clicking it shows the connections page
- no snapshot/no connections renders the expected empty state

- [ ] **Step 3: Cover snapshot rendering**

Emit a mock snapshot and assert:

- target host renders
- process renders
- node renders
- rule renders
- traffic values render

- [ ] **Step 4: Cover search, filter, and expansion**

Assert:

- search filters by host/process/rule/node
- TCP/UDP filters work
- `有流量` filters out zero-traffic connections
- expanding a row shows source, destination, chains, and ID

- [ ] **Step 5: Cover close actions**

Assert:

- row close invokes `close_connection` with the connection ID
- all close invokes `close_all_connections`
- failed close displays an inline error

---

### Task 6: Final verification

**Files:**
- No source edits unless verification exposes a bug

- [ ] **Step 1: Run frontend tests**

```sh
npm test
```

- [ ] **Step 2: Run frontend build**

```sh
npm run build
```

- [ ] **Step 3: Run Rust tests**

```sh
cd src-tauri && cargo test
```

- [ ] **Step 4: Run the app manually**

Start the app and verify:

- opening `连接` starts realtime updates
- browsing websites creates rows
- search and filters work
- detail mode reveals metadata
- closing one connection removes or terminates it
- closing all connections clears or sharply reduces the list
- WebSocket reconnect state is visible if mihomo is stopped

- [ ] **Step 5: Commit implementation in focused batches**

Commit backend, frontend UI, styles, and tests in focused commits. Do not include unrelated pre-existing dirty files unless they were intentionally edited for this module.
