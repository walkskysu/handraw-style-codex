import { isTauri } from '@tauri-apps/api/core'

export const MAX_PROMPT_LENGTH = 4000
export const isDesktop = (): boolean => isTauri()
