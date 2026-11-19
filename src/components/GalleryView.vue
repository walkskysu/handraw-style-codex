<script setup lang="ts">
import { nextTick, onBeforeUnmount, ref, watch } from 'vue'
import { isDesktop } from '../services/imageGeneration'
import { displayTime, exportArtwork, exportGroup, getArtwork, getGroup, listGallery, message, purgeArtwork, retrySave, setGroupCover, updateArtwork, updateGroup, type Artwork, type GalleryCard, type GalleryPage, type GroupDetail } from '../services/gallery'
import { label as statusLabel } from '../services/creation'
import { optionCategories } from '../services/generationOptions'

const props = defineProps<{ generationPending: boolean; revision?: string; groupId?: string }>()
const emit = defineEmits<{ reuse: [artwork: Artwork]; saved: [id: string] }>()
const search = ref('')
const favorite = ref(false)
const photoFavorite = ref(false)
const deleted = ref(false)
const result = ref<GalleryPage>({ items: [], total: 0, page: 1, photo_total: 0 })
const groupDetail = ref<GroupDetail | null>(null)
const groupTitle = ref('')
const groupTags = ref('')
const matchingIds = ref<string[]>([])
const loading = ref(false)
const busy = ref(false)
const error = ref('')
const notice = ref('')
const selected = ref<Artwork | null>(null)
const fullImage = ref('')
const title = ref('')
const tags = ref('')
const includeNotes = ref(true)
const confirmPurge = ref(false)
const detail = ref<HTMLDialogElement | null>(null)
const zoom = ref<HTMLDialogElement | null>(null)
const editing = ref(false)
const menu = ref<'group' | 'photo' | 'export' | null>(null)
const titleInput = ref<HTMLInputElement | null>(null)
const moreButton = ref<HTMLButtonElement | null>(null)
const thumbnails = ref<HTMLElement | null>(null)
const settingLabels = { style: '画风', layout: '排版', color: '配色' }
let request = 0
let opener: HTMLElement | null = null

async function load(page = result.value.page) {
  const current = ++request
  if (!isDesktop()) return
  loading.value = true
  error.value = ''
  try {
    const data = await listGallery(search.value, favorite.value, deleted.value, page, photoFavorite.value)
    if (current === request) { result.value = data }
  } catch (cause) { if (current === request) error.value = message(cause) }
  finally { if (current === request) loading.value = false }
}
watch([search, favorite, photoFavorite, deleted], () => load(1))
watch(() => props.revision, () => load())
load()
onBeforeUnmount(() => { request++; zoom.value?.close(); detail.value?.close() })

async function selectPhoto(id: string) {
  if (editing.value && selected.value && selected.value.id !== id && (title.value !== selected.value.title || tags.value !== selected.value.tags.join(', '))) {
    // A rejected save leaves the editor and current photo intact.
    await update({ title: title.value, tags: tags.value.split(/[,，]/).map(t => t.trim()).filter(Boolean) })
  }
  const [artwork, image] = await getArtwork(id)
  selected.value = artwork; fullImage.value = image
  title.value = artwork.title; tags.value = artwork.tags.join(', ')
  confirmPurge.value = false
  editing.value = false; menu.value = null
  await nextTick()
  thumbnails.value?.querySelector<HTMLElement>('[aria-pressed="true"]')?.scrollIntoView?.({ block: 'nearest', inline: 'nearest' })
}

async function open(card: Pick<GalleryCard, 'id' | 'matching_ids'>, event?: Event) {
  if (busy.value) return
  busy.value = true
  error.value = ''; notice.value = ''
  opener = event?.currentTarget as HTMLElement ?? null
  try {
    // Reopening must not submit an abandoned editor from the previous dialog.
    editing.value = false; menu.value = null
    groupDetail.value = await getGroup(card.id, deleted.value)
    groupTitle.value = groupDetail.value.group.title
    groupTags.value = groupDetail.value.group.tags.join(', ')
    matchingIds.value = card.matching_ids
    const photos = groupDetail.value.photos
    const first = (search.value.trim()
      ? photos.find(a => card.matching_ids.includes(a.id))
      : photos.find(a => a.id === groupDetail.value!.group.cover_id)) ?? photos[0]
    if (!first) throw new Error('本组已无可查看的照片，请刷新图库。')
    await selectPhoto(first.id)
    await nextTick()
    detail.value?.showModal()
    thumbnails.value?.querySelector<HTMLElement>('[aria-pressed="true"]')?.scrollIntoView?.({ block: 'nearest', inline: 'nearest' })
  } catch (cause) { error.value = message(cause) }
  finally { busy.value = false }
}

