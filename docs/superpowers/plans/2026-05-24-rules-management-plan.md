# Rules Management Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add custom rule creation and rule search to the proxy rules page, with tab-based switching between custom and subscription rules.

**Architecture:** Frontend-only changes to `src/App.tsx` and `src/App.css`. Custom rules stored in localStorage key `easyproxy.customRules`. No Rust backend changes. Rule type auto-detected from user input (IP-CIDR / DOMAIN-SUFFIX / DOMAIN-KEYWORD).

**Tech Stack:** React 19, TypeScript, localStorage, existing CSS patterns

---

### Task 1: Add state variables and localStorage helpers

**Files:**
- Modify: `src/App.tsx`

- [ ] **Step 1: Add localStorage key constant and helper functions**

Add these after the existing `selectedNodesKey` constant (line 49):

```typescript
const customRulesKey = 'easyproxy.customRules'

function readCustomRules(): string[] {
  try {
    const saved = JSON.parse(localStorage.getItem(customRulesKey) ?? '[]')
    return Array.isArray(saved) ? saved.filter((r): r is string => typeof r === 'string') : []
  } catch {
    return []
  }
}

function saveCustomRules(rules: string[]) {
  localStorage.setItem(customRulesKey, JSON.stringify(rules))
}

function detectRuleType(input: string): string | null {
  const trimmed = input.trim()
  if (!trimmed) return null
  if (trimmed.includes('://')) return null
  // IP/CIDR pattern
  if (/^\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}(\/\d{1,2})?$/.test(trimmed)) return 'IP-CIDR'
  // Domain pattern (contains dot)
  if (trimmed.includes('.')) return 'DOMAIN-SUFFIX'
  // Plain keyword
  return 'DOMAIN-KEYWORD'
}
```

- [ ] **Step 2: Add state variables for rules page**

Add these state declarations after the existing `testingGroup` state (line 354):

```typescript
const [customRules, setCustomRules] = useState<string[]>(readCustomRules)
const [rulesTab, setRulesTab] = useState<'custom' | 'subscription'>('custom')
const [rulesSearch, setRulesSearch] = useState('')
const [showAddRuleModal, setShowAddRuleModal] = useState(false)
const [newRuleInput, setNewRuleInput] = useState('')
const [newRuleTarget, setNewRuleTarget] = useState<'Proxy' | 'DIRECT' | 'REJECT'>('Proxy')
```

- [ ] **Step 3: Add derived filtered rules**

Add these `useMemo` calls after the existing `proxyNodes` useMemo (line 360):

```typescript
const filteredCustomRules = useMemo(
  () => rulesSearch.trim()
    ? customRules.filter(r => r.toLowerCase().includes(rulesSearch.toLowerCase().trim()))
    : customRules,
  [customRules, rulesSearch],
)

const filteredSubscriptionRules = useMemo(
  () => rulesSearch.trim()
    ? rules.filter(r => r.toLowerCase().includes(rulesSearch.toLowerCase().trim()))
    : rules,
  [rules, rulesSearch],
)
```

- [ ] **Step 4: Commit**

```bash
git add src/App.tsx
git commit -m "feat: add custom rules state, helpers, and search filtering"
```

---

### Task 2: Implement Tab UI and search bar

**Files:**
- Modify: `src/App.tsx`

- [ ] **Step 1: Replace the rules page content (lines 1139-1157)**

Replace the entire `{page === 'rules' && (` block (from line 1139 to 1157) with:

