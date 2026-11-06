import { expect, test, type Page } from '@playwright/test'
import catalog from '../../src/data/generationCatalog.json' with { type: 'json' }

async function expectNoDocumentScroll(page: Page) {
  expect(await page.evaluate(() => {
    const root = document.scrollingElement!
    return root.scrollHeight <= innerHeight && root.scrollWidth <= innerWidth && root.scrollTop === 0
  })).toBe(true)
  await expect(page.locator('.app-header')).toBeInViewport()
}

for (const viewport of [
  { width: 1659, height: 900 },
  { width: 1440, height: 560 },
  { width: 1100, height: 760 },
  { width: 390, height: 844 },
  { width: 360, height: 560 },
]) {
  test(`catalog scroll stays inside the panel at ${viewport.width}x${viewport.height}`, async ({ page }) => {
    await page.setViewportSize(viewport)
    await page.goto('/')
    await expectNoDocumentScroll(page)
    await page.locator('.catalog-panel').scrollIntoViewIfNeeded()
    await expect(page.locator('main .catalog-panel')).toBeVisible()
    await expect(page.locator('.preview-panel')).not.toBeVisible()

    for (const category of ['style', 'layout', 'color'] as const) {
      await page.locator(`#${category}-tab`).click()
      const panel = page.getByRole('tabpanel')
      await expect(panel).toHaveAttribute('tabindex', '0')
      await expect(panel.locator('.option-card')).toHaveCount(catalog[category].length)
      await expect(page.locator('.picker-footer button')).toHaveCount(0)
      await expect(page.locator('.picker-footer [role="status"]')).toHaveText(`共 ${catalog[category].length} 项`)
      await expect(panel.locator('nav')).toHaveCount(0)
      await expect.poll(() => panel.evaluate((el) => el.scrollTop)).toBe(0)
      const scrollRange = await panel.evaluate((el) => el.scrollHeight - el.clientHeight)
      expect(scrollRange).toBeGreaterThan(0)
      // Keyboard scrolling must work without moving the application shell.
      await panel.focus()
      await page.keyboard.press('End')
      await expect.poll(() => panel.evaluate((el) => el.scrollTop)).toBe(scrollRange)
      await expect(page.locator('#catalog-title')).toBeInViewport()
      await expect(page.locator(`#${category}-tab`)).toBeInViewport()
      await expectNoDocumentScroll(page)
      await expect(panel.locator('.option-card').last()).toBeInViewport()
      await expect(panel.locator('.option-card').last()).toHaveAttribute('data-option', catalog[category].at(-1)!.id)
      await panel.locator('.option-card').last().click()
      await expect(panel.locator('.option-card').last()).toHaveAttribute('aria-pressed', 'true')
      const preference = page.locator(`[data-selection="${category}"]`)
      await expect(preference.locator('.selection-name')).toHaveText(catalog[category].at(-1)!.name)
      await expect.poll(() => preference.locator('img').evaluate((el: HTMLImageElement) => el.complete && el.naturalWidth > 0)).toBe(true)
      await page.locator('#catalog-search').fill('no matching option')
      await expect(panel.locator('.input-hint')).toBeVisible()
      await expect.poll(() => panel.evaluate((el) => el.scrollTop)).toBe(0)
      await page.locator('#catalog-search').fill('')
      await expect(panel.locator('.option-card')).toHaveCount(catalog[category].length)
      await page.locator(`[data-selection="${category}"] .selection-link`).click()
      await expect(panel.locator('.option-card').last()).toHaveAttribute('aria-pressed', 'true')
    }

    // Short and stacked layouts must still expose creation controls and navigation.
    await page.locator('.creation-panel button[type="submit"]').scrollIntoViewIfNeeded()
    await expect(page.locator('.creation-panel button[type="submit"]')).toBeInViewport()
    if (viewport.width < 640) {
      const creation = await page.locator('.creation-panel').boundingBox()
      const catalog = await page.locator('.catalog-panel').boundingBox()
      expect(catalog!.y).toBeGreaterThanOrEqual(creation!.y + creation!.height)
    }
    const navigation = page.getByRole('navigation', { name: '工作区', exact: true })
    await expect(navigation.getByRole('button')).toHaveText(['创作', '我的图库'])
    await expect(navigation.getByRole('button', { name: '创作', exact: true })).toBeInViewport()
    await expect(navigation.getByRole('button', { name: '我的图库', exact: true })).toBeInViewport()
    await expectNoDocumentScroll(page)
  })
}

