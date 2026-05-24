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
  it('shows the homepage control center entries', () => {
    render(<App />)

    expect(screen.queryByLabelText('首页状态栏')).not.toBeInTheDocument()
    expect(screen.queryByRole('heading', { name: '代理开关' })).not.toBeInTheDocument()
    expect(screen.getByRole('switch', { name: '系统代理' })).toBeInTheDocument()
    expect(screen.getByRole('switch', { name: 'TUN 模式' })).toBeInTheDocument()
    expect(screen.queryByRole('heading', { name: '订阅管理' })).not.toBeInTheDocument()
    expect(screen.queryByRole('heading', { name: '节点切换' })).not.toBeInTheDocument()
    expect(screen.queryByRole('heading', { name: '代理模式' })).not.toBeInTheDocument()
    expect(screen.getByText('订阅')).toBeInTheDocument()
    expect(screen.queryByText('模式')).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: '规则模式' })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: '全局模式' })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: '直连模式' })).not.toBeInTheDocument()
    expect(screen.getByLabelText('订阅地址')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: '导入' })).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: '刷新' })).not.toBeInTheDocument()
    expect(screen.getByText('状态：未连接')).toBeInTheDocument()
    expect(screen.getByText('节点：未选择')).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /自动选择/ })).not.toBeInTheDocument()
    expect(screen.queryByText('系统代理端口：127.0.0.1:7890')).not.toBeInTheDocument()
  })

  it('switches proxy mode through the mode card', async () => {
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

    expect(screen.getByText('模式')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: '规则模式' })).toHaveClass('selected')

    fireEvent.click(screen.getByRole('button', { name: '全局模式' }))

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith('set_proxy_mode', { mode: 'Global' })
    })
    expect(screen.getByRole('button', { name: '全局模式' })).toHaveClass('selected')
  })

  it('saves imported subscription configs and switches without refetching urls', async () => {
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

  it('shows saved subscription cards and opens the import form from the add card', () => {
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

    expect(screen.queryByLabelText('订阅地址')).not.toBeInTheDocument()
    expect(screen.queryByText('当前订阅')).not.toBeInTheDocument()
    expect(screen.getAllByText('one.example')).toHaveLength(2)
    expect(screen.getByRole('button', { name: '编辑订阅名称' })).toHaveTextContent('✎')
    expectTextContent('当前线路HK 01')
    expectTextContent('节点数量1 个')
    expect(screen.queryByText(/clash-yaml ·/)).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: /one.example/ })).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: '增加订阅' }))

    expect(screen.getByLabelText('订阅地址')).toHaveValue('')
    expect(screen.getByRole('button', { name: '导入' })).toBeInTheDocument()
  })

  it('renames the active subscription from the subscription header', () => {
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

    const editButton = screen.getByRole('button', { name: '编辑订阅名称' })
    expect(editButton).toHaveTextContent('✎')

    fireEvent.click(editButton)
    fireEvent.change(screen.getByLabelText('订阅名称'), {
      target: { value: '工作订阅' },
    })
    expect(screen.getByRole('button', { name: '保存订阅名称' })).toHaveTextContent('✓')

    fireEvent.click(screen.getByRole('button', { name: '保存订阅名称' }))

    expect(screen.getAllByText('工作订阅')).toHaveLength(2)
    expect(JSON.parse(localStorage.getItem('easyproxy.subscriptions') ?? '[]')[0]).toMatchObject({
      name: '工作订阅',
      url: 'https://one.example/sub.yaml',
    })
  })

  it('expands the node card into a full-page node panel', () => {
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

    expect(screen.queryByText('暂无流量')).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: '展开线路' })).toBeInTheDocument()
    expect(screen.queryByRole('region', { name: '线路切换' })).not.toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: '展开线路' }))

    expect(screen.getByRole('region', { name: '线路切换' })).toBeInTheDocument()
    expect(screen.queryByText('订阅')).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: '收起线路' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /HK 01/ })).toHaveClass('selected')

    fireEvent.click(screen.getByRole('button', { name: '收起线路' }))

    expect(screen.queryByRole('region', { name: '线路切换' })).not.toBeInTheDocument()
    expect(screen.getByText('订阅')).toBeInTheDocument()
  })

  it('moves subscription info nodes into the subscription card and filters them from node switching', () => {
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
    expect(screen.getByRole('button', { name: /HK 01/ })).toHaveClass('selected')
    expect(screen.queryByRole('button', { name: /剩余流量/ })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /距离下次重置剩余/ })).not.toBeInTheDocument()
    expectTextContent('节点数量1 个')
  })

  it('does not refill the import url when switching saved subscriptions', async () => {
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
      expect(localStorage.getItem('easyproxy.activeSubscription')).toBe('https://two.example/sub.yaml')
    })

    fireEvent.click(screen.getByRole('button', { name: '增加订阅' }))

    expect(screen.getByLabelText('订阅地址')).toHaveValue('')
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

  it('restores the cached node for the last selected subscription on next launch', () => {
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
    localStorage.setItem(
      'easyproxy.selectedNodes',
      JSON.stringify({ 'https://one.example/sub.yaml': 'SG 02' }),
    )

    render(<App />)

    expect(screen.getByText('节点：SG 02')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /SG 02/ })).toHaveClass('selected')
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
    expect(screen.getAllByText('dy.boost1.shop')).toHaveLength(2)
    expectTextContent('剩余流量197.92 GB')
    expectTextContent('距离下次重置剩余31 天')
    expect(screen.getByText('节点：香港 01')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /香港 01/ })).toHaveClass('selected')
    expect(screen.queryByRole('button', { name: /剩余流量/ })).not.toBeInTheDocument()
  })
})
