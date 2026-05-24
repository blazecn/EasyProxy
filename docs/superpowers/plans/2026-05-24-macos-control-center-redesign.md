# macOS Control Center Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the 2×2 feature grid layout with a macOS system-settings-style sidebar + content area layout.

**Architecture:** A 224px fixed sidebar contains navigation, subscription list, status, and toggle switches. The content area shows one of three pages (总览/线路切换/代理规则) based on sidebar navigation state. All existing business logic (subscription persistence, node caching, browser preview) is preserved.

**Tech Stack:** React 19, TypeScript, Tauri API, lucide-react icons, react-swipeable, plain CSS

---

### Task 1: Rewrite CSS — shell, sidebar, and global styles

**Files:**
- Modify: `src/App.css` (complete rewrite)

- [ ] **Step 1: Replace entire App.css**

Write the complete new CSS file:

```css
/* === Shell === */
.app-shell {
  display: flex;
  min-height: 100vh;
  background: #f0f0f2;
  font-family: -apple-system, BlinkMacSystemFont, "SF Pro Text", "Helvetica Neue", sans-serif;
  color: var(--text-h, #1d1d1f);
}

/* === Sidebar === */
.sidebar {
  width: 224px;
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 20px 12px;
  background: rgba(240, 240, 242, 0.78);
  backdrop-filter: blur(20px);
  -webkit-backdrop-filter: blur(20px);
  border-right: 1px solid rgba(0, 0, 0, 0.06);
  overflow-y: auto;
}

.sidebar-section-title {
  padding: 6px 10px 4px;
  font-size: 10px;
  font-weight: 700;
  color: #8e8e93;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.sidebar-section-title:not(:first-child) {
  margin-top: 18px;
}

.sidebar-section-title button {
  width: 18px;
  height: 18px;
  border: 0;
  border-radius: 4px;
  padding: 0;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  background: transparent;
  color: #8e8e93;
  font-size: 14px;
  cursor: pointer;
  line-height: 1;
}

.sidebar-section-title button:hover {
  background: rgba(0, 0, 0, 0.06);
}

/* Nav items */
.nav-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 10px;
  border: 0;
  border-radius: 6px;
  background: transparent;
  color: #3a3a3c;
  font: inherit;
  font-size: 13px;
  text-align: left;
  cursor: pointer;
  transition: background 120ms ease, color 120ms ease;
}

.nav-item:hover {
  background: rgba(0, 0, 0, 0.04);
}

.nav-item.selected {
  background: rgba(0, 122, 255, 0.12);
  color: #007aff;
  font-weight: 600;
}

.nav-item .nav-icon {
  font-size: 15px;
  width: 20px;
  text-align: center;
  line-height: 1;
}

/* Subscription items in sidebar */
.subscription-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 5px 10px;
  border: 0;
  border-radius: 6px;
  background: transparent;
  color: #3a3a3c;
  font: inherit;
  font-size: 12px;
  text-align: left;
  cursor: pointer;
  width: 100%;
  transition: background 120ms ease, color 120ms ease;
  position: relative;
  z-index: 1;
}

.subscription-item:hover {
  background: rgba(0, 0, 0, 0.04);
}

.subscription-item.selected {
  background: rgba(0, 122, 255, 0.08);
  color: #007aff;
  font-weight: 600;
}

.subscription-item small {
  color: inherit;
  opacity: 0.6;
  font-size: 10px;
}

/* Sidebar swipe container */
.sidebar-subscription-swipe {
  position: relative;
  overflow: hidden;
  border-radius: 6px;
}

.sidebar-subscription-swipe .swipe-target {
  transform: translateX(0);
  transition: transform 180ms ease;
}

.sidebar-subscription-swipe .swipe-target.swiped {
  transform: translateX(-64px);
}

.sidebar-delete-action {
  position: absolute;
  top: 0;
  right: 0;
  bottom: 0;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 56px;
  border: 0;
  border-radius: 0 6px 6px 0;
  color: #ffffff;
  background: #ff3b30;
  font-size: 12px;
  cursor: pointer;
  transition: background 120ms ease;
}

.sidebar-delete-action:hover {
  background: #d6332a;
}

/* Status block */
.sidebar-status {
  padding: 8px 10px;
  border-radius: 8px;
  background: rgba(0, 0, 0, 0.03);
  font-size: 11px;
  color: #8e8e93;
  margin-top: auto;
}

.sidebar-status-row {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-bottom: 2px;
}

.sidebar-status-dot {
  width: 7px;
  height: 7px;
  border-radius: 999px;
  background: #cbd5e1;
  flex-shrink: 0;
}

.sidebar-status-dot.active {
  background: #34c759;
}

.sidebar-status-text {
  font-size: 12px;
  color: #3a3a3c;
  font-weight: 500;
}

.sidebar-status-detail {
  font-size: 11px;
  color: #8e8e93;
  padding-left: 13px;
}

/* Sidebar toggle switches */
.sidebar-toggles {
  display: flex;
  flex-direction: column;
  gap: 4px;
  margin-top: 8px;
}

.sidebar-toggle {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 6px 10px;
  border: 0;
  border-radius: 6px;
  background: transparent;
  font: inherit;
  font-size: 12px;
  font-weight: 500;
  color: #8e8e93;
  cursor: pointer;
  transition: background 120ms ease, color 120ms ease;
}

.sidebar-toggle:hover {
  background: rgba(0, 0, 0, 0.04);
}

.sidebar-toggle.active {
  color: #3a3a3c;
  background: rgba(52, 199, 89, 0.08);
}

.sidebar-toggle:disabled {
  cursor: wait;
  opacity: 0.6;
}

.toggle-track {
  position: relative;
  width: 30px;
  height: 17px;
  border-radius: 999px;
  background: #d1d1d6;
  transition: background 160ms ease;
  flex-shrink: 0;
}

.toggle-track::after {
  content: "";
  position: absolute;
  top: 2px;
  left: 2px;
  width: 13px;
  height: 13px;
  border-radius: 999px;
  background: #ffffff;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.1);
  transition: transform 160ms ease;
}

.sidebar-toggle.active .toggle-track {
  background: #34c759;
}

.sidebar-toggle.active .toggle-track::after {
  transform: translateX(13px);
}

/* Sidebar inline import form */
.sidebar-import-form {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 4px 8px;
}

.sidebar-import-form input {
  width: 100%;
  box-sizing: border-box;
  border: 1px solid rgba(0, 0, 0, 0.1);
  border-radius: 6px;
  padding: 5px 8px;
  font: inherit;
  font-size: 11px;
  color: #3a3a3c;
  background: #ffffff;
}

.sidebar-import-form input::placeholder {
  color: #c0c0c4;
}

.sidebar-import-form button {
  width: 100%;
  padding: 4px;
  border: 0;
  border-radius: 5px;
  background: #007aff;
  color: #ffffff;
  font: inherit;
  font-size: 11px;
  font-weight: 600;
  cursor: pointer;
}

.sidebar-import-form button:disabled {
  opacity: 0.6;
  cursor: wait;
}

/* === Content Area === */
.content-area {
  flex: 1;
  min-width: 0;
  padding: 28px 32px;
  display: flex;
  flex-direction: column;
  gap: 18px;
  overflow-y: auto;
}

.content-area h2 {
  margin: 0;
  font-size: 22px;
  font-weight: 700;
  color: #1d1d1f;
  letter-spacing: -0.02em;
}

.content-area .subtitle {
  margin: 2px 0 0;
  font-size: 13px;
  color: #8e8e93;
}

/* Status bar */
.status-bar {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px 16px;
  border-radius: 10px;
  background: #ffffff;
  border: 1px solid rgba(0, 0, 0, 0.04);
  font-size: 12px;
  color: #8e8e93;
}

.status-bar .status-bar-sep {
  color: #d1d1d6;
}

.status-bar strong {
  color: #3a3a3c;
  font-weight: 600;
}

/* Overview cards */
.overview-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 14px;
  flex: 1;
}

.info-card,
.node-preview-card {
  border: 1px solid rgba(0, 0, 0, 0.04);
  border-radius: 12px;
  padding: 18px;
  background: #ffffff;
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.card-label {
  font-size: 12px;
  font-weight: 700;
  color: #8e8e93;
  text-transform: uppercase;
  letter-spacing: 0.03em;
}

/* Info grid */
.info-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 10px;
}

.info-grid .info-cell {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.info-cell .info-cell-label {
  font-size: 10px;
  color: #8e8e93;
}

.info-cell .info-cell-value {
  font-size: 16px;
  font-weight: 700;
  color: #1d1d1f;
}

.info-cell .info-cell-value.accent {
  color: #007aff;
}

.info-card-actions {
  display: flex;
  gap: 6px;
  margin-top: auto;
}

.info-card-actions button {
  padding: 6px 10px;
  border: 1px solid rgba(0, 0, 0, 0.08);
  border-radius: 6px;
  background: #ffffff;
  font: inherit;
  font-size: 11px;
  font-weight: 500;
  cursor: pointer;
  color: #3a3a3c;
  transition: background 120ms ease;
}

.info-card-actions button:hover {
  background: rgba(0, 0, 0, 0.04);
}

.info-card-actions button.danger {
  color: #ff3b30;
}

.info-card-actions button.danger:hover {
  background: rgba(255, 59, 48, 0.06);
}

/* Info card mode section */
.info-card-modes {
  display: flex;
  gap: 6px;
  margin-top: auto;
}

.info-card-modes button {
  flex: 1;
  padding: 6px 8px;
  border: 1px solid rgba(0, 0, 0, 0.08);
  border-radius: 6px;
  background: #ffffff;
  font: inherit;
  font-size: 11px;
  font-weight: 500;
  color: #3a3a3c;
  cursor: pointer;
  transition: all 120ms ease;
}

.info-card-modes button:hover:not(:disabled) {
  background: rgba(0, 0, 0, 0.04);
}

.info-card-modes button.selected {
  background: #007aff;
  border-color: #007aff;
  color: #ffffff;
}

.info-card-modes button:disabled {
  opacity: 0.6;
  cursor: wait;
}

/* Node preview */
.node-preview-card .node-preview-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.node-preview-card .node-preview-header button {
  border: 0;
  background: transparent;
  color: #007aff;
  font: inherit;
  font-size: 11px;
  font-weight: 500;
  cursor: pointer;
}

.node-preview-list {
  display: flex;
  flex-direction: column;
  gap: 5px;
  flex: 1;
}

.node-preview-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 7px 10px;
  border: 0;
  border-radius: 6px;
  background: transparent;
  font: inherit;
  font-size: 12px;
  color: #3a3a3c;
  cursor: pointer;
  transition: background 120ms ease, color 120ms ease;
}

.node-preview-item:hover:not(:disabled) {
  background: rgba(0, 0, 0, 0.04);
}

.node-preview-item.selected {
  background: rgba(0, 122, 255, 0.06);
  color: #007aff;
  font-weight: 600;
}

.node-preview-item:disabled {
  cursor: wait;
  opacity: 0.6;
}

.node-preview-item small {
  color: inherit;
  opacity: 0.6;
  font-size: 10px;
}

.node-preview-more {
  padding: 7px 10px;
  font-size: 12px;
  color: #8e8e93;
}

/* Full node list page */
.node-page-list {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
  gap: 8px;
  flex: 1;
  align-content: start;
}

/* Rules page */
.rules-page-list {
  display: flex;
  flex-direction: column;
  gap: 6px;
  flex: 1;
}

.rules-page-item {
  padding: 8px 12px;
  border-radius: 8px;
  background: rgba(0, 0, 0, 0.03);
  font-family: ui-monospace, "SF Mono", SFMono-Regular, Menlo, Monaco, Consolas, monospace;
  font-size: 12px;
  color: #3a3a3c;
  word-break: break-all;
}

.rules-page-placeholder {
  padding: 8px 12px;
  border-radius: 8px;
  background: rgba(0, 0, 0, 0.03);
  font-size: 12px;
  color: #8e8e93;
}

/* Responsive */
@media (max-width: 860px) {
  .app-shell {
    flex-direction: column;
  }

  .sidebar {
    width: 100%;
    flex-direction: row;
    flex-wrap: wrap;
    gap: 6px;
    padding: 12px;
    border-right: 0;
    border-bottom: 1px solid rgba(0, 0, 0, 0.06);
    overflow-y: visible;
  }

  .sidebar-section-title {
    display: none;
  }

  .sidebar-status,
  .sidebar-toggles,
  .sidebar-import-form {
    display: none;
  }

  .subscription-item {
    display: none;
  }

  .content-area {
    padding: 18px;
  }

  .overview-grid {
    grid-template-columns: 1fr;
  }
}

@media (max-width: 620px) {
  .content-area {
    padding: 12px;
  }

  .info-grid {
    grid-template-columns: 1fr;
  }

  .node-page-list {
    grid-template-columns: 1fr;
  }
}
```

