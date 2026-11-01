<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { active, itemImage, label, type Item, type JobSnapshot } from '../services/creation'
import { exportArtwork, message } from '../services/gallery'
import { taskStatus } from '../services/creationTasks'
import PlanPanel from './PlanPanel.vue'
const props = defineProps<{ job: JobSnapshot; disabled: boolean }>()
const emit = defineEmits<{ stop: []; resume: []; retry: [Item]; save: [Item]; discard: [Item]; gallery: [] }>()
const zoom = ref<HTMLDialogElement | null>(null)
const selected = ref(0)
const follow = ref(true)
const latestPosition = ref(0)
const image = ref('')
const error = ref('')
const notice = ref('')
const exporting = ref(false)
const discardConfirmation = ref(false)
const item = computed(() => props.job.items[selected.value])
let request = 0
watch(() => props.job.id, () => { selected.value = 0; latestPosition.value = 0; follow.value = true })
watch(() => props.job, (j, previous) => {
  const generated = j.items.filter(i => i.generation_status === 'generated' && i.save_status !== 'discarded')
  const newlyGenerated = generated.filter(i => !previous || previous.id !== j.id || !previous.items.some(old =>
    old.id === i.id && old.attempt_id === i.attempt_id && old.generation_status === 'generated'))
  // A retry at an earlier position is newer than the later pictures already on screen.
  const newest = newlyGenerated.at(-1) ?? (!previous || previous.id !== j.id ? generated.at(-1) : undefined)
  if (newest) latestPosition.value = newest.position
  if (follow.value) selected.value = latestPosition.value
}, { immediate: true })
watch(follow, enabled => { if (enabled) selected.value = latestPosition.value })
watch([() => item.value?.id, () => item.value?.attempt_id, () => item.value?.generation_status, () => item.value?.save_status], async () => {
  const current = ++request; image.value = ''; error.value = ''; notice.value = ''; discardConfirmation.value = false
  if (item.value?.generation_status !== 'generated' || item.value.save_status === 'discarded') return
  try { const value = await itemImage(item.value.id); if (current === request) image.value = value }
  catch (e) { if (current === request) error.value = message(e) }
}, { immediate: true })
function select(n: number) { follow.value = false; selected.value = n }
function move(e: KeyboardEvent) {
  if (!['ArrowLeft', 'ArrowRight'].includes(e.key)) return
  e.preventDefault(); select((selected.value + (e.key === 'ArrowRight' ? 1 : -1) + props.job.items.length) % props.job.items.length)
}
async function exportImage() {
  if (!item.value?.artwork_id) return
  exporting.value = true
  try { if (await exportArtwork(item.value.artwork_id, true)) notice.value = '已导出此图及完整提示词' }
  catch (e) { error.value = message(e) }
  finally { exporting.value = false }
}
</script>
<template>
  <section class="job-preview" aria-labelledby="job-title">
    <div class="result-heading"><h3 id="job-title">{{ job.plan?.content.title || job.input?.prompt.slice(0, 80) || job.items[0]?.title || '创作任务' }}</h3><span class="result-badge" :data-state="taskStatus(job) === '已完成' ? 'completed' : active(job.status) ? 'active' : 'attention'">{{ taskStatus(job) }}</span></div>
    <p class="result-summary" role="status" aria-live="polite">{{ job.mode === 'quick' ? '快速生成' : '规划生成' }} · 已生成 {{ job.generated_count }} / {{ job.expected_count }} 张<span v-if="job.saved_count !== job.expected_count"> · 已保存 {{ job.saved_count }} 张</span></p>
    <progress v-if="active(job.status)" :value="job.generated_count" :max="job.expected_count" aria-label="生成进度" />
    <p v-if="job.plan">按第 {{ job.plan.version }} 版计划执行</p>
    <figure v-if="image" class="image-preview"><button type="button" class="image-enlarge" aria-label="放大查看" @click="zoom?.showModal()"><img :src="image" :alt="item?.title" /></button><figcaption>第 {{ (item?.position ?? 0) + 1 }} / {{ job.expected_count }} 张</figcaption></figure>
    <dialog ref="zoom" class="image-zoom" aria-label="放大查看图片" @click="($event.target === zoom) && zoom?.close()"><button type="button" @click="zoom?.close()">关闭放大</button><img v-if="image" :src="image" :alt="item?.title" /></dialog>
    <div v-if="!image" class="preview-placeholder"><p>第 {{ (item?.position ?? 0) + 1 }} 张 · {{ label(item?.generation_status ?? 'queued') }}</p></div>
    <nav v-if="job.items.length > 1" class="photo-navigation" aria-label="任务照片" @keydown="move">
      <button v-for="i in job.items" :key="i.id" type="button" :aria-current="selected === i.position ? 'true' : undefined" @click="select(i.position)">{{ i.position + 1 }} · {{ label(i.generation_status === 'generated' ? i.save_status : i.generation_status) }}</button>
    </nav>
    <label v-if="job.items.length > 1"><input v-model="follow" type="checkbox" /> 跟随最新结果</label>
    <p v-if="item?.error" role="alert">{{ item.error }}</p>
    <p v-if="item?.save_status === 'saved'" class="saved-notice"><span aria-hidden="true">✓</span> 已保存到图库</p>
    <div class="result-actions">
    <button v-if="active(job.status)" type="button" :disabled="disabled || job.status === 'stopping'" @click="emit('stop')">{{ job.status === 'stopping' ? '当前图片完成后停止' : '停止后续生成' }}</button>
    <button v-else-if="job.status !== 'completed' && !job.pending_save_count" type="button" :disabled="disabled" @click="emit('resume')">继续生成</button>
    <button v-if="item && ['failed', 'interrupted', 'cancelled'].includes(item.generation_status)" type="button" :disabled="disabled || active(job.status)" @click="emit('retry', item)">仅重试第 {{ item.position + 1 }} 张</button>
    <template v-if="item?.generation_status === 'generated' && !['saved', 'discarded'].includes(item.save_status)">
      <button type="button" :disabled="disabled || active(job.status)" @click="emit('save', item)">重试保存此图</button>
      <button type="button" :disabled="disabled || active(job.status)" @click="discardConfirmation = true">放弃此图暂存</button>
      <div v-if="discardConfirmation" role="alert"><p>仅放弃第 {{ item.position + 1 }} 张未保存的结果。此图不会入库，关闭后无法找回；请先导出需要保留的图片。</p><button type="button" @click="emit('discard', item); discardConfirmation = false">确认放弃此图</button><button type="button" @click="discardConfirmation = false">取消</button></div>
    </template>
    <button v-if="item?.generation_status === 'generated' && item.save_status !== 'discarded'" type="button" :disabled="exporting || !image" @click="exportImage">导出此图</button>
    <button v-if="job.saved_count" type="button" class="gallery-action" @click="emit('gallery')">在图库查看</button>
    </div>
    <details v-if="job.input"><summary>创作输入与配置</summary><p class="preserve-text">{{ job.input.prompt }}</p><p v-for="key in (['style', 'layout', 'color'] as const)" :key="key">{{ { style: '风格', layout: '排版', color: '配色' }[key] }}：{{ job.input.snapshot[key]?.name ?? '自动' }}</p><p>数量：{{ job.input.requested_count ?? '自动' }}</p></details>
    <details v-if="item"><summary>本图提示词</summary><p>{{ item.title }}</p><pre>{{ item.prompt }}</pre><p>{{ label(item.save_status) }} · 尝试 {{ item.attempt_no }}</p></details>
    <details v-if="job.plan"><summary>查看确认计划</summary><PlanPanel :plan="job.plan" :versions="[]" feedback="" disabled :stale="false" readonly /></details>
    <p v-if="error" role="alert">{{ error }}</p><p aria-live="polite">{{ notice }}</p>
  </section>
