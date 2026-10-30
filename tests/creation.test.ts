import { afterEach, describe, expect, it, vi } from 'vitest'
import { clearMocks, mockIPC } from '@tauri-apps/api/mocks'
import { saveDraft, start, planCreate, resume, retrySave, type Draft, type Plan, type Job, type Item } from '../src/services/creation'

afterEach(clearMocks)
const draft = { id: 'draft', input_revision: 7 } as Draft
describe('creation IPC contracts', () => {
  it.each(Array.from({ length: 8 }, (_, mask) => mask))('persists literal prompts and independent visual selections (%i)', async mask => {
    const ipc = vi.fn().mockResolvedValue(draft); mockIPC(ipc)
    const input = { mode: 'planned' as const, prompt: '猫\n" & $(echo test) <script>', selections: { ...(mask & 1 ? { style: 'FH-044' } : {}), ...(mask & 2 ? { layout: 'SC-024' } : {}), ...(mask & 4 ? { color: 'C-36' } : {}) }, requested_count: 12, feedback: '' }
    await expect(saveDraft(input)).resolves.toEqual(draft)
    expect(ipc).toHaveBeenCalledExactlyOnceWith('draft_update', { input })
  })
  it('quick starts directly while planned admission carries current version, digest and idempotency key', async () => {
    const ipc = vi.fn(); mockIPC(ipc)
    await start(draft, 'one-click')
    expect(ipc).toHaveBeenLastCalledWith('generation_start_quick', { draftId: 'draft', inputRevision: 7, idempotencyKey: 'one-click' })
    await start(draft, 'confirmation', { id: 'plan-v2', content_hash: 'sha256' } as Plan)
    expect(ipc).toHaveBeenLastCalledWith('plan_confirm_and_start', { draftId: 'draft', inputRevision: 7, idempotencyKey: 'confirmation', planId: 'plan-v2', contentHash: 'sha256' })
  })
  it('revision uses its complete base version; retry uses the exact item and attempt', async () => {
    const ipc = vi.fn(); mockIPC(ipc)
    await planCreate(draft, 'base-plan')
    expect(ipc).toHaveBeenLastCalledWith('plan_create', { draftId: 'draft', inputRevision: 7, basePlanId: 'base-plan' })
    await resume({ id: 'job', revision: 23 } as Job, { id: 'item-2', attempt_no: 4 } as Item)
    expect(ipc).toHaveBeenLastCalledWith('generation_continue', { id: 'job', revision: 23, itemId: 'item-2', attemptNo: 4 })
    await retrySave('item-2')
    expect(ipc).toHaveBeenLastCalledWith('generation_retry_save', { itemId: 'item-2' })
  })
  it('preserves actionable backend errors', async () => {
    mockIPC(() => Promise.reject('计划已过期，请更新计划'))
    await expect(start(draft, 'click', { id: 'old', content_hash: 'old' } as Plan)).rejects.toBe('计划已过期，请更新计划')
  })
})