- [ ] **Step 2: Verify the CSS file saves correctly**

Run: `wc -l src/App.css`
Expected: ~400+ lines

- [ ] **Step 3: Commit**

```bash
git add src/App.css
git commit -m "style: rewrite CSS for sidebar + content layout

Co-Authored-By: Claude Code | deepseek-v4-pro | code"
```

---

### Task 2: Rewrite App.tsx — types, helpers, and data layer (no JSX changes yet)

**Files:**
- Modify: `src/App.tsx`

- [ ] **Step 1: Keep existing imports, types, and helper functions. Add a `Page` type.**

All helper functions and types from lines 1-216 must remain exactly as-is. Add at line 7 (after existing imports):

```typescript
type Page = 'overview' | 'nodes' | 'rules'
```

- [ ] **Step 2: Verify existing code is intact**

Run: `npx tsc --noEmit src/App.tsx 2>&1 | head -20`

- [ ] **Step 3: Commit**

```bash
git add src/App.tsx
git commit -m "refactor: add Page type for sidebar navigation

Co-Authored-By: Claude Code | deepseek-v4-pro | code"
```

---

### Task 3: Rewrite App.tsx — sidebar SubscriptionItem component

**Files:**
- Modify: `src/App.tsx`

- [ ] **Step 1: Add sidebar subscription item component**

