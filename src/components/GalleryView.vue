<script setup lang="ts">
import { nextTick, onBeforeUnmount, ref, watch } from 'vue'
import { isDesktop } from '../services/imageGeneration'
import { displayTime, exportArtwork, exportGroup, getArtwork, getGroup, listGallery, message, purgeArtwork, retrySave, updateArtwork, updateGroup, type Artwork, type GalleryCard, type GalleryPage, type GroupDetail } from '../services/gallery'
import { optionCategories } from '../services/generationOptions'

const props = defineProps<{ generationPending: boolean; revision?: string }>()
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
onBeforeUnmount(() => { request++; detail.value?.close() })

async function selectPhoto(id: string) {
  const [artwork, image] = await getArtwork(id)
  selected.value = artwork; fullImage.value = image
  title.value = artwork.title; tags.value = artwork.tags.join(', ')
  confirmPurge.value = false
}

async function open(card: GalleryCard, event: Event) {
  if (busy.value) return
  busy.value = true
  error.value = ''; notice.value = ''
  opener = event.currentTarget as HTMLElement
  try {
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
  } catch (cause) { error.value = message(cause) }
  finally { busy.value = false }
}

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
  if ('title' in changes) title.value = a.title
  if ('tags' in changes) tags.value = a.tags.join(', ')
  notice.value = '已更新'
  await load()
}

