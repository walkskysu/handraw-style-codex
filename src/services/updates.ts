import { Channel, invoke } from '@tauri-apps/api/core'

export interface Release { version: string; notes: string }
export interface UpdateProgress { downloaded: number; total: number | null }
export interface About { current_version: string; install_error: string | null }
export const RELEASE_PAGE = 'https://github.com/walkskysu/handraw-style-codex/releases/latest'

export const getAbout = () => invoke<About>('update_about')
export const checkUpdate = () => invoke<Release | null>('update_check')
export const installUpdate = () => invoke<void>('update_install')
export const openManualUpdate = () => invoke<void>('update_manual')
export function downloadUpdate(onProgress: (value: UpdateProgress) => void) {
  const progress = new Channel<UpdateProgress>()
  progress.onmessage = onProgress
  return invoke<void>('update_download', { progress })
}