Insert after the `SubscriptionCard` component (before `function App()`):

```typescript
interface SidebarSubscriptionItemProps {
  subscription: SavedSubscription
  isSwiped: boolean
  isSelected: boolean
  onSwipeOpen: () => void
  onSwipeClose: () => void
  onSwitch: () => void
  onDelete: () => void
  disabled: boolean
  nodeCount: number
}

function SidebarSubscriptionItem({
  subscription,
  isSwiped,
  isSelected,
  onSwipeOpen,
  onSwipeClose,
  onSwitch,
  onDelete,
  disabled,
  nodeCount,
}: SidebarSubscriptionItemProps) {
  const swipeHandlers = useSwipeable({
    onSwipedLeft: () => onSwipeOpen(),
    onSwipedRight: () => onSwipeClose(),
    onTap: () => {
      if (isSwiped) {
        onSwipeClose()
      } else {
        onSwitch()
      }
    },
    delta: 60,
    preventScrollOnSwipe: true,
    trackMouse: true,
  })

  return (
    <div className="sidebar-subscription-swipe">
      <button
        className={`subscription-item ${isSwiped ? 'swiped' : ''} ${isSelected ? 'selected' : ''}`}
        type="button"
        disabled={disabled}
        {...swipeHandlers}
      >
        <span>{subscription.name}</span>
        <small>{nodeCount} 个</small>
      </button>
      <button
        className="sidebar-delete-action"
        type="button"
        onClick={(event) => {
          event.stopPropagation()
          onDelete()
        }}
      >
        <Trash2 size={14} />
      </button>
    </div>
  )
}
```

- [ ] **Step 2: Commit**

