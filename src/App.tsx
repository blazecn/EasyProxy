import { useEffect, useMemo, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { useSwipeable } from 'react-swipeable'
import { Pencil, Trash2, Layout, Share2, List } from 'lucide-react'
import './App.css'

type CoreStatus = 'Stopped' | 'Running'
type BackendProxyMode = 'Rule' | 'Global' | 'Direct'
type Page = 'overview' | 'nodes' | 'rules'

const proxyModeOptions: Array<{ value: BackendProxyMode; label: string }> = [
  { value: 'Rule', label: '规则模式' },
  { value: 'Global', label: '全局模式' },
  { value: 'Direct', label: '直连模式' },
]

interface AppStatus {
  core: CoreStatus
  mode: BackendProxyMode
  system_proxy: string
}

interface SubscriptionSummary {
  nodes: string[]
  format: string
}

interface ImportedSubscription extends SubscriptionSummary {
  content: string
}

interface SavedSubscription extends ImportedSubscription {
  name: string
  url: string
}

const savedSubscriptionsKey = 'easyproxy.subscriptions'
const activeSubscriptionKey = 'easyproxy.activeSubscription'
const selectedNodesKey = 'easyproxy.selectedNodes'
const browserPreviewMessage = '浏览器预览模式'
function buildBrowserPreviewContent() {
  const regions: Array<[string, number]> = [
    ['香港', 5],
    ['新加坡', 5],
    ['日本', 5],
    ['美国', 5],
    ['韩国', 3],
    ['台湾', 3],
    ['德国', 2],
    ['英国', 2],
  ]

  const proxyNames: string[] = []
  const yamlLines: string[] = ['proxies:']
  for (const [region, count] of regions) {
    for (let i = 1; i <= count; i++) {
      const name = `${region} ${String(i).padStart(2, '0')}`
      proxyNames.push(name)
      yamlLines.push(`  - name: ${name}`)
    }
  }

  return {
    content: yamlLines.join('\n'),
    nodes: proxyNames,
  }
}

const browserPreviewContent = buildBrowserPreviewContent()

const browserPreviewRules: string[] = [
  'DOMAIN-SUFFIX,google.com,Proxy',
  'DOMAIN-SUFFIX,youtube.com,Proxy',
  'DOMAIN-SUFFIX,twitter.com,Proxy',
  'DOMAIN-KEYWORD,github,Proxy',
  'DOMAIN-KEYWORD,openai,Proxy',
  'GEOIP,CN,DIRECT',
  'MATCH,Proxy',
]

const browserPreviewSubscription: SavedSubscription = {
  name: 'dy.boost1.shop',
  url: 'https://dy.boost1.shop/sub.yaml',
  content: browserPreviewContent.content,
  format: 'clash-yaml',
  nodes: [
    '剩余流量：197.92 GB',
    '距离下次重置剩余：31 天',
    '套餐到期：2027-02-22',
    '永久官网:666.boostqz.com',
    ...browserPreviewContent.nodes,
  ],
}

function isTauriRuntimeMissing(error: unknown) {
  const message = String(error)
  return message.includes("__TAURI_INTERNALS__") || message.includes("reading 'invoke'")
}

function displayError(error: unknown) {
  return isTauriRuntimeMissing(error) ? browserPreviewMessage : String(error)
}

function getSubscriptionName(url: string) {
  try {
    return new URL(url).hostname || url
  } catch {
    return url.replace(/^https?:\/\//, '').split('/')[0] || url
  }
}

function readSavedSubscriptions() {
  try {
    const saved = JSON.parse(localStorage.getItem(savedSubscriptionsKey) ?? '[]')
    return Array.isArray(saved)
      ? saved.filter(
          (item): item is SavedSubscription =>
            typeof item?.url === 'string' &&
            typeof item?.content === 'string' &&
            typeof item?.format === 'string' &&
            Array.isArray(item?.nodes),
        ).map((item) => ({
          ...item,
          name: typeof item.name === 'string' && item.name.trim() ? item.name : getSubscriptionName(item.url),
        }))
      : []
  } catch {
    return []
  }
}

function saveSubscriptions(subscriptions: SavedSubscription[]) {
  localStorage.setItem(savedSubscriptionsKey, JSON.stringify(subscriptions))
}

function saveActiveSubscription(url: string) {
  localStorage.setItem(activeSubscriptionKey, url)
}

function readSelectedNodes() {
  try {
    const saved = JSON.parse(localStorage.getItem(selectedNodesKey) ?? '{}')
    return saved && typeof saved === 'object' && !Array.isArray(saved)
      ? Object.fromEntries(
          Object.entries(saved).filter(
            (entry): entry is [string, string] =>
              typeof entry[0] === 'string' && typeof entry[1] === 'string',
          ),
        )
      : {}
  } catch {
    return {}
  }
}

function saveSelectedNode(subscriptionUrl: string, node: string) {
  localStorage.setItem(
    selectedNodesKey,
    JSON.stringify({
      ...readSelectedNodes(),
      [subscriptionUrl]: node,
    }),
  )
}

function isSubscriptionInfoNode(node: string) {
  return /剩余流量|重置剩余|套餐到期|到期|官网|流量/i.test(node)
}

function getProxyNodes(nodes: string[]) {
  return nodes.filter((node) => !isSubscriptionInfoNode(node))
}

function splitInfoLine(line: string) {
  const separatorIndex = line.search(/[：:]/)
  if (separatorIndex === -1) {
    return { label: line, value: '' }
  }

  return {
    label: line.slice(0, separatorIndex),
    value: line.slice(separatorIndex + 1).trim(),
  }
}

function applySubscriptionSummary(summary: SubscriptionSummary, preferredNode?: string) {
  const proxyNodes = getProxyNodes(summary.nodes)

  return {
    nodes: summary.nodes,
    selectedNode:
      preferredNode && proxyNodes.includes(preferredNode)
        ? preferredNode
        : proxyNodes[0] ?? '无可用节点',
  }
}

function readInitialSubscriptionState() {
  const savedSubscriptions = readSavedSubscriptions()
  const savedActiveSubscription = localStorage.getItem(activeSubscriptionKey) ?? ''
  const activeSubscription =
    savedSubscriptions.find((item) => item.url === savedActiveSubscription)?.url ??
    savedSubscriptions[0]?.url ??
    ''
  const activeSummary = savedSubscriptions.find((item) => item.url === activeSubscription)
  const selectedNodes = readSelectedNodes()
  const appliedSummary = activeSummary
    ? applySubscriptionSummary(activeSummary, selectedNodes[activeSubscription])
    : null

  return {
    savedSubscriptions,
    activeSubscription,
    nodes: appliedSummary?.nodes ?? [],
    selectedNode: appliedSummary?.selectedNode ?? '未选择',
  }
}

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

function App() {
  const [initialSubscriptionState] = useState(readInitialSubscriptionState)
  const [enabled, setEnabled] = useState(false)
  const [tunEnabled, setTunEnabled] = useState(false)
  const [subscriptionUrl, setSubscriptionUrl] = useState('')
  const [savedSubscriptions, setSavedSubscriptions] = useState(
    initialSubscriptionState.savedSubscriptions,
  )
  const [activeSubscription, setActiveSubscription] = useState(
    initialSubscriptionState.activeSubscription,
  )
  const [isAddingSubscription, setIsAddingSubscription] = useState(
    initialSubscriptionState.savedSubscriptions.length === 0,
  )
  const [nodes, setNodes] = useState(initialSubscriptionState.nodes)
  const [selectedNode, setSelectedNode] = useState(initialSubscriptionState.selectedNode)
  const [proxyMode, setProxyMode] = useState<BackendProxyMode>('Rule')
  const [editingSubscriptionName, setEditingSubscriptionName] = useState(false)
  const [subscriptionNameDraft, setSubscriptionNameDraft] = useState('')
  const [message, setMessage] = useState('等待导入订阅')
  const [busyAction, setBusyAction] = useState<string | null>(null)
  const [rules, setRules] = useState<string[]>([])
  const [swipedUrl, setSwipedUrl] = useState<string | null>(null)
  const [page, setPage] = useState<Page>('overview')

  const statusText = enabled ? '已连接' : '未连接'
  const currentSubscription = savedSubscriptions.find((item) => item.url === activeSubscription)
  const proxyNodes = useMemo(() => getProxyNodes(nodes), [nodes])
  const subscriptionInfoNodes = useMemo(
    () => nodes.filter((node) => isSubscriptionInfoNode(node)),
    [nodes],
  )

  useEffect(() => {
    let mounted = true

    invoke<AppStatus>('core_status')
      .then((status) => {
        if (!mounted) return
        setEnabled(status.core === 'Running')
        setProxyMode(status.mode)
      })
      .catch((error) => {
        if (!mounted) return
        setMessage(displayError(error))
        if (isTauriRuntimeMissing(error) && initialSubscriptionState.savedSubscriptions.length === 0) {
          const nextSummary = applySubscriptionSummary(browserPreviewSubscription)
          setSavedSubscriptions([browserPreviewSubscription])
          setActiveSubscription(browserPreviewSubscription.url)
          setIsAddingSubscription(false)
          setNodes(nextSummary.nodes)
          setSelectedNode(nextSummary.selectedNode)
          setRules(browserPreviewRules)
        }
      })

    return () => {
      mounted = false
    }
  }, [initialSubscriptionState.savedSubscriptions.length])

  async function toggleProxy() {
    const nextEnabled = !enabled
    setEnabled(nextEnabled)
    setMessage(nextEnabled ? '正在启动 Mihomo 内核并设置系统代理' : '正在关闭系统代理')
    setBusyAction('proxy')

    try {
      const status = await invoke<AppStatus>(nextEnabled ? 'start_core' : 'stop_core')
      setEnabled(status.core === 'Running')
      setProxyMode(status.mode)
      setMessage(nextEnabled ? '代理已启动' : '代理已停止')
    } catch (error) {
      setEnabled(!nextEnabled)
      setMessage(displayError(error))
    } finally {
      setBusyAction(null)
    }
  }

  async function importSubscription(url = subscriptionUrl.trim()) {
    const nextUrl = url.trim()
    if (!nextUrl) {
      setMessage('请输入订阅地址')
      return
    }

    setSubscriptionUrl(nextUrl)
    setMessage('正在导入订阅')
    setBusyAction('subscription')
    try {
      const summary = await invoke<ImportedSubscription>('refresh_subscription', {
        url: nextUrl,
      })
      const nextSummary = applySubscriptionSummary(summary, readSelectedNodes()[nextUrl])
      const nextSubscription = {
        name: getSubscriptionName(nextUrl),
        url: nextUrl,
        content: summary.content,
        nodes: summary.nodes,
        format: summary.format,
      }
      setNodes(nextSummary.nodes)
      setSelectedNode(nextSummary.selectedNode)
      setActiveSubscription(nextUrl)
      saveActiveSubscription(nextUrl)
      setSavedSubscriptions((current) => {
        const nextSubscriptions = [
          nextSubscription,
          ...current.filter((item) => item.url !== nextUrl),
        ]
        saveSubscriptions(nextSubscriptions)
        return nextSubscriptions
      })
      setIsAddingSubscription(false)
      setMessage(`订阅导入成功，识别为 ${summary.format}，共 ${summary.nodes.length} 个节点`)
    } catch (error) {
      setMessage(displayError(error))
    } finally {
      setBusyAction(null)
    }
  }

  async function switchSavedSubscription(url: string) {
    const saved = savedSubscriptions.find((item) => item.url === url)
    if (!saved) return

    setMessage('正在切换订阅')
    setBusyAction('subscription')
    try {
      const summary = await invoke<SubscriptionSummary>('save_subscription', {
        content: saved.content,
      })
      const nextSummary = applySubscriptionSummary(summary, readSelectedNodes()[saved.url])
      setNodes(nextSummary.nodes)
      setSelectedNode(nextSummary.selectedNode)
      setActiveSubscription(saved.url)
      saveActiveSubscription(saved.url)
      setMessage(`已切换订阅，共 ${summary.nodes.length} 个节点`)
    } catch (error) {
      setMessage(displayError(error))
    } finally {
      setBusyAction(null)
    }
  }

  async function switchNode(node: string) {
    if (!enabled) {
      setSelectedNode(node)
      if (activeSubscription) {
        saveSelectedNode(activeSubscription, node)
      }
      setMessage(`已选择 ${node}`)
      return
    }

    const previousNode = selectedNode
    setSelectedNode(node)
    setBusyAction('node')

    try {
      await invoke('select_proxy_node', { node })
      if (activeSubscription) {
        saveSelectedNode(activeSubscription, node)
      }
      setMessage(`已切换到 ${node}`)
    } catch (error) {
      setSelectedNode(previousNode)
      setMessage(displayError(error))
    } finally {
      setBusyAction(null)
    }
  }

  async function switchProxyMode(mode: BackendProxyMode) {
    const previousMode = proxyMode
    setProxyMode(mode)
    setMessage('正在切换代理模式')
    setBusyAction('mode')

    try {
      const status = await invoke<AppStatus>('set_proxy_mode', { mode })
      setEnabled(status.core === 'Running')
      setProxyMode(status.mode)
      setMessage(`已切换到 ${proxyModeOptions.find((item) => item.value === status.mode)?.label ?? mode}`)
    } catch (error) {
      setProxyMode(previousMode)
      setMessage(displayError(error))
    } finally {
      setBusyAction(null)
    }
  }

  function deleteSubscription(url: string) {
    setSwipedUrl(null)
    setSavedSubscriptions((current) => {
      const nextSubscriptions = current.filter((item) => item.url !== url)
      saveSubscriptions(nextSubscriptions)
      if (url === activeSubscription) {
        const nextActive = nextSubscriptions[0]
        if (nextActive) {
          setActiveSubscription(nextActive.url)
          saveActiveSubscription(nextActive.url)
        } else {
          setActiveSubscription('')
          localStorage.removeItem(activeSubscriptionKey)
          setNodes([])
          setSelectedNode('未选择')
          setIsAddingSubscription(true)
        }
      }
      return nextSubscriptions
    })
    setMessage('已删除订阅')
  }

  function startEditingSubscriptionName() {
    if (!currentSubscription) return
    setSubscriptionNameDraft(currentSubscription.name)
    setEditingSubscriptionName(true)
  }

  function saveSubscriptionName() {
    if (!currentSubscription) return

    const nextName = subscriptionNameDraft.trim()
    if (!nextName) {
      setMessage('订阅名称不能为空')
      return
    }

    setSavedSubscriptions((current) => {
      const nextSubscriptions = current.map((subscription) =>
        subscription.url === currentSubscription.url
          ? { ...subscription, name: nextName }
          : subscription,
      )
      saveSubscriptions(nextSubscriptions)
      return nextSubscriptions
    })
    setEditingSubscriptionName(false)
    setMessage('订阅名称已更新')
  }

  function toggleTunMode() {
    const nextEnabled = !tunEnabled
    setTunEnabled(nextEnabled)
    setMessage(nextEnabled ? 'TUN 模式已开启' : 'TUN 模式已关闭')
  }

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
}

export default App
