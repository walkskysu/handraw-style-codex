import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { enableAutoUnmount, flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import AboutSettings from '../src/components/AboutSettings.vue'
import * as updates from '../src/services/updates'
import { isDesktop } from '../src/services/imageGeneration'

vi.mock('../src/services/imageGeneration', () => ({ isDesktop: vi.fn(() => true) }))
vi.mock('../src/services/updates', () => ({
  getAbout: vi.fn(), checkUpdate: vi.fn(), downloadUpdate: vi.fn(), installUpdate: vi.fn(), openManualUpdate: vi.fn(),
  RELEASE_PAGE: 'https://github.com/walkskysu/handraw-style-codex/releases/latest',
}))
enableAutoUnmount(afterEach)
const release = { version: '0.3.0', notes: '• 新增图库搜索\n• 优化图片预览\n<img src=x onerror=alert(1)>' }
const button = (wrapper: VueWrapper, label: string) => wrapper.findAll('button').find(b => b.text() === label)!
const render = () => mount(AboutSettings, { props: { working: false, unsaved: false }, attachTo: document.body })
async function opened() {
  const wrapper = render()
  await button(wrapper, '设置').trigger('click')
  await flushPromises()
  return wrapper
}
async function downloaded() {
  const wrapper = await opened()
  await button(wrapper, '下载更新').trigger('click')
  await flushPromises()
  return wrapper
}
beforeEach(() => {
  vi.useFakeTimers()
  vi.mocked(isDesktop).mockReturnValue(true)
  vi.mocked(updates.getAbout).mockResolvedValue({ current_version: '0.2.4', install_error: null })
  vi.mocked(updates.checkUpdate).mockResolvedValue(release)
  vi.mocked(updates.downloadUpdate).mockResolvedValue()
  vi.mocked(updates.installUpdate).mockResolvedValue()
  vi.mocked(updates.openManualUpdate).mockResolvedValue()
  Object.defineProperties(HTMLDialogElement.prototype, {
    showModal: { configurable: true, value() { this.open = true } },
    close: { configurable: true, value() { this.open = false; this.dispatchEvent(new Event('close')) } },
  })
})
afterEach(() => { vi.useRealTimers(); vi.resetAllMocks() })

describe('Settings / About updates', () => {
  it('shows real versions, renders release notes as text and keeps downloading separate from installation', async () => {
    const wrapper = await opened()
    expect(wrapper.text()).toContain('当前版本：0.2.4')
    expect(wrapper.text()).toContain('发现新版本：0.3.0')
    expect(wrapper.get('.release-notes').text()).toContain(release.notes)
    expect(wrapper.find('img').exists()).toBe(false)
    let finish!: () => void
    vi.mocked(updates.downloadUpdate).mockImplementationOnce(onProgress => {
      onProgress({ downloaded: 50, total: 100 })
      return new Promise(resolve => { finish = resolve })
    })
    await button(wrapper, '下载更新').trigger('click')
    expect(wrapper.get('progress').attributes('value')).toBe('50')
    expect(button(wrapper, '下载更新')).toBeUndefined()
    expect(button(wrapper, '重启并安装')).toBeUndefined()
    expect(updates.installUpdate).not.toHaveBeenCalled()
    finish(); await flushPromises()
    expect(wrapper.text()).toContain('签名校验通过')
    await button(wrapper, '重启并安装').trigger('click')
    await flushPromises()
    expect(updates.installUpdate).toHaveBeenCalledOnce()
    expect(wrapper.emitted('lock')).toEqual([[true]])
  })

  it('does not install until generation and failed saves are resolved, even with the dialog closed', async () => {
    const wrapper = await downloaded()
    await wrapper.setProps({ working: true })
    await button(wrapper, '重启并安装').trigger('click')
    expect(wrapper.text()).toContain('等待当前任务结束')
    await wrapper.get('[aria-label="关闭设置"]').trigger('click')
    await wrapper.setProps({ working: false, unsaved: true })
    await flushPromises()
    expect(wrapper.text()).toContain('作品尚未保存')
    expect(updates.installUpdate).not.toHaveBeenCalled()
    await wrapper.setProps({ working: true, unsaved: true })
    await wrapper.setProps({ working: true, unsaved: false })
    expect(updates.installUpdate).not.toHaveBeenCalled()
    await wrapper.setProps({ working: false })
    await flushPromises()
    expect(updates.installUpdate).toHaveBeenCalledOnce()
  })

  it('can cancel queued installation and resume creation', async () => {
    const wrapper = await downloaded()
    await wrapper.setProps({ working: true })
    await button(wrapper, '重启并安装').trigger('click')
    await button(wrapper, '取消自动安装').trigger('click')
    await wrapper.setProps({ working: false })
    expect(updates.installUpdate).not.toHaveBeenCalled()
    expect(wrapper.emitted('lock')).toEqual([[true], [false]])
    expect(button(wrapper, '重启并安装')).toBeDefined()
  })

  it.each(['网络连接中断', '签名校验失败'])('offers redownload and manual installation after %s', async failure => {
    vi.mocked(updates.downloadUpdate).mockRejectedValueOnce(failure)
    const wrapper = await opened()
    await button(wrapper, '下载更新').trigger('click')
    await flushPromises()
    expect(wrapper.get('[role="alert"]').text()).toContain(failure)
    expect(button(wrapper, '重启并安装')).toBeUndefined()
    await button(wrapper, '手动安装').trigger('click')
    expect(updates.openManualUpdate).toHaveBeenCalledOnce()
    await button(wrapper, '重新下载').trigger('click')
    await flushPromises()
    expect(button(wrapper, '重启并安装')).toBeDefined()
    expect(updates.installUpdate).not.toHaveBeenCalled()
  })

  it('unlocks creation and requires a new download after installation fails', async () => {
    vi.mocked(updates.installUpdate).mockRejectedValueOnce('安装程序无法启动')
    const wrapper = await downloaded()
    await button(wrapper, '重启并安装').trigger('click')
    await flushPromises()
    expect(wrapper.get('[role="alert"]').text()).toContain('安装程序无法启动')
    expect(button(wrapper, '重新下载')).toBeDefined()
    expect(button(wrapper, '手动安装')).toBeDefined()
    expect(wrapper.emitted('lock')).toEqual([[true], [false]])
  })

  it('recovers an interrupted installer from the previous launch', async () => {
    vi.mocked(updates.getAbout).mockResolvedValue({ current_version: '0.2.4', install_error: '上次更新未完成' })
    const wrapper = await opened()
    expect(wrapper.text()).toContain('上次更新未完成')
    expect(button(wrapper, '重新下载')).toBeDefined()
    expect(button(wrapper, '手动安装')).toBeDefined()
  })

  it('automatically checks without a modal and snoozes reminders for an hour', async () => {
    const wrapper = render()
    await vi.advanceTimersByTimeAsync(5000)
    await flushPromises()
    expect(updates.checkUpdate).toHaveBeenCalledOnce()
    expect(wrapper.find('.update-reminder').exists()).toBe(true)
    expect(wrapper.get<HTMLDialogElement>('dialog').element.open).toBe(false)
    await wrapper.get('.update-reminder').findAll('button')[1]!.trigger('click')
    expect(wrapper.find('.update-reminder').exists()).toBe(false)
    await vi.advanceTimersByTimeAsync(60 * 60 * 1000)
    expect(wrapper.find('.update-reminder').exists()).toBe(true)
    wrapper.unmount()
    expect(vi.getTimerCount()).toBe(0)
  })

  it('shows latest-version and offline states without claiming a failed check is up to date', async () => {
    vi.mocked(updates.checkUpdate).mockResolvedValueOnce(null).mockRejectedValueOnce('离线')
    const wrapper = await opened()
    expect(wrapper.text()).toContain('当前已是最新版本')
    await button(wrapper, '检查更新').trigger('click')
    await flushPromises()
    expect(wrapper.text()).toContain('离线')
    expect(wrapper.text()).not.toContain('当前已是最新版本')
    expect(button(wrapper, '重新检查')).toBeDefined()
  })

  it('offers a copyable manual URL if the system browser cannot open', async () => {
    vi.mocked(updates.openManualUpdate).mockRejectedValueOnce('浏览器启动失败')
    const wrapper = await opened()
    await button(wrapper, '手动安装').trigger('click')
    await flushPromises()
    expect(wrapper.get('[role="alert"]').text()).toContain(updates.RELEASE_PAGE)
  })

  it('disables update actions in browser preview', async () => {
    vi.mocked(isDesktop).mockReturnValue(false)
    const wrapper = await opened()
    await vi.advanceTimersByTimeAsync(5000)
    expect(wrapper.text()).toContain('请在桌面应用中')
    expect(updates.checkUpdate).not.toHaveBeenCalled()
    expect(button(wrapper, '下载更新')).toBeUndefined()
  })
})
