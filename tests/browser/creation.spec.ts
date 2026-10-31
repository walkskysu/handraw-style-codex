import { expect, test } from '@playwright/test'
import { readFileSync } from 'node:fs'

const image = `data:image/png;base64,${readFileSync('src-tauri/icons/icon.png').toString('base64')}`
for (const width of [1440, 360]) {
  test(`plan confirmation and incremental results remain accessible at ${width}px`, async ({ page }, testInfo) => {
    await page.setViewportSize({ width, height: 900 })
    await page.addInitScript(({ image }) => {
      const state: any = { drafts: [], plans: [], planning_jobs: [], jobs: [] }
      const snapshot = { style: null, layout: null, color: null, catalog_version: 'fixture' }
      Object.defineProperty(window, 'isTauri', { value: true })
      Object.defineProperty(window, 'creationFixture', { value: state })
      Object.defineProperty(window, '__TAURI_INTERNALS__', { value: {
        invoke: async (command: string, args: any) => {
          if (command === 'job_list') return structuredClone(state)
          if (command === 'draft_update') {
            const old = state.drafts.find((d: any) => d.id === args.input.id)
            const d = { ...old, id: old?.id ?? `draft-${state.drafts.length}`, ...args.input, snapshot, input_revision: 1, feedback_draft: args.input.feedback, current_plan_id: old?.current_plan_id ?? null, updated_at: '2026-10-27' }
            // Undefined input IDs must not replace the allocated ID.
            d.id = old?.id ?? `draft-${state.drafts.length}`
            state.drafts = [d, ...state.drafts.filter((v: any) => v.id !== d.id)]; return structuredClone(d)
          }
          if (command === 'plan_create') {
            const d = state.drafts.find((d: any) => d.id === args.draftId)
            const plan = { id: 'plan', draft_id: d.id, version: 1, input_revision: 1, content_hash: 'hash', confirmed_at: null, snapshot, content: { title: '猫咪变体', goal: '探索不同姿态', audience: '儿童', kind: 'variants', visual: '蓝色水彩', dimensions: '1:1', verification: '无事实主张', change_summary: '初始计划', items: [0, 1].map(position => ({ id: `entry-${position}`, position, title: `猫咪 ${position + 1}`, copy: '喵', description: '窗边的小猫', requirements: '独立图片' })) } }
            d.current_plan_id = plan.id; state.plans = [plan]
            state.planning_jobs = [{ id: 'planning', draft_id: d.id, status: 'completed', output_plan_id: plan.id, revision: 2 }]; return 'planning'
          }
          if (command === 'plan_confirm_and_start') {
            state.plans[0].confirmed_at = 'now'
            const job = { id: 'job', group_id: 'group', mode: 'planned', status: 'running', revision: 1, expected_count: 2, generated_count: 0, saved_count: 0, pending_save_count: 0, plan: state.plans[0], items: [0, 1].map(position => ({ id: `item-${position}`, position, title: `猫咪 ${position + 1}`, prompt: '仅输出本张独立图片', generation_status: position ? 'queued' : 'generating', save_status: 'none', attempt_no: 1, attempt_id: `attempt-${position}` })) }
            state.jobs = [job]; return structuredClone(job)
          }
          if (command === 'job_get') return structuredClone(state.jobs[0])
          if (command === 'generation_item_image') return image
          throw new Error(`Unexpected fixture command: ${command}`)
        },
      } })
    }, { image })
    await page.goto('/')
    await page.getByRole('textbox', { name: '你想画什么？' }).fill('两张猫咪变体')
    await page.getByRole('switch', { name: '快速生成' }).click()
    await page.getByRole('button', { name: '生成计划', exact: true }).click()
    await expect(page.getByRole('heading', { name: '创作计划 · 第 1 版 · 共 2 张' })).toBeVisible()
    expect(await page.evaluate(() => (window as any).creationFixture.jobs.length)).toBe(0)
    await page.getByRole('button', { name: '确认计划，生成 2 张' }).click()
    await expect(page.getByRole('navigation', { name: '任务照片' }).locator('button')).toHaveCount(2)
    await page.evaluate(() => {
      const j = (window as any).creationFixture.jobs[0]
      Object.assign(j.items[0], { generation_status: 'generated', save_status: 'saved', artwork_id: 'item-0' })
      j.items[1].generation_status = 'generating'; j.saved_count = j.generated_count = 1; j.revision++
    })
    await expect(page.locator('.job-preview')).toContainText('已保存 1 张', { timeout: 10000 })
    await page.locator('.job-preview img').scrollIntoViewIfNeeded()
    await expect(page.locator('.job-preview img')).toBeVisible()
    await page.screenshot({ path: testInfo.outputPath('incremental-result.png') })
    await page.getByRole('navigation', { name: '任务照片' }).locator('button').nth(1).click()
    await expect(page.locator('.job-preview')).toContainText('第 2 张 · 生成中')
    await expect(page.getByRole('button', { name: '导出此图', exact: true })).toHaveCount(0)
    expect(await page.evaluate(() => document.scrollingElement!.scrollWidth <= innerWidth)).toBe(true)
  })
}
