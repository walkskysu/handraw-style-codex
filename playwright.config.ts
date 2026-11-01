import { defineConfig } from '@playwright/test'
const port = process.env.PLAYWRIGHT_PORT ?? '1420'

export default defineConfig({
  testDir: './tests/browser',
  use: { baseURL: `http://127.0.0.1:${port}`, browserName: 'chromium', channel: 'chromium', screenshot: 'only-on-failure',
    launchOptions: { executablePath: process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE } },
  webServer: {
    command: `npm run dev -- --port ${port}`,
    url: `http://127.0.0.1:${port}`,
    reuseExistingServer: !process.env.CI,
  },
})
