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

    // Overview page (text appears in both subtitle and info-card message)
    expect(screen.getAllByText('等待导入订阅')).toHaveLength(2)
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

    expect(screen.getByRole('heading', { name: '线路切换' })).toBeInTheDocument()
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
    expectTextContent('当前线路HK 01')
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

    expectTextContent('当前线路SG 02')
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

    // When proxy is off, node selection is cached locally without invoking backend
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
      expect(screen.getByRole('button', { name: /dy.boost1.shop/ })).toBeInTheDocument()
    })
    expect(screen.queryByText(/Cannot read properties/)).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: /dy.boost1.shop/ })).toBeInTheDocument()
    expectTextContent('剩余流量197.92 GB')
    expectTextContent('距离下次重置剩余31 天')
    expectTextContent('当前线路香港 01')
    expect(screen.getByRole('button', { name: /香港 01/ })).toBeInTheDocument()
  })
})
