import { expect, test } from '@playwright/test'

for (const width of [1100, 360]) {
  test(`settings update dialog fits and restores focus at ${width}px`, async ({ page }) => {
    await page.setViewportSize({ width, height: 560 })
    await page.addInitScript(() => {
      Object.defineProperty(window, 'isTauri', { value: true })
      Object.defineProperty(window, '__TAURI_INTERNALS__', { value: {
        transformCallback: () => 1,
        invoke: async (command: string) => {
          if (command === 'update_about') return { current_version: '0.2.4', install_error: null }
          if (command === 'update_check') return { version: '0.3.0', notes: '• 新增图库搜索\n• 优化图片预览\n• 修复生成失败的问题\n'.repeat(8) }
          if (command === 'update_download') return
          throw new Error(`Unexpected command: ${command}`)
        },
      } })
    })
    await page.goto('/')
    const settings = page.getByRole('button', { name: '设置', exact: true })
    await expect(settings).toBeInViewport()
    await settings.click()
    const dialog = page.getByRole('dialog', { name: '设置', exact: true })
    await expect(dialog).toBeVisible()
    await expect(dialog).toContainText('当前版本：0.2.4')
    await expect(dialog).toContainText('发现新版本：0.3.0')
    const bounds = await dialog.boundingBox()
    expect(bounds!.x).toBeGreaterThanOrEqual(0)
    expect(bounds!.x + bounds!.width).toBeLessThanOrEqual(width)
    expect(bounds!.height).toBeLessThanOrEqual(560)
    await dialog.getByRole('button', { name: '下载更新', exact: true }).click()
    await expect(dialog.getByRole('button', { name: '重启并安装', exact: true })).toBeVisible()
    await expect(dialog).toContainText('签名校验通过')
    await page.keyboard.press('Escape')
    await expect(dialog).not.toBeVisible()
    await expect(settings).toBeFocused()
    expect(await page.evaluate(() => document.scrollingElement!.scrollWidth <= innerWidth)).toBe(true)
  })
}
