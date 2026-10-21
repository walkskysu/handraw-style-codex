import { expect, test } from '@playwright/test'
import { readFileSync } from 'node:fs'

const image = `data:image/png;base64,${readFileSync('src-tauri/icons/icon.png').toString('base64')}`

for (const width of [1440, 360]) {
  test(`gallery and native detail dialog remain accessible at ${width}px`, async ({ page }) => {
    await page.setViewportSize({ width, height: 900 })
    await page.addInitScript(({ image }) => {
      const artwork = {
        id: '1', title: '窗边的小猫', prompt: '一只在窗边喝咖啡的小猫',
        snapshot: { style: { id: 'FA-001', name: '手绘风格', prompt: '原始的完整手绘规范' }, layout: null, color: null, catalog_version: 'sha256:example' },
        submitted_prompt: '完整的生成提示词', created_at: '2026-10-21T00:00:00Z', width: 1024, height: 1024,
        format: 'png', file_size: 10240, favorite: false, tags: ['小猫'], deleted: false, model: null, tool_version: null, app_version: '0.1.0',
      }
      Object.defineProperty(window, 'isTauri', { value: true })
      Object.defineProperty(window, '__TAURI_INTERNALS__', { value: {
        invoke: async (command: string) => {
          if (command === 'gallery_list') return { items: Array.from({ length: 9 }, (_, i) => ({ ...artwork, id: String(i), thumbnail: image })), total: 9, page: 1 }
          if (command === 'gallery_detail') return [artwork, image]
          throw new Error(`Unexpected command: ${command}`)
        },
      } })
    }, { image })
    await page.goto('/')
    await page.getByRole('button', { name: '我的图库', exact: true }).click()
    await expect(page.locator('.gallery-card')).toHaveCount(9)
    expect(await page.evaluate(() => document.scrollingElement!.scrollHeight <= innerHeight && document.scrollingElement!.scrollWidth <= innerWidth)).toBe(true)
    await page.locator('.gallery-card').first().click()
    const dialog = page.getByRole('dialog', { name: '作品详情' })
    await expect(dialog).toBeVisible()
    const left = await dialog.locator('.artwork-original').boundingBox()
    const right = await dialog.locator('.artwork-info').boundingBox()
    if (width > 900) expect(right!.x).toBeGreaterThan(left!.x + left!.width)
    else expect(right!.y).toBeGreaterThan(left!.y + left!.height)
    await dialog.getByRole('button', { name: '删除', exact: true }).scrollIntoViewIfNeeded()
    await expect(dialog.getByRole('button', { name: '删除', exact: true })).toBeInViewport()
    await page.keyboard.press('Escape')
    await expect(dialog).not.toBeVisible()
    await expect(page.locator('.gallery-card').first()).toBeFocused()
    await expect(page.locator('.app-footer')).toBeInViewport()
  })
}
