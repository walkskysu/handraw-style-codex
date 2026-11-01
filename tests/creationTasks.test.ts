import { describe, expect, it } from 'vitest'
import { canDeleteTask, creationTasks, filterTasks, taskStatus, taskTime } from '../src/services/creationTasks'
import type { Draft, JobSnapshot, Library, Plan, PlanningJob } from '../src/services/creation'

const draft = (id: string, prompt = id): Draft => ({ id, prompt, mode: 'quick', snapshot: { style: null, layout: null, color: null, catalog_version: 'v1' }, requested_count: null, input_revision: 1, current_plan_id: null, feedback_draft: '', updated_at: '2026-10-31T10:00:00Z' })
const job = (id: string, input: Draft): JobSnapshot => ({ id, input, draft_id: input.id, group_id: id, plan_id: null, mode: input.mode, status: 'completed', revision: 2, expected_count: 1, items: [], plan: null, generated_count: 1, saved_count: 1, available_count: 1, failed_count: 0, pending_save_count: 0, updated_at: '2026-10-31T12:00:00Z' })
describe('unified creation records', () => {
  it('offers deletion for drafts and idle tasks, never for active workers even when saving needs attention', () => {
    const d = draft('draft')
    const data: Library = { drafts: [d], jobs: [], plans: [], planning_jobs: [] }
    expect(canDeleteTask(creationTasks(data)[0]!)).toBe(true)
    data.planning_jobs = [{ draft_id: d.id, status: 'running' } as PlanningJob]
    expect(canDeleteTask(creationTasks(data)[0]!)).toBe(false)
    data.planning_jobs = []
    const j = job('job', d)
    data.jobs = [j]
    expect(canDeleteTask(creationTasks(data)[0]!)).toBe(true)
    for (const status of ['queued', 'running', 'stopping']) {
      Object.assign(j, { status, pending_save_count: 1, saved_count: 0 })
      expect(canDeleteTask(creationTasks(data)[0]!)).toBe(false)
    }
    for (const status of ['paused', 'failed', 'interrupted', 'cancelled', 'partial']) {
      j.status = status
      expect(canDeleteTask(creationTasks(data)[0]!)).toBe(true)
    }
  })
  it('deduplicates linked drafts while preserving every historical run and its own input', () => {
    const d = draft('shared', 'current edited description')
    const first = job('first', { ...d, prompt: 'original cat' })
    const second = job('second', { ...d, prompt: 'later dog' })
    second.updated_at = '2026-10-31T13:00:00Z'
    const data: Library = { drafts: [d, draft('unexecuted')], jobs: [first, second], plans: [], planning_jobs: [] }
    const tasks = creationTasks(data)
    expect(tasks.map(t => t.id)).toEqual(['second', 'first', 'unexecuted'])
    expect(tasks.map(t => t.prompt)).toEqual(['later dog', 'original cat', 'unexecuted'])
    expect(filterTasks(tasks, 'CAT', 'completed').map(t => t.id)).toEqual(['first'])
    expect(filterTasks(tasks, 'cat', 'draft')).toEqual([])
  })
  it('places planning, confirmation, interruptions and save failures in the correct filters', () => {
    const d = { ...draft('planned', '森林'), mode: 'planned' as const, current_plan_id: 'p' }
    const p = { id: 'p', content: { title: '咖啡馆海报', items: [{}, {}] } } as Plan
    const data: Library = { drafts: [d], jobs: [], plans: [p], planning_jobs: [] }
    expect(creationTasks(data)[0]).toMatchObject({ title: '咖啡馆海报', count: 2, status: '待确认', filter: 'attention' })
    expect(filterTasks(creationTasks(data), '森林', 'attention')).toHaveLength(1)
    data.planning_jobs = [{ draft_id: d.id, status: 'running', updated_at: '2026-10-31T14:00:00Z' } as PlanningJob]
    expect(creationTasks(data)[0]).toMatchObject({ status: '规划中', filter: 'active', updatedAt: '2026-10-31T14:00:00Z' })
    data.planning_jobs[0]!.status = 'interrupted'; data.planning_jobs[0]!.error = 'interrupted'
    expect(creationTasks(data)[0]?.filter).toBe('attention')
    const j = job('paused', d)
    Object.assign(j, { status: 'paused', saved_count: 0, pending_save_count: 1 })
    data.jobs = [j]
    expect(creationTasks(data)[0]).toMatchObject({ status: '待保存', filter: 'attention' })
    // Even if a disk failure prevented the job status update, saving needs attention.
    j.status = 'running'
    expect(creationTasks(data)[0]).toMatchObject({ status: '待保存', filter: 'attention' })
    j.status = 'paused'
    j.pending_save_count = 0; j.failed_count = 1
    expect(taskStatus(j)).toBe('失败')
    j.failed_count = 0; j.status = 'interrupted'
    expect(taskStatus(j)).toBe('已中断')
  })
  it('only marks all-saved jobs completed and formats local, compact timestamps', () => {
    const j = job('one', draft('d'))
    j.saved_count = 0
    expect(taskStatus(j)).not.toBe('已完成')
    expect(taskTime('2026-10-31T10:40:00Z')).toContain('10月31日')
    expect(taskTime('2026-10-31T10:40:00Z')).not.toContain('2026')
    expect(taskTime('invalid')).toBe('时间未知')
  })
})
