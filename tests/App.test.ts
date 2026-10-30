import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils'
import * as core from '@tauri-apps/api/core'
import { clearMocks, mockIPC } from '@tauri-apps/api/mocks'
import App from '../src/App.vue'
import type { Draft, Item, JobSnapshot, Library, Notice, Plan } from '../src/services/creation'
import { acceptSnapshot } from '../src/services/creation'
import { generationOptions } from '../src/services/generationOptions'
vi.mock('@tauri-apps/api/core', { spy: true })
const events = vi.hoisted(() => ({ change: undefined as undefined | ((event: { payload: Notice }) => void) }))
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async (name: string, fn: typeof events.change) => { if (name === 'creation-changed') events.change = fn; return () => {} }) }))
enableAutoUnmount(afterEach)
beforeEach(() => {
  localStorage.clear(); vi.spyOn(core, 'isTauri').mockReturnValue(true)
  Object.defineProperties(HTMLDialogElement.prototype, {
    showModal: { configurable: true, value: function (this: HTMLDialogElement) { this.open = true } },
    close: { configurable: true, value: function (this: HTMLDialogElement) { this.open = false; this.dispatchEvent(new Event('close')) } },
  })
})
afterEach(() => { clearMocks(); vi.unstubAllGlobals(); document.body.innerHTML = ''; events.change = undefined })
const image = 'data:image/png;base64,aW1hZ2U='
const visual = { style: null, layout: null, color: null, catalog_version: 'catalog' }
const button = (w: ReturnType<typeof mount>, text: string) => w.findAll('button').find(b => b.text() === text)!
function backend() {
  const data: Library = { drafts: [], plans: [], planning_jobs: [], jobs: [] }
  const jobs: Record<string, JobSnapshot> = {}
  let serial = 0, failPlan = false
  const ipc = vi.fn(async (cmd: string, args: any) => {
    if (cmd === 'job_list') return structuredClone(data)
    if (cmd === 'draft_update') {
      const input = args.input, old = data.drafts.find(d => d.id === input.id)
      const d: Draft = { id: old?.id ?? `draft-${++serial}`, mode: input.mode, prompt: input.prompt, snapshot: visual, requested_count: input.requested_count, input_revision: old ? old.input_revision + (old.prompt !== input.prompt ? 1 : 0) : 1, current_plan_id: old?.current_plan_id ?? null, feedback_draft: input.feedback, updated_at: 'now' }
      data.drafts = [d, ...data.drafts.filter(x => x.id !== d.id)]; return structuredClone(d)
    }
    if (cmd === 'plan_create') {
      const d = data.drafts.find(d => d.id === args.draftId)!, id = `planning-${++serial}`
      if (failPlan) { data.planning_jobs.unshift({ id, draft_id: d.id, status: 'failed', revision: 2, error: '规划失败', output_plan_id: null }); return id }
      const p: Plan = { id: `plan-${++serial}`, draft_id: d.id, version: data.plans.filter(p => p.draft_id === d.id).length + 1, input_revision: d.input_revision, input_hash: 'input', content_hash: 'digest', snapshot: visual, original_prompt: d.prompt, confirmed_at: null, content: { title: '猫咪计划', goal: '不同姿态', audience: '初学者', kind: 'variants', visual: '水彩', dimensions: '1:1', verification: '未核实', change_summary: d.feedback_draft, items: Array.from({ length: d.requested_count ?? 3 }, (_, position) => ({ id: `entry-${position}`, position, title: `猫 ${position + 1}`, copy: '喵', description: '窗边', requirements: '水彩' })) } }
      d.current_plan_id = p.id; d.feedback_draft = ''; data.plans.push(p)
      data.planning_jobs.unshift({ id, draft_id: d.id, status: 'completed', revision: 2, error: null, output_plan_id: p.id }); return id
    }
    if (cmd === 'generation_start_quick' || cmd === 'plan_confirm_and_start') {
      const d = data.drafts.find(d => d.id === args.draftId)!, p = data.plans.find(p => p.id === args.planId) ?? null, id = `job-${++serial}`
      const items: Item[] = Array.from({ length: p?.content.items.length ?? 1 }, (_, position) => ({ id: `image-${id}-${position}`, job_id: id, position, title: `猫 ${position + 1}`, prompt: `画面 ${position + 1}`, generation_status: 'queued', save_status: 'none', attempt_no: 0, attempt_id: null, artwork_id: null, error: null }))
      if (p) p.confirmed_at = 'now'
      const job: JobSnapshot = { id, group_id: `group-${id}`, draft_id: d.id, plan_id: p?.id ?? null, mode: d.mode, status: 'running', revision: 1, expected_count: items.length, updated_at: 'now', items, plan: p, generated_count: 0, saved_count: 0, available_count: 0, failed_count: 0, pending_save_count: 0 }
      jobs[id] = job; data.jobs.unshift(job); return structuredClone(job)
    }
    if (cmd === 'job_get') return structuredClone(jobs[args.id])
    if (cmd === 'generation_item_image') return image + args.itemId
    if (cmd === 'generation_stop') { const j = jobs[args.id]!; j.status = 'stopping'; j.revision++; return }
    if (cmd === 'generation_retry_save') {
      const j = Object.values(jobs).find(j => j.items.some(i => i.id === args.itemId))!
      j.items.find(i => i.id === args.itemId)!.save_status = 'saved'; j.saved_count++; j.pending_save_count--; j.revision++
      if (j.saved_count === j.expected_count) j.status = 'completed'
      return
    }
    if (cmd === 'gallery_export') return true
    if (cmd === 'gallery_list') return { items: [], total: 0, page: 1, photo_total: 0 }
    if (cmd === 'update_about') return { current_version: '0.2.4', install_error: null }
    if (cmd === 'update_check') return null
    throw new Error(`Unexpected ${cmd}`)
  })
  mockIPC(ipc)
  async function changed(j: JobSnapshot) { j.revision++; events.change?.({ payload: { job_id: j.id, revision: j.revision, event_type: 'item_saved' } }); await flushPromises() }
  return { data, jobs, ipc, changed, failPlan: () => { failPlan = true } }
}
async function open() { const w = mount(App, { attachTo: document.body }); await flushPromises(); return w }
describe('persistent two-mode workspace', () => {
  it('quick creates one image without planning, prevents duplicates and survives page changes', async () => {
    const b = backend(), w = await open()
    await w.get('#prompt').setValue('请画十二张猫'); await w.get('form').trigger('submit'); await flushPromises()
    expect(b.ipc.mock.calls.filter(([c]) => c === 'plan_create')).toHaveLength(0); expect(b.data.jobs[0]!.expected_count).toBe(1)
    await w.get('form').trigger('submit'); await flushPromises()
    expect(b.ipc.mock.calls.filter(([c]) => c === 'generation_start_quick')).toHaveLength(1)
    expect(button(w, '规划生成').attributes('disabled')).toBeDefined()
    await button(w, '我的图库').trigger('click'); await flushPromises(); await button(w, '创作工作台').trigger('click'); await flushPromises()
    expect(w.get<HTMLTextAreaElement>('#prompt').element.value).toBe('请画十二张猫'); expect(w.text()).toContain('已生成 0 / 1 张')
  })
  it('requires explicit confirmation and preserves complete versions and failed feedback', async () => {
    const b = backend(), w = await open()
    await w.get('#prompt').setValue('三张猫'); await button(w, '规划生成').trigger('click'); await flushPromises(); await w.get('select').setValue(2)
    expect(w.text()).toContain('本次按所选数量 2 张规划')
    await w.get('form').trigger('submit'); await flushPromises(); expect(w.text()).toContain('第 1 版 · 共 2 张')
    for (const feedback of ['加一只小猫', '减少文字']) {
      await w.get('.plan-panel textarea').setValue(feedback); expect(button(w, '确认计划，生成 2 张').attributes('disabled')).toBeDefined()
      await button(w, '按意见修改计划').trigger('click'); await flushPromises()
    }
    expect(b.data.plans).toHaveLength(3); expect(b.data.jobs).toHaveLength(0)
    b.failPlan(); await w.get('.plan-panel textarea').setValue('保留失败反馈'); await button(w, '按意见修改计划').trigger('click'); await flushPromises()
    expect(w.get<HTMLTextAreaElement>('.plan-panel textarea').element.value).toBe('保留失败反馈'); expect(w.text()).toContain('第 3 版')
    await w.get('.plan-panel textarea').setValue(''); await button(w, '确认计划，生成 2 张').trigger('click'); await flushPromises()
    expect(b.data.jobs).toHaveLength(1); expect(w.findAll('[aria-label="任务照片"] button')).toHaveLength(2)
  })
  it('blocks stale plans as soon as inputs change', async () => {
    backend(); const w = await open()
    await w.get('#prompt').setValue('猫'); await button(w, '规划生成').trigger('click'); await flushPromises(); await w.get('form').trigger('submit'); await flushPromises()
    await w.get('#prompt').setValue('狗'); expect(button(w, '确认计划，生成 3 张').attributes('disabled')).toBeDefined(); expect(w.text()).toContain('输入或配置已变化')
  })
  it('flushes unapplied feedback when switching modes and restores the independent plan draft', async () => {
    const b = backend(), w = await open()
    await w.get('#prompt').setValue('猫'); await button(w, '规划生成').trigger('click'); await flushPromises()
    await w.get('form').trigger('submit'); await flushPromises()
    await w.get('.plan-panel textarea').setValue('尚未应用的反馈')
    await button(w, '快速生成').trigger('click'); await flushPromises()
    expect(b.data.drafts.find(d => d.mode === 'planned')?.feedback_draft).toBe('尚未应用的反馈')
    await button(w, '规划生成').trigger('click'); await flushPromises()
    expect(w.get<HTMLTextAreaElement>('.plan-panel textarea').element.value).toBe('尚未应用的反馈')
    expect(b.data.jobs).toHaveLength(0)
  })
  it('previews the first result before later slots finish and preserves manual selection', async () => {
    const b = backend(), w = await open()
    await w.get('#prompt').setValue('猫'); await button(w, '规划生成').trigger('click'); await flushPromises(); await w.get('form').trigger('submit'); await flushPromises()
    await button(w, '确认计划，生成 3 张').trigger('click'); await flushPromises(); const j = Object.values(b.jobs)[0]!
    Object.assign(j.items[0]!, { generation_status: 'generated', save_status: 'saved', artwork_id: j.items[0]!.id }); j.saved_count = j.generated_count = 1; j.items[1]!.generation_status = 'generating'; await b.changed(j)
    expect(w.get('.image-preview img').attributes('src')).toBe(image + j.items[0]!.id); expect(w.text()).toContain('已生成 1 / 3 张')
    await w.get('[aria-label="任务照片"] button').trigger('click')
    Object.assign(j.items[2]!, { generation_status: 'generated', save_status: 'saved', artwork_id: j.items[2]!.id }); j.saved_count = j.generated_count = 2; await b.changed(j)
    expect(w.get('.image-preview img').attributes('src')).toBe(image + j.items[0]!.id)
    await w.get('[aria-label="任务照片"]').trigger('keydown', { key: 'ArrowRight' }); expect(w.find('.image-preview').exists()).toBe(false); expect(w.text()).toContain('第 2 张 · 生成中')
    expect(w.findAll('button').some(b => b.text() === '导出此图')).toBe(false)
    await button(w, '停止后续生成').trigger('click'); await flushPromises(); expect(w.text()).toContain('当前图片完成后停止')
  })
  it('restores unfinished tasks and retries saving bytes without generating again', async () => {
    const b = backend(), w = await open()
    await w.get('#prompt').setValue('猫'); await w.get('form').trigger('submit'); await flushPromises(); const j = Object.values(b.jobs)[0]!
    Object.assign(j.items[0]!, { generation_status: 'generated', save_status: 'save_failed', artwork_id: j.items[0]!.id, error: '磁盘已满' }); j.status = 'paused'; j.generated_count = j.pending_save_count = 1; await b.changed(j); w.unmount()
    const restored = await open(); expect(restored.text()).toContain('磁盘已满'); expect(restored.get('.image-preview img').attributes('src')).toBe(image + j.items[0]!.id)
    await button(restored, '重试保存此图').trigger('click'); await flushPromises()
    expect(b.ipc.mock.calls.filter(([c]) => c === 'generation_start_quick')).toHaveLength(1); expect(b.ipc).toHaveBeenCalledWith('generation_retry_save', { itemId: j.items[0]!.id }); expect(restored.text()).toContain('已保存 1 张')
  })
  it('preserves selections through modes and catalog keyboard navigation', async () => {
    backend(); const w = await open()
    await w.get('#prompt').setValue('猫'); await w.get('[data-option="FA-001"]').trigger('click'); await button(w, '规划生成').trigger('click'); await flushPromises(); await button(w, '快速生成').trigger('click')
    expect(w.get<HTMLTextAreaElement>('#prompt').element.value).toBe('猫'); expect(w.get('[data-selection="style"]').text()).toContain(generationOptions.style[0]!.name)
    await w.get('.catalog-tabs').trigger('keydown', { key: 'ArrowRight' }); expect(w.get('#layout-tab').attributes('aria-selected')).toBe('true')
    await w.get('[data-selection="style"] .selection-link').trigger('click'); await flushPromises(); expect(document.activeElement).toBe(w.get('#style-tab').element)
    await w.get('[aria-label="清除风格选择"]').trigger('click'); expect(w.get('[data-selection="style"]').text()).toContain('自由生成')
  })
  it('disables browser generation without IPC', async () => {
    vi.spyOn(core, 'isTauri').mockReturnValue(false); const ipc = vi.fn(); mockIPC(ipc); const w = await open()
    await w.get('#prompt').setValue('猫'); await w.get('form').trigger('submit'); expect(w.get('button[type="submit"]').attributes('disabled')).toBeDefined(); expect(ipc).not.toHaveBeenCalled()
  })
  it('ignores regressive snapshots and accepts revision gaps as full state', () => {
    const current = { id: 'job', revision: 4 } as JobSnapshot
    expect(acceptSnapshot(current, { id: 'job', revision: 2 } as JobSnapshot)).toBe(current)
    expect(acceptSnapshot(current, { id: 'job', revision: 9 } as JobSnapshot).revision).toBe(9)
  })
})