</template>
<style scoped>
.job-preview { display: flex; flex-direction: column; align-items: stretch; gap: 12px; min-width: 0; }
.result-heading { display: flex; align-items: center; flex-wrap: wrap; gap: 12px; }
.job-preview h3 { font-size: 24px; margin: 0; overflow-wrap: anywhere; }
.result-summary { color: var(--muted); font-size: 14px; }
.result-badge { padding: 5px 12px; border-radius: 7px; font-size: 13px; background: #fff0d4; color: #805314; }
.result-badge[data-state="completed"] { background: #dceee0; color: var(--green); }
.result-badge[data-state="active"] { background: #e1f0fa; color: #22638a; }
.image-preview { flex: none; }
.image-enlarge { border: 0; padding: 0; border-radius: 10px; background: transparent; cursor: zoom-in; }
.image-preview img { flex: none; width: 100%; height: min(42dvh, 380px); max-height: 380px; object-fit: contain; }
.saved-notice { border-top: 1px solid var(--border); padding-top: 14px; text-align: center; color: var(--green); font-size: 13px; }
.saved-notice span { display: inline-grid; place-items: center; width: 20px; height: 20px; margin-right: 6px; background: var(--green); color: white; border-radius: 50%; }
.result-actions { display: flex; flex-wrap: wrap; gap: 16px; padding-block: 4px 8px; }
.result-actions > button { flex: 1; }
.result-actions > .gallery-action { background: var(--green); color: white; }
.result-actions > .gallery-action:hover { background: #194735; }
.job-preview > details { padding: 16px; border: 1px solid #e6e8e5; border-radius: 10px; }
.job-preview > details summary { cursor: pointer; }
.job-preview > details[open] summary { margin-bottom: 16px; }
.job-preview > button { align-self: flex-end; padding: 6px 12px; font-size: 13px; }
.image-preview figcaption { margin-top: 6px; font-size: 12px; text-align: center; }
.image-zoom { max-width: 96vw; max-height: 94dvh; padding: 12px; border: 1px solid var(--border); border-radius: 10px; }
.image-zoom::backdrop { background: #202d28bb; }
.image-zoom img { display: block; max-width: 100%; height: auto; margin-top: 12px; }
.image-zoom button { position: sticky; top: 0; }
pre { white-space: pre-wrap; overflow-wrap: anywhere; }
</style>