```bash
git add src/App.tsx
git commit -m "feat: add SidebarSubscriptionItem component for sidebar

Co-Authored-By: Claude Code | deepseek-v4-pro | code"
```

---

### Task 4: Rewrite App.tsx — JSX restructure

**Files:**
- Modify: `src/App.tsx`

- [ ] **Step 1: Add page state and replace the entire return JSX**

In `function App()`, add state after existing state declarations (after `swipedUrl`):

```typescript
const [page, setPage] = useState<Page>('overview')
```

Replace the entire `return (...)` block with:

```tsx
  const currentSubscription = savedSubscriptions.find((item) => item.url === activeSubscription)
  const proxyNodes = useMemo(() => getProxyNodes(nodes), [nodes])
  const subscriptionInfoNodes = useMemo(
    () => nodes.filter((node) => isSubscriptionInfoNode(node)),
    [nodes],
  )

  return (
    <main className="app-shell">
      {/* === Sidebar === */}
      <aside className="sidebar">
        <div className="sidebar-section-title">EasyProxy</div>

        <button
          className={`nav-item ${page === 'overview' ? 'selected' : ''}`}
          type="button"
          onClick={() => setPage('overview')}
        >
          <span className="nav-icon">
            <Layout size={16} />
          </span>
          总览
        </button>
        <button
          className={`nav-item ${page === 'nodes' ? 'selected' : ''}`}
          type="button"
          onClick={() => setPage('nodes')}
        >
          <span className="nav-icon">
            <Share2 size={16} />
          </span>
          线路切换
        </button>
        <button
          className={`nav-item ${page === 'rules' ? 'selected' : ''}`}
          type="button"
          onClick={() => setPage('rules')}
        >
          <span className="nav-icon">
            <List size={16} />
          </span>
          代理规则
        </button>

        <div className="sidebar-section-title">
          <span>订阅</span>
          <button
            type="button"
            aria-label="增加订阅"
            onClick={() => {
              setSubscriptionUrl('')
              setIsAddingSubscription(true)
            }}
          >
            +
          </button>
        </div>

        {savedSubscriptions.map((subscription) => (
          <SidebarSubscriptionItem
            key={subscription.url}
            subscription={subscription}
            isSwiped={swipedUrl === subscription.url}
            isSelected={activeSubscription === subscription.url}
            onSwipeOpen={() => setSwipedUrl(subscription.url)}
            onSwipeClose={() => setSwipedUrl(null)}
            onSwitch={() => {
              setSwipedUrl(null)
              switchSavedSubscription(subscription.url)
            }}
            onDelete={() => deleteSubscription(subscription.url)}
            disabled={busyAction === 'subscription'}
            nodeCount={getProxyNodes(subscription.nodes).length}
          />
        ))}

        {isAddingSubscription && (
          <div className="sidebar-import-form">
            <input
              value={subscriptionUrl}
              onChange={(event) => setSubscriptionUrl(event.target.value)}
              placeholder="https://example.com/sub.yaml"
              aria-label="订阅地址"
            />
            <button
              type="button"
              onClick={() => importSubscription()}
              disabled={busyAction === 'subscription'}
            >
              {busyAction === 'subscription' ? '导入中' : '导入'}
            </button>
          </div>
        )}

        <div className="sidebar-status">
          <div className="sidebar-status-row">
            <span className={`sidebar-status-dot ${enabled ? 'active' : ''}`} />
            <span className="sidebar-status-text">{statusText}</span>
          </div>
          <div className="sidebar-status-detail">
            {selectedNode} · {proxyModeOptions.find((m) => m.value === proxyMode)?.label ?? proxyMode}
          </div>
        </div>

        <div className="sidebar-toggles">
          <button
            className={enabled ? 'sidebar-toggle active' : 'sidebar-toggle'}
            type="button"
            role="switch"
            aria-checked={enabled}
            aria-label="系统代理"
            onClick={toggleProxy}
            disabled={busyAction === 'proxy'}
          >
            <span>系统代理</span>
            <span className="toggle-track" aria-hidden="true" />
          </button>
          <button
            className={tunEnabled ? 'sidebar-toggle active' : 'sidebar-toggle'}
            type="button"
            role="switch"
            aria-checked={tunEnabled}
            aria-label="TUN 模式"
            onClick={toggleTunMode}
          >
            <span>TUN 模式</span>
            <span className="toggle-track" aria-hidden="true" />
          </button>
        </div>
      </aside>

      {/* === Content Area === */}
      <section className="content-area">
        {page === 'overview' && (
          <>
            <div>
              <h2>总览</h2>
              <p className="subtitle">
                {currentSubscription ? `${currentSubscription.name} 的代理状态` : '等待导入订阅'}
              </p>
            </div>

            <div className="status-bar">
              <strong>127.0.0.1:7890</strong>
              <span className="status-bar-sep">·</span>
              <span>{statusText}</span>
              {currentSubscription && (
                <>
                  <span className="status-bar-sep">·</span>
                  <span>{currentSubscription.format}</span>
                </>
              )}
            </div>

            <div className="overview-grid">
              <article className="info-card">
                <span className="card-label">订阅信息</span>
                {currentSubscription ? (
                  <>
                    <div className="info-grid">
                      {subscriptionInfoNodes.map((node) => {
                        const info = splitInfoLine(node)
                        return (
                          <div key={node} className="info-cell">
                            <span className="info-cell-label">{info.label}</span>
                            <span className="info-cell-value">{info.value}</span>
                          </div>
                        )
                      })}
                      <div className="info-cell">
                        <span className="info-cell-label">当前线路</span>
                        <span className="info-cell-value accent">{selectedNode}</span>
                      </div>
                      <div className="info-cell">
                        <span className="info-cell-label">节点数量</span>
                        <span className="info-cell-value">{proxyNodes.length} 个</span>
                      </div>
                      <div className="info-cell">
                        <span className="info-cell-label">代理模式</span>
                        <span className="info-cell-value">
                          {proxyModeOptions.find((m) => m.value === proxyMode)?.label ?? proxyMode}
                        </span>
                      </div>
                    </div>
                    <div className="info-card-actions">
                      <button type="button" onClick={startEditingSubscriptionName}>
                        <Pencil size={12} />
                        {' '}重命名
                      </button>
                      <button
                        className="danger"
                        type="button"
                        onClick={() => deleteSubscription(activeSubscription)}
                      >
                        <Trash2 size={12} />
                        {' '}删除
                      </button>
                    </div>
                    <div className="info-card-modes">
                      {proxyModeOptions.map((mode) => (
                        <button
                          key={mode.value}
                          className={mode.value === proxyMode ? 'selected' : ''}
                          type="button"
                          onClick={() => switchProxyMode(mode.value)}
                          disabled={busyAction === 'mode'}
                        >
                          {mode.label}
                        </button>
                      ))}
                    </div>
                  </>
                ) : (
                  <p className="node-preview-more">{message}</p>
                )}
              </article>

              <article className="node-preview-card">
                <div className="node-preview-header">
                  <span className="card-label">线路</span>
                  {proxyNodes.length > 3 && (
                    <button type="button" onClick={() => setPage('nodes')}>
                      查看全部 →
                    </button>
                  )}
                </div>
                {proxyNodes.length > 0 ? (
                  <div className="node-preview-list">
                    {proxyNodes.slice(0, 6).map((node, index) => (
                      <button
                        key={node}
                        className={`node-preview-item ${node === selectedNode ? 'selected' : ''}`}
                        type="button"
                        onClick={() => switchNode(node)}
                        disabled={busyAction === 'node'}
                      >
                        <span>{node}</span>
                        <small>{index === 0 ? '推荐' : `${42 + index * 18} ms`}</small>
                      </button>
                    ))}
                    {proxyNodes.length > 6 && (
                      <div className="node-preview-more">
                        + {proxyNodes.length - 6} 个更多
                      </div>
                    )}
                  </div>
                ) : (
                  <p className="node-preview-more">暂无可用线路</p>
                )}
              </article>
            </div>

            {editingSubscriptionName && currentSubscription && (
              <div style={{
                position: 'fixed', inset: 0, display: 'flex', alignItems: 'center',
                justifyContent: 'center', background: 'rgba(0,0,0,0.2)', zIndex: 100
              }}>
                <div style={{
                  background: '#fff', borderRadius: 12, padding: 24, minWidth: 320,
                  display: 'flex', flexDirection: 'column', gap: 14
                }}>
                  <h3 style={{ margin: 0, fontSize: 16 }}>编辑订阅名称</h3>
                  <input
                    style={{
                      border: '1px solid rgba(0,0,0,0.1)', borderRadius: 8, padding: '8px 12px',
                      font: 'inherit', fontSize: 14
                    }}
                    value={subscriptionNameDraft}
                    onChange={(event) => setSubscriptionNameDraft(event.target.value)}
                    aria-label="订阅名称"
                  />
                  <div style={{ display: 'flex', gap: 8, justifyContent: 'flex-end' }}>
                    <button
                      style={{
                        border: '1px solid rgba(0,0,0,0.1)', borderRadius: 8, padding: '6px 16px',
                        background: '#fff', font: 'inherit', fontSize: 13, cursor: 'pointer'
                      }}
                      type="button"
                      onClick={() => setEditingSubscriptionName(false)}
                    >
                      取消
                    </button>
                    <button
                      style={{
                        border: 0, borderRadius: 8, padding: '6px 16px',
                        background: '#007aff', color: '#fff', font: 'inherit', fontSize: 13,
                        fontWeight: 600, cursor: 'pointer'
                      }}
                      type="button"
                      onClick={saveSubscriptionName}
                    >
                      保存
                    </button>
                  </div>
                </div>
              </div>
            )}
          </>
        )}

        {page === 'nodes' && (
          <>
            <div>
              <h2>线路切换</h2>
              <p className="subtitle">
                {proxyNodes.length} 个可用线路
              </p>
            </div>
            <div className="node-page-list">
              {proxyNodes.map((node, index) => (
                <button
                  key={node}
                  className={`node-preview-item ${node === selectedNode ? 'selected' : ''}`}
                  type="button"
                  onClick={() => switchNode(node)}
                  disabled={busyAction === 'node'}
                >
                  <span>{node}</span>
                  <small>{index === 0 ? '推荐' : `${42 + index * 18} ms`}</small>
                </button>
              ))}
            </div>
          </>
        )}

        {page === 'rules' && (
          <>
            <div>
              <h2>代理规则</h2>
              <p className="subtitle">
                {rules.length > 0 ? `${rules.length} 条规则` : '暂无规则数据'}
              </p>
            </div>
            <div className="rules-page-list">
              {rules.length > 0 ? (
                rules.map((rule) => (
                  <span key={rule} className="rules-page-item">{rule}</span>
                ))
              ) : (
                <span className="rules-page-placeholder">暂无规则数据</span>
              )}
            </div>
          </>
        )}
      </section>
    </main>
  )
```

