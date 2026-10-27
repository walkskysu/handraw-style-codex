import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest'
import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils'
import { clearMocks, mockIPC as installIPC } from '@tauri-apps/api/mocks'
import * as core from '@tauri-apps/api/core'
import GalleryView from '../src/components/GalleryView.vue'
import App from '../src/App.vue'
import type { Artwork } from '../src/services/gallery'

vi.mock('@tauri-apps/api/core', { spy: true })
enableAutoUnmount(afterEach)
afterEach(clearMocks)
beforeAll(() => {
  Object.defineProperties(HTMLDialogElement.prototype, {
    showModal: { configurable: true, value: function (this: HTMLDialogElement) { this.open = true } },
    close: { configurable: true, value: function (this: HTMLDialogElement) { this.open = false; this.dispatchEvent(new Event('close')) } },
  })
})
afterAll(() => {
  Reflect.deleteProperty(HTMLDialogElement.prototype, 'showModal')
  Reflect.deleteProperty(HTMLDialogElement.prototype, 'close')
})
beforeEach(() => vi.spyOn(core, 'isTauri').mockReturnValue(true))
const mockIPC = (handler: Parameters<typeof installIPC>[0]) => installIPC(async (command, args) => {
  if (command === 'gallery_group_detail') {
    const [photo] = await handler('gallery_detail', { id: (args as { id: string }).id }) as [Artwork, string]
    return { group: group(photo), photos: [photo] }
  }
  const result: any = await handler(command, args)
  if (command === 'gallery_list') return { ...result, photo_total: result.total, items: result.items.map((a: Artwork) => ({ ...group(a), ...a, thumbnails: [], photo_count: 1, pending_count: 0, matching_count: 1, matching_ids: [a.id] })) }
  if (command === 'generate_image' && result?.id) return { group_id: result.id, photos: [result], expected_count: 1, generation_errors: [] }
  return result
})
const group = (a: Artwork) => ({ ...a, expected_count: 1, generated_count: 1, generation_errors: [], cover_id: a.id, source_group_id: null, source_photo_id: null, status: 'complete' })
const image = 'data:image/png;base64,aW1hZ2U='
const artwork = (id = 'saved-1'): Artwork => ({
  id, group_id: id, position: 0, save_status: 'saved', title: '旧作品', prompt: '旧的描述', submitted_prompt: '实际提交的完整提示词',
  snapshot: { style: { id: 'FA-001', name: '历史风格名称', prompt: '历史完整规范' }, layout: null, color: null, catalog_version: 'sha256:old' },
  created_at: '2026-10-21T00:00:00Z', width: 1024, height: 768, file_size: 1024, format: 'png',
  tags: ['水彩'], favorite: false, deleted: false, model: null, tool_version: null, app_version: '0.1.0',
})

function galleryIPC() {
  let current = artwork()
  const ipc = vi.fn(async (command: string, args: any) => {
    switch (command) {
      case 'gallery_list': return { items: args.deleted === current.deleted ? [{ ...current, thumbnail: image }] : [], total: args.deleted === current.deleted ? 1 : 0, page: 1 }
      case 'gallery_detail': return [current, image]
      case 'gallery_update': current = { ...current, ...args }; return current
      case 'gallery_purge': return null
      case 'gallery_export': return true
      default: throw new Error(`Unexpected command: ${command}`)
    }
  })
  mockIPC(ipc)
  return ipc
}

const button = (wrapper: ReturnType<typeof mount>, text: string) => wrapper.findAll('button').find(b => b.text() === text)!

