import { invoke, isTauri } from '@tauri-apps/api/core'

export function isDesktop(): boolean {
  return isTauri()
}

export async function greet(name: string): Promise<string> {
  const trimmedName = name.trim()
  if (!trimmedName) {
    throw new Error('请先填写你的名字。')
  }

  if (!isDesktop()) {
    return `你好，${trimmedName}！这是浏览器预览。`
  }

  return invoke<string>('greet', { name: trimmedName })
}