watch(() => props.groupId, async id => {
  if (!id || !isDesktop()) return
  await nextTick()
  await open({ id, matching_ids: [] })
}, { immediate: true })

async function editGroup(changes = {}) {
  if (!groupDetail.value) return
  groupDetail.value.group = await updateGroup({ ...groupDetail.value.group, ...changes })
  notice.value = '已更新照片组'
  await load()
}

async function savePhoto() {
  if (!selected.value || !groupDetail.value) return
  selected.value = await retrySave(selected.value.id)
  emit('saved', selected.value.id)
  groupDetail.value = await getGroup(groupDetail.value.group.id, deleted.value)
  notice.value = '此图已保存'
  await load()
}

async function act(action: () => Promise<void>) {
  if (busy.value) return
  busy.value = true; error.value = ''; notice.value = ''
  try { await action() } catch (cause) { error.value = message(cause) }
  finally { busy.value = false }
}

async function update(changes: Partial<Artwork> = {}) {
  if (!selected.value) return
  const a = await updateArtwork({ ...selected.value, ...changes })
  selected.value = a
  const photoIndex = groupDetail.value?.photos.findIndex(photo => photo.id === a.id) ?? -1
  if (groupDetail.value && photoIndex >= 0) groupDetail.value.photos[photoIndex] = a
  if ('title' in changes) title.value = a.title
  if ('tags' in changes) tags.value = a.tags.join(', ')
  notice.value = '已更新'
  await load()
}

function reuse() {
  if (!selected.value) return
  emit('reuse', selected.value)
}

function toggleMenu(value: 'group' | 'photo' | 'export') {
  menu.value = menu.value === value ? null : value
}

async function startEditing() {
  editing.value = true; menu.value = null
  await nextTick()
  titleInput.value?.focus()
}

function cancelEditing() {
  title.value = selected.value!.title; tags.value = selected.value!.tags.join(', ')
  editing.value = false
  moreButton.value?.focus()
}

function closeMenu(event: KeyboardEvent) {
  if (event.key !== 'Escape' || !menu.value) return
  event.preventDefault(); event.stopPropagation()
  detail.value?.querySelector<HTMLButtonElement>(`[aria-controls="${menu.value}-menu"]`)?.focus()
  menu.value = null
}
</script>

