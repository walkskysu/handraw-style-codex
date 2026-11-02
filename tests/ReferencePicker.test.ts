import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils'
import { clearMocks, mockIPC } from '@tauri-apps/api/mocks'
import * as core from '@tauri-apps/api/core'
import ReferencePicker from '../src/components/ReferencePicker.vue'
import { referenceData, referenceImages, referenceLoading, referenceError, uploadReferences } from '../src/services/referenceImages'
vi.mock('@tauri-apps/api/core', { spy: true })
enableAutoUnmount(afterEach)
beforeEach(() => {
  referenceImages.value = []; referenceError.value = ''; referenceLoading.value = false
  vi.spyOn(core, 'isTauri').mockReturnValue(true)
})
afterEach(() => { clearMocks(); vi.restoreAllMocks() })
const records = [
  { id: 'person', name: '人物.png', thumbnail: 'data:image/png;base64,YQ==' },
  { id: 'style', name: '风格.webp', thumbnail: 'data:image/webp;base64,Yg==' },
]
describe('reference image library', () => {
  it('uploads multiple files, then explicitly selects, filters and deselects in stable reference order', async () => {
    const ipc = vi.fn(async (cmd: string, args: any) => cmd === 'reference_list' ? [] : records.find(r => r.name === args.name))
    mockIPC(ipc)
    const w = mount(ReferencePicker, { props: { modelValue: [] } }); await flushPromises()
    const input = w.get('input[type="file"]')
    Object.defineProperty(input.element, 'files', { value: [new File(['png'], '人物.png', { type: 'image/png' }), new File(['webp'], '风格.webp', { type: 'image/webp' })] })
    await input.trigger('change')
    await vi.waitFor(() => expect(w.findAll('[data-reference]')).toHaveLength(2))
    expect(ipc.mock.calls.filter(([cmd]) => cmd === 'reference_import').map(([, args]) => args)).toEqual([
      { name: '人物.png', dataUrl: 'data:image/png;base64,cG5n' }, { name: '风格.webp', dataUrl: 'data:image/webp;base64,d2VicA==' },
    ])
    expect(w.emitted('update:modelValue')).toBeUndefined()
    await w.get('[data-reference="person"]').trigger('click')
    expect(w.emitted('update:modelValue')!.at(-1)).toEqual([['person']])
    await w.setProps({ modelValue: ['person'] })
    await w.get('[data-reference="style"]').trigger('click')
    await w.setProps({ modelValue: ['person', 'style'] })
    await w.get('input[type="search"]').setValue('风格')
    expect(w.findAll('[data-reference]')).toHaveLength(1)
    expect(w.get('.selected-check').text()).toBe('2')
    await w.get('[data-reference="style"]').trigger('click')
    expect(w.emitted('update:modelValue')!.at(-1)).toEqual([['person']])
    await w.setProps({ disabled: true })
    expect(w.get('[data-reference="style"]').attributes('disabled')).toBeDefined()
    expect(input.attributes('disabled')).toBeDefined()
  })
  it('preserves earlier uploads on a later failure and rejects unsupported/oversized inputs before IPC', async () => {
    mockIPC((_cmd, args: any) => args.name === '人物.png' ? records[0] : Promise.reject('磁盘已满'))
    await expect(uploadReferences([new File(['a'], '人物.png', { type: 'image/png' }), new File(['b'], '风格.webp', { type: 'image/webp' })])).rejects.toBe('磁盘已满')
    expect(referenceImages.value).toEqual([records[0]])
    await expect(referenceData(new File(['<svg/>'], 'x.svg', { type: 'image/svg+xml' }))).rejects.toThrow('仅支持')
    const large = new File([], 'large.png', { type: 'image/png' }); Object.defineProperty(large, 'size', { value: 20 * 1024 * 1024 + 1 })
    await expect(referenceData(large)).rejects.toThrow('20 MB')
  })
  it('shows upload errors and permits retry without changing the current selection', async () => {
    let fail = true
    mockIPC((cmd) => cmd === 'reference_list' ? records : fail ? Promise.reject('保存失败') : { ...records[0], id: 'new-person' })
    const w = mount(ReferencePicker, { props: { modelValue: ['style'] } }); await flushPromises()
    const input = w.get('input[type="file"]')
    Object.defineProperty(input.element, 'files', { value: [new File(['a'], '人物.png', { type: 'image/png' })] })
    await input.trigger('change'); await vi.waitFor(() => expect(w.get('[role="alert"]').text()).toContain('保存失败'))
    expect(w.emitted('update:modelValue')).toBeUndefined()
    fail = false; await input.trigger('change')
    await vi.waitFor(() => expect(w.find('[role="alert"]').exists()).toBe(false))
    expect(w.get('[data-reference="style"]').attributes('aria-pressed')).toBe('true')
  })
  it('keeps browser preview upload disabled and limits selections', async () => {
    vi.spyOn(core, 'isTauri').mockReturnValue(false)
    referenceImages.value = records
    const w = mount(ReferencePicker, { props: { modelValue: Array.from({ length: 12 }, (_, i) => String(i)) } })
    expect(w.get('input[type="file"]').attributes('disabled')).toBeDefined()
    await w.get('[data-reference="person"]').trigger('click')
    expect(w.get('[role="alert"]').text()).toContain('最多选择 12 张')
    expect(w.emitted('update:modelValue')).toBeUndefined()
  })
})