- [ ] **Step 2: Remove unused variables and the old SubscriptionCard component**

Remove these now-unused declarations:
- `shouldShowSubscriptionForm` (no longer needed)
- `isNodePanelExpanded` state
- `renderNodeButtons` function (inlined now)
- `SubscriptionCard` component (replaced by SidebarSubscriptionItem)

Also remove unused imports. Add to imports from lucide-react:
```typescript
import { ChevronDown, ChevronUp, Pencil, Check, Trash2, Layout, Share2, List } from 'lucide-react'
```

- [ ] **Step 3: Verify type checking**

Run: `npx tsc --noEmit 2>&1`
Expected: No errors

- [ ] **Step 4: Commit**

```bash
git add src/App.tsx
git commit -m "feat: restructure App into sidebar + content layout

- Replace 2x2 feature grid with 224px sidebar + content area
- Sidebar: nav items + subscription list + status + toggle switches
- Content: overview / node switching / rules pages
- Add rename modal dialog instead of inline editing
- Preserve all business logic (subscription persistence, node caching, browser preview)

Co-Authored-By: Claude Code | deepseek-v4-pro | code"
```

---

### Task 5: Update tests for new DOM structure

**Files:**
- Modify: `src/App.test.tsx`

- [ ] **Step 1: Rewrite tests**

