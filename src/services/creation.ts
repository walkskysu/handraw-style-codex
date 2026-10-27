import { invoke } from '@tauri-apps/api/core'
import type { GenerationSelections } from './generationOptions'
import type { Snapshot } from './gallery'
export type Mode = 'quick' | 'planned'
export interface Draft { id: string; mode: Mode; prompt: string; snapshot: Snapshot; requested_count: number | null; input_revision: number; current_plan_id: string | null; feedback_draft: string; updated_at: string }
export interface PlanItem { id: string; position: number; title: string; copy: string; description: string; requirements: string }
export interface Plan { id: string; draft_id: string; version: number; input_revision: number; input_hash: string; content_hash: string; snapshot: Snapshot; original_prompt: string; confirmed_at: string | null; content: { title: string; goal: string; audience: string; kind: string; visual: string; dimensions: string; verification: string; change_summary: string; items: PlanItem[] } }
export interface PlanningJob { id: string; draft_id: string; status: string; revision: number; error: string | null; output_plan_id: string | null }
export interface Job { id: string; group_id: string; draft_id: string; plan_id: string | null; mode: Mode; status: string; revision: number; expected_count: number; updated_at: string }
export interface Item { id: string; job_id: string; position: number; title: string; prompt: string; generation_status: string; save_status: string; attempt_no: number; attempt_id: string | null; artwork_id: string | null; error: string | null }
export interface JobSnapshot extends Job { items: Item[]; plan: Plan | null; generated_count: number; saved_count: number; available_count: number; failed_count: number; pending_save_count: number }
export interface Library { warnings?: string[]; drafts: Draft[]; plans: Plan[]; planning_jobs: PlanningJob[]; jobs: Job[] }
export interface Notice { job_id: string; revision: number; event_type: string }
export const active = (status: string) => ['queued', 'running', 'stopping'].includes(status)
export const label = (status: string) => ({ queued: '等待', running: '生成中', generating: '生成中', generated: '已生成', stopping: '当前图片完成后停止', paused: '已暂停', interrupted: '已中断', completed: '已完成', complete: '已完成', partial: '部分完成', failed: '失败', cancelled: '已停止', pending: '待保存', save_failed: '保存失败', saved: '已保存', missing: '文件不可用', none: '未生成', discarded: '已放弃暂存（未入库）' })[status] ?? status
export const saveDraft = (input: { id?: string; mode: Mode; prompt: string; selections: GenerationSelections; reuse_id?: string; requested_count: number | null; feedback: string; input_revision?: number }) => invoke<Draft>('draft_update', { input })
export const library = () => invoke<Library>('job_list')
export const snapshot = (id: string) => invoke<JobSnapshot>('job_get', { id })
export const planCreate = (d: Draft, basePlanId?: string) => invoke<string>('plan_create', { draftId: d.id, inputRevision: d.input_revision, basePlanId })
export const selectVersion = (draftId: string, planId: string) => invoke<Plan>('plan_select_version', { draftId, planId })
export const start = (d: Draft, key: string, plan?: Plan) => invoke<Job>(plan ? 'plan_confirm_and_start' : 'generation_start_quick', { draftId: d.id, inputRevision: d.input_revision, idempotencyKey: key, ...(plan ? { planId: plan.id, contentHash: plan.content_hash } : {}) })
export const stop = (id: string) => invoke<void>('generation_stop', { id })
export const resume = (j: Job, item?: Item) => invoke<Job>('generation_continue', { id: j.id, revision: j.revision, ...(item ? { itemId: item.id, attemptNo: item.attempt_no } : {}) })
export const retrySave = (itemId: string) => invoke<void>('generation_retry_save', { itemId })
export const itemImage = (itemId: string) => invoke<string>('generation_item_image', { itemId })

// Every accepted event triggers a full snapshot. Duplicate/old events do not move state backward.
export function acceptSnapshot(current: JobSnapshot | null, incoming: JobSnapshot): JobSnapshot {
  return current?.id === incoming.id && current.revision > incoming.revision ? current : incoming
}
