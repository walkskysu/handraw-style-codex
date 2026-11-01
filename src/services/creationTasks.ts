import { active, type Draft, type JobSnapshot, type Library, type Mode } from './creation'

export type TaskFilter = 'all' | 'draft' | 'active' | 'attention' | 'completed'
export interface CreationTask {
  id: string; title: string; prompt: string; mode: Mode; count: number | null
  status: string; filter: TaskFilter; updatedAt: string; draft?: Draft; job?: JobSnapshot
}
export function taskStatus(job: JobSnapshot) {
  if (job.pending_save_count) return '待保存'
  if (active(job.status)) return job.status === 'stopping' ? '正在停止' : '生成中'
  if (job.saved_count === job.expected_count) return '已完成'
  if (job.failed_count || job.status === 'failed') return '失败'
  return job.status === 'interrupted' ? '已中断' : '已暂停'
}
export function creationTasks(data: Library): CreationTask[] {
  const linked = new Set(data.jobs.map(j => j.draft_id))
  const jobs: CreationTask[] = data.jobs.map(job => {
    const prompt = job.input?.prompt ?? job.plan?.original_prompt ?? job.items[0]?.prompt ?? ''
    const status = taskStatus(job)
    return { id: job.id, job, title: job.plan?.content.title || prompt.slice(0, 80) || '未命名创作', prompt,
      mode: job.mode, count: job.expected_count, status,
      filter: status === '已完成' ? 'completed' : job.pending_save_count ? 'attention' : active(job.status) ? 'active' : 'attention', updatedAt: job.updated_at }
  })
  const drafts: CreationTask[] = data.drafts.filter(d => !linked.has(d.id)).map(draft => {
    const plan = data.plans.find(p => p.id === draft.current_plan_id)
    const planning = data.planning_jobs.filter(p => p.draft_id === draft.id)
      .sort((a, b) => (b.updated_at ?? '').localeCompare(a.updated_at ?? ''))[0]
    const status = planning && active(planning.status) ? '规划中' : planning?.error ? '失败' : plan ? '待确认' : '草稿'
    return { id: draft.id, draft, title: plan?.content.title || draft.prompt.slice(0, 80) || '未命名创作', prompt: draft.prompt,
      mode: draft.mode, count: draft.mode === 'quick' ? 1 : plan?.content.items.length ?? draft.requested_count,
      status, filter: status === '草稿' ? 'draft' : status === '规划中' ? 'active' : 'attention',
      updatedAt: [draft.updated_at, planning?.updated_at ?? ''].sort().at(-1)! }
  })
  return [...jobs, ...drafts].sort((a, b) => b.updatedAt.localeCompare(a.updatedAt))
}
export function filterTasks(tasks: CreationTask[], search: string, filter: TaskFilter) {
  const term = search.trim().toLocaleLowerCase()
  return tasks.filter(t => (filter === 'all' || t.filter === filter) && `${t.title} ${t.prompt}`.toLocaleLowerCase().includes(term))
}
export function taskTime(value: string) {
  const date = new Date(value)
  return Number.isNaN(date.getTime()) ? '时间未知' : date.toLocaleString('zh-CN', { month: 'long', day: 'numeric', hour: '2-digit', minute: '2-digit', hour12: false })
}
export function canDeleteTask(task: CreationTask) {
  return task.filter !== 'active' && (!task.job || !active(task.job.status))
}