```tsx
{page === 'rules' && (
  <>
    <div>
      <h2>代理规则</h2>
      <p className="subtitle">
        {customRules.length} 条自定义 · {rules.length} 条订阅
      </p>
    </div>

    <div className="rules-toolbar">
      <input
        className="rules-search"
        value={rulesSearch}
        onChange={(e) => setRulesSearch(e.target.value)}
        placeholder="搜索规则..."
        aria-label="搜索规则"
      />
      <button
        className="rules-add-btn"
        type="button"
        onClick={() => {
          setNewRuleInput('')
          setNewRuleTarget('Proxy')
          setShowAddRuleModal(true)
        }}
      >
        + 添加规则
      </button>
    </div>

    <div className="rules-tabs">
      <button
        className={`rules-tab ${rulesTab === 'custom' ? 'active' : ''}`}
        type="button"
        onClick={() => setRulesTab('custom')}
      >
        自定义规则 · {customRules.length} 条
      </button>
      <button
        className={`rules-tab ${rulesTab === 'subscription' ? 'active' : ''}`}
        type="button"
        onClick={() => setRulesTab('subscription')}
      >
        订阅规则 · {rules.length} 条
      </button>
    </div>

    {rulesTab === 'custom' && (
      <div className="rules-page-list">
        {filteredCustomRules.length > 0 ? (
          filteredCustomRules.map((rule) => (
            <div key={rule} className="rules-page-item rules-page-item-custom">
              <span>{rule}</span>
              <button
                className="rules-delete-btn"
                type="button"
                title="删除规则"
                onClick={() => {
                  const next = customRules.filter(r => r !== rule)
                  setCustomRules(next)
                  saveCustomRules(next)
                }}
              >
                ×
              </button>
            </div>
          ))
        ) : (
          <span className="rules-page-placeholder">
            {rulesSearch.trim()
              ? '未找到匹配的规则'
              : customRules.length === 0
                ? '暂无自定义规则，点击上方按钮添加'
                : '未找到匹配的规则'}
          </span>
        )}
      </div>
    )}

    {rulesTab === 'subscription' && (
      <div className="rules-page-list">
        {filteredSubscriptionRules.length > 0 ? (
          filteredSubscriptionRules.map((rule) => (
            <span key={rule} className="rules-page-item">{rule}</span>
          ))
        ) : (
          <span className="rules-page-placeholder">
            {rules.length === 0
              ? '暂无规则数据'
              : '未找到匹配的规则'}
          </span>
        )}
      </div>
    )}
  </>
)}
```

- [ ] **Step 2: Commit**

```bash
git add src/App.tsx
git commit -m "feat: add rules tab UI with search bar"
```

---

### Task 3: Implement Add Rule modal

**Files:**
- Modify: `src/App.tsx`

- [ ] **Step 1: Derive rule type and validation from input**

After the filtered rules useMemos (added in Task 1), add:

```typescript
const newRuleType = useMemo(() => detectRuleType(newRuleInput), [newRuleInput])
const newRuleValid = newRuleType !== null && newRuleInput.trim().length > 0
const newRuleDuplicate = customRules.some(
  r => r === `${newRuleType},${newRuleInput.trim()},${newRuleTarget}`
)
```

- [ ] **Step 2: Add modal JSX after the `</section>` closing tag (before `</main>`)**

Insert before line 1159's `</main>`:

```tsx
{showAddRuleModal && (
  <div className="modal-overlay" onClick={() => setShowAddRuleModal(false)}>
    <div className="modal-content" onClick={(e) => e.stopPropagation()}>
      <h3 className="modal-title">添加规则</h3>

      <label className="modal-label">域名 或 IP 地址</label>
      <input
        className="modal-input"
        value={newRuleInput}
        onChange={(e) => setNewRuleInput(e.target.value)}
        placeholder="例如 google.com 或 192.168.1.0/24"
        aria-label="规则匹配值"
      />
      {newRuleInput.trim() && !newRuleType && (
        <span className="modal-hint modal-hint-error">
          无法识别输入格式，请输入域名或 IP 地址
        </span>
      )}
      {newRuleType && (
        <span className="modal-hint">将生成为 {newRuleType} 类型规则</span>
      )}
      {newRuleDuplicate && (
        <span className="modal-hint modal-hint-error">该规则已存在</span>
      )}

      <label className="modal-label">策略</label>
      <div className="modal-targets">
        {(['Proxy', 'DIRECT', 'REJECT'] as const).map((target) => (
          <button
            key={target}
            className={`modal-target-btn ${target === newRuleTarget ? 'selected' : ''}`}
            type="button"
            onClick={() => setNewRuleTarget(target)}
          >
            {target}
          </button>
        ))}
      </div>

      <div className="modal-actions">
        <button
          className="modal-cancel-btn"
          type="button"
          onClick={() => setShowAddRuleModal(false)}
        >
          取消
        </button>
        <button
          className="modal-confirm-btn"
          type="button"
          disabled={!newRuleValid || newRuleDuplicate}
          onClick={() => {
            if (!newRuleType) return
            const rule = `${newRuleType},${newRuleInput.trim()},${newRuleTarget}`
            const next = [...customRules, rule]
            setCustomRules(next)
            saveCustomRules(next)
            setShowAddRuleModal(false)
          }}
        >
          添加
        </button>
      </div>
    </div>
  </div>
)}
```

