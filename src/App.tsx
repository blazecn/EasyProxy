import { useEffect, useMemo, useRef, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { Pencil, Trash2, Layout, Share2, List, ChevronDown, Globe, Settings } from 'lucide-react'
import './App.css'

type CoreStatus = 'Stopped' | 'Running'
type BackendProxyMode = 'Rule' | 'Global' | 'Direct'
type Page = 'overview' | 'nodes' | 'rules' | 'dns' | 'settings'

const proxyModeOptions: Array<{ value: BackendProxyMode; label: string }> = [
  { value: 'Rule', label: '规则模式' },
  { value: 'Global', label: '全局模式' },
  { value: 'Direct', label: '直连模式' },
]

interface AppStatus {
  core: CoreStatus
  mode: BackendProxyMode
  system_proxy: string
  tun_enabled: boolean
}

interface ProxyGroupSummary {
  name: string
  type: string
  nodes: string[]
}

interface SubscriptionSummary {
  nodes: string[]
  format: string
  rules: string[]
  groups: ProxyGroupSummary[]
  node_types: Record<string, string>
}

interface ImportedSubscription extends SubscriptionSummary {
  content: string
}

interface SavedSubscription extends ImportedSubscription {
  name: string
  url: string
}

export function detectRuleType(input: string): string | null {
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

const browserPreviewGroups: ProxyGroupSummary[] = [
  { name: '香港 - 5 条', type: 'url-test', nodes: browserPreviewContent.nodes.filter(n => n.startsWith('香港')) },
  { name: '新加坡 - 5 条', type: 'url-test', nodes: browserPreviewContent.nodes.filter(n => n.startsWith('新加坡')) },
  { name: '日本 - 5 条', type: 'url-test', nodes: browserPreviewContent.nodes.filter(n => n.startsWith('日本')) },
  { name: '美国 - 5 条', type: 'url-test', nodes: browserPreviewContent.nodes.filter(n => n.startsWith('美国')) },
  { name: '韩国 - 3 条', type: 'url-test', nodes: browserPreviewContent.nodes.filter(n => n.startsWith('韩国')) },
  { name: '台湾 - 3 条', type: 'url-test', nodes: browserPreviewContent.nodes.filter(n => n.startsWith('台湾')) },
  { name: '德国 - 2 条', type: 'url-test', nodes: browserPreviewContent.nodes.filter(n => n.startsWith('德国')) },
  { name: '英国 - 2 条', type: 'url-test', nodes: browserPreviewContent.nodes.filter(n => n.startsWith('英国')) },
]

const browserPreviewSubscription: SavedSubscription = {
  name: 'dy.boost1.shop',
  url: 'https://dy.boost1.shop/sub.yaml',
  content: browserPreviewContent.content,
  format: 'clash-yaml',
  rules: browserPreviewRules,
  groups: browserPreviewGroups,
  nodes: [
    '剩余流量：197.92 GB',
    '距离下次重置剩余：31 天',
    '套餐到期：2027-02-22',
    '永久官网:666.boostqz.com',
    ...browserPreviewContent.nodes,
  ],
  node_types: {},
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

function isSubscriptionInfoNode(node: string) {
  return /流量|重置|到期|官网/i.test(node)
}

function getProxyNodes(nodes: string[]) {
  return nodes.filter((node) => !isSubscriptionInfoNode(node))
}

const specialIcons: Record<string, string> = {
  '自动选择': '⚡',   // ⚡
  '故障转移': '\u{1F504}', // 🔄
  'DIRECT': '\u{1F3E0}',  // 🏠
  'REJECT': '\u{1F6AB}',  // 🚫
}

const countryFlags: Record<string, string> = {
  '香港': '\u{1F1ED}\u{1F1F0}',   // 🇭🇰
  '日本': '\u{1F1EF}\u{1F1F5}',   // 🇯🇵
  '新加坡': '\u{1F1F8}\u{1F1EC}', // 🇸🇬
  '美国': '\u{1F1FA}\u{1F1F8}',   // 🇺🇸
  '韩国': '\u{1F1F0}\u{1F1F7}',   // 🇰🇷
  '台湾': '\u{1F1E8}\u{1F1F3}',   // 🇨🇳
  '德国': '\u{1F1E9}\u{1F1EA}',   // 🇩🇪
  '英国': '\u{1F1EC}\u{1F1E7}',   // 🇬🇧
  '马来西亚': '\u{1F1F2}\u{1F1FE}', // 🇲🇾
  '土耳其': '\u{1F1F9}\u{1F1F7}',  // 🇹🇷
  '阿根廷': '\u{1F1E6}\u{1F1F7}',  // 🇦🇷
  '澳大利亚': '\u{1F1E6}\u{1F1FA}', // 🇦🇺
  '澳洲': '\u{1F1E6}\u{1F1FA}',     // 🇦🇺
  '印度': '\u{1F1EE}\u{1F1F3}',     // 🇮🇳
  '加拿大': '\u{1F1E8}\u{1F1E6}',   // 🇨🇦
  '法国': '\u{1F1EB}\u{1F1F7}',     // 🇫🇷
  '泰国': '\u{1F1F9}\u{1F1ED}',     // 🇹🇭
  '越南': '\u{1F1FB}\u{1F1F3}',     // 🇻🇳
  '菲律宾': '\u{1F1F5}\u{1F1ED}',   // 🇵🇭
  '俄罗斯': '\u{1F1F7}\u{1F1FA}',   // 🇷🇺
  '巴西': '\u{1F1E7}\u{1F1F7}',     // 🇧🇷
  '南非': '\u{1F1FF}\u{1F1E6}',     // 🇿🇦
  '荷兰': '\u{1F1F3}\u{1F1F1}',     // 🇳🇱
  '瑞典': '\u{1F1F8}\u{1F1EA}',     // 🇸🇪
  '瑞士': '\u{1F1E8}\u{1F1ED}',     // 🇨🇭
  '阿联酋': '\u{1F1E6}\u{1F1EA}',   // 🇦🇪
  '意大利': '\u{1F1EE}\u{1F1F9}',   // 🇮🇹
  '西班牙': '\u{1F1EA}\u{1F1F8}',   // 🇪🇸
  '墨西哥': '\u{1F1F2}\u{1F1FD}',   // 🇲🇽
  '印尼': '\u{1F1EE}\u{1F1E9}',     // 🇮🇩
}

const flagPairRe = /^(?:[\u{1F1E6}-\u{1F1FF}]{2})+/u

function stripLeadingFlags(name: string): string {
  return name.replace(flagPairRe, '').trim()
}

function nodeIcon(name: string): string {
  // Match on stripped name (without any subscription-built-in flag)
  const stripped = stripLeadingFlags(name)

  for (const [key, icon] of Object.entries(specialIcons)) {
    if (stripped === key) return icon
  }
  for (const [country, flag] of Object.entries(countryFlags)) {
    if (stripped.includes(country)) return flag
  }
  return ''
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

function computeDefaultSelections(groups: ProxyGroupSummary[]): Record<string, string> {
  const selections: Record<string, string> = {}
  for (const group of groups) {
    if (group.nodes.length > 0) {
      selections[group.name] = group.nodes[0]
    }
  }
  return selections
}

function SidebarSubscriptionItem({
  subscription,
  isSelected,
  onSwitch,
  disabled,
  nodeCount,
}: {
  subscription: SavedSubscription
  isSelected: boolean
  onSwitch: () => void
  disabled: boolean
  nodeCount: number
}) {
  return (
    <button
      className={`subscription-item ${isSelected ? 'selected' : ''}`}
      type="button"
      disabled={disabled}
      onClick={onSwitch}
    >
      <span>{subscription.name}</span>
      <small>{nodeCount} 个</small>
    </button>
  )
}

function App() {
  const [enabled, setEnabled] = useState(false)
  const [tunEnabled, setTunEnabled] = useState(false)
  const [autostartEnabled, setAutostartEnabled] = useState(false)
  const [subscriptionUrl, setSubscriptionUrl] = useState('')
  const [savedSubscriptions, setSavedSubscriptions] = useState<SavedSubscription[]>([])
  const [activeSubscription, setActiveSubscription] = useState('')
  const [isAddingSubscription, setIsAddingSubscription] = useState(true)
  const [nodes, setNodes] = useState<string[]>([])
  const [selectedNodes, setSelectedNodes] = useState<Record<string, string>>({})
  const [proxyMode, setProxyMode] = useState<BackendProxyMode>('Rule')
  const [editingSubscriptionName, setEditingSubscriptionName] = useState(false)
  const [subscriptionNameDraft, setSubscriptionNameDraft] = useState('')
  const [message, setMessage] = useState('等待导入订阅')
  const [busyAction, setBusyAction] = useState<string | null>(null)
  const [rules, setRules] = useState<string[]>([])
  const [groups, setGroups] = useState<ProxyGroupSummary[]>([])
  const [nodeTypes, setNodeTypes] = useState<Record<string, string>>({})
  const [delays, setDelays] = useState<Record<string, { delay: number | null; error?: string | null }>>({})
  const [testingGroup, setTestingGroup] = useState<string | null>(null)
  const [customRules, setCustomRules] = useState<string[]>([])
  const [rulesTab, setRulesTab] = useState<'custom' | 'subscription'>('custom')
  const [rulesSearch, setRulesSearch] = useState('')
  const [showAddRuleModal, setShowAddRuleModal] = useState(false)
  const [newRuleInput, setNewRuleInput] = useState('')
  const [newRuleTarget, setNewRuleTarget] = useState<'Proxy' | 'DIRECT' | 'REJECT'>('Proxy')
  const [page, setPage] = useState<Page>('overview')
  const [overviewTab, setOverviewTab] = useState<'info' | 'nodes'>('info')
  const [proxyBypassDomains, setProxyBypassDomains] = useState<string[]>([])
  const [newBypassDomain, setNewBypassDomain] = useState('')
  const hasRestored = useRef(false)

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

  interface DnsOverrideData {
    enabled: boolean
    config: DnsOverrideConfig
  }

  const defaultDnsConfig: DnsOverrideConfig = {
    enable: true,
    listen: '0.0.0.0:53',
    'enhanced-mode': 'fake-ip',
    ipv6: false,
    nameserver: ['223.5.5.5'],
    fallback: ['8.8.8.8'],
    'default-nameserver': ['223.5.5.5'],
  }

  const [dnsOverride, setDnsOverride] = useState<DnsOverrideData | null>(null)
  const [dnsTab, setDnsTab] = useState<'form' | 'yaml'>('form')
  const [dnsForm, setDnsForm] = useState<DnsOverrideConfig>(defaultDnsConfig)
  const [dnsYaml, setDnsYaml] = useState('')
  const [dnsYamlError, setDnsYamlError] = useState('')
  const [dnsDirty, setDnsDirty] = useState(false)
  const [dnsSaveFeedback, setDnsSaveFeedback] = useState<{ kind: 'ok' | 'err'; text: string } | null>(null)

  const statusText = enabled ? '已连接' : '未连接'
  const currentSubscription = savedSubscriptions.find((item) => item.url === activeSubscription)
  const proxyNodes = useMemo(() => getProxyNodes(nodes), [nodes])
  const subscriptionInfoNodes = useMemo(
    () => nodes.filter((node) => isSubscriptionInfoNode(node)),
    [nodes],
  )

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

  const newRuleType = useMemo(() => detectRuleType(newRuleInput), [newRuleInput])
  const newRuleValid = newRuleType !== null && newRuleInput.trim().length > 0
  const newRuleDuplicate = customRules.some(
    r => r === `${newRuleType},${newRuleInput.trim()},${newRuleTarget}`
  )

  // Get a representative "current node" for the sidebar status — first select group's choice
  const statusNode = useMemo(() => {
    if (groups.length === 0) return proxyNodes[0] ?? '未选择'
    const firstGroup = groups[0]
    return selectedNodes[firstGroup.name] ?? firstGroup.nodes[0] ?? '未选择'
  }, [groups, selectedNodes, proxyNodes])

  // Load persisted data from backend on mount
  useEffect(() => {
    let mounted = true

    async function loadData() {
      try {
        const [subscriptions, savedNodes, savedRules, savedBypass] = await Promise.all([
          invoke<SavedSubscription[]>('load_subscriptions'),
          invoke<Record<string, string>>('load_selected_nodes'),
          invoke<string[]>('load_custom_rules'),
          invoke<string[]>('load_proxy_bypass'),
        ])

        if (!mounted) return

        setProxyBypassDomains(savedBypass)

        const validated = subscriptions.filter(
          (item): item is SavedSubscription =>
            typeof item?.url === 'string' &&
            typeof item?.content === 'string' &&
            typeof item?.format === 'string' &&
            Array.isArray(item?.nodes),
        ).map((item) => ({
          ...item,
          rules: Array.isArray(item.rules) ? item.rules : [],
          groups: Array.isArray(item.groups) ? item.groups : [],
          name: typeof item.name === 'string' && item.name.trim() ? item.name : getSubscriptionName(item.url),
        }))

        setSavedSubscriptions(validated)
        const activeUrl = validated[0]?.url ?? ''
        setActiveSubscription(activeUrl)

        const active = validated.find((s) => s.url === activeUrl)
        const activeGroups = active?.groups ?? []
        const defaults = computeDefaultSelections(activeGroups)
        setSelectedNodes({ ...defaults, ...savedNodes })
        setNodes(active?.nodes ?? [])
        setRules(active?.rules ?? [])
        setGroups(activeGroups)
        setNodeTypes(active?.node_types ?? {})
        setCustomRules(Array.isArray(savedRules) ? savedRules.filter((r): r is string => typeof r === 'string') : [])
        setIsAddingSubscription(validated.length === 0)
      } catch (error) {
        if (!mounted) return
        if (isTauriRuntimeMissing(error)) {
          const defaults = computeDefaultSelections(browserPreviewSubscription.groups)
          setSavedSubscriptions([browserPreviewSubscription])
          setActiveSubscription(browserPreviewSubscription.url)
          setIsAddingSubscription(false)
          setNodes(browserPreviewSubscription.nodes)
          setNodeTypes(browserPreviewSubscription.node_types ?? {})
          setSelectedNodes(defaults)
          setRules(browserPreviewRules)
          setGroups(browserPreviewSubscription.groups)
        }
      }
    }

    loadData()

    return () => {
      mounted = false
    }
  }, [])

  useEffect(() => {
    let mounted = true

    invoke<AppStatus>('core_status')
      .then((status) => {
        if (!mounted) return
        setEnabled(status.system_proxy !== '')
        setProxyMode(status.mode)
        setTunEnabled(status.tun_enabled)
      })
      .catch((error) => {
        if (!mounted) return
        setMessage(displayError(error))
      })

    invoke<boolean>('get_autostart')
      .then((v) => { if (mounted) setAutostartEnabled(v) })
      .catch(() => {})

    return () => {
      mounted = false
    }
  }, [])

  // Listen for system proxy changes from tray menu
  useEffect(() => {
    const unlisten = listen<boolean>('system-proxy-changed', (event) => {
      setEnabled(event.payload)
    })
    return () => {
      unlisten.then((fn) => fn())
    }
  }, [])

  // Listen for proxy mode changes from tray menu
  useEffect(() => {
    const unlisten = listen<BackendProxyMode>('proxy-mode-changed', (event) => {
      setProxyMode(event.payload)
    })
    return () => {
      unlisten.then((fn) => fn())
    }
  }, [])

  // Restore saved node selections to Mihomo core on startup
  useEffect(() => {
    if (!enabled || hasRestored.current) return
    if (Object.keys(selectedNodes).length === 0 || groups.length === 0) return

    hasRestored.current = true
    const entries = Object.entries(selectedNodes)
    entries.forEach(([group, node]) => {
      invoke('select_proxy_node', { group, node }).catch(() => {})
    })
  }, [enabled, selectedNodes, groups])

  async function toggleProxy() {
    const nextEnabled = !enabled
    setEnabled(nextEnabled)

    try {
      const status = await invoke<AppStatus>('set_system_proxy', { enable: nextEnabled })
      setEnabled(status.system_proxy !== '')
      setProxyMode(status.mode)
    } catch (error) {
      setEnabled(!nextEnabled)
      setMessage(displayError(error))
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
      const defaults = computeDefaultSelections(summary.groups)
      const nextSelections = { ...defaults, ...selectedNodes }

      const nextSubscription = {
        name: getSubscriptionName(nextUrl),
        url: nextUrl,
        content: summary.content,
        nodes: summary.nodes,
        format: summary.format,
        rules: summary.rules,
        groups: summary.groups,
        node_types: summary.node_types,
      }
      setNodes(summary.nodes)
      setNodeTypes(summary.node_types)
      setSelectedNodes(nextSelections)
      hasRestored.current = false
      setRules(summary.rules)
      setGroups(summary.groups)
      setActiveSubscription(nextUrl)
      setSavedSubscriptions((current) => {
        const nextSubscriptions = [
          nextSubscription,
          ...current.filter((item) => item.url !== nextUrl),
        ]
        invoke('save_subscriptions', { subscriptions: nextSubscriptions })
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
      const defaults = computeDefaultSelections(summary.groups)
      const nextSelections = { ...defaults, ...selectedNodes }

      setNodes(summary.nodes)
      setNodeTypes(summary.node_types)
      setSelectedNodes(nextSelections)
      hasRestored.current = false
      setRules(summary.rules)
      setGroups(summary.groups)
      setSavedSubscriptions((current) => {
        const nextSubscriptions = current.map((item) =>
          item.url === saved.url
            ? { ...item, nodes: summary.nodes, format: summary.format, rules: summary.rules, groups: summary.groups, node_types: summary.node_types }
            : item,
        )
        invoke('save_subscriptions', { subscriptions: nextSubscriptions })
        return nextSubscriptions
      })
      setActiveSubscription(saved.url)
      setMessage(`已切换订阅，共 ${summary.nodes.length} 个节点`)
    } catch (error) {
      setMessage(displayError(error))
    } finally {
      setBusyAction(null)
    }
  }

  async function testGroupSpeeds(groupName: string, groupNodes: string[]) {
    setTestingGroup(groupName)
    setMessage(`正在测速 ${groupName} (${groupNodes.length} 个节点)...`)
    try {
      const result = await invoke<Record<string, { delay: number | null; error?: string | null }>>('test_delays', { nodes: groupNodes })
      setDelays((prev) => ({ ...prev, ...result }))
      const successCount = Object.values(result).filter((d) => d.delay !== null).length
      const errors = Object.entries(result).filter(([, d]) => d.error)
      if (errors.length > 0) {
        const sample = errors.slice(0, 3).map(([name, d]) => `${name}: ${d.error}`).join('; ')
        setMessage(`${groupName} 测速完成 (${successCount}/${Object.keys(result).length}) 错误示例: ${sample}`)
      } else {
        setMessage(`${groupName} 测速完成 (${successCount}/${Object.keys(result).length})`)
      }
    } catch (error) {
      setMessage(displayError(error))
    } finally {
      setTestingGroup(null)
    }
  }

  async function switchNode(group: string, node: string) {
    if (!enabled) {
      setSelectedNodes((prev) => {
        const next = { ...prev, [group]: node }
        invoke('save_selected_nodes', { nodes: next })
        return next
      })
      setMessage(`${group} → ${node}`)
      return
    }

    const previousNode = selectedNodes[group]
    setSelectedNodes((prev) => {
      const next = { ...prev, [group]: node }
      invoke('save_selected_nodes', { nodes: next })
      return next
    })
    setBusyAction('node')
    setMessage(`正在切换 ${group}`)

    try {
      await invoke('select_proxy_node', { group, node })
      setMessage(`${group} → ${node}`)
    } catch (error) {
      setSelectedNodes((prev) => ({ ...prev, [group]: previousNode ?? node }))
      setMessage(displayError(error))
    } finally {
      setBusyAction(null)
    }
  }

  async function switchProxyMode(mode: BackendProxyMode) {
    setProxyMode(mode)
    setMessage(`已切换到 ${proxyModeOptions.find((item) => item.value === mode)?.label ?? mode}`)

    if (!enabled) return

    const previousMode = proxyMode
    setBusyAction('mode')

    try {
      const status = await invoke<AppStatus>('set_proxy_mode', { mode })
      setEnabled(status.system_proxy !== '')
      setProxyMode(status.mode)
    } catch (error) {
      setProxyMode(previousMode)
      setMessage(displayError(error))
    } finally {
      setBusyAction(null)
    }
  }

  function deleteSubscription(url: string) {
    setSavedSubscriptions((current) => {
      const nextSubscriptions = current.filter((item) => item.url !== url)
      invoke('save_subscriptions', { subscriptions: nextSubscriptions })
      if (url === activeSubscription) {
        const nextActive = nextSubscriptions[0]
        if (nextActive) {
          setActiveSubscription(nextActive.url)
        } else {
          setActiveSubscription('')
          setNodes([])
          setNodeTypes({})
          setSelectedNodes({})
          setGroups([])
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
      invoke('save_subscriptions', { subscriptions: nextSubscriptions })
      return nextSubscriptions
    })
    setEditingSubscriptionName(false)
    setMessage('订阅名称已更新')
  }

  async function toggleTunMode() {
    const nextEnabled = !tunEnabled
    setTunEnabled(nextEnabled)

    try {
      const status = await invoke<AppStatus>('set_tun_mode', { enabled: nextEnabled })
      setTunEnabled(status.tun_enabled)
      setEnabled(status.system_proxy !== '')
      setProxyMode(status.mode)
    } catch (error) {
      setTunEnabled(!nextEnabled)
      setMessage(displayError(error))
    }
  }

  function formToYaml(config: DnsOverrideConfig): string {
    const lines: string[] = ['dns:']
    if (config.enable !== undefined) lines.push(`  enable: ${config.enable}`)
    if (config.listen) lines.push(`  listen: ${config.listen}`)
    if (config['enhanced-mode']) lines.push(`  enhanced-mode: ${config['enhanced-mode']}`)
    if (config.ipv6 !== undefined) lines.push(`  ipv6: ${config.ipv6}`)
    if (config['default-nameserver'] && config['default-nameserver'].length > 0) {
      lines.push('  default-nameserver:')
      for (const ns of config['default-nameserver']) lines.push(`    - ${ns}`)
    }
    if (config.nameserver && config.nameserver.length > 0) {
      lines.push('  nameserver:')
      for (const ns of config.nameserver) lines.push(`    - ${ns}`)
    }
    if (config.fallback && config.fallback.length > 0) {
      lines.push('  fallback:')
      for (const ns of config.fallback) lines.push(`    - ${ns}`)
    }
    if (config['nameserver-policy'] && Object.keys(config['nameserver-policy']).length > 0) {
      lines.push('  nameserver-policy:')
      for (const [domain, server] of Object.entries(config['nameserver-policy'])) {
        lines.push(`    '${domain}': '${server}'`)
      }
    }
    if (config.hosts && Object.keys(config.hosts).length > 0) {
      lines.push('  hosts:')
      for (const [domain, ip] of Object.entries(config.hosts)) {
        lines.push(`    '${domain}': ${ip}`)
      }
    }
    if (config['fallback-filter']) {
      lines.push('  fallback-filter:')
      if (config['fallback-filter'].geoip !== undefined) {
        lines.push(`    geoip: ${config['fallback-filter'].geoip}`)
      }
      if (config['fallback-filter']['geoip-code']) {
        lines.push(`    geoip-code: ${config['fallback-filter']['geoip-code']}`)
      }
      if (config['fallback-filter'].domain && config['fallback-filter'].domain.length > 0) {
        lines.push('    domain:')
        for (const d of config['fallback-filter'].domain) lines.push(`      - '${d}'`)
      }
    }
    return lines.join('\n')
  }

  function parseDnsYaml(yaml: string): DnsOverrideConfig | null {
    const lines = yaml.trim().split('\n').filter(l => l.trim() !== 'dns:')
    const result: DnsOverrideConfig = {}
    const listFields = ['nameserver', 'fallback', 'default-nameserver']
    const policyMap: Record<string, string> = {}
    const hostsMap: Record<string, string> = {}
    const ffDomain: string[] = []

    let currentList: string | null = null
    let currentMap: 'policy' | 'hosts' | 'ff-domain' | null = null

    for (const line of lines) {
      const trimmed = line.trim()
      if (!trimmed) continue

      const kvMatch = trimmed.match(/^(\S[^:]*):\s*(.+)$/)
      const listItemMatch = trimmed.match(/^\s*-\s+(.+)$/)

      if (kvMatch && !trimmed.startsWith('-') && !trimmed.startsWith("'")) {
        const key = kvMatch[1].trim()
        const value = kvMatch[2].trim()

        if (key === 'enable' || key === 'ipv6') {
          ;(result as Record<string, unknown>)[key] = value === 'true'
          currentList = null
          currentMap = null
        } else if (listFields.includes(key)) {
          ;(result as Record<string, unknown>)[key] = []
          currentList = key
          currentMap = null
        } else if (key === 'listen' || key === 'enhanced-mode' || key === 'geoip-code') {
          ;(result as Record<string, unknown>)[key] = value
          currentList = null
          currentMap = null
        } else if (key === 'nameserver-policy') {
          currentMap = 'policy'
          currentList = null
        } else if (key === 'hosts') {
          currentMap = 'hosts'
          currentList = null
        } else if (key === 'geoip') {
          if (!result['fallback-filter']) result['fallback-filter'] = {}
          ;(result as Record<string, unknown>)['fallback-filter'] = {
            ...result['fallback-filter'],
            geoip: value === 'true',
          }
          currentList = null
          currentMap = null
        } else if (key === 'domain') {
          currentMap = 'ff-domain'
          currentList = null
          if (!result['fallback-filter']) result['fallback-filter'] = {}
          result['fallback-filter'].domain = []
        }
      } else if (listItemMatch && currentList) {
        const arr = (result as Record<string, unknown>)[currentList] as string[]
        if (arr) arr.push(listItemMatch[1].trim())
      } else if (listItemMatch && currentMap === 'ff-domain') {
        ffDomain.push(listItemMatch[1].trim().replace(/^'|'$/g, ''))
      }
    }

    if (Object.keys(policyMap).length > 0) result['nameserver-policy'] = policyMap
    if (Object.keys(hostsMap).length > 0) result.hosts = hostsMap
    if (ffDomain.length > 0 && result['fallback-filter']) {
      result['fallback-filter'].domain = ffDomain
    }

    return result
  }

  async function loadDnsOverride() {
    try {
      const data = await invoke<DnsOverrideData | null>('get_dns_override')
      if (data) {
        setDnsOverride(data)
        setDnsForm(data.config)
        setDnsYaml(formToYaml(data.config))
      } else {
        setDnsOverride({ enabled: true, config: defaultDnsConfig })
        setDnsForm(defaultDnsConfig)
        setDnsYaml(formToYaml(defaultDnsConfig))
      }
      setDnsDirty(false)
    } catch (error) {
      setMessage(displayError(error))
    }
  }

  async function saveDnsOverride() {
    if (!dnsOverride) return

    let config: DnsOverrideConfig

    if (dnsTab === 'yaml') {
      const parsed = parseDnsYaml(dnsYaml)
      if (!parsed) {
        setDnsYamlError('YAML 格式错误，请检查后重试')
        return
      }
      config = parsed
    } else {
      config = dnsForm
    }

    const data: DnsOverrideData = { enabled: dnsOverride.enabled, config }
    setDnsSaveFeedback(null)
    try {
      const status = await invoke<AppStatus>('set_dns_override', { dnsOverride: data })
      setDnsOverride(data)
      setDnsForm(config)
      setDnsYaml(formToYaml(config))
      setDnsYamlError('')
      setDnsDirty(false)
      setEnabled(status.system_proxy !== '')
      setProxyMode(status.mode)
      setTunEnabled(status.tun_enabled)
      setMessage('DNS 覆写配置已保存')
      setDnsSaveFeedback({ kind: 'ok', text: 'DNS 覆写配置已保存' })
    } catch (error) {
      const text = displayError(error)
      setMessage(text)
      setDnsSaveFeedback({ kind: 'err', text })
    }
  }

  useEffect(() => {
    if (page === 'dns') {
      loadDnsOverride()
    }
  }, [page])

  return (
    <main className="app-shell">
      {/* === Sidebar === */}
      <aside className="sidebar">
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
        <button
          className={`nav-item ${page === 'dns' ? 'selected' : ''}`}
          type="button"
          onClick={() => setPage('dns')}
        >
          <span className="nav-icon">
            <Globe size={16} />
          </span>
          DNS
        </button>
        <button
          className={`nav-item ${page === 'settings' ? 'selected' : ''}`}
          type="button"
          onClick={() => setPage('settings')}
        >
          <span className="nav-icon">
            <Settings size={16} />
          </span>
          设置
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
            isSelected={activeSubscription === subscription.url}
            onSwitch={() => switchSavedSubscription(subscription.url)}
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
            {nodeIcon(statusNode) ? `${nodeIcon(statusNode)} ${stripLeadingFlags(statusNode)}` : statusNode} · {proxyModeOptions.find((m) => m.value === proxyMode)?.label ?? proxyMode}
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


            <div className="status-bar">
              <strong>127.0.0.1:7897</strong>
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
                <div className="info-card-header">
                  <span className="card-label">订阅信息</span>
                  {currentSubscription && (
                    <div className="info-card-header-actions">
                      <button
                        type="button"
                        title="重命名"
                        onClick={startEditingSubscriptionName}
                      >
                        <Pencil size={13} />
                      </button>
                      <button
                        className="danger"
                        type="button"
                        title="删除"
                        onClick={() => deleteSubscription(activeSubscription)}
                      >
                        <Trash2 size={13} />
                      </button>
                    </div>
                  )}
                </div>
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
                        <span className="info-cell-value accent">{nodeIcon(statusNode) ? `${nodeIcon(statusNode)} ${stripLeadingFlags(statusNode)}` : statusNode}</span>
                      </div>
                      <div className="info-cell">
                        <span className="info-cell-label">节点数量</span>
                        <span className="info-cell-value">{proxyNodes.length} 个</span>
                      </div>
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
                <span className="card-label">线路</span>
                {proxyNodes.length > 0 ? (
                  <div className="node-preview-list">
                    {proxyNodes.map((node, index) => {
                      const icon = nodeIcon(node)
                      return (
                        <button
                          key={node}
                          className={`node-preview-item ${node === statusNode ? 'selected' : ''}`}
                          type="button"
                          onClick={() => {
                            const targetGroup = groups.find(g => g.nodes.includes(node))
                            if (targetGroup) switchNode(targetGroup.name, node)
                          }}
                          disabled={busyAction === 'node'}
                        >
                          <div className="node-left">
                            <span className="node-name">{icon ? `${icon} ${stripLeadingFlags(node)}` : node}</span>
                            {nodeTypes[node] && <span className="node-protocol">{nodeTypes[node]}</span>}
                          </div>
                          {delays[node] !== undefined ? (
                            <small className="node-delay">{delays[node].delay !== null ? `${delays[node].delay} ms` : '超时'}</small>
                          ) : (
                            <small className="node-delay">{index === 0 ? '推荐' : `${42 + index * 18} ms`}</small>
                          )}
                        </button>
                      )
                    })}
                  </div>
                ) : (
                  <p className="node-preview-more">暂无可用线路</p>
                )}
              </article>
            </div>

            <div className="overview-carousel">
              <div className="carousel-tabs">
                <button
                  className={`carousel-tab ${overviewTab === 'info' ? 'active' : ''}`}
                  type="button"
                  onClick={() => setOverviewTab('info')}
                >
                  订阅信息
                </button>
                <button
                  className={`carousel-tab ${overviewTab === 'nodes' ? 'active' : ''}`}
                  type="button"
                  onClick={() => setOverviewTab('nodes')}
                >
                  线路
                </button>
              </div>
              <div className="carousel-track">
                <div className={`carousel-panel ${overviewTab === 'info' ? 'active' : ''}`}>
                  {currentSubscription ? (
                    <>
                      <div className="info-card-header">
                        <span className="card-label">订阅信息</span>
                        <div className="info-card-header-actions">
                          <button type="button" title="重命名" onClick={startEditingSubscriptionName}>
                            <Pencil size={13} />
                          </button>
                          <button className="danger" type="button" title="删除" onClick={() => deleteSubscription(activeSubscription)}>
                            <Trash2 size={13} />
                          </button>
                        </div>
                      </div>
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
                          <span className="info-cell-value accent">{nodeIcon(statusNode) ? `${nodeIcon(statusNode)} ${stripLeadingFlags(statusNode)}` : statusNode}</span>
                        </div>
                        <div className="info-cell">
                          <span className="info-cell-label">节点数量</span>
                          <span className="info-cell-value">{proxyNodes.length} 个</span>
                        </div>
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
                </div>
                <div className={`carousel-panel ${overviewTab === 'nodes' ? 'active' : ''}`}>
                  <span className="card-label">线路</span>
                  {proxyNodes.length > 0 ? (
                    <div className="carousel-node-list">
                      {proxyNodes.map((node, index) => {
                        const icon = nodeIcon(node)
                        return (
                          <button
                            key={node}
                            className={`node-preview-item ${node === statusNode ? 'selected' : ''}`}
                            type="button"
                            onClick={() => {
                              const targetGroup = groups.find(g => g.nodes.includes(node))
                              if (targetGroup) switchNode(targetGroup.name, node)
                            }}
                            disabled={busyAction === 'node'}
                          >
                            <div className="node-left">
                              <span className="node-name">{icon ? `${icon} ${stripLeadingFlags(node)}` : node}</span>
                              {nodeTypes[node] && <span className="node-protocol">{nodeTypes[node]}</span>}
                            </div>
                            {delays[node] !== undefined ? (
                          <small className="node-delay">{delays[node].delay !== null ? `${delays[node].delay} ms` : '超时'}</small>
                        ) : (
                          <small className="node-delay">{index === 0 ? '推荐' : `${42 + index * 18} ms`}</small>
                        )}
                          </button>
                        )
                      })}
                    </div>
                  ) : (
                    <p className="node-preview-more">暂无可用线路</p>
                  )}
                </div>
              </div>
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

            <div className="group-page-list">
              {groups.length > 0 ? (
                groups.map((group) => (
                  <details key={group.name} className="group-section" open>
                    <summary className="group-summary">
                      <span className="group-summary-name">{group.name}</span>
                      <span className="group-summary-right">
                        <span className="group-summary-meta">
                          {selectedNodes[group.name] ?? group.nodes[0] ?? '-'}
                        </span>
                        <span className="group-summary-meta">{group.type} · {group.nodes.length} 条</span>
                        <button
                          className="group-speed-test-btn"
                          type="button"
                          disabled={testingGroup === group.name}
                          onClick={(e) => { e.stopPropagation(); e.preventDefault(); testGroupSpeeds(group.name, group.nodes) }}
                          title="测速"
                        >
                          {testingGroup === group.name ? '...' : '⚡'}
                        </button>
                        <ChevronDown size={14} className="group-chevron" />
                      </span>
                    </summary>
                    <div className="group-node-list">
                      {group.nodes.map((node) => {
                        const icon = nodeIcon(node)
                        const delay = delays[node]
                        return (
                          <button
                            key={node}
                            className={`node-preview-item ${node === (selectedNodes[group.name] ?? '') ? 'selected' : ''}`}
                            type="button"
                            onClick={() => switchNode(group.name, node)}
                            disabled={busyAction === 'node'}
                          >
                            <div className="node-left">
                              <span className="node-name">{icon ? `${icon} ${stripLeadingFlags(node)}` : node}</span>
                              {nodeTypes[node] && <span className="node-protocol">{nodeTypes[node]}</span>}
                            </div>
                            {delay !== undefined && (
                              <small className="node-delay">{delay.delay !== null ? `${delay.delay} ms` : '超时'}</small>
                            )}
                          </button>
                        )
                      })}
                    </div>
                  </details>
                ))
              ) : (
                <div className="node-page-list">
                  {proxyNodes.map((node, index) => {
                    const icon = nodeIcon(node)
                    return (
                      <button
                        key={node}
                        className={`node-preview-item ${node === statusNode ? 'selected' : ''}`}
                        type="button"
                        onClick={() => switchNode(groups[0]?.name ?? '', node)}
                        disabled={busyAction === 'node'}
                      >
                        <div className="node-left">
                          <span className="node-name">{icon ? `${icon} ${stripLeadingFlags(node)}` : node}</span>
                          {nodeTypes[node] && <span className="node-protocol">{nodeTypes[node]}</span>}
                        </div>
                        {delays[node] !== undefined ? (
                          <small className="node-delay">{delays[node].delay !== null ? `${delays[node].delay} ms` : '超时'}</small>
                        ) : (
                          <small className="node-delay">{index === 0 ? '推荐' : `${42 + index * 18} ms`}</small>
                        )}
                      </button>
                    )
                  })}
            </div>
      )}
      </div>
      </>
        )}

        {page === 'rules' && (
          <>


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
                          setCustomRules(prev => {
                            const next = prev.filter(r => r !== rule)
                            invoke('save_custom_rules', { rules: next })
                            return next
                          })
                        }}
                      >
                        ×
                      </button>
                    </div>
                  ))
                ) : (
                  <span className="rules-page-placeholder">
                    {customRules.length === 0 && !rulesSearch.trim()
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

        {page === 'dns' && (
          <>


            <div className="dns-toggle-bar">
              <span>启用 DNS 覆写</span>
              <button
                className={dnsOverride?.enabled ? 'dns-toggle active' : 'dns-toggle'}
                type="button"
                role="switch"
                aria-checked={dnsOverride?.enabled ?? false}
                onClick={() => {
                  if (!dnsOverride) return
                  const next = { ...dnsOverride, enabled: !dnsOverride.enabled }
                  setDnsOverride(next)
                  setDnsDirty(true)
                }}
              >
                <span className="toggle-track" aria-hidden="true" />
              </button>
            </div>

            <div className="dns-tabs">
              <button
                className={`dns-tab ${dnsTab === 'form' ? 'active' : ''}`}
                type="button"
                onClick={() => {
                  if (dnsTab === 'yaml') {
                    setDnsYaml(formToYaml(dnsForm))
                    setDnsYamlError('')
                  }
                  setDnsTab('form')
                }}
              >
                表单模式
              </button>
              <button
                className={`dns-tab ${dnsTab === 'yaml' ? 'active' : ''}`}
                type="button"
                onClick={() => {
                  setDnsYaml(formToYaml(dnsForm))
                  setDnsYamlError('')
                  setDnsTab('yaml')
                }}
              >
                YAML 高级编辑
              </button>
            </div>

            {dnsTab === 'form' && (
              <div className="dns-form">
                {/* --- 基础设置 --- */}
                <fieldset className="dns-fieldset">
                  <legend className="dns-legend">基础设置</legend>
                  <p className="dns-field-hint">
                    <strong>DNS 功能</strong>：开关 mihomo 内置 DNS；关闭则下面所有项都不生效。<br />
                    <strong>监听地址</strong>：mihomo 对外提供 DNS 服务的地址。TUN 模式下系统流量会被劫持到这里；普通模式下保持默认即可，无需手动改系统 DNS。<br />
                    <strong>解析模式</strong>：<code>fake-ip</code> 给域名分配虚拟 IP（速度快、TUN 必备），<code>redir-host</code> 真实解析后透传（兼容性好、IP 类规则才会命中）。<br />
                    <strong>IPv6</strong>：是否解析 AAAA 记录。线路无 IPv6 时建议关闭，避免连接超时再回退。
                  </p>

                  <label className="dns-field">
                    <span>DNS 功能</span>
                    <button
                      className={dnsForm.enable ? 'dns-mini-toggle active' : 'dns-mini-toggle'}
                      type="button"
                      role="switch"
                      aria-checked={dnsForm.enable ?? true}
                      onClick={() => {
                        setDnsForm(f => ({ ...f, enable: !f.enable }))
                        setDnsDirty(true)
                      }}
                    >
                      <span className="toggle-track" aria-hidden="true" />
                    </button>
                  </label>

                  <label className="dns-field">
                    <span>监听地址</span>
                    <input
                      className="dns-input"
                      value={dnsForm.listen ?? ''}
                      onChange={(e) => { setDnsForm(f => ({ ...f, listen: e.target.value })); setDnsDirty(true) }}
                      placeholder="0.0.0.0:53"
                    />
                  </label>

                  <label className="dns-field">
                    <span>解析模式</span>
                    <select
                      className="dns-select"
                      value={dnsForm['enhanced-mode'] ?? 'fake-ip'}
                      onChange={(e) => { setDnsForm(f => ({ ...f, 'enhanced-mode': e.target.value })); setDnsDirty(true) }}
                    >
                      <option value="fake-ip">fake-ip</option>
                      <option value="redir-host">redir-host</option>
                    </select>
                  </label>

                  <label className="dns-field">
                    <span>IPv6</span>
                    <button
                      className={dnsForm.ipv6 ? 'dns-mini-toggle active' : 'dns-mini-toggle'}
                      type="button"
                      role="switch"
                      aria-checked={dnsForm.ipv6 ?? false}
                      onClick={() => {
                        setDnsForm(f => ({ ...f, ipv6: !f.ipv6 }))
                        setDnsDirty(true)
                      }}
                    >
                      <span className="toggle-track" aria-hidden="true" />
                    </button>
                  </label>
                </fieldset>

                {/* --- DNS 服务器 --- */}
                <fieldset className="dns-fieldset">
                  <legend className="dns-legend">DNS 服务器</legend>
                  <p className="dns-field-hint">
                    <strong>引导 DNS</strong>：只用来解析下面"主 DNS / 回退 DNS"中写的域名形式服务器（如 <code>https://dns.alidns.com/dns-query</code>）。<u>必须填纯 IP</u>，例如 <code>223.5.5.5</code>。<br />
                    <strong>主 DNS</strong>：日常解析使用的服务器。默认所有域名都走这里。<br />
                    <strong>回退 DNS</strong>：用于解析国外/被污染域名。需配合下方"Fallback 过滤"决定何时启用——常用做法：主 DNS 用国内 DNS，回退 DNS 用 Cloudflare / Google。
                  </p>

                  {(['default-nameserver', 'nameserver', 'fallback'] as const).map((key) => (
                    <div key={key} className="dns-field dns-list-field">
                      <span className="dns-list-label">
                        {key === 'default-nameserver' ? '引导 DNS' : key === 'nameserver' ? '主 DNS' : '回退 DNS'}
                      </span>
                      <div className="dns-list-items">
                        {(dnsForm[key] ?? []).map((item, idx) => (
                          <div key={idx} className="dns-list-row">
                            <input
                              className="dns-input"
                              value={item}
                              onChange={(e) => {
                                setDnsForm(f => {
                                  const arr = [...(f[key] ?? [])]
                                  arr[idx] = e.target.value
                                  return { ...f, [key]: arr }
                                })
                                setDnsDirty(true)
                              }}
                              placeholder="DNS 服务器地址"
                            />
                            <button
                              className="dns-list-remove"
                              type="button"
                              onClick={() => {
                                setDnsForm(f => ({ ...f, [key]: (f[key] ?? []).filter((_, i) => i !== idx) }))
                                setDnsDirty(true)
                              }}
                            >
                              ×
                            </button>
                          </div>
                        ))}
                      </div>
                      <button
                        className="dns-list-add"
                        type="button"
                        onClick={() => {
                          setDnsForm(f => ({ ...f, [key]: [...(f[key] ?? []), ''] }))
                          setDnsDirty(true)
                        }}
                      >
                        + 添加
                      </button>
                    </div>
                  ))}
                </fieldset>

                {/* --- 域名策略 --- */}
                <fieldset className="dns-fieldset">
                  <legend className="dns-legend">域名策略 (nameserver-policy)</legend>
                  <p className="dns-field-hint">
                    为指定域名单独指派 DNS 服务器，优先级高于主 DNS / 回退 DNS。常见用途：让公司内网域名走内部 DNS。<br />
                    匹配子域名请使用 <code>+.</code> 前缀——例如 <code>+.example.com</code> 才能覆盖 <code>cf.example.com</code> 等子域；不带前缀只匹配域名本身。
                  </p>
                  {(dnsForm['nameserver-policy'] ? Object.entries(dnsForm['nameserver-policy']) : []).map(([domain, server], idx) => (
                    <div key={idx} className="dns-list-row">
                      <input
                        className="dns-input"
                        value={domain}
                        onChange={(e) => {
                          setDnsForm(f => {
                            const policy = { ...f['nameserver-policy'] }
                            const entries = Object.entries(policy)
                            const [, srv] = entries[idx]
                            delete policy[domain]
                            policy[e.target.value] = srv
                            return { ...f, 'nameserver-policy': policy }
                          })
                          setDnsDirty(true)
                        }}
                        placeholder="+.company.internal"
                      />
                      <input
                        className="dns-input"
                        value={server}
                        onChange={(e) => {
                          setDnsForm(f => {
                            const policy = { ...f['nameserver-policy'] }
                            const entries = Object.entries(policy)
                            const [dom] = entries[idx]
                            policy[dom] = e.target.value
                            return { ...f, 'nameserver-policy': policy }
                          })
                          setDnsDirty(true)
                        }}
                        placeholder="10.0.0.53"
                      />
                      <button
                        className="dns-list-remove"
                        type="button"
                        onClick={() => {
                          setDnsForm(f => {
                            const policy = { ...f['nameserver-policy'] }
                            const entries = Object.entries(policy)
                            delete policy[entries[idx][0]]
                            return { ...f, 'nameserver-policy': policy }
                          })
                          setDnsDirty(true)
                        }}
                      >
                        ×
                      </button>
                    </div>
                  ))}
                  <button
                    className="dns-list-add"
                    type="button"
                    onClick={() => {
                      setDnsForm(f => {
                        const policy = { ...f['nameserver-policy'], '': '' }
                        return { ...f, 'nameserver-policy': policy }
                      })
                      setDnsDirty(true)
                    }}
                  >
                    + 添加策略
                  </button>
                </fieldset>

                {/* --- Hosts 映射 --- */}
                <fieldset className="dns-fieldset">
                  <legend className="dns-legend">Hosts 映射</legend>
                  <p className="dns-field-hint">
                    把域名直接指向固定 IP，跳过 DNS 查询，等同于系统的 <code>/etc/hosts</code>。键同样支持 <code>+.</code> 前缀匹配子域名。
                  </p>
                  {(dnsForm.hosts ? Object.entries(dnsForm.hosts) : []).map(([domain, ip], idx) => (
                    <div key={idx} className="dns-list-row">
                      <input
                        className="dns-input"
                        value={domain}
                        onChange={(e) => {
                          setDnsForm(f => {
                            const hosts = { ...f.hosts }
                            const entries = Object.entries(hosts)
                            const [, ipAddr] = entries[idx]
                            delete hosts[domain]
                            hosts[e.target.value] = ipAddr
                            return { ...f, hosts }
                          })
                          setDnsDirty(true)
                        }}
                        placeholder="example.local"
                      />
                      <input
                        className="dns-input"
                        value={ip}
                        onChange={(e) => {
                          setDnsForm(f => {
                            const hosts = { ...f.hosts }
                            const entries = Object.entries(hosts)
                            const [dom] = entries[idx]
                            hosts[dom] = e.target.value
                            return { ...f, hosts }
                          })
                          setDnsDirty(true)
                        }}
                        placeholder="192.168.1.100"
                      />
                      <button
                        className="dns-list-remove"
                        type="button"
                        onClick={() => {
                          setDnsForm(f => {
                            const hosts = { ...f.hosts }
                            const entries = Object.entries(hosts)
                            delete hosts[entries[idx][0]]
                            return { ...f, hosts }
                          })
                          setDnsDirty(true)
                        }}
                      >
                        ×
                      </button>
                    </div>
                  ))}
                  <button
                    className="dns-list-add"
                    type="button"
                    onClick={() => {
                      setDnsForm(f => {
                        const hosts = { ...f.hosts, '': '' }
                        return { ...f, hosts }
                      })
                      setDnsDirty(true)
                    }}
                  >
                    + 添加映射
                  </button>
                </fieldset>

                {/* --- Fallback 过滤 --- */}
                <fieldset className="dns-fieldset">
                  <legend className="dns-legend">Fallback 过滤</legend>
                  <p className="dns-field-hint">
                    决定哪些解析结果<u>需要切换到回退 DNS</u>。<br />
                    <strong>GeoIP 过滤</strong>：开启后，主 DNS 解析出的 IP 不属于下方"GeoIP 代码"所指地区时，自动改用回退 DNS。常用来过滤掉国内 DNS 给国外站投的污染 IP。<br />
                    <strong>GeoIP 代码</strong>：视为"本地/可信"的国家代码，通常填 <code>CN</code>。<br />
                    <strong>域名列表</strong>：强制走回退 DNS 的域名（如 <code>+.google.com</code>），即便 GeoIP 检测通过也直接绕过主 DNS。
                  </p>

                  <label className="dns-field">
                    <span>GeoIP 过滤</span>
                    <button
                      className={dnsForm['fallback-filter']?.geoip ? 'dns-mini-toggle active' : 'dns-mini-toggle'}
                      type="button"
                      role="switch"
                      aria-checked={dnsForm['fallback-filter']?.geoip ?? false}
                      onClick={() => {
                        setDnsForm(f => ({
                          ...f,
                          'fallback-filter': { ...f['fallback-filter'], geoip: !f['fallback-filter']?.geoip },
                        }))
                        setDnsDirty(true)
                      }}
                    >
                      <span className="toggle-track" aria-hidden="true" />
                    </button>
                  </label>

                  <label className="dns-field">
                    <span>GeoIP 代码</span>
                    <input
                      className="dns-input"
                      value={dnsForm['fallback-filter']?.['geoip-code'] ?? 'CN'}
                      onChange={(e) => {
                        setDnsForm(f => ({
                          ...f,
                          'fallback-filter': { ...f['fallback-filter'], 'geoip-code': e.target.value },
                        }))
                        setDnsDirty(true)
                      }}
                      placeholder="CN"
                    />
                  </label>

                  <div className="dns-field dns-list-field">
                    <span className="dns-list-label">域名列表</span>
                    <div className="dns-list-items">
                      {(dnsForm['fallback-filter']?.domain ?? []).map((domain, idx) => (
                        <div key={idx} className="dns-list-row">
                          <input
                            className="dns-input"
                            value={domain}
                            onChange={(e) => {
                              setDnsForm(f => {
                                const arr = [...(f['fallback-filter']?.domain ?? [])]
                                arr[idx] = e.target.value
                                return { ...f, 'fallback-filter': { ...f['fallback-filter'], domain: arr } }
                              })
                              setDnsDirty(true)
                            }}
                            placeholder="+.google.com"
                          />
                          <button
                            className="dns-list-remove"
                            type="button"
                            onClick={() => {
                              setDnsForm(f => ({
                                ...f,
                                'fallback-filter': {
                                  ...f['fallback-filter'],
                                  domain: (f['fallback-filter']?.domain ?? []).filter((_, i) => i !== idx),
                                },
                              }))
                              setDnsDirty(true)
                            }}
                          >
                            ×
                          </button>
                        </div>
                      ))}
                    </div>
                    <button
                      className="dns-list-add"
                      type="button"
                      onClick={() => {
                        setDnsForm(f => ({
                          ...f,
                          'fallback-filter': {
                            ...f['fallback-filter'],
                            domain: [...(f['fallback-filter']?.domain ?? []), ''],
                          },
                        }))
                        setDnsDirty(true)
                      }}
                    >
                      + 添加
                    </button>
                  </div>
                </fieldset>
              </div>
            )}

            {dnsTab === 'yaml' && (
              <div className="dns-yaml-editor">
                <textarea
                  className="dns-yaml-textarea"
                  value={dnsYaml}
                  onChange={(e) => {
                    setDnsYaml(e.target.value)
                    setDnsDirty(true)
                  }}
                  spellCheck={false}
                  aria-label="DNS YAML 配置"
                />
                {dnsYamlError && (
                  <span className="dns-yaml-error">{dnsYamlError}</span>
                )}
              </div>
            )}

            {dnsDirty && (
              <div className="dns-actions">
                <button
                  className="dns-save-btn"
                  type="button"
                  onClick={() => saveDnsOverride()}
                >
                  保存
                </button>
                <button
                  className="dns-cancel-btn"
                  type="button"
                  onClick={() => {
                    loadDnsOverride()
                  }}
                >
                  取消
                </button>
              </div>
            )}
            {dnsSaveFeedback && (
              <div
                className={dnsSaveFeedback.kind === 'ok' ? 'dns-save-ok' : 'dns-save-err'}
                role="status"
              >
                {dnsSaveFeedback.text}
              </div>
            )}
          </>
        )}

        {page === 'settings' && (
          <div className="settings-page">
            <fieldset className="dns-fieldset">
              <legend className="dns-legend">启动</legend>
              <p className="dns-field-hint">
                <strong>开机自启</strong>：系统启动后自动以隐藏窗口的方式拉起 EasyProxy，并继续保持上次的代理 / TUN 状态。无需常驻系统也能拦截流量时开启。
              </p>
              <label className="dns-field">
                <span>开机自启</span>
                <button
                  className={autostartEnabled ? 'dns-mini-toggle active' : 'dns-mini-toggle'}
                  type="button"
                  role="switch"
                  aria-checked={autostartEnabled}
                  onClick={async () => {
                    const next = !autostartEnabled
                    setAutostartEnabled(next)
                    try {
                      await invoke('set_autostart', { enabled: next })
                    } catch (error) {
                      setAutostartEnabled(!next)
                      setMessage(displayError(error))
                    }
                  }}
                >
                  <span className="toggle-track" aria-hidden="true" />
                </button>
              </label>
            </fieldset>

            <fieldset className="dns-fieldset">
              <legend className="dns-legend">代理绕过</legend>
              <p className="dns-field-hint">
                <strong>绕过域名</strong>：匹配的域名流量不经过系统代理，直接使用系统网络栈。
                适用于需要系统客户端证书（mTLS）的网站。支持通配符 <code>*</code>（如 <code>*.example.com</code>）。
              </p>
              <div className="bypass-input-row">
                <input
                  className="modal-input"
                  value={newBypassDomain}
                  onChange={(e) => setNewBypassDomain(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter' && newBypassDomain.trim()) {
                      const domain = newBypassDomain.trim()
                      if (proxyBypassDomains.includes(domain)) return
                      const next = [...proxyBypassDomains, domain]
                      setProxyBypassDomains(next)
                      setNewBypassDomain('')
                      invoke('save_proxy_bypass', { domains: next }).catch((error) => {
                        setProxyBypassDomains(proxyBypassDomains)
                        setMessage(displayError(error))
                      })
                    }
                  }}
                  placeholder="例如 *.internal.com 或 example.com"
                  aria-label="绕过域名"
                />
                <button
                  type="button"
                  className="dns-save-btn"
                  disabled={!newBypassDomain.trim()}
                  onClick={() => {
                    const domain = newBypassDomain.trim()
                    if (!domain || proxyBypassDomains.includes(domain)) return
                    const next = [...proxyBypassDomains, domain]
                    setProxyBypassDomains(next)
                    setNewBypassDomain('')
                    invoke('save_proxy_bypass', { domains: next }).catch((error) => {
                      setProxyBypassDomains(proxyBypassDomains)
                      setMessage(displayError(error))
                    })
                  }}
                >
                  添加
                </button>
              </div>
              {proxyBypassDomains.length > 0 ? (
                <div className="rules-page-list">
                  {proxyBypassDomains.map((domain) => (
                    <div key={domain} className="rules-page-item rules-page-item-custom">
                      <span>{domain}</span>
                      <button
                        className="rules-delete-btn"
                        type="button"
                        title="删除"
                        onClick={() => {
                          const next = proxyBypassDomains.filter((d) => d !== domain)
                          setProxyBypassDomains(next)
                          invoke('save_proxy_bypass', { domains: next }).catch((error) => {
                            setProxyBypassDomains(proxyBypassDomains)
                            setMessage(displayError(error))
                          })
                        }}
                      >
                        ×
                      </button>
                    </div>
                  ))}
                </div>
              ) : (
                <span className="rules-page-placeholder">暂无绕过域名</span>
              )}
            </fieldset>
          </div>
        )}
      </section>

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
                  invoke('save_custom_rules', { rules: next })
                  setShowAddRuleModal(false)
                }}
              >
                添加
              </button>
            </div>
          </div>
        </div>
      )}
    </main>
  )
}

export default App
