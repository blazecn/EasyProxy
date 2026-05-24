import { useEffect, useMemo, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { useSwipeable } from 'react-swipeable'
import { ChevronDown, ChevronUp, Pencil, Check, Trash2 } from 'lucide-react'
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

interface SubscriptionCardProps {
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

function SubscriptionCard({
  subscription,
  isSwiped,
  isSelected,
  onSwipeOpen,
  onSwipeClose,
  onSwitch,
  onDelete,
  disabled,
  nodeCount,
}: SubscriptionCardProps) {
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
    <div className="subscription-swipe-container" {...swipeHandlers}>
      <button
        className={`subscription-card-swipe ${isSwiped ? 'swiped' : ''} ${isSelected ? 'selected' : ''}`}
        type="button"
        disabled={disabled}
      >
        <span>{subscription.name}</span>
        <small>{nodeCount} 个节点</small>
      </button>
      <button
        className="subscription-delete-action"
        type="button"
        onClick={(event) => {
          event.stopPropagation()
          onDelete()
        }}
      >
        <Trash2 size={16} />
        <span>删除</span>
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
  const [isNodePanelExpanded, setIsNodePanelExpanded] = useState(false)
  const [rules, setRules] = useState<string[]>([])
  const [swipedUrl, setSwipedUrl] = useState<string | null>(null)

  const statusText = enabled ? '已连接' : '未连接'
  const shouldShowSubscriptionForm = savedSubscriptions.length === 0 || isAddingSubscription
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

  function renderNodeButtons(listClassName = 'node-list') {
    return (
      <div className={listClassName}>
        {proxyNodes.map((node, index) => (
          <button
            key={node}
            className={node === selectedNode ? 'node selected' : 'node'}
            type="button"
            onClick={() => switchNode(node)}
            disabled={busyAction === 'node'}
          >
            <span>{node}</span>
            <small>{index === 0 ? '推荐' : `${42 + index * 18} ms`}</small>
          </button>
        ))}
      </div>
    )
  }

  return (
    <main className="app-shell">
      <section className="control-center" aria-label="EasyProxy 首页控制中心">
        <section className="summary-bar" aria-label="当前代理摘要">
          <p>{message}</p>
          <span>状态：{statusText}</span>
          <span>节点：{selectedNode}</span>
          <button
            className={enabled ? 'switch-control active' : 'switch-control'}
            role="switch"
            aria-checked={enabled}
            aria-label="系统代理"
            type="button"
            onClick={toggleProxy}
            disabled={busyAction === 'proxy'}
          >
            <span>系统代理</span>
            <span className="switch-track" aria-hidden="true" />
          </button>
          <button
            className={tunEnabled ? 'switch-control active' : 'switch-control'}
            role="switch"
            aria-checked={tunEnabled}
            aria-label="TUN 模式"
            type="button"
            onClick={toggleTunMode}
          >
            <span>TUN 模式</span>
            <span className="switch-track" aria-hidden="true" />
          </button>
        </section>

        {isNodePanelExpanded && proxyNodes.length > 0 ? (
          <section className="node-panel-page" aria-label="线路切换">
            <div className="node-heading">
              <p className="label">线路</p>
              <button
                className="node-panel-icon"
                type="button"
                aria-label="收起线路"
                onClick={() => setIsNodePanelExpanded(false)}
              >
                <ChevronUp size={14} />
              </button>
            </div>
            {renderNodeButtons('node-list node-list-fullpage')}
          </section>
        ) : (
          <section className="feature-grid" aria-label="核心功能">
          <article className="feature-card">
            <div className="card-heading">
              <div className="subscription-heading">
                <p className="label">订阅</p>
                {currentSubscription &&
                  (editingSubscriptionName ? (
                    <div className="subscription-name-editor">
                      <label htmlFor="subscription-name">订阅名称</label>
                      <input
                        id="subscription-name"
                        value={subscriptionNameDraft}
                        onChange={(event) => setSubscriptionNameDraft(event.target.value)}
                      />
                      <button
                        className="card-icon-button"
                        type="button"
                        aria-label="保存订阅名称"
                        onClick={saveSubscriptionName}
                      >
                        <Check size={14} />
                      </button>
                    </div>
                  ) : (
                    <div className="subscription-name-row">
                      <span>{currentSubscription.name}</span>
                      <button
                        className="card-icon-button"
                        type="button"
                        aria-label="编辑订阅名称"
                        onClick={startEditingSubscriptionName}
                      >
                        <Pencil size={14} />
                      </button>
                    </div>
                  ))}
              </div>
            </div>
            {currentSubscription && (
              <div className="subscription-overview">
                <div>
                  {subscriptionInfoNodes.map((node) => {
                    const info = splitInfoLine(node)

                    return (
                      <span key={node} className="subscription-info-row">
                        <span>{info.label}</span>
                        <span>{info.value}</span>
                      </span>
                    )
                  })}
                  <span className="subscription-info-row">
                    <span>当前线路</span>
                    <span>{selectedNode}</span>
                  </span>
                  <span className="subscription-info-row">
                    <span>节点数量</span>
                    <span>{proxyNodes.length} 个</span>
                  </span>
                </div>
              </div>
            )}
            {savedSubscriptions.length > 0 && (
              <div className="subscription-list" aria-label="已保存订阅">
                {savedSubscriptions.map((subscription) => (
                  <SubscriptionCard
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
                <button
                  className="subscription-card add-card"
                  type="button"
                  aria-label="增加订阅"
                  onClick={() => {
                    setSubscriptionUrl('')
                    setIsAddingSubscription(true)
                  }}
                  disabled={busyAction === 'subscription'}
                >
                  <span>增加订阅</span>
                  <small>导入新的订阅地址</small>
                </button>
              </div>
            )}
            {shouldShowSubscriptionForm && (
              <div className="subscription-form">
                <label htmlFor="subscription">订阅地址</label>
                <div className="input-row">
                  <input
                    id="subscription"
                    value={subscriptionUrl}
                    onChange={(event) => setSubscriptionUrl(event.target.value)}
                    placeholder="https://example.com/sub.yaml"
                  />
                  <button
                    type="button"
                    onClick={() => importSubscription()}
                    disabled={busyAction === 'subscription'}
                  >
                    {busyAction === 'subscription' ? '导入中' : '导入'}
                  </button>
                </div>
              </div>
            )}
          </article>

          {proxyNodes.length > 0 && (
            <article className="feature-card node-card">
              <div className="node-heading">
                <p className="label">线路</p>
                <button
                  className="node-panel-icon"
                  type="button"
                  aria-label="展开线路"
                  onClick={() => setIsNodePanelExpanded(true)}
                >
                  <ChevronDown size={14} />
                </button>
              </div>
              {renderNodeButtons()}
            </article>
          )}

          {savedSubscriptions.length > 0 && (
            <article className="feature-card">
              <div className="card-heading">
                <div>
                  <p className="label">模式</p>
                </div>
              </div>
              <div className="mode-list" aria-label="模式切换">
                {proxyModeOptions.map((mode) => (
                  <button
                    key={mode.value}
                    className={mode.value === proxyMode ? 'mode-option selected' : 'mode-option'}
                    type="button"
                    onClick={() => switchProxyMode(mode.value)}
                    disabled={busyAction === 'mode'}
                  >
                    {mode.label}
                  </button>
                ))}
              </div>
            </article>
          )}

          {savedSubscriptions.length > 0 && (
            <article className="feature-card">
              <div className="card-heading">
                <p className="label">规则</p>
              </div>
              <div className="rule-list">
                {rules.length > 0 ? (
                  rules.map((rule) => (
                    <span key={rule} className="rule-item">{rule}</span>
                  ))
                ) : (
                  <span className="rule-item rule-placeholder">暂无规则数据</span>
                )}
              </div>
            </article>
          )}

        </section>
        )}
      </section>
    </main>
  )
}

export default App
