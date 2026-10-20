import { expect, test, type Page } from '@playwright/test'

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
    if (compact) await page.locator('.open-catalog').click()

    for (const category of ['style', 'layout', 'color']) {
      await page.locator(`#${category}-tab`).click()
      const panel = page.getByRole('tabpanel')
      await expect(panel).toHaveAttribute('tabindex', '0')
      const scrollRange = await panel.evaluate((el) => el.scrollHeight - el.clientHeight)
      if (viewport.height === 560) expect(scrollRange).toBeGreaterThan(0)
      // Keyboard scrolling must work without moving the application shell.
      await panel.focus()
      await page.keyboard.press('End')
      await expect.poll(() => panel.evaluate((el) => el.scrollTop)).toBe(scrollRange)
      await expect(page.locator('#catalog-title')).toBeInViewport()
      await expect(page.locator(`#${category}-tab`)).toBeInViewport()
      await expectNoDocumentScroll(page)
      await panel.locator('.option-card').last().click()
      await expect(panel.locator('.option-card').last()).toHaveAttribute('aria-pressed', 'true')
      await panel.getByRole('button', { name: '下一页' }).click()
      await expect.poll(() => panel.evaluate((el) => el.scrollTop)).toBe(0)
      await panel.locator('input').fill('no matching option')
      await expect(panel.locator('.input-hint')).toBeVisible()
      await panel.locator('input').fill('')
      await expect(panel.locator('.option-card')).toHaveCount(9)
    }

    if (compact) {
      await page.getByRole('button', { name: '关闭图鉴' }).click()
      await expect(page.locator('#catalog-dialog')).not.toBeVisible()
    }
    // Short and stacked layouts must still expose creation controls and preview.
    await page.locator('button[type="submit"]').scrollIntoViewIfNeeded()
    await expect(page.locator('button[type="submit"]')).toBeInViewport()
    if (viewport.width < 640) {
      const creation = await page.locator('.creation-panel').boundingBox()
      const preview = await page.locator('.preview-panel').boundingBox()
      expect(preview!.y).toBeGreaterThanOrEqual(creation!.y + creation!.height)
    }
    await page.locator('.preview-placeholder').scrollIntoViewIfNeeded()
    await expect(page.locator('.preview-placeholder')).toBeInViewport()
    await expectNoDocumentScroll(page)
  })
}

for (const width of [1440, 1100]) {
  test(`reveals a selected bottom-row card at width ${width}`, async ({ page }) => {
    await page.setViewportSize({ width, height: 560 })
    await page.goto('/')
    if (width < 1200) await page.locator('.open-catalog').click()
    await page.locator('[data-option="FA-009"]').click()
    await page.locator('#color-tab').click()
    if (width < 1200) await page.getByRole('button', { name: '关闭图鉴' }).click()
    await page.locator('[data-selection="style"] .selection-link').click()
    await expect(page.locator('[data-option="FA-009"]')).toHaveAttribute('aria-pressed', 'true')
    await expect(page.locator('[data-option="FA-009"]')).toBeInViewport()
    await expectNoDocumentScroll(page)
  })
}
