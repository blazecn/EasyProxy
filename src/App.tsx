import { useMemo, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import './App.css'

type ProxyMode = '规则模式' | '全局模式' | '直连模式'

const modes: ProxyMode[] = ['规则模式', '全局模式', '直连模式']

function App() {
  const [enabled, setEnabled] = useState(false)
  const [mode, setMode] = useState<ProxyMode>('规则模式')
  const [subscriptionUrl, setSubscriptionUrl] = useState('')
  const [nodes, setNodes] = useState(['自动选择', 'HK 01', 'SG 02'])
  const [selectedNode, setSelectedNode] = useState('自动选择')
  const [message, setMessage] = useState('等待导入订阅')

  const statusText = enabled ? '已连接' : '未连接'
  const traffic = useMemo(() => (enabled ? '下行 12.4 MB · 上行 820 KB' : '暂无流量'), [enabled])

  async function toggleProxy() {
    const nextEnabled = !enabled
    setEnabled(nextEnabled)
    setMessage(nextEnabled ? '正在启动 Mihomo 内核并设置系统代理' : '正在关闭系统代理')

    try {
      await invoke(nextEnabled ? 'start_core' : 'stop_core')
      setMessage(nextEnabled ? '代理已启动' : '代理已停止')
    } catch (error) {
      setEnabled(!nextEnabled)
      setMessage(String(error))
    }
  }

  async function refreshSubscription() {
    if (!subscriptionUrl.trim()) {
      setMessage('请输入订阅地址')
      return
    }

    setMessage('正在刷新订阅')
    try {
      const summary = await invoke<{ nodes: string[]; format: string }>('refresh_subscription', {
        url: subscriptionUrl.trim(),
      })
      setNodes(summary.nodes)
      setSelectedNode(summary.nodes[0] ?? '无可用节点')
      setMessage(`订阅刷新成功，识别为 ${summary.format}，共 ${summary.nodes.length} 个节点`)
    } catch (error) {
      setMessage(String(error))
    }
  }

  return (
    <main className="app-shell">
      <aside className="sidebar">
        <div>
          <p className="eyebrow">Mihomo 简洁客户端</p>
          <h1>EasyProxy</h1>
          <p className="subtitle">订阅、连接、模式和节点选择，只保留日常使用最需要的入口。</p>
        </div>

        <button className={enabled ? 'power active' : 'power'} type="button" onClick={toggleProxy}>
          {enabled ? '停止代理' : '启动代理'}
        </button>
      </aside>

      <section className="dashboard" aria-label="代理控制台">
        <div className="status-card">
          <div>
            <p className="label">当前状态</p>
            <h2>{statusText}</h2>
            <p>{message}</p>
          </div>
          <span className={enabled ? 'status-dot active' : 'status-dot'} />
        </div>

        <div className="grid">
          <section className="panel">
            <div className="panel-title">
              <h2>订阅</h2>
              <span>支持 YAML / Base64 / URI</span>
            </div>
            <label htmlFor="subscription">订阅地址</label>
            <div className="input-row">
              <input
                id="subscription"
                value={subscriptionUrl}
                onChange={(event) => setSubscriptionUrl(event.target.value)}
                placeholder="https://example.com/sub.yaml"
              />
              <button type="button" onClick={refreshSubscription}>
                刷新
              </button>
            </div>
          </section>

          <section className="panel">
            <div className="panel-title">
              <h2>模式</h2>
              <span>简化规则</span>
            </div>
            <div className="segmented" role="group" aria-label="代理模式">
              {modes.map((item) => (
                <button
                  key={item}
                  className={item === mode ? 'selected' : ''}
                  type="button"
                  onClick={async () => {
                    setMode(item)
                    try {
                      await invoke('set_proxy_mode', {
                        mode:
                          item === '全局模式' ? 'Global' : item === '直连模式' ? 'Direct' : 'Rule',
                      })
                      setMessage(`已切换到${item}`)
                    } catch (error) {
                      setMessage(String(error))
                    }
                  }}
                >
                  {item}
                </button>
              ))}
            </div>
          </section>
        </div>

        <section className="panel">
          <div className="panel-title">
            <h2>节点</h2>
            <span>{traffic}</span>
          </div>
          <div className="node-list">
            {nodes.map((node, index) => (
              <button
                key={node}
                className={node === selectedNode ? 'node selected' : 'node'}
                type="button"
                onClick={async () => {
                  setSelectedNode(node)
                  try {
                    await invoke('select_proxy_node', { node })
                    setMessage(`已切换到 ${node}`)
                  } catch (error) {
                    setMessage(String(error))
                  }
                }}
              >
                <span>{node}</span>
                <small>{index === 0 ? '推荐' : `${42 + index * 18} ms`}</small>
              </button>
            ))}
          </div>
        </section>

        <section className="panel log-panel">
          <div className="panel-title">
            <h2>基础日志</h2>
            <span>只显示关键事件</span>
          </div>
          <p>{message}</p>
          <p>系统代理端口：127.0.0.1:7890 · Controller：127.0.0.1:9090</p>
          <p>当前节点：{selectedNode} · 当前模式：{mode}</p>
        </section>
      </section>
    </main>
  )
}

export default App