describe('persistent gallery UI', () => {
  it('keeps photos visible when only group metadata failed to save and retries that group', async () => {
    const ipc = vi.fn((command: string) => {
      if (command === 'generate_image') return { group_id: 'metadata-pending', photos: [{ id: 'one', image, save_error: null }], expected_count: 1, generation_errors: [], group_save_error: 'disk full' }
      if (command === 'gallery_group_retry') return null
      throw new Error(command)
    })
    installIPC(ipc)
    const wrapper = mount(App)
    await wrapper.get('#prompt').setValue('小猫')
    await wrapper.get('form').trigger('submit')
    await flushPromises()
    expect(wrapper.get('.image-preview img').attributes('src')).toBe(image)
    expect(wrapper.text()).toContain('照片组信息保存失败')
    expect(wrapper.get('button[type="submit"]').attributes('disabled')).toBeDefined()
    await button(wrapper, '重试保存组信息').trigger('click')
    await flushPromises()
    expect(ipc).toHaveBeenCalledWith('gallery_group_retry', { id: 'metadata-pending' })
    expect(wrapper.get('button[type="submit"]').attributes('disabled')).toBeUndefined()
    expect(ipc.mock.calls.filter(([cmd]) => cmd === 'generate_image')).toHaveLength(1)
  })

  it('opens the matching photo first while keeping the whole group browsable and actions scoped', async () => {
    const photos = [artwork('one'), artwork('two'), artwork('three')].map((a, position) => ({ ...a, group_id: 'group', position }))
    let g = { ...group(photos[0]!), id: 'group', expected_count: 4, generated_count: 3, generation_errors: ['第 4 张生成失败'] }
    const ipc = vi.fn((command: string, args: any) => {
      if (command === 'gallery_list') return { items: [{ ...g, thumbnail: image, thumbnails: [image, image], photo_count: 3, pending_count: 0, matching_count: 1, matching_ids: ['two'] }], total: 1, page: 1, photo_total: 3 }
      if (command === 'gallery_group_detail') return { group: g, photos }
      if (command === 'gallery_detail') return [photos.find(p => p.id === args.id), image]
      if (command === 'gallery_group_update') { g = { ...g, ...args }; return g }
      if (command === 'gallery_group_export') return true
      if (command === 'gallery_export') return true
      throw new Error(command)
    })
    installIPC(ipc)
    const wrapper = mount(GalleryView, { props: { generationPending: false } })
    await flushPromises()
    await wrapper.get('input[type="search"]').setValue('match')
    await flushPromises()
    expect(wrapper.get('.gallery-card').text()).toContain('本组 3 张 · 1 张匹配')
    expect(wrapper.get('.gallery-card').text()).toContain('3 / 4 张成功')
    expect(wrapper.findAll('.group-thumbnails img')).toHaveLength(2)
    await wrapper.get('.gallery-card').trigger('click')
    await flushPromises()
    expect(ipc).toHaveBeenLastCalledWith('gallery_detail', { id: 'two' })
    expect(wrapper.findAll('.photo-navigation button')).toHaveLength(3)
    await wrapper.findAll('.photo-navigation button')[0]!.trigger('click')
    await flushPromises()
    expect(ipc).toHaveBeenLastCalledWith('gallery_detail', { id: 'one' })
    await button(wrapper, '收藏整组').trigger('click')
    await flushPromises()
    expect(ipc).toHaveBeenCalledWith('gallery_group_update', expect.objectContaining({ id: 'group', favorite: true }))
    expect(button(wrapper, '收藏此图').attributes('aria-pressed')).toBe('false')
    await button(wrapper, '导出整组').trigger('click')
    await flushPromises()
    expect(ipc).toHaveBeenCalledWith('gallery_group_export', { id: 'group', includeNotes: true, deleted: false })
    await button(wrapper, '导出此图').trigger('click')
    await flushPromises()
    expect(ipc).toHaveBeenCalledWith('gallery_export', { id: 'one', includeNotes: true })
    await button(wrapper, '删除整组').trigger('click')
    await flushPromises()
    expect(ipc).toHaveBeenCalledWith('gallery_group_update', expect.objectContaining({ id: 'group', deleted: true }))
    expect(wrapper.get<HTMLDialogElement>('dialog').element.open).toBe(false)
  })

  it('retries only the unsaved photo in a partial result and the next generate creates a new request', async () => {
    const ipc = vi.fn((command: string) => {
      if (command === 'generate_image') return { group_id: 'batch', expected_count: 4, generation_errors: ['第 4 张：生成失败'], photos: [
        { id: 'one', image, save_error: null }, { id: 'two', image, save_error: '磁盘已满' }, { id: 'three', image, save_error: null },
      ] }
      if (command === 'gallery_retry') return artwork('two')
      throw new Error(command)
    })
    installIPC(ipc)
    const wrapper = mount(App)
    await wrapper.get('#prompt').setValue('多图小猫')
    await wrapper.get('select').setValue('4')
    await wrapper.get('form').trigger('submit')
    await flushPromises()
    expect(ipc).toHaveBeenCalledWith('generate_image', { prompt: '多图小猫', selections: {}, count: 4 })
    expect(wrapper.get('.group-status').text()).toContain('3 / 4 张成功 · 已保存 2 张 · 1 张待保存')
    expect(wrapper.get('button[type="submit"]').attributes('disabled')).toBeDefined()
    await wrapper.findAll('.photo-navigation button')[1]!.trigger('click')
    await button(wrapper, '重试保存').trigger('click')
    await flushPromises()
    expect(ipc).toHaveBeenCalledWith('gallery_retry', { id: 'two' })
    expect(ipc.mock.calls.filter(([cmd]) => cmd === 'generate_image')).toHaveLength(1)
    expect(wrapper.get('.group-status').text()).toContain('已保存 3 张 · 0 张待保存')
    expect(wrapper.get('button[type="submit"]').attributes('disabled')).toBeUndefined()
    await wrapper.get('form').trigger('submit')
    await flushPromises()
    expect(ipc.mock.calls.filter(([cmd]) => cmd === 'generate_image')).toHaveLength(2)
  })

  it('can retry an unsaved photo from its group and notifies the workspace', async () => {
    let a = { ...artwork('pending'), save_status: 'pending' as 'saved' | 'pending' }
    const ipc = vi.fn((command: string) => {
      if (command === 'gallery_list') return { items: [{ ...group(a), thumbnail: '', thumbnails: [], photo_count: 0, pending_count: 1, matching_count: 1, matching_ids: [a.id] }], total: 1, page: 1, photo_total: 0 }
      if (command === 'gallery_group_detail') return { group: group(a), photos: [a] }
      if (command === 'gallery_detail') return [a, image]
      if (command === 'gallery_retry') { a = { ...a, save_status: 'saved' }; return a }
      throw new Error(command)
    })
    installIPC(ipc)
    const wrapper = mount(GalleryView, { props: { generationPending: false } })
    await flushPromises()
    expect(wrapper.get('.gallery-card').text()).toContain('已保存 0 张 · 1 张待保存')
    await wrapper.get('.gallery-card').trigger('click')
    await flushPromises()
    expect(button(wrapper, '删除整组').attributes('disabled')).toBeDefined()
    await button(wrapper, '重试保存此图').trigger('click')
    await flushPromises()
    expect(wrapper.emitted('saved')).toEqual([['pending']])
    expect(ipc).toHaveBeenCalledWith('gallery_retry', { id: 'pending' })
    expect(button(wrapper, '删除整组').attributes('disabled')).toBeUndefined()
  })

  it('loads nine cards, requests subsequent pages and resets filters to page one', async () => {
    const ipc = vi.fn((command: string, args: any) => {
      expect(command).toBe('gallery_list')
      return { items: Array.from({ length: args.page === 1 ? 9 : 1 }, (_, i) => ({ id: `${args.page}-${i}`, title: `作品 ${i}`, thumbnail: image, created_at: '2026-10-21T00:00:00Z', favorite: false })), total: 10, page: args.page }
    })
    mockIPC(ipc)
    const wrapper = mount(GalleryView, { props: { generationPending: false } })
    await flushPromises()
    expect(wrapper.findAll('.gallery-card')).toHaveLength(9)
    await button(wrapper, '下一页').trigger('click')
    await flushPromises()
    expect(wrapper.findAll('.gallery-card')).toHaveLength(1)
    expect(ipc).toHaveBeenLastCalledWith('gallery_list', { search: '', favorite: false, photoFavorite: false, deleted: false, page: 2 })
    await wrapper.get('input[type="search"]').setValue("猫 %_'<script>")
    await flushPromises()
    expect(ipc).toHaveBeenLastCalledWith('gallery_list', { search: "猫 %_'<script>", favorite: false, photoFavorite: false, deleted: false, page: 1 })
    await wrapper.findAll('.gallery-tools input[type="checkbox"]')[0]!.setValue(true)
    await flushPromises()
    expect(ipc).toHaveBeenLastCalledWith('gallery_list', expect.objectContaining({ favorite: true, page: 1 }))
    await wrapper.findAll('.gallery-tools input[type="checkbox"]')[1]!.setValue(true)
    await flushPromises()
    expect(ipc).toHaveBeenLastCalledWith('gallery_list', expect.objectContaining({ favorite: true, photoFavorite: true, page: 1 }))
  })

  it('shows historical details, edits metadata, favorites, exports, trashes, restores and confirms purge', async () => {
    const ipc = galleryIPC()
    const wrapper = mount(GalleryView, { props: { generationPending: false }, attachTo: document.body })
    await flushPromises()
    await wrapper.get('.gallery-card').trigger('click')
    await flushPromises()
    const dialog = wrapper.get<HTMLDialogElement>('.artwork-dialog')
    expect(dialog.element.open).toBe(true)
    expect(dialog.text()).toContain('历史完整规范')
    expect(dialog.text()).toContain('实际提交的完整提示词')
    expect(dialog.text()).toContain('未提供')
    await dialog.findAll('.artwork-info input')[0]!.setValue('新标题')
    await dialog.findAll('.artwork-info input')[1]!.setValue('水彩，猫')
    await button(wrapper, '保存标题与标签').trigger('click')
    await flushPromises()
    expect(ipc).toHaveBeenCalledWith('gallery_update', expect.objectContaining({ title: '新标题', tags: ['水彩', '猫'] }))
    await button(wrapper, '收藏此图').trigger('click')
    await flushPromises()
    expect(button(wrapper, '取消收藏此图').attributes('aria-pressed')).toBe('true')
    await button(wrapper, '导出此图').trigger('click')
    await flushPromises()
    expect(ipc).toHaveBeenCalledWith('gallery_export', { id: 'saved-1', includeNotes: true })
    await button(wrapper, '复用创作').trigger('click')
    expect(wrapper.emitted('reuse')![0]![0]).toMatchObject({ prompt: '旧的描述', snapshot: { catalog_version: 'sha256:old' } })
    await button(wrapper, '删除此图').trigger('click')
    await flushPromises()
    expect(dialog.element.open).toBe(false)
    expect(wrapper.findAll('.gallery-card')).toHaveLength(0)
    await button(wrapper, '最近删除').trigger('click')
    await flushPromises()
    await wrapper.get('.gallery-card').trigger('click')
    await flushPromises()
    await button(wrapper, '恢复此图').trigger('click')
    await flushPromises()
    expect(ipc).toHaveBeenCalledWith('gallery_update', expect.objectContaining({ deleted: false, favorite: true }))
    await button(wrapper, '返回图库').trigger('click')
    await flushPromises()
    await wrapper.get('.gallery-card').trigger('click')
    await flushPromises()
    await button(wrapper, '删除此图').trigger('click')
    await flushPromises()
    await button(wrapper, '最近删除').trigger('click')
    await flushPromises()
    await wrapper.get('.gallery-card').trigger('click')
    await flushPromises()
    await button(wrapper, '永久删除此图').trigger('click')
    expect(ipc.mock.calls.filter(([cmd]) => cmd === 'gallery_purge')).toHaveLength(0)
    await button(wrapper, '取消').trigger('click')
    expect(wrapper.find('.purge-confirm').exists()).toBe(false)
    await button(wrapper, '永久删除此图').trigger('click')
    await button(wrapper, '确认永久删除').trigger('click')
    await flushPromises()
    expect(ipc).toHaveBeenCalledWith('gallery_purge', { id: 'saved-1' })
  })

  it('can paginate again after removing the last artwork on the final page', async () => {
    let total = 10
    const ipc = vi.fn((command: string, args: any) => {
      if (command === 'gallery_list') {
        const page = Math.min(args.page, Math.ceil(total / 9))
        return { items: [{ ...artwork(), thumbnail: image }], total, page }
      }
      if (command === 'gallery_detail') return [artwork(), image]
      if (command === 'gallery_update') { total = 9; return { ...artwork(), ...args } }
      throw new Error(command)
    })
    mockIPC(ipc)
    const wrapper = mount(GalleryView, { props: { generationPending: false } })
    await flushPromises()
    await button(wrapper, '下一页').trigger('click')
    await flushPromises()
    await wrapper.get('.gallery-card').trigger('click')
    await flushPromises()
    await button(wrapper, '删除此图').trigger('click')
    await flushPromises()
    expect(wrapper.get('.gallery-pagination').text()).toContain('第 1 / 1 页')
    // A new generation makes page two available again while the gallery stays open.
    total = 10
    await wrapper.setProps({ revision: 'new-artwork' })
    await flushPromises()
    await button(wrapper, '下一页').trigger('click')
    await flushPromises()
    expect(ipc).toHaveBeenLastCalledWith('gallery_list', expect.objectContaining({ page: 2 }))
    expect(wrapper.get('.gallery-pagination').text()).toContain('第 2 / 2 页')
  })

  it('ignores stale search responses and allows retrying a read failure', async () => {
    let completeFirst!: (result: unknown) => void
    const ipc = vi.fn().mockImplementationOnce(() => new Promise(resolve => { completeFirst = resolve }))
      .mockResolvedValueOnce({ items: [], total: 0, page: 1 }).mockRejectedValueOnce('图库无法读取')
      .mockResolvedValue({ items: [], total: 0, page: 1 })
    mockIPC(ipc)
    const wrapper = mount(GalleryView, { props: { generationPending: false } })
    await wrapper.get('input[type="search"]').setValue('新搜索')
    await flushPromises()
    completeFirst({ items: [{ ...artwork(), thumbnail: image }], total: 1, page: 1 })
    await flushPromises()
    expect(wrapper.findAll('.gallery-card')).toHaveLength(0)
    await button(wrapper, '刷新').trigger('click')
    await flushPromises()
    expect(wrapper.get('[role="alert"]').text()).toContain('图库无法读取')
    await button(wrapper, '刷新').trigger('click')
    await flushPromises()
    expect(wrapper.find('[role="alert"]').exists()).toBe(false)
  })

  it('preserves the generated preview on save failure, supports cancelled export and retries the same ID', async () => {
    const ipc = vi.fn((command: string) => {
      if (command === 'generate_image') return { id: 'pending-id', image, save_error: '磁盘已满' }
      if (command === 'gallery_export') return false
      if (command === 'gallery_retry') return artwork('pending-id')
      throw new Error(command)
    })
    mockIPC(ipc)
    const wrapper = mount(App)
    await wrapper.get('#prompt').setValue('小猫')
    await wrapper.get('form').trigger('submit')
    await flushPromises()
    expect(wrapper.get('.image-preview img').attributes('src')).toBe(image)
    expect(wrapper.text()).toContain('图库保存失败：磁盘已满')
    expect(wrapper.text()).not.toContain('已保存到图库')
    expect(wrapper.get('button[type="submit"]').attributes('disabled')).toBeDefined()
    await button(wrapper, '导出图片').trigger('click')
    await flushPromises()
    expect(wrapper.text()).not.toContain('已导出图片')
    expect(ipc).toHaveBeenCalledWith('gallery_export', { id: 'pending-id', includeNotes: true })
    await button(wrapper, '重试保存').trigger('click')
    await flushPromises()
    expect(ipc).toHaveBeenCalledWith('gallery_retry', { id: 'pending-id' })
    expect(wrapper.text()).toContain('已保存到图库')
    expect(wrapper.get('.image-preview img').attributes('src')).toBe(image)
    expect(wrapper.get('button[type="submit"]').attributes('disabled')).toBeUndefined()
    expect(ipc.mock.calls.filter(([cmd]) => cmd === 'generate_image')).toHaveLength(1)
  })

  it('confirms before replacing edits, refills historical configuration and generates a new artwork', async () => {
    const handler = galleryIPC()
    mockIPC(async (command, args) => command === 'generate_image'
      ? { id: 'new-artwork', image, save_error: null } : handler(command, args))
    const invoke = vi.spyOn(core, 'invoke')
    const wrapper = mount(App, { attachTo: document.body })
    await wrapper.get('#prompt').setValue('未保存的修改')
    await button(wrapper, '我的图库').trigger('click')
    await flushPromises()
    await wrapper.get('.gallery-card').trigger('click')
    await flushPromises()
    await button(wrapper, '复用创作').trigger('click')
    const confirm = wrapper.get<HTMLDialogElement>('.reuse-dialog')
    expect(confirm.element.open).toBe(true)
    await confirm.findAll('button')[1]!.trigger('click')
    expect(wrapper.get<HTMLTextAreaElement>('#prompt').element.value).toBe('未保存的修改')
    await button(wrapper, '复用创作').trigger('click')
    await button(wrapper, '确认复用').trigger('click')
    await flushPromises()
    expect(wrapper.get<HTMLTextAreaElement>('#prompt').element.value).toBe('旧的描述')
    expect(wrapper.get('[data-selection="style"]').text()).toContain('历史风格名称')
    await wrapper.get('form').trigger('submit')
    await flushPromises()
    expect(invoke).toHaveBeenCalledWith('generate_image', { prompt: '旧的描述', selections: { style: 'FA-001' }, reuseId: 'saved-1', count: 1 })
    expect(wrapper.text()).toContain('已保存到图库')
  })
})
