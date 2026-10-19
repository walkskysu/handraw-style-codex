import { afterEach, describe, expect, it, vi } from 'vitest'
import * as core from '@tauri-apps/api/core'
import { clearMocks, mockIPC } from '@tauri-apps/api/mocks'
import { generateImage } from '../src/services/imageGeneration'

vi.mock('@tauri-apps/api/core', { spy: true })
afterEach(() => clearMocks())

describe('image generation IPC', () => {
  it('explains browser limitations without invoking a desktop command', async () => {
    vi.spyOn(core, 'isTauri').mockReturnValue(false)
    const ipc = vi.fn()
    mockIPC(ipc)
    await expect(generateImage('小猫')).rejects.toThrow('桌面应用')
    expect(ipc).not.toHaveBeenCalled()
  })

  it('passes a trimmed, otherwise unchanged prompt to Rust', async () => {
    vi.spyOn(core, 'isTauri').mockReturnValue(true)
    const ipc = vi.fn().mockResolvedValue('data:image/png;base64,test')
    mockIPC(ipc)
    const prompt = '小猫\n" & $(echo test) <script>'
    await expect(generateImage(`  ${prompt}  `)).resolves.toBe('data:image/png;base64,test')
    expect(ipc).toHaveBeenCalledExactlyOnceWith('generate_image', { prompt })
  })

  it.each(['', ' \t\n', '\u3000', '画'.repeat(4001)])('rejects invalid prompts before IPC (%#)', async (prompt) => {
    vi.spyOn(core, 'isTauri').mockReturnValue(true)
    const ipc = vi.fn()
    mockIPC(ipc)
    await expect(generateImage(prompt)).rejects.toThrow()
    expect(ipc).not.toHaveBeenCalled()
  })

  it('preserves actionable native errors', async () => {
    vi.spyOn(core, 'isTauri').mockReturnValue(true)
    mockIPC(() => Promise.reject('未找到 Codex，请安装后重试。'))
    await expect(generateImage('小猫')).rejects.toBe('未找到 Codex，请安装后重试。')
  })
})
