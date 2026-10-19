import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from 'vitest'
import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils'
import * as core from '@tauri-apps/api/core'
import { clearMocks, mockIPC } from '@tauri-apps/api/mocks'
import App from '../src/App.vue'

vi.mock('@tauri-apps/api/core', { spy: true })
enableAutoUnmount(afterEach)
afterEach(() => clearMocks())
const image = 'data:image/png;base64,aW1hZ2U='

// jsdom does not implement the native dialog API. Model its open/close lifecycle;
// focus trapping and Escape dismissal remain browser/WebView responsibilities.
const showModal = vi.fn(function (this: HTMLDialogElement) { this.open = true })
beforeAll(() => {
  Object.defineProperties(HTMLDialogElement.prototype, {
    showModal: { configurable: true, value: showModal },
    close: { configurable: true, value: function (this: HTMLDialogElement) {
      if (!this.open) return
      this.open = false
      this.dispatchEvent(new Event('close'))
    } },
  })
})
afterAll(() => {
  Reflect.deleteProperty(HTMLDialogElement.prototype, 'showModal')
  Reflect.deleteProperty(HTMLDialogElement.prototype, 'close')
})

describe('image generation workspace', () => {
  it('submits all three selections, preserves them on failure, and allows independent clearing', async () => {
    vi.spyOn(core, 'isTauri').mockReturnValue(true)
    const ipc = vi.fn().mockRejectedValueOnce('请重试').mockResolvedValueOnce(image)
    mockIPC(ipc)
    const wrapper = mount(App)
    await wrapper.get('textarea').setValue('小猫')
    for (const [category, id] of [['style', 'FH-044'], ['layout', 'EC-024'], ['color', 'C-36']]) {
      const picker = wrapper.get(`[data-category="${category}"]`)
      await picker.get('input').setValue(id)
      await picker.get(`[data-option="${id}"]`).trigger('click')
    }
    await wrapper.get('form').trigger('submit')
    expect(wrapper.findAll('fieldset').every((field) => field.attributes('disabled') !== undefined)).toBe(true)
    await flushPromises()
    expect(ipc).toHaveBeenNthCalledWith(1, 'generate_image', {
      prompt: '小猫', selections: { style: 'FH-044', layout: 'EC-024', color: 'C-36' },
    })
    expect(wrapper.get('[data-category="style"] summary').text()).toContain('FH-044')
    await wrapper.get('[data-category="layout"] .clear-selection').trigger('click')
    expect(wrapper.get('[data-category="layout"] summary').text()).toContain('未选择')
    await wrapper.get('form').trigger('submit')
    await flushPromises()
    expect(ipc).toHaveBeenNthCalledWith(2, 'generate_image', {
      prompt: '小猫', selections: { style: 'FH-044', color: 'C-36' },
    })
    expect(wrapper.get('.image-preview img').attributes('src')).toBe(image)
  })

  it('shows an honest browser preview with generation disabled', async () => {
    vi.spyOn(core, 'isTauri').mockReturnValue(false)
    const ipc = vi.fn()
    mockIPC(ipc)
    const wrapper = mount(App)
    expect(wrapper.get('.mode-badge').text()).toBe('浏览器预览')
    expect(wrapper.get('.preview-hint').text()).toContain('桌面应用')
    await wrapper.get('textarea').setValue('小猫')
    expect(wrapper.get('button[type="submit"]').attributes('disabled')).toBeDefined()
    await wrapper.get('form').trigger('submit')
    expect(ipc).not.toHaveBeenCalled()
    expect(wrapper.find('.image-preview img').exists()).toBe(false)
  })

  it('rejects blank submissions and keeps the empty preview', async () => {
    vi.spyOn(core, 'isTauri').mockReturnValue(true)
    const ipc = vi.fn()
    mockIPC(ipc)
    const wrapper = mount(App)
    await wrapper.get('textarea').setValue(' \n ')
    expect(wrapper.get('button[type="submit"]').attributes('disabled')).toBeDefined()
    await wrapper.get('form').trigger('submit')
    await flushPromises()
    expect(wrapper.get('[role="alert"]').text()).toContain('请先描述')
    expect(ipc).not.toHaveBeenCalled()
    expect(wrapper.find('.preview-placeholder').exists()).toBe(true)
  })

  it('prevents duplicate requests and displays the returned image and prompt', async () => {
    vi.spyOn(core, 'isTauri').mockReturnValue(true)
    let resolveImage!: (result: string) => void
    const ipc = vi.fn(() => new Promise<string>((resolve) => { resolveImage = resolve }))
    mockIPC(ipc)
    const wrapper = mount(App)
    expect(wrapper.get('.mode-badge').text()).toBe('桌面应用')
    await wrapper.get('textarea').setValue('  水彩小猫  ')
    await wrapper.get('form').trigger('submit')
    expect(wrapper.get('button[type="submit"]').text()).toBe('正在生成…')
    expect(wrapper.get('textarea').attributes('disabled')).toBeDefined()
    expect(wrapper.get('form').attributes('aria-busy')).toBe('true')
    expect(wrapper.get('[role="status"]').text()).toContain('请稍候')
    await wrapper.get('form').trigger('submit')
    expect(ipc).toHaveBeenCalledTimes(1)
    resolveImage(image)
    await flushPromises()
    expect(wrapper.get('.image-preview img').attributes('src')).toBe(image)
    expect(wrapper.get('.image-preview img').attributes('alt')).toBe('水彩小猫')
    expect(wrapper.get('figcaption').text()).toBe('水彩小猫')
    expect(wrapper.get('form').attributes('aria-busy')).toBe('false')
    expect(wrapper.get('button[type="submit"]').attributes('disabled')).toBeUndefined()
    expect(showModal).not.toHaveBeenCalled()
  })

  it('preserves the previous image on failure and replaces it after retry', async () => {
    vi.spyOn(core, 'isTauri').mockReturnValue(true)
    const replacement = 'data:image/png;base64,bmV3'
    const ipc = vi.fn().mockResolvedValueOnce(image)
      .mockRejectedValueOnce('Codex 执行失败，请检查登录后重试。')
      .mockResolvedValueOnce(replacement)
    mockIPC(ipc)
    const wrapper = mount(App)
    await wrapper.get('textarea').setValue('小猫')
    await wrapper.get('form').trigger('submit')
    await flushPromises()
    await wrapper.get('textarea').setValue('小狗')
    await wrapper.get('form').trigger('submit')
    expect(wrapper.get('.image-preview img').attributes('src')).toBe(image)
    await flushPromises()
    expect(wrapper.get('[role="alert"]').text()).toContain('检查登录')
    expect(wrapper.get('dialog').element.open).toBe(true)
    expect(wrapper.get('dialog').text()).toContain('检查登录')
    expect(wrapper.find('#upgrade-hint').exists()).toBe(false)
    expect(wrapper.get('figcaption').text()).toBe('小猫')
    expect(wrapper.get('textarea').element.value).toBe('小狗')
    expect(wrapper.get('button[type="submit"]').attributes('disabled')).toBeUndefined()
    await wrapper.get('dialog button').trigger('click')
    expect(wrapper.get('dialog').element.open).toBe(false)
    await wrapper.get('form').trigger('submit')
    await flushPromises()
    expect(wrapper.find('[role="alert"]').exists()).toBe(false)
    expect(wrapper.get('dialog').element.open).toBe(false)
    expect(wrapper.get('.image-preview img').attributes('src')).toBe(replacement)
    expect(wrapper.get('figcaption').text()).toBe('小狗')
  })

  it('renders prompt and errors as text and handles image load failures', async () => {
    vi.spyOn(core, 'isTauri').mockReturnValue(true)
    const markup = '<img src=x onerror=alert(1)>'
    mockIPC(vi.fn().mockRejectedValueOnce(markup).mockResolvedValueOnce(image))
    const wrapper = mount(App)
    await wrapper.get('textarea').setValue(markup)
    await wrapper.get('form').trigger('submit')
    await flushPromises()
    expect(wrapper.get('[role="alert"]').text()).toBe(markup)
    expect(wrapper.get('#error-detail').text()).toBe(markup)
    expect(wrapper.find('.image-preview img').exists()).toBe(false)
    await wrapper.get('form').trigger('submit')
    await flushPromises()
    expect(wrapper.get('figcaption').text()).toBe(markup)
    expect(wrapper.findAll('.image-preview img')).toHaveLength(1)
    await wrapper.get('.image-preview img').trigger('error')
    await flushPromises()
    expect(wrapper.find('.image-preview img').exists()).toBe(false)
    expect(wrapper.get('[role="alert"]').text()).toContain('图片无法显示')
    expect(wrapper.get('dialog').element.open).toBe(true)
    expect(wrapper.get('#error-detail').text()).toContain('图片无法显示')
    expect(wrapper.get('button[type="submit"]').attributes('disabled')).toBeUndefined()
  })

  it('shows the requested upgrade command once for repeated CLI version errors and clears it on retry', async () => {
    vi.spyOn(core, 'isTauri').mockReturnValue(true)
    const diagnostic = 'ERROR: ' + JSON.stringify({
      type: 'error', status: 400,
      error: { type: 'invalid_request_error', message: "The 'gpt-6-astra' model requires a newer version of Codex. Please upgrade to the latest app or CLI and try again." },
    })
    const error = `Codex 执行失败。\n${diagnostic}\n${diagnostic}`
    mockIPC(vi.fn().mockRejectedValueOnce(error).mockRejectedValueOnce('网络连接失败'))
    const wrapper = mount(App, { attachTo: document.body })
    await wrapper.get('textarea').setValue('小猫')
    await wrapper.get('form').trigger('submit')
    await flushPromises()
    const dialog = wrapper.get('dialog')
    expect(dialog.element.open).toBe(true)
    expect(showModal).toHaveBeenCalledTimes(1)
    expect(dialog.attributes('role')).toBe('alertdialog')
    expect(dialog.attributes('aria-labelledby')).toBe('error-title')
    expect(dialog.attributes('aria-describedby')).toBe('upgrade-hint error-detail')
    expect(dialog.get('#error-detail').text()).toBe(error)
    expect(dialog.findAll('code')).toHaveLength(1)
    expect(dialog.get('code').text()).toBe('npm.cmd install -g @openai/codex@latest')
    await dialog.get('button').trigger('click')
    expect(dialog.element.open).toBe(false)
    expect(document.activeElement).toBe(wrapper.get('textarea').element)
    expect(wrapper.get('textarea').element.value).toBe('小猫')
    await wrapper.get('form').trigger('submit')
    await flushPromises()
    expect(dialog.element.open).toBe(true)
    expect(showModal).toHaveBeenCalledTimes(2)
    expect(dialog.find('#upgrade-hint').exists()).toBe(false)
    expect(dialog.attributes('aria-describedby')).toBe('error-detail')
    expect(dialog.get('#error-detail').text()).toBe('网络连接失败')
  })

  it.each([new Error('Codex 无法启动'), null, '   '])('opens a readable error dialog for non-string or empty failures (%#)', async (cause) => {
    vi.spyOn(core, 'isTauri').mockReturnValue(true)
    mockIPC(() => Promise.reject(cause))
    const wrapper = mount(App)
    await wrapper.get('textarea').setValue('小猫')
    await wrapper.get('form').trigger('submit')
    await flushPromises()
    expect(wrapper.get('dialog').element.open).toBe(true)
    expect(wrapper.get('#error-detail').text()).toBe(cause instanceof Error ? cause.message : '暂时无法生成图片，请再试一次。')
    expect(wrapper.get('button[type="submit"]').attributes('disabled')).toBeUndefined()
  })
})
