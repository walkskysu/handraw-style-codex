import { expect, test } from '@playwright/test'
import { readFileSync } from 'node:fs'

for (const width of [1440, 640, 360]) {
  test(`reference uploads, selection order and previews at ${width}px`, async ({ page }, testInfo) => {
    await page.setViewportSize({ width, height: 900 })
    await page.addInitScript(() => {
      Object.defineProperty(window, 'isTauri', { value: true })
      Object.defineProperty(window, '__TAURI_INTERNALS__', { value: {
        invoke: async (command: string, args: any) => {
          if (command === 'job_list') return { drafts: [], plans: [], jobs: [], planning_jobs: [] }
          if (command === 'reference_list') return JSON.parse(localStorage.getItem('reference-fixture') ?? '[]')
          if (command === 'reference_import') {
            if (!args.dataUrl.startsWith('data:image/png;base64,')) throw new Error('Expected converted PNG input')
            const library = JSON.parse(localStorage.getItem('reference-fixture') ?? '[]')
            const item = { id: `ref-${library.length}`, name: args.name, thumbnail: args.dataUrl }
            localStorage.setItem('reference-fixture', JSON.stringify([...library, item])); return item
          }
          if (command === 'update_about') return { current_version: '0.2.7' }
          if (command === 'update_check') return null
          throw new Error(`Unexpected fixture command: ${command}`)
        },
      } })
    })
    await page.goto('/')
    await page.locator('#reference-tab').click()
    const jpeg = await page.evaluate(() => {
      const canvas = document.createElement('canvas'); canvas.width = 20; canvas.height = 10
      const context = canvas.getContext('2d')!; context.fillStyle = '#efba42'; context.fillRect(0, 0, 20, 10)
      return canvas.toDataURL('image/jpeg').split(',')[1]!
    })
    await page.locator('input[type="file"]').setInputFiles([
      { name: '人物.jpg', mimeType: 'image/jpeg', buffer: Buffer.from(jpeg, 'base64') },
      { name: '风格.png', mimeType: 'image/png', buffer: readFileSync('src-tauri/icons/icon.png') },
    ])
    await expect(page.locator('[data-reference]')).toHaveCount(2)
    await page.locator('[data-reference="ref-0"]').click()
    await page.locator('[data-reference="ref-1"]').click()
    const selection = page.locator('.creation-panel [data-selection="reference"]')
    await expect(selection).toContainText('参考图 1 · 人物.jpg')
    await expect(selection).toContainText('参考图 2 · 风格.png')
    for (const image of await selection.locator('img').all()) {
      await expect.poll(() => image.evaluate((el: HTMLImageElement) => el.naturalWidth)).toBeGreaterThan(0)
    }
    await page.getByRole('searchbox', { name: '搜索参考图像' }).fill('风格')
    await expect(page.locator('[data-reference]')).toHaveCount(1)
    await expect(page.locator('.selected-check')).toHaveText('2')
    await selection.getByRole('button', { name: '移除参考图 1', exact: true }).click()
    await expect(selection).toContainText('参考图 1 · 风格.png')
    await expect(page.locator('.selected-check')).toHaveText('1')
    // Upload help must never squeeze the scrollable image list out of short layouts.
    expect(await page.locator('.reference-picker > .option-picker').evaluate(el => el.clientHeight)).toBeGreaterThanOrEqual(180)
    await page.locator('[data-reference="ref-1"]').scrollIntoViewIfNeeded()
    await expect(page.locator('[data-reference="ref-1"]')).toBeInViewport()
    await page.locator('.catalog-panel').scrollIntoViewIfNeeded()
    expect(await page.locator('.catalog-panel').evaluate(el => el.scrollWidth <= el.clientWidth)).toBe(true)
    expect(await page.evaluate(() => document.scrollingElement!.scrollWidth <= innerWidth)).toBe(true)
    await page.screenshot({ path: testInfo.outputPath('reference-images.png') })
    await page.reload(); await page.locator('#reference-tab').click()
    await expect(page.locator('[data-reference]')).toHaveCount(2)
  })
}