<template>
  <section class="gallery-view panel" aria-labelledby="gallery-title">
    <div class="section-heading"><h2 id="gallery-title">{{ deleted ? '最近删除' : '我的图库' }}</h2>
      <button type="button" :aria-pressed="deleted" @click="deleted = !deleted">{{ deleted ? '返回图库' : '最近删除' }}</button>
    </div>
    <p v-if="!isDesktop()" class="input-hint">本地图库仅在桌面应用中可用。</p>
    <div class="gallery-tools">
      <label>搜索作品 <input v-model="search" type="search" placeholder="按组或照片的标题、标签、提示词搜索" /></label>
      <label><input v-model="favorite" type="checkbox" /> 收藏的组</label>
      <label><input v-model="photoFavorite" type="checkbox" /> 含收藏照片的组</label>
      <button type="button" :disabled="loading" @click="load()">刷新</button>
    </div>
    <p v-if="error && !detail?.open" role="alert" class="error">{{ error }}</p>
    <p v-if="loading" role="status">正在读取图库…</p>
    <div v-else class="gallery-grid" :aria-busy="busy">
      <button v-for="card in result.items" :key="card.id" type="button" class="gallery-card" :disabled="busy" @click="open(card, $event)">
        <span class="group-cover"><img v-if="card.thumbnail" :src="card.thumbnail" :alt="card.title" loading="lazy" /><span v-else class="input-hint">照片待保存</span><span class="photo-count">{{ card.photo_count }} 张</span></span>
        <span v-if="card.thumbnails.length" class="group-thumbnails"><img v-for="(thumbnail, index) in card.thumbnails" :key="index" :src="thumbnail" alt="组内照片" loading="lazy" /></span>
        <strong>{{ card.favorite ? '★ ' : '' }}{{ card.title }}</strong>
        <time :datetime="card.created_at">{{ displayTime(card.created_at) }}</time>
        <span>{{ statusLabel(card.status) }}</span>
        <span v-if="search.trim()">本组 {{ card.photo_count }} 张 · {{ card.matching_count }} 张匹配</span>
        <span v-if="card.generated_count < card.expected_count">{{ card.generated_count }} / {{ card.expected_count }} 张成功</span>
        <span v-if="card.pending_count">已保存 {{ card.photo_count }} 张 · {{ card.pending_count }} 张待保存</span>
      </button>
      <p v-if="!result.items.length" class="input-hint">{{ search || favorite || photoFavorite ? '没有找到匹配的作品。' : deleted ? '最近删除为空。' : '还没有作品，去创作工作台画一张吧。' }}</p>
    </div>
    <nav class="gallery-pagination" aria-label="图库分页">
      <button type="button" :disabled="loading || result.page <= 1" @click="load(result.page - 1)">上一页</button>
      <span>第 {{ result.page }} / {{ Math.max(1, Math.ceil(result.total / 9)) }} 页 · {{ result.total }} 组 · {{ result.photo_total }} 张</span>
      <button type="button" :disabled="loading || result.page * 9 >= result.total" @click="load(result.page + 1)">下一页</button>
    </nav>

    <dialog ref="detail" class="artwork-dialog" aria-labelledby="artwork-title" @close="opener?.focus()" @cancel="busy && $event.preventDefault()" @keydown="closeMenu" @click="!($event.target as HTMLElement).closest('.detail-menu') && (menu = null)">
      <template v-if="selected && groupDetail">
        <header class="artwork-header"><span class="brand">IdeaCanvas</span><span class="artwork-breadcrumb">我的图库 / {{ groupDetail.group.title }}</span><button type="button" :disabled="busy" aria-label="关闭作品详情" @click="detail?.close()">×</button></header>
        <div class="artwork-groupbar">
          <strong>{{ groupDetail.group.title }}</strong><span class="input-hint">共 {{ groupDetail.photos.length }} 张作品</span>
          <div class="detail-menu group-management"><button type="button" :aria-expanded="menu === 'group'" aria-controls="group-menu" @click="toggleMenu('group')">组管理 <span aria-hidden="true">···</span></button><div v-if="menu === 'group'" id="group-menu" class="detail-popover">
            <fieldset class="group-info" :disabled="busy">
          <legend>照片组</legend>
          <label>组标题 <input v-model="groupTitle" maxlength="200" /></label>
          <label>组标签（逗号分隔）<input v-model="groupTags" /></label>
          <div class="artwork-actions">
            <button type="button" @click="act(() => editGroup({ title: groupTitle, tags: groupTags.split(/[,，]/).map(t => t.trim()).filter(Boolean) }))">保存组标题与标签</button>
            <button type="button" :aria-pressed="groupDetail.group.favorite" @click="act(() => editGroup({ favorite: !groupDetail!.group.favorite }))">{{ groupDetail.group.favorite ? '取消收藏组' : '收藏整组' }}</button>
            <button v-if="!deleted" type="button" :disabled="generationPending || groupDetail.photos.some(p => p.save_status === 'pending')" @click="act(async () => { await editGroup({ deleted: true }); detail?.close() })">删除整组</button>
            <button v-if="deleted && groupDetail.group.deleted" type="button" @click="act(async () => { await editGroup({ deleted: false }); detail?.close() })">恢复整组</button>
          </div>
          <p>{{ groupDetail.group.generated_count }} / {{ groupDetail.group.expected_count }} 张成功 · {{ displayTime(groupDetail.group.created_at) }}</p>
          <p v-if="groupDetail.group.source_photo_id" class="input-hint">{{ groupDetail.source_available === false ? '来源已删除' : `复用来源组：${groupDetail.group.source_group_id} · 来源照片：${groupDetail.group.source_photo_id}` }}</p>
          <details v-if="groupDetail.group.confirmed_plan"><summary>确认计划</summary><pre>{{ JSON.stringify(groupDetail.group.confirmed_plan, null, 2) }}</pre></details>
          <details v-if="groupDetail.group.generation_errors.length"><summary>生成失败详情</summary><p v-for="failure in groupDetail.group.generation_errors" :key="failure">{{ failure }}</p></details>
        </fieldset>
            </div>
          </div>
        </div>
        <div class="artwork-detail">
          <div class="artwork-stage">
            <div class="artwork-image-area"><img v-if="fullImage" class="artwork-original" :src="fullImage" :alt="selected.title" /><p v-else role="status">原图文件缺失；生成条件与元信息已保留。</p></div>
            <nav ref="thumbnails" class="photo-navigation artwork-thumbnails" aria-label="组内照片">
              <button v-for="photo in groupDetail.photos" :key="photo.id" type="button" :disabled="busy" :aria-label="'第 ' + (photo.position + 1) + ' 张：' + photo.title + (search.trim() && matchingIds.includes(photo.id) ? ' · 匹配' : '') + (photo.save_status === 'pending' ? ' · 待保存' : '')" :aria-pressed="selected.id === photo.id" @click="act(() => selectPhoto(photo.id))">
                <img v-if="groupDetail.thumbnails?.[photo.id] || (photo.id === selected.id && fullImage)" :src="groupDetail.thumbnails?.[photo.id] || fullImage" alt="" /><span v-else class="thumbnail-placeholder">{{ photo.save_status === 'pending' ? '待保存' : '暂无预览' }}</span>
                <span v-if="search.trim() && matchingIds.includes(photo.id)" class="thumbnail-badge">匹配</span><span v-else-if="photo.save_status === 'pending'" class="thumbnail-badge">待保存</span>
              </button>
            </nav>
            <div class="artwork-image-footer"><span>{{ groupDetail.photos.findIndex(photo => photo.id === selected!.id) + 1 }} / {{ groupDetail.photos.length }}</span><button type="button" :disabled="!fullImage" aria-label="放大查看图片" @click="zoom?.showModal()">⛶</button></div>
          </div>
          <div class="artwork-info">
            <div :key="selected.id" class="artwork-info-content">
              <h2 id="artwork-title">作品详情</h2>
              <div class="artwork-heading">
                <div class="artwork-heading-copy"><h3 class="artwork-name">{{ selected.title }}</h3><p class="input-hint">{{ displayTime(selected.created_at) }} · 第 {{ selected.position + 1 }} 张</p><div class="artwork-tags"><span v-for="tag in selected.tags" :key="tag">{{ tag }}</span></div></div>
                <div class="artwork-icon-actions">
                  <button type="button" class="favorite-artwork" :disabled="busy || selected.save_status === 'pending'" :aria-label="selected.favorite ? '取消收藏此图' : '收藏此图'" :title="selected.favorite ? '取消收藏此图' : '收藏此图'" :aria-pressed="selected.favorite" @click="act(() => update({ favorite: !selected!.favorite }))"><svg viewBox="0 0 24 24" width="23" height="23" aria-hidden="true"><path d="M12 21 3.5 12.5C-2 7 6 0 12 6c6-6 14 1 8.5 6.5Z" /></svg></button>
                  <div class="detail-menu"><button ref="moreButton" type="button" aria-label="更多作品操作" :aria-expanded="menu === 'photo'" aria-controls="photo-menu" @click="toggleMenu('photo')">···</button><fieldset v-if="menu === 'photo'" id="photo-menu" class="detail-popover action-popover" :disabled="busy"><button type="button" :disabled="selected.save_status === 'pending'" @click="startEditing">编辑标题与标签</button>
                <button v-if="!selected.deleted && selected.save_status !== 'pending'" type="button" :disabled="!fullImage || groupDetail?.group.cover_id === selected.id" @click="act(async () => { groupDetail!.group = await setGroupCover(groupDetail!.group.id, selected!.id); await load() })">设为组封面</button>
                <button v-if="!selected.deleted" type="button" :disabled="generationPending || selected.save_status === 'pending'" @click="act(async () => { await update({ deleted: true }); detail?.close() })">删除此图</button>
                <template v-else>
                  <button type="button" @click="act(async () => { await update({ deleted: false }); detail?.close() })">恢复此图</button>
                  <button type="button" @click="confirmPurge = true">永久删除此图</button>
                </template>
                  </fieldset></div>
                </div>
              </div>
              <fieldset v-if="editing" class="artwork-editor" :disabled="busy"><legend>编辑作品</legend><label>标题 <input ref="titleInput" v-model="title" maxlength="200" /></label><label>标签（逗号分隔）<input v-model="tags" /></label><div class="artwork-actions"><button type="button" @click="act(async () => { await update({ title, tags: tags.split(/[,，]/).map(t => t.trim()).filter(Boolean) }); editing = false; moreButton?.focus() })">保存标题与标签</button><button type="button" @click="cancelEditing">取消编辑</button></div></fieldset>
              <section class="artwork-description"><h3>创作描述</h3><p class="preserve-text">{{ selected.prompt }}</p></section>
              <section class="artwork-settings"><h3>创作设定</h3><dl><template v-for="category in optionCategories" :key="category.key"><dt>{{ settingLabels[category.key] }}</dt><dd>{{ selected.snapshot[category.key]?.name ?? (category.key === 'layout' ? '自由构图' : '自动') }}</dd></template></dl></section>
              <details class="artwork-disclosure"><summary>完整提示词与参数</summary><div v-for="category in optionCategories" :key="category.key"><h3>{{ category.label }}</h3><p>{{ selected.snapshot[category.key]?.id ?? '未选择 · 自由生成' }}</p><p class="preserve-text">{{ selected.snapshot[category.key]?.prompt }}</p></div><h3>实际提交的完整提示词</h3><p class="preserve-text">{{ selected.submitted_prompt }}</p><p>模型：{{ selected.model ?? '未提供' }} · 工具版本：{{ selected.tool_version ?? '未提供' }} · 应用：{{ selected.app_version }}</p><p>图鉴版本：{{ selected.snapshot.catalog_version }}</p><p class="input-hint">复用生成条件不保证再次得到完全相同的图片。</p></details>
              <details class="artwork-disclosure"><summary>文件信息<span class="file-summary">{{ selected.width }} × {{ selected.height }} · {{ selected.format.toUpperCase() }}</span></summary><p>{{ (selected.file_size / 1024).toFixed(1) }} KB · {{ displayTime(selected.created_at) }}</p><p>{{ fullImage ? '原图可用' : '原图文件缺失' }} · {{ selected.save_status === 'pending' ? '待保存' : '已保存' }}</p></details>
              <div v-if="selected.save_status === 'pending'" class="artwork-save-warning"><p>此图尚未保存，请重试。</p><button type="button" :disabled="busy" @click="act(savePhoto)">重试保存此图</button></div>
              <fieldset v-if="confirmPurge" :disabled="busy">
                <div class="purge-confirm" role="alert">
                <p>永久删除原图和全部记录？此操作无法恢复。</p>
                <button type="button" @click="act(async () => { await purgeArtwork(selected!.id); detail?.close(); await load() })">确认永久删除</button>
                <button type="button" @click="confirmPurge = false">取消</button>
              </div>
              </fieldset>
              <p v-if="error" role="alert" class="error">{{ error }}</p><p role="status">{{ busy ? '正在处理…' : notice }}</p>
            </div>
            <footer class="artwork-footer"><div class="artwork-primary-actions">
              <button type="button" class="reuse-artwork" :disabled="busy || selected.deleted || generationPending || selected.save_status === 'pending'" @click="reuse">复用创作</button>
              <div class="detail-menu export-menu"><button type="button" :aria-expanded="menu === 'export'" aria-controls="export-menu" @click="toggleMenu('export')">导出 <span aria-hidden="true">⌄</span></button><div v-if="menu === 'export'" id="export-menu" class="detail-popover action-popover"><label><input v-model="includeNotes" type="checkbox" :disabled="busy" /> 同时导出提示词说明（.txt）</label><button type="button" :disabled="busy || !fullImage" @click="act(async () => { if (await exportArtwork(selected!.id, includeNotes)) notice = '已导出'; menu = null })">导出此图</button><button type="button" :disabled="busy" @click="act(async () => { if (await exportGroup(groupDetail!.group.id, includeNotes, deleted)) notice = '已导出整组'; menu = null })">导出整组</button></div></div>
            </div><p class="input-hint">使用相同设定，开启新的创作</p></footer>
          </div>
        </div>
        <dialog ref="zoom" class="artwork-zoom" aria-label="放大查看图片" @click="($event.target === zoom) && zoom?.close()"><button type="button" @click="zoom?.close()">关闭放大</button><img v-if="fullImage" :src="fullImage" :alt="selected.title" /></dialog>
      </template>
    </dialog>
  </section>