function reuse() {
  if (!selected.value) return
  emit('reuse', selected.value)
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

    <dialog ref="detail" class="artwork-dialog" aria-labelledby="artwork-title" @close="opener?.focus()" @cancel="busy && $event.preventDefault()">
      <template v-if="selected">
        <div class="section-heading"><h2 id="artwork-title">作品详情</h2><button type="button" :disabled="busy" aria-label="关闭作品详情" @click="detail?.close()">×</button></div>
        <fieldset v-if="groupDetail" class="group-info" :disabled="busy">
          <legend>照片组</legend>
          <label>组标题 <input v-model="groupTitle" maxlength="200" /></label>
          <label>组标签（逗号分隔）<input v-model="groupTags" /></label>
          <div class="artwork-actions">
            <button type="button" @click="act(() => editGroup({ title: groupTitle, tags: groupTags.split(/[,，]/).map(t => t.trim()).filter(Boolean) }))">保存组标题与标签</button>
            <button type="button" :aria-pressed="groupDetail.group.favorite" @click="act(() => editGroup({ favorite: !groupDetail!.group.favorite }))">{{ groupDetail.group.favorite ? '取消收藏组' : '收藏整组' }}</button>
            <button type="button" @click="act(async () => { if (await exportGroup(groupDetail!.group.id, includeNotes, deleted)) notice = '已导出整组' })">导出整组</button>
            <button v-if="!deleted" type="button" :disabled="generationPending || groupDetail.photos.some(p => p.save_status === 'pending')" @click="act(async () => { await editGroup({ deleted: true }); detail?.close() })">删除整组</button>
            <button v-if="deleted && groupDetail.group.deleted" type="button" @click="act(async () => { await editGroup({ deleted: false }); detail?.close() })">恢复整组</button>
          </div>
          <p>{{ groupDetail.group.generated_count }} / {{ groupDetail.group.expected_count }} 张成功 · {{ displayTime(groupDetail.group.created_at) }}</p>
          <p v-if="groupDetail.group.source_photo_id" class="input-hint">复用来源组：{{ groupDetail.group.source_group_id }} · 来源照片：{{ groupDetail.group.source_photo_id }}</p>
          <details v-if="groupDetail.group.generation_errors.length"><summary>生成失败详情</summary><p v-for="failure in groupDetail.group.generation_errors" :key="failure">{{ failure }}</p></details>
          <nav class="photo-navigation" aria-label="组内照片">
            <button v-for="(photo, index) in groupDetail.photos" :key="photo.id" type="button" :aria-pressed="selected.id === photo.id" @click="act(() => selectPhoto(photo.id))">第 {{ index + 1 }} 张{{ search.trim() && matchingIds.includes(photo.id) ? ' · 匹配' : '' }}{{ photo.save_status === 'pending' ? ' · 待保存' : '' }}</button>
          </nav>
        </fieldset>
        <div class="artwork-detail">
          <img class="artwork-original" :src="fullImage" :alt="selected.title" />
          <div class="artwork-info">
            <fieldset :disabled="busy">
              <label>标题 <input v-model="title" maxlength="200" /></label>
              <label>标签（逗号分隔）<input v-model="tags" /></label>
              <button type="button" :disabled="selected.save_status === 'pending'" @click="act(() => update({ title, tags: tags.split(/[,，]/).map(t => t.trim()).filter(Boolean) }))">保存标题与标签</button>
              <h3>原始提示词</h3><p class="preserve-text">{{ selected.prompt }}</p>
              <div v-for="category in optionCategories" :key="category.key">
                <h3>{{ category.label }}</h3>
                <p>{{ selected.snapshot[category.key] ? `${selected.snapshot[category.key]!.id} · ${selected.snapshot[category.key]!.name}` : '未选择 · 自由生成' }}</p>
                <details v-if="selected.snapshot[category.key]"><summary>当时的完整规范</summary><p class="preserve-text">{{ selected.snapshot[category.key]!.prompt }}</p></details>
              </div>
              <p class="input-hint">{{ displayTime(selected.created_at) }} · {{ selected.width }} × {{ selected.height }} · {{ selected.format.toUpperCase() }} · {{ (selected.file_size / 1024).toFixed(1) }} KB</p>
              <details><summary>生成来源与完整提示词</summary>
                <p>模型：{{ selected.model ?? '未提供' }} · 工具版本：{{ selected.tool_version ?? '未提供' }} · 应用：{{ selected.app_version }}</p>
                <p>图鉴版本：{{ selected.snapshot.catalog_version }}</p>
                <p class="preserve-text">{{ selected.submitted_prompt }}</p>
              </details>
              <p class="input-hint">复用生成条件不保证再次得到完全相同的图片。</p>
              <label><input v-model="includeNotes" type="checkbox" /> 同时导出提示词说明（.txt）</label>
              <div class="artwork-actions">
                <button v-if="selected.save_status === 'pending'" type="button" @click="act(savePhoto)">重试保存此图</button>
                <button v-if="!selected.deleted" type="button" :disabled="generationPending || selected.save_status === 'pending'" @click="reuse">复用创作</button>
                <button type="button" @click="act(async () => { if (await exportArtwork(selected!.id, includeNotes)) notice = '已导出' })">导出此图</button>
                <button type="button" :disabled="selected.save_status === 'pending'" :aria-pressed="selected.favorite" @click="act(() => update({ favorite: !selected!.favorite }))">{{ selected.favorite ? '取消收藏此图' : '收藏此图' }}</button>
                <button v-if="!selected.deleted" type="button" :disabled="selected.save_status === 'pending'" @click="act(async () => { await update({ deleted: true }); detail?.close() })">删除此图</button>
                <template v-else>
                  <button type="button" @click="act(async () => { await update({ deleted: false }); detail?.close() })">恢复此图</button>
                  <button type="button" @click="confirmPurge = true">永久删除此图</button>
                </template>
              </div>
              <div v-if="confirmPurge" class="purge-confirm" role="alert">
                <p>永久删除原图和全部记录？此操作无法恢复。</p>
                <button type="button" @click="act(async () => { await purgeArtwork(selected!.id); detail?.close(); await load() })">确认永久删除</button>
                <button type="button" @click="confirmPurge = false">取消</button>
              </div>
            </fieldset>
            <p v-if="error" role="alert" class="error">{{ error }}</p>
            <p role="status">{{ busy ? '正在处理…' : notice }}</p>
          </div>
        </div>
      </template>
    </dialog>
  </section>
</template>
