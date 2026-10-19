import { invoke, isTauri } from '@tauri-apps/api/core'

export const MAX_PROMPT_LENGTH = 4000
export const isDesktop = (): boolean => isTauri()

export async function generateImage(prompt: string): Promise<string> {
  const trimmed = prompt.trim()
  if (!trimmed) throw new Error('请先描述你想生成的画面。')
  if (Array.from(trimmed).length > MAX_PROMPT_LENGTH) {
    throw new Error('提示词不能超过 4000 个字符。')
  }
  if (!isDesktop()) throw new Error('请在桌面应用中使用本地 Codex 生成图片。')

  return invoke<string>('generate_image', { prompt: trimmed })
}
