import { afterEach, describe, expect, it, vi } from 'vitest'
import * as core from '@tauri-apps/api/core'
import { clearMocks, mockIPC } from '@tauri-apps/api/mocks'
import { greet } from '../src/services/greeting'

vi.mock('@tauri-apps/api/core', { spy: true })

afterEach(() => clearMocks())

describe('greeting service', () => {
  it('provides an explicitly labelled browser preview without calling IPC', async () => {
    vi.spyOn(core, 'isTauri').mockReturnValue(false)
    const ipc = vi.fn()
    mockIPC(ipc)

    await expect(greet('  小画家  ')).resolves.toBe('你好，小画家！这是浏览器预览。')
    expect(ipc).not.toHaveBeenCalled()
  })

  it('invokes the Rust command with the trimmed name in desktop mode', async () => {
    vi.spyOn(core, 'isTauri').mockReturnValue(true)
    const ipc = vi.fn().mockResolvedValue('来自 Rust 的问候')
    mockIPC(ipc)

    await expect(greet('  小画家  ')).resolves.toBe('来自 Rust 的问候')
    expect(ipc).toHaveBeenCalledExactlyOnceWith('greet', { name: '小画家' })
  })

  it.each(['', ' \t\n', '\u3000'])('rejects blank input %j before invoking IPC', async (name) => {
    vi.spyOn(core, 'isTauri').mockReturnValue(true)
    const ipc = vi.fn()
    mockIPC(ipc)

    await expect(greet(name)).rejects.toThrow('请先填写你的名字。')
    expect(ipc).not.toHaveBeenCalled()
  })

  it('propagates native failures so the UI can offer a retry', async () => {
    vi.spyOn(core, 'isTauri').mockReturnValue(true)
    mockIPC(() => Promise.reject('native command failed'))

    await expect(greet('小画家')).rejects.toBe('native command failed')
  })
})
