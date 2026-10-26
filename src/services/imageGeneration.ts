import { invoke, isTauri } from '@tauri-apps/api/core'
import type { GenerationSelections } from './generationOptions'
import type { GeneratedGroup } from './gallery'

export const MAX_PROMPT_LENGTH = 4000
export const isDesktop = (): boolean => isTauri()

export async function generateImage(prompt: string, selections: GenerationSelections = {}, reuseId?: string, count = 1): Promise<GeneratedGroup> {
  const trimmed = prompt.trim()
  if (!trimmed) throw new Error('请先描述你想生成的画面。')
  if (Array.from(trimmed).length > MAX_PROMPT_LENGTH) {
    throw new Error('提示词不能超过 4000 个字符。')
  }
  if (!isDesktop()) throw new Error('请在桌面应用中使用本地 Codex 生成图片。')
  if (!Number.isInteger(count) || count < 1 || count > 4) throw new Error('每组可生成 1–4 张照片。')

  return invoke<GeneratedGroup>('generate_image', { prompt: trimmed, selections, ...(reuseId ? { reuseId } : {}), count })
}