- [ ] **Step 3: Commit**

```bash
git add src/App.tsx
git commit -m "feat: add rule creation modal with auto type detection"
```

---

### Task 4: Add CSS styles

**Files:**
- Modify: `src/App.css`

- [ ] **Step 1: Add styles**

Append to the end of `src/App.css`:

```css
/* === Rules Toolbar === */
.rules-toolbar {
  display: flex;
  gap: 8px;
  align-items: center;
}

.rules-search {
  flex: 1;
  border: 1px solid rgba(0, 0, 0, 0.1);
  border-radius: 8px;
  padding: 7px 12px;
  font: inherit;
  font-size: 13px;
  color: #3a3a3c;
  background: #ffffff;
  transition: border-color 120ms ease;
}

.rules-search::placeholder {
  color: #c0c0c4;
}

.rules-search:focus {
  outline: none;
  border-color: #007aff;
  box-shadow: 0 0 0 2px rgba(0, 122, 255, 0.15);
}

.rules-add-btn {
  flex-shrink: 0;
  padding: 7px 14px;
  border: 0;
  border-radius: 8px;
  background: #007aff;
  color: #ffffff;
  font: inherit;
  font-size: 13px;
  font-weight: 600;
  cursor: pointer;
  transition: background 120ms ease;
}

.rules-add-btn:hover {
  background: #0062cc;
}

/* === Rules Tabs === */
.rules-tabs {
  display: flex;
  gap: 0;
  border-bottom: 1px solid rgba(0, 0, 0, 0.06);
}

.rules-tab {
  flex: 1;
  padding: 10px;
  border: 0;
  background: transparent;
  font: inherit;
  font-size: 13px;
  font-weight: 500;
  color: #8e8e93;
  cursor: pointer;
  position: relative;
  transition: color 160ms ease;
}

.rules-tab.active {
  color: #007aff;
  font-weight: 600;
}

.rules-tab.active::after {
  content: "";
  position: absolute;
  bottom: -1px;
  left: 20%;
  right: 20%;
  height: 2px;
  border-radius: 1px;
  background: #007aff;
}

/* === Rules list enhanced === */
.rules-page-item-custom {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.rules-delete-btn {
  flex-shrink: 0;
  width: 22px;
  height: 22px;
  border: 0;
  border-radius: 4px;
  padding: 0;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  background: transparent;
  color: #8e8e93;
  font-size: 15px;
  cursor: pointer;
  transition: background 120ms ease, color 120ms ease;
}

.rules-delete-btn:hover {
  background: rgba(255, 59, 48, 0.1);
  color: #ff3b30;
}

/* === Modal === */
.modal-overlay {
  position: fixed;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(0, 0, 0, 0.2);
  z-index: 100;
}

.modal-content {
  background: #ffffff;
  border-radius: 12px;
  padding: 24px;
  min-width: 360px;
  max-width: 420px;
  width: 90%;
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.modal-title {
  margin: 0;
  font-size: 16px;
  font-weight: 700;
  color: #1d1d1f;
}

.modal-label {
  font-size: 12px;
  font-weight: 600;
  color: #3a3a3c;
}

.modal-input {
  border: 1px solid rgba(0, 0, 0, 0.1);
  border-radius: 8px;
  padding: 8px 12px;
  font: inherit;
  font-size: 14px;
  color: #3a3a3c;
  background: #ffffff;
  transition: border-color 120ms ease;
}

.modal-input::placeholder {
  color: #c0c0c4;
}

.modal-input:focus {
  outline: none;
  border-color: #007aff;
  box-shadow: 0 0 0 2px rgba(0, 122, 255, 0.15);
}

.modal-hint {
  font-size: 11px;
  color: #8e8e93;
}

.modal-hint-error {
  color: #ff3b30;
}

.modal-targets {
  display: flex;
  gap: 6px;
}

.modal-target-btn {
  flex: 1;
  padding: 8px;
  border: 1px solid rgba(0, 0, 0, 0.1);
  border-radius: 8px;
  background: #ffffff;
  font: inherit;
  font-size: 13px;
  font-weight: 500;
  color: #3a3a3c;
  cursor: pointer;
  transition: all 120ms ease;
}

.modal-target-btn:hover:not(.selected) {
  background: rgba(0, 0, 0, 0.04);
}

.modal-target-btn.selected {
  background: #007aff;
  border-color: #007aff;
  color: #ffffff;
}

.modal-actions {
  display: flex;
  gap: 8px;
  justify-content: flex-end;
  margin-top: 4px;
}

.modal-cancel-btn {
  padding: 8px 18px;
  border: 1px solid rgba(0, 0, 0, 0.1);
  border-radius: 8px;
  background: #ffffff;
  font: inherit;
  font-size: 13px;
  color: #3a3a3c;
  cursor: pointer;
  transition: background 120ms ease;
}

.modal-cancel-btn:hover {
  background: rgba(0, 0, 0, 0.04);
}

.modal-confirm-btn {
  padding: 8px 18px;
  border: 0;
  border-radius: 8px;
  background: #007aff;
  color: #ffffff;
  font: inherit;
  font-size: 13px;
  font-weight: 600;
  cursor: pointer;
  transition: background 120ms ease;
}

.modal-confirm-btn:hover:not(:disabled) {
  background: #0062cc;
}

.modal-confirm-btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

/* === Dark mode overrides === */
@media (prefers-color-scheme: dark) {
  .rules-search {
    background: #2c2c2e;
    border-color: rgba(255, 255, 255, 0.1);
    color: #e5e5e7;
  }

  .rules-tab { color: #a1a1a6; }
  .rules-tab.active { color: #0a84ff; }
  .rules-tab.active::after { background: #0a84ff; }
  .rules-tabs { border-bottom-color: rgba(255, 255, 255, 0.08); }

  .rules-delete-btn { color: #a1a1a6; }
  .rules-delete-btn:hover { background: rgba(255, 69, 58, 0.15); color: #ff453a; }

  .modal-content { background: #2c2c2e; }
  .modal-title { color: #e5e5e7; }
  .modal-label { color: #e5e5e7; }
  .modal-input {
    background: #3a3a3c;
    border-color: rgba(255, 255, 255, 0.1);
    color: #e5e5e7;
  }

  .modal-target-btn {
    background: #3a3a3c;
    border-color: rgba(255, 255, 255, 0.1);
    color: #e5e5e7;
  }
  .modal-target-btn.selected { background: #0a84ff; border-color: #0a84ff; }

  .modal-cancel-btn {
    background: #3a3a3c;
    border-color: rgba(255, 255, 255, 0.1);
    color: #e5e5e7;
  }
}
```

- [ ] **Step 2: Commit**

```bash
git add src/App.css
git commit -m "style: add rules toolbar, tabs, modal, and dark mode styles"
```

---

### Task 5: Manual verification

- [ ] **Step 1: Run the dev server and check UI**

```bash
cd /Users/czh/Projects/EasyProxy && npm run dev
```

Verify:
- Rules page shows "自定义规则" tab selected by default
- Empty custom rules shows placeholder text
- Click "+ 添加规则" opens modal
- Enter `google.com` in modal → shows "将生成为 DOMAIN-SUFFIX 类型规则"
- Enter `192.168.1.0/24` → shows "将生成为 IP-CIDR 类型规则"
- Select target (Proxy/DIRECT/REJECT)
- Click "添加" → modal closes, rule appears in list
- Switch to "订阅规则" tab → subscription rules shown
- Delete a custom rule via × button
- Search in search box filters rules in real-time

- [ ] **Step 2: Commit any fixes if needed**

```bash
git add -A && git commit -m "fix: address verification issues"
```