Replace entire file:

```typescript
import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { vi } from 'vitest'
import App from './App'

const mockInvoke = vi.hoisted(() => vi.fn())

vi.mock('@tauri-apps/api/core', () => ({
  invoke: mockInvoke,
}))

function expectTextContent(text: string) {
  expect(screen.getByText((_, element) => element?.textContent === text)).toBeInTheDocument()
}

beforeEach(() => {
  localStorage.clear()
  mockInvoke.mockReset()
  mockInvoke.mockImplementation((command: string) => {
    if (command === 'core_status') {
      return Promise.resolve({
        core: 'Stopped',
        mode: 'Rule',
        system_proxy: '127.0.0.1:7890',
      })
    }

    if (command === 'refresh_subscription') {
      return Promise.resolve({
        nodes: ['HK 01', 'SG 02'],
        format: 'clash-yaml',
        content: 'proxies:\n  - name: HK 01\n',
      })
    }

    if (command === 'save_subscription') {
      return Promise.resolve({
        nodes: ['HK 01', 'SG 02'],
        format: 'clash-yaml',
      })
    }

    if (command === 'set_proxy_mode') {
      return Promise.resolve({
        core: 'Stopped',
        mode: 'Global',
        system_proxy: '127.0.0.1:7890',
      })
    }

    return Promise.resolve()
  })
})

describe('EasyProxy shell', () => {
  it('shows the sidebar with navigation and controls', () => {
    render(<App />)

    // Sidebar navigation
    expect(screen.getByRole('button', { name: '总览' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: '线路切换' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: '代理规则' })).toBeInTheDocument()

    // Sidebar switches at bottom
    expect(screen.getByRole('switch', { name: '系统代理' })).toBeInTheDocument()
    expect(screen.getByRole('switch', { name: 'TUN 模式' })).toBeInTheDocument()

    // Import form in sidebar (shown when no subscriptions)
    expect(screen.getByLabelText('订阅地址')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: '导入' })).toBeInTheDocument()

    // Overview page
    expect(screen.getByText('等待导入订阅')).toBeInTheDocument()
  })

  it('switches proxy mode from the overview info card', async () => {
    localStorage.setItem(
      'easyproxy.subscriptions',
      JSON.stringify([
        {
          name: 'one.example',
          url: 'https://one.example/sub.yaml',
          content: 'proxies:\n  - name: HK 01\n',
          nodes: ['HK 01'],
          format: 'clash-yaml',
        },
      ]),
    )

    render(<App />)

    expect(screen.getByRole('button', { name: '规则模式' })).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: '全局模式' }))

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith('set_proxy_mode', { mode: 'Global' })
    })
  })

  it('saves imported subscription and switches without refetching urls', async () => {
    localStorage.setItem(
      'easyproxy.subscriptions',
      JSON.stringify([
        {
          name: 'one.example',
          url: 'https://one.example/sub.yaml',
          content: 'proxies:\n  - name: HK 01\n',
          nodes: ['HK 01'],
          format: 'clash-yaml',
        },
        {
          name: 'two.example',
          url: 'https://two.example/sub.yaml',
          content: 'proxies:\n  - name: SG 02\n',
          nodes: ['SG 02'],
          format: 'clash-yaml',
        },
      ]),
    )

    render(<App />)

    fireEvent.click(screen.getByRole('button', { name: /two.example/ }))

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith('save_subscription', {
        content: 'proxies:\n  - name: SG 02\n',
      })
    })
    expect(mockInvoke).not.toHaveBeenCalledWith('refresh_subscription', {
      url: 'https://two.example/sub.yaml',
    })
  })

  it('shows saved subscription cards in sidebar and opens import form from add button', () => {
    localStorage.setItem(
      'easyproxy.subscriptions',
      JSON.stringify([
        {
          name: 'one.example',
          url: 'https://one.example/sub.yaml',
          content: 'proxies:\n  - name: HK 01\n',
          nodes: ['HK 01'],
          format: 'clash-yaml',
        },
      ]),
    )

    render(<App />)

    // Subscription shown in sidebar
    expect(screen.getByRole('button', { name: /one.example/ })).toBeInTheDocument()

    // Import form hidden
    expect(screen.queryByLabelText('订阅地址')).not.toBeInTheDocument()

    // Overview shows info
    expectTextContent('当前线路HK 01')
    expectTextContent('节点数量1 个')
  })

  it('renames the active subscription via modal', () => {
    localStorage.setItem(
      'easyproxy.subscriptions',
      JSON.stringify([
        {
          name: 'one.example',
          url: 'https://one.example/sub.yaml',
          content: 'proxies:\n  - name: HK 01\n',
          nodes: ['HK 01'],
          format: 'clash-yaml',
        },
      ]),
    )

    render(<App />)

    fireEvent.click(screen.getByRole('button', { name: /重命名/ }))

    fireEvent.change(screen.getByLabelText('订阅名称'), {
      target: { value: '工作订阅' },
    })

    fireEvent.click(screen.getByRole('button', { name: '保存' }))

    expect(JSON.parse(localStorage.getItem('easyproxy.subscriptions') ?? '[]')[0]).toMatchObject({
      name: '工作订阅',
      url: 'https://one.example/sub.yaml',
    })
  })

  it('navigates to full node page from sidebar', () => {
    localStorage.setItem(
      'easyproxy.subscriptions',
      JSON.stringify([
        {
          name: 'one.example',
          url: 'https://one.example/sub.yaml',
          content: 'proxies:\n  - name: HK 01\n',
          nodes: ['HK 01', 'SG 02'],
          format: 'clash-yaml',
        },
      ]),
    )

    render(<App />)

    fireEvent.click(screen.getByRole('button', { name: '线路切换' }))

    expect(screen.getByText('线路切换')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /HK 01/ })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /SG 02/ })).toBeInTheDocument()
  })

  it('filters subscription info nodes into the overview info card', () => {
    localStorage.setItem(
      'easyproxy.subscriptions',
      JSON.stringify([
        {
          name: 'one.example',
          url: 'https://one.example/sub.yaml',
          content: 'proxies:\n  - name: HK 01\n',
          nodes: ['剩余流量：197.92 GB', '距离下次重置剩余：31 天', '套餐到期：2027-02-22', 'HK 01'],
          format: 'clash-yaml',
        },
      ]),
    )

    render(<App />)

    expectTextContent('剩余流量197.92 GB')
    expectTextContent('距离下次重置剩余31 天')
    expectTextContent('套餐到期2027-02-22')
    expect(screen.getByText('节点：HK 01')).toBeInTheDocument()
    expectTextContent('节点数量1 个')
  })

  it('names imported subscriptions from the url domain and selects them', async () => {
    render(<App />)

    fireEvent.change(screen.getByLabelText('订阅地址'), {
      target: { value: 'https://sub.example.com/path/sub.yaml' },
    })
    fireEvent.click(screen.getByRole('button', { name: '导入' }))

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /sub.example.com/ })).toBeInTheDocument()
    })

    const savedSubscriptions = JSON.parse(localStorage.getItem('easyproxy.subscriptions') ?? '[]')
    expect(savedSubscriptions[0]).toMatchObject({
      name: 'sub.example.com',
      url: 'https://sub.example.com/path/sub.yaml',
    })
    expect(localStorage.getItem('easyproxy.activeSubscription')).toBe(
      'https://sub.example.com/path/sub.yaml',
    )
  })

  it('restores the last selected subscription on next launch', () => {
    localStorage.setItem(
      'easyproxy.subscriptions',
      JSON.stringify([
        {
          name: 'one.example',
          url: 'https://one.example/sub.yaml',
          content: 'proxies:\n  - name: HK 01\n',
          nodes: ['HK 01'],
          format: 'clash-yaml',
        },
        {
          name: 'two.example',
          url: 'https://two.example/sub.yaml',
          content: 'proxies:\n  - name: SG 02\n',
          nodes: ['SG 02'],
          format: 'clash-yaml',
        },
      ]),
    )
    localStorage.setItem('easyproxy.activeSubscription', 'https://two.example/sub.yaml')

    render(<App />)

    expect(screen.getByText('节点：SG 02')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /two.example/ })).toHaveClass('selected')
  })

  it('caches the selected node for the active subscription', async () => {
    localStorage.setItem(
      'easyproxy.subscriptions',
      JSON.stringify([
        {
          name: 'one.example',
          url: 'https://one.example/sub.yaml',
          content: 'proxies:\n  - name: HK 01\n',
          nodes: ['HK 01', 'SG 02'],
          format: 'clash-yaml',
        },
      ]),
    )
    localStorage.setItem('easyproxy.activeSubscription', 'https://one.example/sub.yaml')

    render(<App />)

    fireEvent.click(screen.getByRole('button', { name: /SG 02/ }))

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith('select_proxy_node', { node: 'SG 02' })
    })
    expect(JSON.parse(localStorage.getItem('easyproxy.selectedNodes') ?? '{}')).toEqual({
      'https://one.example/sub.yaml': 'SG 02',
    })
  })

  it('keeps other controls usable while a subscription import is pending', () => {
    let resolveImport: (value: { nodes: string[]; format: string; content: string }) => void = () => {}
    mockInvoke.mockImplementation((command: string) => {
      if (command === 'core_status') {
        return Promise.resolve({
          core: 'Stopped',
          mode: 'Rule',
          system_proxy: '127.0.0.1:7890',
        })
      }

      if (command === 'refresh_subscription') {
        return new Promise((resolve) => {
          resolveImport = resolve
        })
      }

      return Promise.resolve()
    })

    render(<App />)

    fireEvent.change(screen.getByLabelText('订阅地址'), {
      target: { value: 'https://example.com/sub.yaml' },
    })
    fireEvent.click(screen.getByRole('button', { name: '导入' }))

    expect(screen.getByRole('button', { name: '导入中' })).toBeDisabled()
    expect(screen.getByRole('switch', { name: '系统代理' })).not.toBeDisabled()
    expect(screen.getByRole('switch', { name: 'TUN 模式' })).not.toBeDisabled()

    resolveImport({ nodes: ['HK 01'], format: 'clash-yaml', content: 'proxies:\n  - name: HK 01\n' })
  })

  it('uses a preview status instead of showing raw Tauri invoke errors', async () => {
    mockInvoke.mockRejectedValueOnce(
      new TypeError("Cannot read properties of undefined (reading 'invoke')"),
    )

    render(<App />)

    await waitFor(() => {
      expect(screen.getByText('浏览器预览模式')).toBeInTheDocument()
    })
    expect(screen.queryByText(/Cannot read properties/)).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: /dy.boost1.shop/ })).toBeInTheDocument()
    expectTextContent('剩余流量197.92 GB')
    expectTextContent('距离下次重置剩余31 天')
    expect(screen.getByText('节点：香港 01')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /香港 01/ })).toBeInTheDocument()
  })
})
```