for (const width of [1100, 640, 360]) {
  test(`selected preference names wrap without hiding thumbnails or clear controls at ${width}px`, async ({ page }, testInfo) => {
    await page.setViewportSize({ width, height: 760 })
    await page.goto('/')
    for (const category of ['style', 'layout', 'color'] as const) {
      const option = [...catalog[category]].sort((a, b) => b.name.length - a.name.length)[0]!
      await page.locator(`#${category}-tab`).click()
      await page.locator('#catalog-search').fill(option.id)
      await page.locator(`[data-option="${option.id}"]`).click()
      const row = page.locator(`[data-selection="${category}"]`)
      await row.scrollIntoViewIfNeeded()
      await expect(row.locator('.selection-name')).toHaveText(option.name)
      await expect(row.locator('img')).toBeInViewport()
      await expect(row.locator('.clear-selection')).toBeInViewport()
      expect(await row.evaluate(el => el.scrollWidth <= el.clientWidth)).toBe(true)
      const name = (await row.locator('.selection-name').boundingBox())!
      const clear = (await row.locator('.clear-selection').boundingBox())!
      expect(name.x + name.width).toBeLessThanOrEqual(clear.x)
    }
    await expectNoDocumentScroll(page)
    await page.screenshot({ path: testInfo.outputPath('selected-preferences.png') })
    await page.locator('[data-selection="layout"] .clear-selection').click()
    await expect(page.locator('[data-selection="layout"]')).toContainText('自动')
    await expect(page.locator('[data-selection="layout"] img')).toHaveCount(0)
    await expect(page.locator('.selection-thumbnail')).toHaveCount(2)
  })
}

for (const width of [1440, 1100]) {
  test(`reveals a selected bottom-row card at width ${width}`, async ({ page }) => {
    await page.setViewportSize({ width, height: 560 })
    await page.goto('/')
    const selectedCard = page.locator(`[data-option="${catalog.style.at(-1)!.id}"]`)
    await page.locator('#catalog-search').fill(catalog.style.at(-1)!.id)
    await selectedCard.click()
    await page.locator('#color-tab').click()
    await page.locator('[data-selection="style"] .selection-link').click()
    await expect(selectedCard).toHaveAttribute('aria-pressed', 'true')
    await expect(selectedCard).toBeInViewport()
    await expectNoDocumentScroll(page)
  })
}

for (const width of [1920, 1659, 1440, 1399, 1200, 1100]) {
  test(`catalog occupies the right side of a two-column workspace at ${width}px`, async ({ page }, testInfo) => {
    await page.setViewportSize({ width, height: 920 })
    await page.goto('/')
    await expect(page.locator('main > section')).toHaveCount(2)
    await expect(page.locator('.preview-panel')).not.toBeVisible()
    const catalog = (await page.locator('main .catalog-panel').boundingBox())!
    const creation = (await page.locator('.creation-panel').boundingBox())!
    expect(catalog.width).toBeGreaterThan(creation.width)
    expect(catalog.x).toBeGreaterThan(creation.x + creation.width)
    expect(catalog.y).toBe(creation.y)
    const tabs = (await page.locator('.catalog-tabs').boundingBox())!
    const search = (await page.locator('#catalog-search').boundingBox())!
    expect(search.y).toBeGreaterThanOrEqual(tabs.y + tabs.height)
    await page.locator('[data-option="FA-001"]').click()
    await expectNoDocumentScroll(page)
    if (width === 1440) await page.screenshot({ path: testInfo.outputPath('ideacanvas-workspace.png') })
  })
}

test('resizing preserves selection and the independent task panel, with focus returned on close', async ({ page }) => {
  // A failed save leaves the submit control available for reopening the panel.
  await page.addInitScript(() => {
    Object.defineProperty(window, 'isTauri', { value: true })
    Object.defineProperty(window, '__TAURI_INTERNALS__', { value: {
      invoke: async (command: string) => {
        if (command === 'job_list') return { drafts: [], plans: [], planning_jobs: [], jobs: [] }
        throw new Error('保存失败')
      },
    } })
  })
  await page.setViewportSize({ width: 1199, height: 760 })
  await page.goto('/')
  await page.locator('#color-tab').click()
  await page.locator('[data-option="C-36"]').click()
  await page.locator('#prompt').fill('窗边的小猫')
  const submit = page.locator('.creation-panel button[type="submit"]')
  await submit.click()
  await expect(page.getByRole('dialog')).toBeVisible()
  await expect(page.locator('#preview-dialog')).toContainText('保存失败')
  await expect(page.locator('.close-preview')).toBeFocused()
  await page.keyboard.press('Tab')
  await expect(page.locator('#prompt')).not.toBeFocused()
  await page.setViewportSize({ width: 1440, height: 760 })
  await expect(page.locator('#preview-dialog')).toBeVisible()
  await page.locator('.close-preview').click()
  await expect(submit).toBeFocused()
  await expect(page.locator('.preview-panel')).not.toBeVisible()
  await expect(page.locator('#color-tab')).toHaveAttribute('aria-selected', 'true')
  await expect(page.locator('[data-option="C-36"]')).toHaveAttribute('aria-pressed', 'true')
  // Responsive column counts change pixel offsets; the current selection stays visible.
  await expect(page.locator('[data-option="C-36"]')).toBeInViewport()
  expect(await page.getByRole('tabpanel').evaluate(el => el.scrollTop)).toBeGreaterThan(0)
  await submit.click()
  await expect(page.locator('#preview-dialog')).toContainText('保存失败')
  await page.keyboard.press('Escape')
  await expect(submit).toBeFocused()
  await expectNoDocumentScroll(page)
})
