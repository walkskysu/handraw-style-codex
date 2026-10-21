import { expect, test, type Page } from '@playwright/test'
import catalog from '../../src/data/generationCatalog.json' with { type: 'json' }

async function expectNoDocumentScroll(page: Page) {
  expect(await page.evaluate(() => {
    const root = document.scrollingElement!
    return root.scrollHeight <= innerHeight && root.scrollWidth <= innerWidth && root.scrollTop === 0
  })).toBe(true)
  await expect(page.locator('.app-footer')).toBeInViewport()
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
    const compact = viewport.width < 1200
    await page.locator('.catalog-panel').scrollIntoViewIfNeeded()
    await expect(page.locator('main .catalog-panel')).toBeVisible()
    if (compact) await expect(page.locator('.preview-panel')).not.toBeVisible()

    for (const category of ['style', 'layout', 'color'] as const) {
      await page.locator(`#${category}-tab`).click()
      const panel = page.getByRole('tabpanel')
      await expect(panel).toHaveAttribute('tabindex', '0')
      await expect(panel.locator('.option-card')).toHaveCount(catalog[category].length)
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
      await panel.locator('input').fill('no matching option')
      await expect(panel.locator('.input-hint')).toBeVisible()
      await expect.poll(() => panel.evaluate((el) => el.scrollTop)).toBe(0)
      await panel.locator('input').fill('')
      await expect(panel.locator('.option-card')).toHaveCount(catalog[category].length)
      await expect(panel.locator('.option-card').last()).toHaveAttribute('aria-pressed', 'true')
    }

    // Short and stacked layouts must still expose creation controls and preview.
    await page.locator('button[type="submit"]').scrollIntoViewIfNeeded()
    await expect(page.locator('button[type="submit"]')).toBeInViewport()
    if (viewport.width < 640) {
      const creation = await page.locator('.creation-panel').boundingBox()
      const catalog = await page.locator('.catalog-panel').boundingBox()
      expect(catalog!.y).toBeGreaterThanOrEqual(creation!.y + creation!.height)
    }
    if (compact) await page.locator('.open-preview').click()
    await page.locator('.preview-placeholder').scrollIntoViewIfNeeded()
    await expect(page.locator('.preview-placeholder')).toBeInViewport()
    await expectNoDocumentScroll(page)
    if (compact) {
      await page.keyboard.press('Escape')
      await expect(page.locator('#preview-dialog')).not.toBeVisible()
      await expect(page.locator('.open-preview')).toBeFocused()
    }
  })
}

for (const width of [1440, 1100]) {
  test(`reveals a selected bottom-row card at width ${width}`, async ({ page }) => {
    await page.setViewportSize({ width, height: 560 })
    await page.goto('/')
    const selectedCard = page.locator(`[data-option="${catalog.style.at(-1)!.id}"]`)
    await selectedCard.click()
    await page.locator('#color-tab').click()
    await page.locator('[data-selection="style"] .selection-link').click()
    await expect(selectedCard).toHaveAttribute('aria-pressed', 'true')
    await expect(selectedCard).toBeInViewport()
    await expectNoDocumentScroll(page)
  })
}

for (const width of [1920, 1659, 1399, 1200]) {
  test(`catalog receives the former preview width at ${width}px`, async ({ page }) => {
    await page.setViewportSize({ width, height: 900 })
    await page.goto('/')
    const catalog = (await page.locator('main .catalog-panel').boundingBox())!
    const preview = (await page.locator('main .preview-panel').boundingBox())!
    expect(catalog.width).toBeGreaterThan(preview.width)
    const ratio = width < 1400 ? 1.25 : 1.45
    expect(Math.abs(catalog.width / preview.width - ratio)).toBeLessThan(0.02)
    expect(catalog.x + catalog.width).toBeLessThan(preview.x)
    await expectNoDocumentScroll(page)
  })
}

test('resizing keeps catalog browsing state and moves the accessible preview between panel and dialog', async ({ page }) => {
  await page.setViewportSize({ width: 1199, height: 760 })
  await page.goto('/')
  await page.locator('#color-tab').click()
  await page.locator('[data-option="C-36"]').click()
  const scrollTop = await page.getByRole('tabpanel').evaluate(el => el.scrollTop)
  await page.locator('.open-preview').click()
  await expect(page.getByRole('dialog')).toBeVisible()
  await expect(page.locator('.close-preview')).toBeFocused()
  await page.keyboard.press('Tab')
  // Chromium can focus browser chrome between cycles; background controls stay inert.
  await expect(page.locator('.open-preview')).not.toBeFocused()
  await expect(page.locator('#prompt')).not.toBeFocused()
  await page.keyboard.press('Tab')
  await expect(page.locator('.close-preview')).toBeFocused()
  await page.setViewportSize({ width: 1200, height: 760 })
  await expect(page.locator('#preview-dialog')).not.toBeVisible()
  await expect(page.locator('main .preview-panel')).toBeVisible()
  await page.setViewportSize({ width: 1199, height: 760 })
  await expect(page.locator('main .catalog-panel')).toBeVisible()
  await expect(page.locator('.preview-panel')).not.toBeVisible()
  await expect(page.locator('#color-tab')).toHaveAttribute('aria-selected', 'true')
  await expect(page.locator('[data-option="C-36"]')).toHaveAttribute('aria-pressed', 'true')
  await expect.poll(() => page.getByRole('tabpanel').evaluate(el => el.scrollTop)).toBe(scrollTop)
  await page.locator('.open-preview').click()
  await page.locator('.close-preview').click()
  await expect(page.locator('.open-preview')).toBeFocused()
  await expectNoDocumentScroll(page)
})