</template>

<style scoped>
.artwork-dialog { width: min(1220px, calc(100vw - 32px)); height: min(850px, calc(100dvh - 32px)); max-height: calc(100dvh - 32px); padding: 0; overflow: hidden; border-radius: 4px; background: #fff; }
.artwork-dialog[open] { display: flex; flex-direction: column; }
.artwork-header, .artwork-groupbar { flex: none; display: flex; align-items: center; gap: 24px; padding: 10px 28px; border-bottom: 1px solid var(--border); background: #faf9f7; }
.artwork-header .brand { font-size: 23px; letter-spacing: -.5px; }
.artwork-header > button { margin-left: auto; border: 0; background: transparent; padding: 0 6px; font-size: 24px; }
.artwork-breadcrumb { border-left: 1px solid var(--border); padding-left: 24px; font-size: 14px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.artwork-groupbar > strong { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 18px; }
.artwork-groupbar > .input-hint { white-space: nowrap; }
.group-management { margin-left: auto; flex-shrink: 0; }
.artwork-detail { flex: 1; min-height: 0; gap: 0; grid-template-columns: minmax(0, 1.6fr) minmax(340px, 1fr); }
.artwork-stage { min-width: 0; min-height: 0; display: flex; flex-direction: column; align-items: center; padding: 24px 24px 12px; background: #f8f7f4; }
.artwork-image-area { flex: 1; min-height: 0; width: 100%; display: flex; align-items: center; justify-content: center; }
.artwork-original { position: static; width: 100%; height: 100%; max-height: 100%; object-fit: contain; }
.artwork-thumbnails { flex: none; flex-wrap: nowrap; max-width: 100%; overflow-x: auto; gap: 10px; padding: 4px; margin: 12px 0 0; }
.artwork-thumbnails button { position: relative; flex: 0 0 78px; width: 78px; height: 82px; padding: 3px; border: 2px solid transparent; background: transparent; }
.artwork-thumbnails button[aria-pressed="true"] { border-color: var(--green); background: white; }
.artwork-thumbnails img { width: 100%; height: 100%; object-fit: cover; border-radius: 3px; }
.thumbnail-placeholder { font-size: 12px; color: var(--muted); }
.thumbnail-badge { position: absolute; bottom: 4px; left: 4px; right: 4px; background: #fffD; font-size: 11px; border-radius: 3px; }
.artwork-image-footer { position: relative; width: 100%; flex: none; text-align: center; padding: 8px 32px; font-size: 14px; }
.artwork-image-footer button { position: absolute; right: 0; top: 0; padding: 3px 7px; font-size: 22px; border: 0; background: transparent; }
.artwork-info { display: flex; flex-direction: column; min-height: 0; border-left: 1px solid var(--border); line-height: 1.6; }
.artwork-info-content { flex: 1; min-height: 0; overflow-y: auto; padding: 20px 28px; }
.artwork-info h2 { font-size: 13px; font-weight: 400; margin-bottom: 14px; }
.artwork-heading { display: flex; align-items: flex-start; gap: 10px; }
.artwork-heading-copy { flex: 1; min-width: 0; }
.artwork-info .artwork-name { font-size: 26px; line-height: 1.3; margin: 0 0 6px; }
.artwork-heading .input-hint { margin: 0; }
.artwork-icon-actions { display: flex; flex: none; gap: 2px; }
.artwork-icon-actions button { padding: 5px; border: 0; background: transparent; min-width: 32px; }
.favorite-artwork svg { display: block; fill: none; stroke: currentColor; stroke-width: 1.7; }
.favorite-artwork[aria-pressed="true"] svg { fill: var(--green); stroke: var(--green); }
.artwork-tags { display: flex; gap: 6px; flex-wrap: wrap; margin-top: 10px; }
.artwork-tags span { padding: 2px 9px; background: #eef0ed; border-radius: 6px; font-size: 13px; }
.artwork-description, .artwork-settings { border-top: 1px solid var(--border); margin-top: 20px; padding-top: 14px; }
.artwork-info h3 { font-size: 15px; margin: 0 0 8px; }
.artwork-settings dl { display: grid; grid-template-columns: 70px minmax(0, 1fr); gap: 8px 16px; margin: 0 0 18px; font-size: 14px; }
.artwork-settings dd { margin: 0; }
.artwork-info .artwork-disclosure { border-top: 1px solid var(--border); margin: 0; padding: 10px 0; }
.artwork-disclosure summary { position: relative; list-style: none; cursor: pointer; padding-right: 20px; font-weight: 600; }
.artwork-disclosure summary::-webkit-details-marker { display: none; }
.artwork-disclosure summary::after { content: '›'; position: absolute; right: 2px; top: 0; }
.artwork-disclosure[open] summary::after { transform: rotate(90deg); }
.artwork-disclosure[open] summary { margin-bottom: 12px; }
.artwork-disclosure p { margin-bottom: 10px; }
.file-summary { display: block; font-size: 13px; font-weight: 400; color: var(--muted); }
.artwork-footer { flex: none; padding: 16px 28px 12px; border-top: 1px solid var(--border); background: white; }
.artwork-primary-actions { display: grid; grid-template-columns: 1.6fr 1fr; gap: 10px; }
.artwork-primary-actions > button, .export-menu > button { width: 100%; font-size: 16px; font-weight: 600; }
.artwork-primary-actions .reuse-artwork { background: var(--green); border-color: var(--green); color: white; }
.artwork-footer .input-hint { margin: 8px 0 0; text-align: center; }
.detail-menu { position: relative; }
.detail-popover { position: absolute; z-index: 5; right: 0; top: calc(100% + 6px); width: min(340px, calc(100vw - 64px)); max-height: 55dvh; overflow: auto; border: 1px solid var(--border); border-radius: 8px; padding: 14px; background: white; box-shadow: 0 8px 28px #202d2826; font-size: 14px; }
.detail-popover .group-info { padding: 0; border: 0; margin: 0; }
.detail-popover input:not([type="checkbox"]) { width: 100%; }
.detail-popover p { margin-top: 12px; }
.detail-popover pre { white-space: pre-wrap; overflow-wrap: anywhere; }
.artwork-info fieldset.detail-popover { padding: 8px; margin: 0; border: 1px solid var(--border); }
.action-popover { width: 240px; display: flex; flex-direction: column; gap: 6px; }
.action-popover button { text-align: left; font-size: 14px; padding: 9px; }
.action-popover label { margin: 0; }
.export-menu .detail-popover { top: auto; bottom: calc(100% + 8px); }
.artwork-info .artwork-editor { margin-top: 14px; padding: 12px; border: 1px solid var(--border); border-radius: 6px; }
.artwork-save-warning { padding: 12px; background: #fff5df; border-radius: 6px; }
.artwork-zoom { width: min(1200px, 96vw); height: min(1000px, 94dvh); max-width: 96vw; max-height: 94dvh; padding: 16px; border: 1px solid var(--border); border-radius: 8px; }
.artwork-zoom[open] { display: flex; flex-direction: column; gap: 12px; }
.artwork-zoom::backdrop { background: #202d28bb; }
.artwork-zoom button { align-self: flex-end; }
.artwork-zoom img { min-height: 0; flex: 1; width: 100%; object-fit: contain; }
@media (max-width: 900px) {
  .artwork-dialog { overflow-y: auto; }
  .artwork-header, .artwork-groupbar { padding: 10px 16px; gap: 12px; }
  .artwork-header .brand { font-size: 20px; }
  .artwork-breadcrumb { padding-left: 12px; }
  .artwork-groupbar { flex-wrap: wrap; }
  .artwork-groupbar > strong { max-width: 50%; }
  .artwork-detail { flex: none; grid-template-columns: minmax(0, 1fr); }
  .artwork-stage { padding: 16px 12px 8px; }
  .artwork-image-area { flex: none; height: 42dvh; }
  .artwork-thumbnails button { flex-basis: 66px; width: 66px; height: 70px; }
  .artwork-info { border-left: 0; border-top: 1px solid var(--border); }
  .artwork-info-content { overflow: visible; padding: 20px 16px; }
  .artwork-footer { padding: 14px 16px; }
  .artwork-info .artwork-name { font-size: 22px; }
  .group-management .detail-popover { max-height: 65dvh; }
}
</style>
