import { invoke } from '@tauri-apps/api/core'
import type { OptionCategory } from './generationOptions'

export interface SnapshotOption { id: string; name: string; prompt: string }
export type Snapshot = Record<OptionCategory, SnapshotOption | null> & { catalog_version: string }
export interface Artwork {
  id: string
  title: string
  prompt: string
  snapshot: Snapshot
  submitted_prompt: string
  created_at: string
  width: number
  height: number
  format: string
  file_size: number
  favorite: boolean
  tags: string[]
  deleted: boolean
  model: string | null
  tool_version: string | null
  app_version: string
}
export interface Generated { id: string; image: string; save_error: string | null }
export interface GalleryCard { id: string; title: string; created_at: string; favorite: boolean; thumbnail: string }
export interface GalleryPage { items: GalleryCard[]; total: number; page: number }

export const listGallery = (search: string, favorite: boolean, deleted: boolean, page: number) =>
  invoke<GalleryPage>('gallery_list', { search, favorite, deleted, page })
export const getArtwork = (id: string) => invoke<[Artwork, string]>('gallery_detail', { id })
export const retrySave = (id: string) => invoke<Artwork>('gallery_retry', { id })
export const discardPending = (id: string) => invoke<void>('gallery_discard_pending', { id })
export const updateArtwork = (a: Artwork) => invoke<Artwork>('gallery_update', {
  id: a.id, title: a.title, tags: a.tags, favorite: a.favorite, deleted: a.deleted,
})
export const purgeArtwork = (id: string) => invoke<void>('gallery_purge', { id })
export const exportArtwork = (id: string, includeNotes: boolean) => invoke<boolean>('gallery_export', { id, includeNotes })
export const message = (error: unknown) => typeof error === 'string' ? error : error instanceof Error ? error.message : '操作失败，请重试。'
export const displayTime = (value: string) => new Date(value).toLocaleString('zh-CN')
