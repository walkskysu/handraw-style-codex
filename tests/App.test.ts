import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from 'vitest'
import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils'
import { nextTick } from 'vue'
import * as core from '@tauri-apps/api/core'
import { clearMocks, mockIPC as installIPC } from '@tauri-apps/api/mocks'
import App from '../src/App.vue'
import { generationOptions } from '../src/services/generationOptions'

vi.mock('@tauri-apps/api/core', { spy: true })
enableAutoUnmount(afterEach)
afterEach(() => { clearMocks(); vi.unstubAllGlobals() })
const image = 'data:image/png;base64,aW1hZ2U='
// Keep generation scenarios concise while exercising the structured desktop response.
const mockIPC = (callback: Parameters<typeof installIPC>[0]) => installIPC(async (...args) => {
  const result = await callback(...args)
  return typeof result === 'string' && result.startsWith('data:image/')
    ? { id: 'generated-artwork', image: result, save_error: null } : result
})

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
  it('keeps selections across tabs and search, and locates a selected card from the left panel', async () => {
    const wrapper = mount(App, { attachTo: document.body })
    const selected = generationOptions.style.at(-1)!
    await wrapper.get('#catalog-search').setValue(selected.id)
    await wrapper.get(`[data-option="${selected.id}"]`).trigger('click')
    const summary = wrapper.get('[data-selection="style"]')
    expect(summary.text()).toContain(selected.name)
    expect(summary.get('img').attributes('src')).toBe(selected.image)
    await wrapper.get('#color-tab').trigger('click')
    expect(wrapper.get('#color-tab').attributes('aria-selected')).toBe('true')
    await wrapper.get('#catalog-search').setValue('')
    await wrapper.get('[data-option="C-01"]').trigger('click')
    await wrapper.get('#catalog-search').setValue('no matches')
    expect(summary.text()).toContain(selected.name)
    await summary.get('.selection-link').trigger('click')
    await flushPromises()
    expect(wrapper.get('#style-tab').attributes('aria-selected')).toBe('true')
    expect(document.activeElement).toBe(wrapper.get('#style-tab').element)
    expect(wrapper.get<HTMLInputElement>('#catalog-search').element.value).toBe('')
    expect(wrapper.get(`[data-option="${selected.id}"]`).attributes('aria-pressed')).toBe('true')
    expect(wrapper.findAll('.option-card')).toHaveLength(generationOptions.style.length)
    await summary.get('.clear-selection').trigger('click')
    expect(summary.text()).toContain('自由生成')
    expect(summary.find('img').exists()).toBe(false)
    expect(wrapper.get(`[data-option="${selected.id}"]`).attributes('aria-pressed')).toBe('false')
    expect(wrapper.get('[data-selection="color"]').text()).toContain(generationOptions.color[0]!.name)
  })

  it('supports arrow, Home and End keys in the category tabs', async () => {
    const wrapper = mount(App, { attachTo: document.body })
    for (const [key, category] of [['ArrowRight', 'layout'], ['End', 'color'], ['ArrowRight', 'style'], ['ArrowLeft', 'color'], ['Home', 'style']]) {
      await wrapper.get('.catalog-tabs').trigger('keydown', { key })
      expect(wrapper.get(`#${category}-tab`).attributes('aria-selected')).toBe('true')
      expect(document.activeElement).toBe(wrapper.get(`#${category}-tab`).element)
      expect(wrapper.findAll('[role="tab"][tabindex="0"]')).toHaveLength(1)
      expect(wrapper.get('[role="tabpanel"]').attributes('aria-labelledby')).toBe(`${category}-tab`)
    }
  })

  it('prioritizes the catalog and preserves the preview and selections across narrow and wide layouts', async () => {
    vi.spyOn(core, 'isTauri').mockReturnValue(true)
    mockIPC(() => image)
    let resize!: (event: { matches: boolean }) => void
    const removeEventListener = vi.fn()
    vi.stubGlobal('matchMedia', vi.fn(() => ({
      matches: true,
      addEventListener: (_event: string, listener: typeof resize) => { resize = listener },
      removeEventListener,
    })))
    const wrapper = mount(App, { attachTo: document.body })
    await nextTick()
    const dialog = wrapper.get<HTMLDialogElement>('#preview-dialog')
    expect(wrapper.get('main').classes()).toContain('compact-workspace')
    expect(dialog.element.open).toBe(false)
    expect(wrapper.get('main').find('.catalog-panel').exists()).toBe(true)
    expect(wrapper.get('main').find('.preview-panel').exists()).toBe(false)
    await wrapper.get('[data-option="FA-001"]').trigger('click')
    await wrapper.get('textarea').setValue('小猫')
    await wrapper.get('form').trigger('submit')
    await flushPromises()
    const opener = wrapper.get('.open-preview')
    expect(opener.text()).toContain('已完成')
    await opener.trigger('click')
    await flushPromises()
    expect(dialog.element.open).toBe(true)
    expect(opener.attributes('aria-expanded')).toBe('true')
    expect(dialog.get('.image-preview img').attributes('src')).toBe(image)
    await dialog.get('.close-preview').trigger('click')
    expect(dialog.element.open).toBe(false)
    expect(document.activeElement).toBe(opener.element)
    await wrapper.get('[data-selection="style"] .selection-link').trigger('click')
    await flushPromises()
    expect(dialog.element.open).toBe(false)
    expect(wrapper.get('main [data-option="FA-001"]').attributes('aria-pressed')).toBe('true')
    await opener.trigger('click')
    resize({ matches: false })
    await flushPromises()
    expect(dialog.element.open).toBe(false)
    expect(wrapper.get('main .catalog-panel [data-option="FA-001"]').attributes('aria-pressed')).toBe('true')
    expect(wrapper.get('main .image-preview img').attributes('src')).toBe(image)
    resize({ matches: true })
    await flushPromises()
    expect(dialog.element.open).toBe(false)
    await wrapper.get('.open-preview').trigger('click')
    await flushPromises()
    expect(dialog.get('.image-preview img').attributes('src')).toBe(image)
    expect(dialog.get('figcaption').text()).toBe('小猫')
    // Native Escape emits close; jsdom models the same lifecycle here.
    dialog.element.close()
    await nextTick()
    expect(wrapper.get('.open-preview').attributes('aria-expanded')).toBe('false')
    wrapper.unmount()
    expect(removeEventListener).toHaveBeenCalledWith('change', resize)
  })

  it('submits all three selections, preserves them on failure, and allows independent clearing', async () => {
    vi.spyOn(core, 'isTauri').mockReturnValue(true)
    const ipc = vi.fn().mockRejectedValueOnce('请重试').mockResolvedValueOnce(image)
    mockIPC(ipc)
    const wrapper = mount(App)
    await wrapper.get('textarea').setValue('小猫')
    for (const [category, id] of [['style', 'FH-044'], ['layout', 'EC-024'], ['color', 'C-36']]) {
      await wrapper.get(`#${category}-tab`).trigger('click')
      await wrapper.get('#catalog-search').setValue(id)
      await wrapper.get(`[data-option="${id}"]`).trigger('click')
    }
    await wrapper.get('form').trigger('submit')
    expect(wrapper.findAll('fieldset').every((field) => field.attributes('disabled') !== undefined)).toBe(true)
    await flushPromises()
    expect(ipc).toHaveBeenNthCalledWith(1, 'generate_image', {
      prompt: '小猫', selections: { style: 'FH-044', layout: 'EC-024', color: 'C-36' },
    })
    expect(wrapper.get('[data-selection="style"] img').attributes('src')).toBeTruthy()
    await wrapper.get('[data-selection="layout"] .clear-selection').trigger('click')
    expect(wrapper.get('[data-selection="layout"]').text()).toContain('未选择')
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
    await wrapper.get('[data-option="FA-001"]').trigger('click')
    await wrapper.get('textarea').setValue('  水彩小猫  ')
    await wrapper.get('form').trigger('submit')
    expect(wrapper.get('button[type="submit"]').text()).toBe('正在生成…')
    expect(wrapper.get('textarea').attributes('disabled')).toBeDefined()
    expect(wrapper.get('form').attributes('aria-busy')).toBe('true')
    expect(wrapper.get('[role="status"]').text()).toContain('请稍候')
    expect(wrapper.get('.generation-status').text()).toBe('生成中')
    expect(wrapper.get('.submitted-prompt').text()).toContain('水彩小猫')
    const clear = wrapper.get('[data-selection="style"] .clear-selection')
    expect(clear.attributes('disabled')).toBeDefined()
    await clear.trigger('click')
    await wrapper.get('[data-option="FA-002"]').trigger('click')
    expect(wrapper.get('[data-option="FA-001"]').attributes('aria-pressed')).toBe('true')
    await wrapper.get('form').trigger('submit')
    expect(ipc).toHaveBeenCalledTimes(1)
    resolveImage(image)
    await flushPromises()
    expect(wrapper.get('.image-preview img').attributes('src')).toBe(image)
    expect(wrapper.get('.image-preview img').attributes('alt')).toBe('水彩小猫')
    expect(wrapper.get('figcaption').text()).toBe('水彩小猫')
    expect(wrapper.get('form').attributes('aria-busy')).toBe('false')
    expect(wrapper.get('.generation-status').text()).toBe('已完成')
    expect(wrapper.find('.submitted-prompt').exists()).toBe(false)
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
    expect(wrapper.get<HTMLDialogElement>('.error-dialog').element.open).toBe(true)
    expect(wrapper.get<HTMLDialogElement>('.error-dialog').text()).toContain('检查登录')
    expect(wrapper.find('#upgrade-hint').exists()).toBe(false)
    expect(wrapper.get('figcaption').text()).toBe('小猫')
    expect(wrapper.get('.generation-status').text()).toBe('生成失败')
    expect(wrapper.get('.submitted-prompt').text()).toContain('小狗')
    expect(wrapper.get('textarea').element.value).toBe('小狗')
    expect(wrapper.get('button[type="submit"]').attributes('disabled')).toBeUndefined()
    await wrapper.get('.error-dialog button').trigger('click')
    expect(wrapper.get<HTMLDialogElement>('.error-dialog').element.open).toBe(false)
    await wrapper.get('form').trigger('submit')
    await flushPromises()
    expect(wrapper.find('[role="alert"]').exists()).toBe(false)
    expect(wrapper.get<HTMLDialogElement>('.error-dialog').element.open).toBe(false)
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
    expect(wrapper.get<HTMLDialogElement>('.error-dialog').element.open).toBe(true)
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
    const dialog = wrapper.get<HTMLDialogElement>('.error-dialog')
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
    expect(wrapper.get<HTMLDialogElement>('.error-dialog').element.open).toBe(true)
    expect(wrapper.get('#error-detail').text()).toBe(cause instanceof Error ? cause.message : '暂时无法生成图片，请再试一次。')
    expect(wrapper.get('button[type="submit"]').attributes('disabled')).toBeUndefined()
  })
})
