import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { vi } from 'vitest'
import App from './App'

const mockInvoke = vi.hoisted(() => vi.fn())

vi.mock('@tauri-apps/api/core', () => ({
  invoke: mockInvoke,
}))

function expectTextContent(text: string) {
  const matches = screen.getAllByText((_, element) => element?.textContent === text)
  expect(matches.length).toBeGreaterThan(0)
}

// Mutable store so each test can configure what load_* commands return
const store = {
  subscriptions: [] as Array<Record<string, unknown>>,
  selectedNodes: {} as Record<string, string>,
  customRules: [] as string[],
}

function savedSubscription(overrides: Record<string, unknown> = {}) {
  return {
    name: 'one.example',
    url: 'https://one.example/sub.yaml',
    content: 'proxies:\n  - name: HK 01\n',
    nodes: ['HK 01'],
    format: 'clash-yaml',
    rules: [],
    groups: [],
    node_types: {},
    ...overrides,
  }
}

beforeEach(() => {
  store.subscriptions = []
  store.selectedNodes = {}
  store.customRules = []
  mockInvoke.mockReset()
  mockInvoke.mockImplementation((command: string, args?: Record<string, unknown>) => {
    if (command === 'core_status') {
      return Promise.resolve({
        core: 'Stopped',
        mode: 'Rule',
        system_proxy: '127.0.0.1:7890',
      })
    }

    if (command === 'load_subscriptions') {
      return Promise.resolve(store.subscriptions)
    }

    if (command === 'load_selected_nodes') {
      return Promise.resolve(store.selectedNodes)
    }

    if (command === 'load_custom_rules') {
      return Promise.resolve(store.customRules)
    }

    if (command === 'save_subscriptions') {
      store.subscriptions = (args?.subscriptions as Array<Record<string, unknown>>) ?? []
      return Promise.resolve()
    }

    if (command === 'save_selected_nodes') {
      store.selectedNodes = (args?.nodes as Record<string, string>) ?? {}
      return Promise.resolve()
    }

    if (command === 'save_custom_rules') {
      store.customRules = (args?.rules as string[]) ?? []
      return Promise.resolve()
    }

    if (command === 'refresh_subscription') {
      return Promise.resolve({
        nodes: ['HK 01', 'SG 02'],
        format: 'clash-yaml',
        content: 'proxies:\n  - name: HK 01\n',
        rules: [],
        groups: [],
        node_types: {},
      })
    }

    if (command === 'save_subscription') {
      return Promise.resolve({
        nodes: ['HK 01', 'SG 02'],
        format: 'clash-yaml',
        rules: [],
        groups: [],
        node_types: {},
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
  it('shows the sidebar with navigation and controls', async () => {
    render(<App />)

    // Wait for data load to complete
    await waitFor(() => {
      expect(screen.getByLabelText('订阅地址')).toBeInTheDocument()
    })

    // Sidebar navigation
    expect(screen.getByRole('button', { name: '总览' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: '线路切换' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: '代理规则' })).toBeInTheDocument()

    // Sidebar switches at bottom
    expect(screen.getByRole('switch', { name: '系统代理' })).toBeInTheDocument()
    expect(screen.getByRole('switch', { name: 'TUN 模式' })).toBeInTheDocument()

    // Import form in sidebar (shown when no subscriptions)
    expect(screen.getByRole('button', { name: '导入' })).toBeInTheDocument()

    // Overview page
    expect(screen.getAllByText('等待导入订阅')).toHaveLength(2)
  })

  it('switches proxy mode from the overview info card', async () => {
    store.subscriptions = [savedSubscription()]

    render(<App />)

    await waitFor(() => {
      const btns = screen.getAllByRole('button', { name: '规则模式' })
      expect(btns.length).toBeGreaterThan(0)
    })

    fireEvent.click(screen.getAllByRole('button', { name: '全局模式' })[0])

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith('set_proxy_mode', { mode: 'Global' })
    })
  })

  it('saves imported subscription and switches without refetching urls', async () => {
    store.subscriptions = [
      savedSubscription(),
      savedSubscription({
        name: 'two.example',
        url: 'https://two.example/sub.yaml',
        content: 'proxies:\n  - name: SG 02\n',
        nodes: ['SG 02'],
      }),
    ]

    render(<App />)

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /two.example/ })).toBeInTheDocument()
    })

    const twoExampleBtn = screen.getByRole('button', { name: /two.example/ })
    fireEvent.mouseDown(twoExampleBtn)
    fireEvent.mouseUp(twoExampleBtn)
    fireEvent.click(twoExampleBtn)

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith('save_subscription', {
        content: 'proxies:\n  - name: SG 02\n',
      })
    })
    expect(mockInvoke).not.toHaveBeenCalledWith('refresh_subscription', {
      url: 'https://two.example/sub.yaml',
    })
  })

  it('shows saved subscription cards in sidebar and opens import form from add button', async () => {
    store.subscriptions = [savedSubscription()]

    render(<App />)

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /one.example/ })).toBeInTheDocument()
    })

    // Import form hidden
    expect(screen.queryByLabelText('订阅地址')).not.toBeInTheDocument()

    // Overview shows info
    expectTextContent('当前线路HK 01')
    expectTextContent('节点数量1 个')
  })

  it('renames the active subscription via modal', async () => {
    store.subscriptions = [savedSubscription()]

    render(<App />)

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /one.example/ })).toBeInTheDocument()
    })

    fireEvent.click(screen.getAllByRole('button', { name: /重命名/ })[0])

    fireEvent.change(screen.getByLabelText('订阅名称'), {
      target: { value: '工作订阅' },
    })

    fireEvent.click(screen.getByRole('button', { name: '保存' }))

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith('save_subscriptions', {
        subscriptions: expect.arrayContaining([
          expect.objectContaining({
            name: '工作订阅',
            url: 'https://one.example/sub.yaml',
          }),
        ]),
      })
    })
  })

  it('navigates to full node page from sidebar', async () => {
    store.subscriptions = [
      savedSubscription({ nodes: ['HK 01', 'SG 02'] }),
    ]

    render(<App />)

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /one.example/ })).toBeInTheDocument()
    })

    fireEvent.click(screen.getByRole('button', { name: '线路切换' }))

    expect(screen.getByRole('button', { name: /HK 01/ })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /SG 02/ })).toBeInTheDocument()
  })

  it('filters subscription info nodes into the overview info card', async () => {
    store.subscriptions = [
      savedSubscription({
        nodes: ['剩余流量：197.92 GB', '距离下次重置剩余：31 天', '套餐到期：2027-02-22', 'HK 01'],
      }),
    ]

    render(<App />)

    await waitFor(() => {
      expectTextContent('剩余流量197.92 GB')
    })

    expectTextContent('距离下次重置剩余31 天')
    expectTextContent('套餐到期2027-02-22')
    expectTextContent('当前线路HK 01')
    expectTextContent('节点数量1 个')
  })

  it('names imported subscriptions from the url domain and selects them', async () => {
    render(<App />)

    await waitFor(() => {
      expect(screen.getByLabelText('订阅地址')).toBeInTheDocument()
    })

    fireEvent.change(screen.getByLabelText('订阅地址'), {
      target: { value: 'https://sub.example.com/path/sub.yaml' },
    })
    fireEvent.click(screen.getByRole('button', { name: '导入' }))

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /sub.example.com/ })).toBeInTheDocument()
    })

    expect(mockInvoke).toHaveBeenCalledWith('save_subscriptions', {
      subscriptions: expect.arrayContaining([
        expect.objectContaining({
          name: 'sub.example.com',
          url: 'https://sub.example.com/path/sub.yaml',
        }),
      ]),
    })
  })

  it('restores the last selected subscription on next launch', async () => {
    // Active subscription is the first one in the array
    store.subscriptions = [
      savedSubscription({
        name: 'two.example',
        url: 'https://two.example/sub.yaml',
        content: 'proxies:\n  - name: SG 02\n',
        nodes: ['SG 02'],
      }),
      savedSubscription(),
    ]

    render(<App />)

    await waitFor(() => {
      expectTextContent('当前线路SG 02')
    })

    expect(screen.getByRole('button', { name: /two.example/ })).toHaveClass('selected')
  })

  it('caches the selected node for the active subscription', async () => {
    store.subscriptions = [
      savedSubscription({
        nodes: ['HK 01', 'SG 02'],
        groups: [{ name: 'Proxy', type: 'select', nodes: ['HK 01', 'SG 02'] }],
      }),
    ]

    render(<App />)

    await waitFor(() => {
      const btns = screen.getAllByRole('button', { name: /HK 01/ })
      expect(btns.length).toBeGreaterThan(0)
    })

    fireEvent.click(screen.getAllByRole('button', { name: /SG 02/ })[0])

    // When proxy is off, node selection is cached via backend
    await waitFor(() => {
      const saveCalls = mockInvoke.mock.calls.filter(
        (call: [string, unknown]) => call[0] === 'save_selected_nodes',
      )
      expect(saveCalls.length).toBeGreaterThan(0)
    })
  })

  it('keeps other controls usable while a subscription import is pending', async () => {
    let resolveImport: (value: { nodes: string[]; format: string; content: string; rules: string[]; groups: Array<Record<string, unknown>>; node_types: Record<string, string> }) => void = () => {}
    mockInvoke.mockImplementation((command: string) => {
      if (command === 'core_status') {
        return Promise.resolve({
          core: 'Stopped',
          mode: 'Rule',
          system_proxy: '127.0.0.1:7890',
        })
      }

      if (command === 'load_subscriptions') return Promise.resolve([])
      if (command === 'load_selected_nodes') return Promise.resolve({})
      if (command === 'load_custom_rules') return Promise.resolve([])

      if (command === 'refresh_subscription') {
        return new Promise((resolve) => {
          resolveImport = resolve
        })
      }

      return Promise.resolve()
    })

    render(<App />)

    await waitFor(() => {
      expect(screen.getByLabelText('订阅地址')).toBeInTheDocument()
    })

    fireEvent.change(screen.getByLabelText('订阅地址'), {
      target: { value: 'https://example.com/sub.yaml' },
    })
    fireEvent.click(screen.getByRole('button', { name: '导入' }))

    expect(screen.getByRole('button', { name: '导入中' })).toBeDisabled()
    expect(screen.getByRole('switch', { name: '系统代理' })).not.toBeDisabled()
    expect(screen.getByRole('switch', { name: 'TUN 模式' })).not.toBeDisabled()

    resolveImport({ nodes: ['HK 01'], format: 'clash-yaml', content: 'proxies:\n  - name: HK 01\n', rules: [], groups: [], node_types: {} })
  })

  it('uses a preview status instead of showing raw Tauri invoke errors', async () => {
    mockInvoke.mockRejectedValue(
      new TypeError("Cannot read properties of undefined (reading 'invoke')"),
    )

    render(<App />)

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /dy.boost1.shop/ })).toBeInTheDocument()
    })
    expect(screen.queryByText(/Cannot read properties/)).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: /dy\.boost1\.shop/ })).toBeInTheDocument()
    expect(screen.getAllByRole('button', { name: /香港 01/ }).length).toBeGreaterThan(0)
  })
})