- [ ] **Step 2: Run tests**

Run: `npx vitest run src/App.test.tsx 2>&1`
Expected: All 12 tests pass

- [ ] **Step 3: Commit**

```bash
git add src/App.test.tsx
git commit -m "test: update tests for sidebar + content layout

Co-Authored-By: Claude Code | deepseek-v4-pro | test"
```

---

### Task 6: Final verification and cleanup

**Files:**
- All modified files

- [ ] **Step 1: Verify the app compiles**

Run: `npx tsc --noEmit 2>&1`
Expected: No errors

- [ ] **Step 2: Run all tests**

Run: `npx vitest run 2>&1`
Expected: All 12 tests pass

- [ ] **Step 3: Remove unused CSS class references in App.tsx**

Check that no old class names are referenced: `summary-bar`, `control-center`, `feature-grid`, `feature-card`, `subscription-card-swipe`, `switch-control`, `node-panel-page`. These should all be gone from App.tsx.

Run: `grep -n 'summary-bar\|control-center\|feature-grid\|feature-card\|subscription-card-swipe\|switch-control\|node-panel-page\|node-card\|card-heading\|subscription-heading' src/App.tsx`
Expected: No matches

- [ ] **Step 4: Verify dark mode media query is covered by system preference**

The CSS uses light colors. Check that there's a dark mode fallback.

