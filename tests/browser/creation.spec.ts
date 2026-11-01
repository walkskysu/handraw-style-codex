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
          if (command === 'task_delete') {
            const draftId = args.kind === 'draft' ? args.id : state.jobs.find((j: any) => j.id === args.id).draft_id
            state.jobs = state.jobs.filter((j: any) => j.id !== args.id)
            state.drafts = state.drafts.filter((d: any) => d.id !== draftId)
            state.plans = state.plans.filter((p: any) => p.draft_id !== draftId)
            state.planning_jobs = state.planning_jobs.filter((p: any) => p.draft_id !== draftId)
            return
          }
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
            const d = state.drafts.find((d: any) => d.id === args.draftId)
            state.plans[0].confirmed_at = 'now'
            const job = { id: 'job', group_id: 'group', mode: 'planned', status: 'running', revision: 1, expected_count: 2, generated_count: 0, saved_count: 0, pending_save_count: 0, plan: state.plans[0], items: [0, 1].map(position => ({ id: `item-${position}`, position, title: `猫咪 ${position + 1}`, prompt: '仅输出本张独立图片', generation_status: position ? 'queued' : 'generating', save_status: 'none', attempt_no: 1, attempt_id: `attempt-${position}` })) }
            Object.assign(job, { input: structuredClone(d), draft_id: d.id, updated_at: '2026-10-31T12:00:00Z' })
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
    await page.locator('.job-preview .image-preview img').scrollIntoViewIfNeeded()
    await expect(page.locator('.job-preview .image-preview img')).toBeVisible()
    expect(await page.locator('.image-preview img').evaluate(img => img.getBoundingClientRect().height)).toBeLessThanOrEqual(480)
    await page.getByRole('button', { name: '放大查看', exact: true }).click()
    await expect(page.getByRole('dialog', { name: '放大查看图片' })).toBeVisible()
    await page.getByRole('button', { name: '关闭放大' }).click()
    await page.screenshot({ path: testInfo.outputPath('incremental-result.png') })
    await page.getByRole('navigation', { name: '任务照片' }).locator('button').nth(1).click()
    await expect(page.locator('.job-preview')).toContainText('第 2 张 · 生成中')
    await expect(page.getByRole('button', { name: '导出此图', exact: true })).toHaveCount(0)
    expect(await page.evaluate(() => document.scrollingElement!.scrollWidth <= innerWidth)).toBe(true)
    await page.getByRole('button', { name: '全部任务' }).click()
    await expect(page.locator('.task-card')).toHaveCount(1)
    await expect(page.getByRole('button', { name: '待处理', exact: true })).toBeVisible()
    await page.getByRole('searchbox', { name: '搜索创作任务' }).fill('猫咪')
    await page.getByRole('button', { name: '进行中', exact: true }).click()
    await expect(page.locator('.task-card')).toHaveCount(1)
    await page.screenshot({ path: testInfo.outputPath('creation-task-list.png') })
    await page.locator('.task-card').click()
    await page.locator('.task-detail').getByRole('button', { name: '以此新建' }).click()
    await expect(page.locator('#task-prompt')).toHaveValue('两张猫咪变体')
    await expect(page.locator('.job-preview')).toHaveCount(0)
    await expect(page.locator('.plan-panel')).toHaveCount(0)
    expect(await page.evaluate(() => (window as any).creationFixture.drafts.length)).toBe(2)
    await page.screenshot({ path: testInfo.outputPath('creation-draft-editor.png') })
    expect(await page.locator('.preview-panel').evaluate(el => el.scrollWidth <= el.clientWidth)).toBe(true)
    await page.getByRole('button', { name: '全部任务' }).click()
    await page.getByRole('button', { name: '全部', exact: true }).click()
    await expect(page.locator('.task-card')).toHaveCount(2)
    // Running task has no delete action. The draft menu never opens the editor.
    await expect(page.locator('.task-menu-toggle')).toHaveCount(1)
    await page.locator('.task-menu-toggle').click()
    await page.getByRole('button', { name: '删除任务', exact: true }).click()
    const confirmation = page.getByRole('dialog', { name: '删除创作任务？', exact: true })
    await expect(confirmation).toBeVisible()
    await expect(confirmation.getByRole('button', { name: '取消', exact: true })).toBeFocused()
    await page.keyboard.press('Escape')
    await expect(confirmation).not.toBeVisible()
    await expect(page.locator('.task-menu-toggle')).toBeFocused()
    await expect(page.locator('.task-card')).toHaveCount(2)
    await page.locator('.task-menu-toggle').click()
    await page.getByRole('button', { name: '删除任务', exact: true }).click()
    await page.screenshot({ path: testInfo.outputPath('delete-task-confirmation.png') })
    await confirmation.getByRole('button', { name: '确认删除', exact: true }).click()
    await expect(page.locator('.task-card')).toHaveCount(1)
    await page.evaluate(() => {
      const j = (window as any).creationFixture.jobs[0]
      Object.assign(j, { status: 'completed', generated_count: 2, saved_count: 2 })
      j.items.forEach((i: any) => Object.assign(i, { generation_status: 'generated', save_status: 'saved', artwork_id: i.id }))
      j.revision++
    })
    await expect(page.locator('.task-menu-toggle')).toHaveCount(1, { timeout: 10000 })
    await page.locator('.task-card').click()
    await expect(page.locator('.saved-notice')).toHaveText('✓ 已保存到图库')
    await expect(page.locator('.job-preview .result-badge')).toHaveText('已完成')
    await page.screenshot({ path: testInfo.outputPath('creation-completed-result.png') })
    await page.getByRole('button', { name: '全部任务' }).click()
    await page.locator('.task-menu-toggle').click()
    await page.getByRole('button', { name: '删除任务', exact: true }).click()
    await confirmation.getByRole('button', { name: '确认删除', exact: true }).click()
    await expect(page.locator('.task-card')).toHaveCount(0)
    await expect(page.getByRole('searchbox', { name: '搜索创作任务' })).toBeFocused()
    await page.getByRole('button', { name: '新建创作' }).click()
    await page.locator('#task-prompt').fill('森林咖啡馆海报，温暖的午后阳光、木屋、绿植与手冲咖啡。')
    await page.locator('.task-preference').first().getByRole('button').first().click()
    await page.locator('.task-catalog [data-option="FA-001"]').click()
    await expect(page.locator('.task-preference').first().locator('img')).toBeVisible()
    await page.locator('.task-preference').first().getByRole('button').first().click()
    await page.screenshot({ path: testInfo.outputPath('creation-draft-preferences.png') })
    expect(await page.locator('.preview-panel').evaluate(el => el.scrollWidth <= el.clientWidth)).toBe(true)
  })
}
