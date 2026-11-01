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
  let serial = 0, failPlan = false, failSave = false, lostStart = false
  let waitForSave: Promise<void> | undefined
  let waitForSnapshot: Promise<void> | undefined
  const ipc = vi.fn(async (cmd: string, args: any) => {
    if (cmd === 'job_list') return structuredClone(data)
    if (cmd === 'draft_update') {
      await waitForSave
      if (failSave) throw new Error('磁盘已满，草稿保存失败')
      const input = args.input, old = data.drafts.find(d => d.id === input.id)
      const changed = old && (old.prompt !== input.prompt || old.mode !== input.mode || old.requested_count !== input.requested_count)
      const d: Draft = { id: old?.id ?? `draft-${++serial}`, mode: input.mode, prompt: input.prompt, snapshot: visual, requested_count: input.requested_count, input_revision: old ? old.input_revision + (changed ? 1 : 0) : 1, current_plan_id: old?.current_plan_id ?? null, feedback_draft: input.feedback, updated_at: '2026-10-31T12:00:00Z' }
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
      job.input = structuredClone(d)
      jobs[id] = job; data.jobs.unshift(job)
      if (lostStart) throw new Error('响应丢失')
      return structuredClone(job)
    }
    if (cmd === 'job_get') { await waitForSnapshot; return structuredClone(jobs[args.id]) }
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
  return { data, jobs, ipc, changed, failPlan: () => { failPlan = true }, failSave: (value: boolean) => { failSave = value }, loseStart: () => { lostStart = true }, delaySave: (wait: Promise<void>) => { waitForSave = wait }, delaySnapshot: (wait: Promise<void>) => { waitForSnapshot = wait } }
}
async function open() { const w = mount(App, { attachTo: document.body }); await flushPromises(); return w }
describe('persistent two-mode workspace', () => {
  it.each([false, true])('starts with editable previous input and generates a separate job without changing history (edit: %s)', async (edit) => {
    const b = backend(), w = await open()
    await w.get('#prompt').setValue('窗边的小猫')
    await w.get('.creation-panel form').trigger('submit'); await flushPromises()
    const j = Object.values(b.jobs)[0]!
    j.status = 'completed'
    j.input!.snapshot = { ...visual, style: generationOptions.style[0]!, layout: generationOptions.layout[0]!, color: generationOptions.color[0]! }
    const original = structuredClone(j), originalDraft = structuredClone(b.data.drafts[0]!)
    w.unmount(); b.ipc.mockClear()

    const restored = await open()
    expect(restored.get<HTMLTextAreaElement>('#prompt').element.value).toBe('窗边的小猫')
    expect(restored.get('#prompt').attributes('disabled')).toBeUndefined()
    expect(restored.get('[role="switch"]').attributes('disabled')).toBeUndefined()
    expect(restored.get('.catalog-panel fieldset').attributes('disabled')).toBeUndefined()
    for (const key of ['style', 'layout', 'color'] as const) {
      expect(restored.get(`[data-selection="${key}"]`).text()).toContain(j.input!.snapshot[key]!.id)
    }
    expect(restored.get('.generation-action button[type="submit"]').text()).toBe('生成图片 ↗')
    expect(restored.get('.generation-action button[type="submit"]').attributes('disabled')).toBeUndefined()
    expect(restored.text()).not.toContain('以此新建')
    expect(restored.find('.job-preview').exists()).toBe(false)
    expect(restored.get<HTMLDialogElement>('#preview-dialog').element.open).toBe(false)
    await new Promise(resolve => setTimeout(resolve, 400)); await flushPromises()
    expect(b.ipc.mock.calls.map(([cmd]) => cmd)).not.toContain('draft_update')
    expect(b.ipc.mock.calls.every(([cmd]) => ['job_list', 'job_get', 'update_about'].includes(cmd))).toBe(true)
    expect(b.data.jobs).toHaveLength(1)
    // A normal refresh must not select the historical execution again.
    await b.changed(j)
    expect(restored.get('#prompt').attributes('disabled')).toBeUndefined()
    const prompt = edit ? '窗边的小狗' : '窗边的小猫'
    if (edit) await restored.get('#prompt').setValue(prompt)
    await restored.get('.creation-panel form').trigger('submit'); await flushPromises()
    expect(b.ipc).toHaveBeenCalledWith('draft_update', { input: expect.objectContaining({ id: undefined, prompt, reuse_job_id: j.id, selections: { style: j.input!.snapshot.style!.id, layout: j.input!.snapshot.layout!.id, color: j.input!.snapshot.color!.id } }) })
    expect(b.data.jobs).toHaveLength(2)
    expect(b.data.jobs[0]!.draft_id).not.toBe(j.draft_id)
    expect(b.data.drafts.find(d => d.id === originalDraft.id)).toEqual(originalDraft)
    expect(j).toEqual({ ...original, revision: original.revision + 1 })
  })
  it('restores planned input and count without reusing its confirmed plan', async () => {
    const b = backend(), w = await open()
    await w.get('#prompt').setValue('两张猫')
    await w.get('[role="switch"]').trigger('click'); await flushPromises()
    await w.get('.generation-action select').setValue(2)
    await w.get('.creation-panel form').trigger('submit'); await flushPromises()
    await button(w, '确认计划，生成 2 张').trigger('click'); await flushPromises()
    Object.values(b.jobs)[0]!.status = 'completed'
    w.unmount(); b.ipc.mockClear()

    const restored = await open()
    expect(restored.get<HTMLTextAreaElement>('#prompt').element.value).toBe('两张猫')
    expect(restored.get('[role="switch"]').attributes('aria-checked')).toBe('false')
    expect(restored.get<HTMLSelectElement>('.generation-action select').element.value).toBe('2')
    expect(restored.get('.generation-action button[type="submit"]').text()).toBe('生成计划')
    expect(restored.find('.plan-panel').exists()).toBe(false)
    expect(restored.find('.job-preview').exists()).toBe(false)
    expect(b.ipc.mock.calls.every(([cmd]) => ['job_list', 'job_get', 'update_about'].includes(cmd))).toBe(true)
    await restored.get('.creation-panel form').trigger('submit'); await flushPromises()
    expect(b.data.drafts).toHaveLength(2)
    expect(b.data.plans).toHaveLength(2)
    expect(b.data.plans[1]!.confirmed_at).toBeNull()
    expect(b.data.jobs).toHaveLength(1)
  })
  it('does not let a late startup snapshot overwrite a new creation', async () => {
    const b = backend()
    const d: Draft = { id: 'old-draft', prompt: '历史作品', mode: 'quick', snapshot: visual, requested_count: 1, input_revision: 1, current_plan_id: null, feedback_draft: '', updated_at: '2026-10-31T12:00:00Z' }
    const j: JobSnapshot = { id: 'old-job', draft_id: d.id, input: d, group_id: 'old-group', plan_id: null, mode: 'quick', status: 'completed', revision: 1, expected_count: 1, items: [], plan: null, generated_count: 1, saved_count: 1, available_count: 1, failed_count: 0, pending_save_count: 0, updated_at: d.updated_at }
    b.data.drafts.push(d); b.data.jobs.push(j); b.jobs[j.id] = j
    let release!: () => void
    b.delaySnapshot(new Promise<void>(resolve => { release = resolve }))
    const w = await open()
    expect(b.ipc.mock.calls.some(([cmd]) => cmd === 'job_get')).toBe(true)
    await w.get('#prompt').setValue('刚刚输入的新创作')
    release(); await flushPromises()
    expect(w.get<HTMLTextAreaElement>('#prompt').element.value).toBe('刚刚输入的新创作')
    expect(w.find('.job-preview').exists()).toBe(false)
    expect(w.get('#prompt').attributes('disabled')).toBeUndefined()
  })
  it('serializes slow autosaves and flushes the newest input before switching tasks', async () => {
    const b = backend(), w = await open()
    let release!: () => void
    b.delaySave(new Promise<void>(resolve => { release = resolve }))
    await w.get('#task-prompt').setValue('第一段输入')
    await new Promise(resolve => setTimeout(resolve, 400)); await flushPromises()
    await w.get('#task-prompt').setValue('等待保存时继续编辑')
    await button(w, '← 全部任务').trigger('click')
    expect(w.find('.task-library').exists()).toBe(false)
    release(); await flushPromises()
    expect(b.data.drafts).toHaveLength(1)
    expect(b.data.drafts[0]?.prompt).toBe('等待保存时继续编辑')
    expect(w.get('.task-card').text()).toContain('等待保存时继续编辑')
  })
  it('keeps the selected editor when saving fails during task navigation', async () => {
    const b = backend(), w = await open()
    b.failSave(true)
    await w.get('#task-prompt').setValue('不能丢失的编辑')
    await button(w, '← 全部任务').trigger('click'); await flushPromises()
    expect(w.find('.task-library').exists()).toBe(false)
    expect(w.get<HTMLTextAreaElement>('#task-prompt').element.value).toBe('不能丢失的编辑')
    expect(w.text()).toContain('保存失败')
    b.failSave(false)
    await button(w, '← 全部任务').trigger('click'); await flushPromises()
    expect(w.find('.task-library').exists()).toBe(true)
    expect(b.data.drafts[0]?.prompt).toBe('不能丢失的编辑')
  })
  it('only shows creation and gallery navigation while generation still opens the task panel', async () => {
    const b = backend(), w = await open()
    expect(w.get('.brand').text()).toBe('IdeaCanvas')
    expect(w.findAll('main > section')).toHaveLength(2)
    expect(w.get<HTMLDialogElement>('#preview-dialog').element.open).toBe(false)
    await w.get('#prompt').setValue('窗边的小猫')
    await w.get('[data-option="FA-001"]').trigger('click')
    expect(w.findAll('.app-navigation button').map(b => b.text())).toEqual(['创作', '我的图库'])
    await button(w, '我的图库').trigger('click'); await flushPromises()
    expect(button(w, '我的图库').attributes('aria-current')).toBe('page')
    expect(w.findAll('.app-navigation button').map(b => b.text())).toEqual(['创作', '我的图库'])
    await button(w, '创作').trigger('click'); await flushPromises()
    expect(button(w, '创作').attributes('aria-current')).toBe('page')
    expect(w.get('[data-option="FA-001"]').attributes('aria-pressed')).toBe('true')
    await w.get('.creation-panel form').trigger('submit'); await flushPromises()
    expect(w.get<HTMLDialogElement>('#preview-dialog').element.open).toBe(true)
    expect(w.findAll('.app-navigation button').map(b => b.text())).toEqual(['创作', '我的图库'])
    await w.get('.close-preview').trigger('click')
    expect(w.get<HTMLDialogElement>('#preview-dialog').element.open).toBe(false)
    expect(document.activeElement).toBe(button(w, '创作').element)
    expect(w.get<HTMLTextAreaElement>('#prompt').element.value).toBe('窗边的小猫')
    expect(b.data.jobs).toHaveLength(1)
    expect(b.data.jobs[0]!.status).toBe('running')
  })
  it('returns focus to the generation button when submission fails', async () => {
    const b = backend(), w = await open()
    b.failSave(true)
    await w.get('#prompt').setValue('保留失败的创作')
    await w.get('.creation-panel form').trigger('submit'); await flushPromises()
    expect(w.get<HTMLDialogElement>('#preview-dialog').element.open).toBe(true)
    expect(w.text()).toContain('保存失败')
    await w.get('.close-preview').trigger('click')
    expect(document.activeElement).toBe(w.get('.creation-panel button[type="submit"]').element)
    expect(b.data.jobs).toHaveLength(0)
  })
  it('toggles and remembers quick mode using the description-heading switch', async () => {
    backend(); const w = await open()
    expect(w.get('.creation-heading [role="switch"]').attributes('aria-checked')).toBe('true')
    await w.get('[role="switch"]').trigger('click'); await flushPromises()
    expect(w.get('[role="switch"]').attributes('aria-checked')).toBe('false')
    expect(localStorage.getItem('creation-mode')).toBe('planned')
    expect(w.find('.generation-action select').exists()).toBe(true)
    await w.get('[role="switch"]').trigger('click'); await flushPromises()
    expect(w.find('.generation-action select').exists()).toBe(false)
    expect(localStorage.getItem('creation-mode')).toBe('quick')
  })
  it('quick creates one image without planning, prevents duplicates and survives page changes', async () => {
    const b = backend(), w = await open()
    await w.get('#prompt').setValue('请画十二张猫'); await w.get('form').trigger('submit'); await flushPromises()
    expect(b.ipc.mock.calls.filter(([c]) => c === 'plan_create')).toHaveLength(0); expect(b.data.jobs[0]!.expected_count).toBe(1)
    await w.get('form').trigger('submit'); await flushPromises()
    expect(b.ipc.mock.calls.filter(([c]) => c === 'generation_start_quick')).toHaveLength(1)
    expect(w.get('[role="switch"]').attributes('disabled')).toBeDefined()
    await button(w, '我的图库').trigger('click'); await flushPromises(); await button(w, '创作').trigger('click'); await flushPromises()
    expect(w.get<HTMLTextAreaElement>('#prompt').element.value).toBe('请画十二张猫'); expect(w.text()).toContain('已生成 0 / 1 张')
  })
  it('requires explicit confirmation and preserves complete versions and failed feedback', async () => {
    const b = backend(), w = await open()
    await w.get('#prompt').setValue('三张猫'); await w.get('[role="switch"]').trigger('click'); await flushPromises(); await w.get('select').setValue(2)
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
    await w.get('#prompt').setValue('猫'); await w.get('[role="switch"]').trigger('click'); await flushPromises(); await w.get('form').trigger('submit'); await flushPromises()
    await w.get('#prompt').setValue('狗'); expect(button(w, '确认计划，生成 3 张').attributes('disabled')).toBeDefined(); expect(w.text()).toContain('输入或配置已变化')
  })
  it('keeps one draft and feedback across mode changes, invalidating its old plan', async () => {
    const b = backend(), w = await open()
    await w.get('#prompt').setValue('猫'); await w.get('[role="switch"]').trigger('click'); await flushPromises()
    await w.get('form').trigger('submit'); await flushPromises()
    await w.get('.plan-panel textarea').setValue('尚未应用的反馈')
    await w.get('[role="switch"]').trigger('click'); await flushPromises()
    expect(b.data.drafts).toHaveLength(1)
    expect(b.data.drafts[0]?.mode).toBe('quick')
    expect(b.data.drafts[0]?.feedback_draft).toBe('尚未应用的反馈')
    await w.get('[role="switch"]').trigger('click'); await flushPromises()
    expect(w.get<HTMLTextAreaElement>('.plan-panel textarea').element.value).toBe('尚未应用的反馈')
    expect(b.data.drafts).toHaveLength(1)
    expect(button(w, '确认计划，生成 3 张').attributes('disabled')).toBeDefined()
    expect(w.text()).toContain('旧计划需要更新')
    expect(b.data.jobs).toHaveLength(0)
  })
  it('previews the first result before later slots finish and preserves manual selection', async () => {
    const b = backend(), w = await open()
    await w.get('#prompt').setValue('猫'); await w.get('[role="switch"]').trigger('click'); await flushPromises(); await w.get('form').trigger('submit'); await flushPromises()
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
    const restored = await open()
    expect(restored.get('.generation-action button[type="submit"]').text()).toBe('生成图片 ↗')
    expect(restored.get('#prompt').attributes('disabled')).toBeUndefined()
    expect(restored.find('.job-preview').exists()).toBe(false)
    await button(restored, '← 全部任务').trigger('click'); await flushPromises()
    expect(restored.findAll('.task-card')).toHaveLength(1)
    await restored.get('.task-card').trigger('click'); await flushPromises()
    expect(restored.get('#prompt').attributes('disabled')).toBeDefined()
    expect(restored.text()).toContain('磁盘已满'); expect(restored.get('.image-preview img').attributes('src')).toBe(image + j.items[0]!.id)
    await button(restored, '重试保存此图').trigger('click'); await flushPromises()
    expect(b.ipc.mock.calls.filter(([c]) => c === 'generation_start_quick')).toHaveLength(1); expect(b.ipc).toHaveBeenCalledWith('generation_retry_save', { itemId: j.items[0]!.id }); expect(restored.text()).toContain('已保存 1 张')
  })
  it('preserves selections through modes and catalog keyboard navigation', async () => {
    backend(); const w = await open()
    await w.get('#prompt').setValue('猫'); await w.get('[data-option="FA-001"]').trigger('click'); await w.get('[role="switch"]').trigger('click'); await flushPromises(); await w.get('[role="switch"]').trigger('click')
    expect(w.get<HTMLTextAreaElement>('#prompt').element.value).toBe('猫'); expect(w.get('[data-selection="style"]').text()).toContain(generationOptions.style[0]!.id)
    await w.get('.catalog-tabs').trigger('keydown', { key: 'ArrowRight' }); expect(w.get('#layout-tab').attributes('aria-selected')).toBe('true')
    await w.get('[data-selection="style"] .selection-link').trigger('click'); await flushPromises(); expect(document.activeElement).toBe(w.get('#style-tab').element)
    await w.get('[aria-label="清除风格选择"]').trigger('click'); expect(w.get('[data-selection="style"]').text()).toContain('自动')
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
  it('does not persist blank tasks, autosaves real input, and retries failed saves without losing edits', async () => {
    const b = backend(), w = await open()
    await w.get('[role="switch"]').trigger('click'); await flushPromises()
    expect(b.data.drafts).toHaveLength(0)
    b.failSave(true)
    await w.get('#task-prompt').setValue('保存失败时保留这段内容')
    await new Promise(resolve => setTimeout(resolve, 400)); await flushPromises()
    expect(w.text()).toContain('磁盘已满，草稿保存失败')
    expect(w.get<HTMLTextAreaElement>('#task-prompt').element.value).toBe('保存失败时保留这段内容')
    expect(b.data.drafts).toHaveLength(0)
    b.failSave(false); await button(w, '重试保存').trigger('click'); await flushPromises()
    expect(b.data.drafts).toHaveLength(1)
    expect(w.text()).toContain('已自动保存')
    await w.get('#task-prompt').setValue('继续编辑同一个任务')
    await new Promise(resolve => setTimeout(resolve, 400)); await flushPromises()
    expect(b.data.drafts).toHaveLength(1)
    expect(b.data.drafts[0]?.prompt).toBe('继续编辑同一个任务')
  })
  it('opens a draft inside the panel with no unrelated result, and combines list search with filters', async () => {
    const b = backend(), w = await open()
    await w.get('#prompt').setValue('已经完成的猫')
    await w.get('.creation-panel form').trigger('submit'); await flushPromises()
    const j = Object.values(b.jobs)[0]!
    Object.assign(j.items[0]!, { generation_status: 'generated', save_status: 'saved', artwork_id: 'cat' })
    Object.assign(j, { status: 'completed', generated_count: 1, saved_count: 1 })
    b.data.drafts.push({ ...structuredClone(b.data.drafts[0]!), id: 'forest', prompt: '森林咖啡馆', current_plan_id: null })
    await b.changed(j)
    await button(w, '← 全部任务').trigger('click'); await flushPromises()
    expect(w.findAll('.task-card')).toHaveLength(2)
    await w.get('[aria-label="搜索创作任务"]').setValue('森林')
    await button(w, '草稿').trigger('click')
    expect(w.findAll('.task-card')).toHaveLength(1)
    await w.get('.task-card').trigger('click'); await flushPromises()
    expect(w.get<HTMLTextAreaElement>('#task-prompt').element.value).toBe('森林咖啡馆')
    expect(w.find('.job-preview').exists()).toBe(false)
    expect(w.find('.image-preview').exists()).toBe(false)
    await w.get('#task-prompt').setValue('森林咖啡馆新版')
    await button(w, '← 全部任务').trigger('click'); await flushPromises()
    expect(b.data.drafts.find(d => d.id === 'forest')?.prompt).toBe('森林咖啡馆新版')
  })
  it('freezes executed input and explicitly copies only input into a new draft', async () => {
    const b = backend(), w = await open()
    await w.get('#prompt').setValue('两张猫'); await w.get('[role="switch"]').trigger('click'); await flushPromises()
    await w.get('.generation-action select').setValue(2)
    await w.get('.creation-panel form').trigger('submit'); await flushPromises()
    await button(w, '确认计划，生成 2 张').trigger('click'); await flushPromises()
    const original = structuredClone(b.data.drafts[0]!)
    const j = Object.values(b.jobs)[0]!
    j.status = 'completed'; j.saved_count = j.generated_count = 2; await b.changed(j)
    expect(w.get('#prompt').attributes('disabled')).toBeDefined()
    expect(w.get('.catalog-panel fieldset').attributes('disabled')).toBeDefined()
    await w.get('.creation-panel form').trigger('submit'); await flushPromises()
    expect(b.data.jobs).toHaveLength(1)
    await button(w, '以此新建').trigger('click'); await flushPromises()
    expect(b.data.drafts).toHaveLength(2)
    expect(b.data.drafts[0]).toMatchObject({ prompt: original.prompt, mode: 'planned', requested_count: 2, current_plan_id: null })
    expect(b.data.drafts[0]?.id).not.toBe(original.id)
    expect(b.data.drafts.find(d => d.id === original.id)).toEqual(original)
    expect(w.find('.job-preview').exists()).toBe(false)
    expect(w.find('.plan-panel').exists()).toBe(false)
    expect(w.get('#prompt').attributes('disabled')).toBeUndefined()
    expect(b.ipc.mock.calls.filter(([cmd]) => cmd === 'plan_confirm_and_start')).toHaveLength(1)
  })
  it('recovers an admitted execution after its response is lost without creating another run', async () => {
    const b = backend(), w = await open()
    b.loseStart()
    await w.get('#prompt').setValue('找回本次执行')
    await w.get('.creation-panel form').trigger('submit'); await flushPromises()
    expect(w.text()).toContain('响应丢失')
    const j = Object.values(b.jobs)[0]!
    await b.changed(j)
    expect(w.find('.job-preview').exists()).toBe(true)
    expect(w.get('#prompt').attributes('disabled')).toBeDefined()
    expect(b.data.jobs).toHaveLength(1)
    expect(b.data.drafts).toHaveLength(1)
  })
})
