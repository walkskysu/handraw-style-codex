<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { active, itemImage, label, type Item, type JobSnapshot } from '../services/creation'
import { exportArtwork, message } from '../services/gallery'
const props = defineProps<{ job: JobSnapshot; disabled: boolean }>()
const emit = defineEmits<{ stop: []; resume: []; retry: [Item]; save: [Item]; discard: [Item] }>()
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
    <h3 id="job-title">{{ job.plan?.content.title ?? '快速生成' }}</h3>
    <p role="status" aria-live="polite">{{ label(job.status) }} · 已生成 {{ job.generated_count }} / {{ job.expected_count }} 张 · 已保存 {{ job.saved_count }} 张</p>
    <p v-if="job.plan">按第 {{ job.plan.version }} 版计划执行</p>
    <figure v-if="image" class="image-preview"><img :src="image" :alt="item?.title" /><figcaption>第 {{ (item?.position ?? 0) + 1 }} / {{ job.expected_count }} 张 · {{ item?.title }}</figcaption></figure>
    <div v-else class="preview-placeholder"><p>第 {{ (item?.position ?? 0) + 1 }} 张 · {{ label(item?.generation_status ?? 'queued') }}</p></div>
    <nav v-if="job.items.length > 1" class="photo-navigation" aria-label="任务照片" @keydown="move">
      <button v-for="i in job.items" :key="i.id" type="button" :aria-current="selected === i.position ? 'true' : undefined" @click="select(i.position)">{{ i.position + 1 }} · {{ label(i.save_status === 'saved' ? 'saved' : i.generation_status) }}</button>
    </nav>
    <label v-if="job.items.length > 1"><input v-model="follow" type="checkbox" /> 跟随最新结果</label>
    <p v-if="item?.error" role="alert">{{ item.error }}</p>
    <details v-if="item"><summary>本图信息与计划</summary><p>{{ item.title }}</p><pre>{{ item.prompt }}</pre><p>{{ label(item.save_status) }} · 尝试 {{ item.attempt_no }}</p></details>
    <details v-if="job.plan"><summary>查看确认计划</summary><pre>{{ JSON.stringify(job.plan, null, 2) }}</pre></details>
    <button v-if="active(job.status)" type="button" :disabled="disabled || job.status === 'stopping'" @click="emit('stop')">{{ job.status === 'stopping' ? '当前图片完成后停止' : '停止后续生成' }}</button>
    <button v-else-if="job.status !== 'completed' && !job.pending_save_count" type="button" :disabled="disabled" @click="emit('resume')">继续生成</button>
    <button v-if="item && ['failed', 'interrupted', 'cancelled'].includes(item.generation_status)" type="button" :disabled="disabled || active(job.status)" @click="emit('retry', item)">仅重试第 {{ item.position + 1 }} 张</button>
    <template v-if="item?.generation_status === 'generated' && !['saved', 'discarded'].includes(item.save_status)">
      <button type="button" :disabled="disabled || active(job.status)" @click="emit('save', item)">重试保存此图</button>
      <button type="button" :disabled="disabled || active(job.status)" @click="discardConfirmation = true">放弃此图暂存</button>
      <div v-if="discardConfirmation" role="alert"><p>仅放弃第 {{ item.position + 1 }} 张未保存的结果。此图不会入库，关闭后无法找回；请先导出需要保留的图片。</p><button type="button" @click="emit('discard', item); discardConfirmation = false">确认放弃此图</button><button type="button" @click="discardConfirmation = false">取消</button></div>
    </template>
    <button v-if="item?.generation_status === 'generated' && item.save_status !== 'discarded'" type="button" :disabled="exporting || !image" @click="exportImage">导出此图</button>
    <p v-if="error" role="alert">{{ error }}</p><p aria-live="polite">{{ notice }}</p>
  </section>
</template>
