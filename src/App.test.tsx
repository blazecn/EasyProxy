import { render, screen } from '@testing-library/react'
import App from './App'

describe('EasyProxy shell', () => {
  it('shows the simplified proxy dashboard', () => {
    render(<App />)

    expect(screen.getByRole('heading', { name: 'EasyProxy' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /启动代理/ })).toBeInTheDocument()
    expect(screen.getByLabelText('订阅地址')).toBeInTheDocument()
    expect(screen.getByText('支持 YAML / Base64 / URI')).toBeInTheDocument()
    expect(screen.getByText('规则模式')).toBeInTheDocument()
  })
})
