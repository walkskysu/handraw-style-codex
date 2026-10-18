import { afterEach, describe, expect, it, vi } from 'vitest'
import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils'
import * as core from '@tauri-apps/api/core'
import { clearMocks, mockIPC } from '@tauri-apps/api/mocks'
import App from '../src/App.vue'

vi.mock('@tauri-apps/api/core', { spy: true })

enableAutoUnmount(afterEach)
afterEach(() => clearMocks())

describe('application shell', () => {
  it('renders a usable browser preview and greets the entered name', async () => {
    vi.spyOn(core, 'isTauri').mockReturnValue(false)
    const wrapper = mount(App)

    expect(wrapper.get('h1').text()).toContain('从这里开始。')
    expect(wrapper.get('.mode-badge').text()).toBe('浏览器预览')
    expect(wrapper.get('button').attributes('disabled')).toBeDefined()
    await wrapper.get('input').setValue('   ')
    expect(wrapper.get('button').attributes('disabled')).toBeDefined()
    await wrapper.get('input').setValue('小画家')
    expect(wrapper.get('button').attributes('disabled')).toBeUndefined()
    await wrapper.get('form').trigger('submit')
    await flushPromises()

    expect(wrapper.get('[role="status"]').text()).toBe('你好，小画家！这是浏览器预览。')
    expect(wrapper.find('[role="alert"]').exists()).toBe(false)
  })

  it('disables input and prevents duplicate requests while waiting for Rust', async () => {
    vi.spyOn(core, 'isTauri').mockReturnValue(true)
    let resolveGreeting!: (message: string) => void
    const ipc = vi.fn(() => new Promise<string>((resolve) => { resolveGreeting = resolve }))
    mockIPC(ipc)
    const wrapper = mount(App)

    expect(wrapper.get('.mode-badge').text()).toBe('桌面应用')
    expect(wrapper.find('.preview-hint').exists()).toBe(false)
    await wrapper.get('input').setValue('小画家')
    await wrapper.get('form').trigger('submit')
    expect(wrapper.get('button').text()).toBe('问候中…')
    expect(wrapper.get('button').attributes('disabled')).toBeDefined()
    expect(wrapper.get('input').attributes('disabled')).toBeDefined()
    expect(wrapper.get('form').attributes('aria-busy')).toBe('true')
    await wrapper.get('form').trigger('submit')
    expect(ipc).toHaveBeenCalledTimes(1)

    resolveGreeting('你好，小画家！欢迎来到 Handraw。')
    await flushPromises()
    expect(wrapper.get('[role="status"]').text()).toBe('你好，小画家！欢迎来到 Handraw。')
    expect(wrapper.get('button').attributes('disabled')).toBeUndefined()
    expect(wrapper.get('form').attributes('aria-busy')).toBe('false')
  })

  it('shows a recoverable error and clears it after a successful retry', async () => {
    vi.spyOn(core, 'isTauri').mockReturnValue(true)
    const ipc = vi.fn().mockRejectedValueOnce('connection failed').mockResolvedValueOnce('重试成功')
    mockIPC(ipc)
    const wrapper = mount(App)
    await wrapper.get('input').setValue('小画家')
    await wrapper.get('form').trigger('submit')
    await flushPromises()

    expect(wrapper.get('[role="alert"]').text()).toContain('请再试一次')
    expect(wrapper.get('button').attributes('disabled')).toBeUndefined()
    await wrapper.get('form').trigger('submit')
    await flushPromises()

    expect(wrapper.find('[role="alert"]').exists()).toBe(false)
    expect(wrapper.get('[role="status"]').text()).toBe('重试成功')
    expect(ipc).toHaveBeenCalledTimes(2)
  })

  it('renders the response as text rather than injecting markup', async () => {
    vi.spyOn(core, 'isTauri').mockReturnValue(false)
    const wrapper = mount(App)
    await wrapper.get('input').setValue('<img src=x onerror=alert(1)>')
    await wrapper.get('form').trigger('submit')
    await flushPromises()

    expect(wrapper.get('[role="status"]').text()).toContain('<img src=x onerror=alert(1)>')
    expect(wrapper.find('img').exists()).toBe(false)
  })
})
