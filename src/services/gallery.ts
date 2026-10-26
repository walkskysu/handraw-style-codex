import { invoke } from '@tauri-apps/api/core'
import type { OptionCategory } from './generationOptions'

export interface SnapshotOption { id: string; name: string; prompt: string }
export type Snapshot = Record<OptionCategory, SnapshotOption | null> & { catalog_version: string }
export interface Artwork {
  id: string
  group_id: string
  position: number
  save_status: 'saved' | 'pending'
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
export interface GeneratedGroup { group_id: string; photos: Generated[]; expected_count: number; generation_errors: string[]; group_save_error?: string | null }
export interface PhotoGroup {
  id: string; title: string; tags: string[]; favorite: boolean; cover_id: string | null
  prompt: string; snapshot: Snapshot; created_at: string; status: string
  expected_count: number; generated_count: number; generation_errors: string[]
  source_group_id: string | null; source_photo_id: string | null; deleted: boolean
}
export interface GroupDetail { group: PhotoGroup; photos: Artwork[] }
export interface GalleryCard extends PhotoGroup {
  thumbnail: string; thumbnails: string[]; photo_count: number; pending_count: number; matching_ids: string[]; matching_count: number
}
export interface GalleryPage { items: GalleryCard[]; total: number; page: number; photo_total: number }

export const listGallery = (search: string, favorite: boolean, deleted: boolean, page: number, photoFavorite = false) =>
  invoke<GalleryPage>('gallery_list', { search, favorite, deleted, page, photoFavorite })
export const getGroup = (id: string, deleted: boolean) => invoke<GroupDetail>('gallery_group_detail', { id, deleted })
export const updateGroup = (g: PhotoGroup) => invoke<PhotoGroup>('gallery_group_update', {
  id: g.id, title: g.title, tags: g.tags, favorite: g.favorite, deleted: g.deleted,
})
export const exportGroup = (id: string, includeNotes: boolean, deleted: boolean) => invoke<boolean>('gallery_group_export', { id, includeNotes, deleted })
export const getArtwork = (id: string) => invoke<[Artwork, string]>('gallery_detail', { id })
export const retrySave = (id: string) => invoke<Artwork>('gallery_retry', { id })
export const retryGroupSave = (id: string) => invoke<void>('gallery_group_retry', { id })
export const discardPending = (id: string) => invoke<void>('gallery_discard_pending', { id })
export const updateArtwork = (a: Artwork) => invoke<Artwork>('gallery_update', {
  id: a.id, title: a.title, tags: a.tags, favorite: a.favorite, deleted: a.deleted,
})
export const purgeArtwork = (id: string) => invoke<void>('gallery_purge', { id })
export const exportArtwork = (id: string, includeNotes: boolean) => invoke<boolean>('gallery_export', { id, includeNotes })
export const message = (error: unknown) => typeof error === 'string' ? error : error instanceof Error ? error.message : '操作失败，请重试。'
export const displayTime = (value: string) => new Date(value).toLocaleString('zh-CN')
