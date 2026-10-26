import { expect, test } from '@playwright/test'
import { readFileSync } from 'node:fs'

const image = `data:image/png;base64,${readFileSync('src-tauri/icons/icon.png').toString('base64')}`

for (const width of [1440, 360]) {
  test(`gallery and native detail dialog remain accessible at ${width}px`, async ({ page }, testInfo) => {
    await page.setViewportSize({ width, height: 900 })
    await page.addInitScript(({ image }) => {
      const artwork = {
        id: '1', group_id: '1', position: 0, save_status: 'saved', title: '窗边的小猫', prompt: '一只在窗边喝咖啡的小猫',
        snapshot: { style: { id: 'FA-001', name: '手绘风格', prompt: '原始的完整手绘规范' }, layout: null, color: null, catalog_version: 'sha256:example' },
        submitted_prompt: '完整的生成提示词', created_at: '2026-10-21T00:00:00Z', width: 1024, height: 1024,
        format: 'png', file_size: 10240, favorite: false, tags: ['小猫'], deleted: false, model: null, tool_version: null, app_version: '0.1.0',
      }
      const photos = Array.from({ length: 4 }, (_, position) => ({ ...artwork, id: `photo-${position}`, position }))
      const group = { ...artwork, expected_count: 4, generated_count: 4, generation_errors: [], cover_id: photos[0]!.id }
      Object.defineProperty(window, 'isTauri', { value: true })
      Object.defineProperty(window, '__TAURI_INTERNALS__', { value: {
        invoke: async (command: string, args: { id: string }) => {
          if (command === 'gallery_list') return { items: Array.from({ length: 9 }, (_, i) => ({ ...group, id: String(i), thumbnail: image, thumbnails: [image, image, image], photo_count: 4, pending_count: 0, matching_count: 1, matching_ids: [photos[0]!.id] })), total: 9, page: 1, photo_total: 36 }
          if (command === 'gallery_group_detail') return { group, photos }
          if (command === 'gallery_detail') return [photos.find(photo => photo.id === args.id), image]
          throw new Error(`Unexpected command: ${command}`)
        },
      } })
    }, { image })
    await page.goto('/')
    await page.getByRole('button', { name: '我的图库', exact: true }).click()
    await expect(page.locator('.gallery-card')).toHaveCount(9)
    await expect(page.locator('.gallery-card').first().locator('.group-thumbnails img')).toHaveCount(3)
    await expect(page.locator('.photo-count').first()).toHaveText('4 张')
    await page.screenshot({ path: testInfo.outputPath('groups.png') })
    expect(await page.evaluate(() => document.scrollingElement!.scrollHeight <= innerHeight && document.scrollingElement!.scrollWidth <= innerWidth)).toBe(true)
    await page.locator('.gallery-card').first().click()
    const dialog = page.getByRole('dialog', { name: '作品详情' })
    await expect(dialog).toBeVisible()
    await expect(dialog.locator('.photo-navigation button')).toHaveCount(4)
    await expect(dialog.locator('.photo-navigation button').first()).toHaveAttribute('aria-pressed', 'true')
    await dialog.locator('.photo-navigation button').nth(2).click()
    await expect(dialog.locator('.photo-navigation button').nth(2)).toHaveAttribute('aria-pressed', 'true')
    await page.screenshot({ path: testInfo.outputPath('group-detail.png') })
    expect(await dialog.evaluate(el => el.scrollWidth <= el.clientWidth)).toBe(true)
    const left = await dialog.locator('.artwork-original').boundingBox()
    const right = await dialog.locator('.artwork-info').boundingBox()
    if (width > 900) expect(right!.x).toBeGreaterThan(left!.x + left!.width)
    else expect(right!.y).toBeGreaterThan(left!.y + left!.height)
    await dialog.getByRole('button', { name: '删除此图', exact: true }).scrollIntoViewIfNeeded()
    await expect(dialog.getByRole('button', { name: '删除此图', exact: true })).toBeInViewport()
    await page.keyboard.press('Escape')
    await expect(dialog).not.toBeVisible()
    await expect(page.locator('.gallery-card').first()).toBeFocused()
    await expect(page.locator('.app-footer')).toBeInViewport()
  })
}