Run: `grep -n 'prefers-color-scheme' src/App.css`
Expected: At least one match

If missing, add dark mode section:

```css
@media (prefers-color-scheme: dark) {
  .app-shell {
    background: #1c1c1e;
  }

  .sidebar {
    background: rgba(28, 28, 30, 0.78);
    border-right-color: rgba(255, 255, 255, 0.08);
  }

  .nav-item { color: #d1d1d6; }
  .nav-item:hover { background: rgba(255, 255, 255, 0.06); }
  .nav-item.selected { background: rgba(10, 132, 255, 0.16); color: #0a84ff; }

  .subscription-item { color: #d1d1d6; }
  .subscription-item:hover { background: rgba(255, 255, 255, 0.06); }
  .subscription-item.selected { background: rgba(10, 132, 255, 0.12); color: #0a84ff; }

  .sidebar-status { background: rgba(255, 255, 255, 0.04); }
  .sidebar-status-dot { background: #5a5a5e; }
  .sidebar-status-text { color: #e5e5e7; }

  .status-bar,
  .info-card,
  .node-preview-card {
    background: #2c2c2e;
    border-color: rgba(255, 255, 255, 0.06);
  }

  .toggle-track { background: #5a5a5e; }
  .toggle-track::after { box-shadow: 0 1px 3px rgba(0,0,0,0.3); }

  .content-area h2 { color: #e5e5e7; }
  .card-label { color: #a1a1a6; }
  .info-cell-value { color: #e5e5e7; }
  .info-cell-label { color: #a1a1a6; }
  .node-preview-item { color: #d1d1d6; }

  .sidebar-section-title button:hover { background: rgba(255, 255, 255, 0.08); }

  .sidebar-import-form input {
    background: #3a3a3c;
    border-color: rgba(255, 255, 255, 0.1);
    color: #e5e5e7;
  }
}
```

- [ ] **Step 5: Commit**

```bash
git add src/App.css src/App.tsx src/App.test.tsx
git commit -m "chore: final verification and dark mode support

Co-Authored-By: Claude Code | deepseek-v4-pro | code"
```
