<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import * as api from '../services/creation'
import { isDesktop } from '../services/imageGeneration'
import { discardPending, openBackups, message, type Artwork } from '../services/gallery'
import type { GenerationSelections } from '../services/generationOptions'
import PlanPanel from './PlanPanel.vue'
import JobPreview from './JobPreview.vue'
const props = defineProps<{ prompt: string; selections: GenerationSelections; reuseId?: string; locked: boolean; controls: HTMLElement | null; modeControl: HTMLElement | null }>()
const emit = defineEmits<{ busy: [boolean]; unsaved: [boolean]; revision: [string]; restore: [string, GenerationSelections] }>()
const mode = ref<api.Mode>(localStorage.getItem('creation-mode') === 'planned' ? 'planned' : 'quick')
const count = ref<number | null>(null)
const drafts = ref<Partial<Record<api.Mode, api.Draft>>>({})
const current = computed(() => drafts.value[mode.value])
const feedback = ref('')
const plans = ref<api.Plan[]>([])
const all = ref<api.Library>({ drafts: [], plans: [], planning_jobs: [], jobs: [] })
const job = ref<api.JobSnapshot | null>(null)
const planningId = ref<string | null>(null)
const pending = ref(false)
const error = ref('')
const showLibrary = ref(false)
const mounted = ref(false)
const dirty = ref(false)
const planning = computed(() => all.value.planning_jobs.some(j => j.status === 'running'))
const busy = computed(() => pending.value || planning.value || all.value.jobs.some(j => api.active(j.status)) || !!job.value && api.active(job.value.status))
const plan = computed(() => plans.value.find(p => p.id === current.value?.current_plan_id))
const versions = computed(() => plans.value.filter(p => p.draft_id === current.value?.id))
const stale = computed(() => dirty.value || plan.value?.input_revision !== current.value?.input_revision)
let autosave: ReturnType<typeof setTimeout> | undefined
let timer: ReturnType<typeof setInterval> | undefined
let unlisten: UnlistenFn | undefined
let closeListener: UnlistenFn | undefined
let disposed = false
let saveChain = Promise.resolve()
let intentKey = ''
let intentSignature = ''
let refreshSequence = 0
watch(busy, value => emit('busy', value), { immediate: true })
watch(() => job.value, j => { emit('unsaved', !!j?.pending_save_count); emit('revision', `${j?.id}-${j?.revision}`) })
watch(mode, () => { localStorage.setItem('creation-mode', mode.value); feedback.value = current.value?.feedback_draft ?? ''; dirty.value = true })
watch(() => [props.prompt, JSON.stringify(props.selections), props.reuseId, count.value, mode.value], () => { dirty.value = true; queueSave() })
watch(feedback, queueSave)
function queueSave() {
  clearTimeout(autosave)
  if (!mounted.value || !isDesktop()) return
  autosave = setTimeout(() => { void persist().catch(e => { error.value = message(e) }) }, 350)
}
async function persist() {
  clearTimeout(autosave)
  const selectedMode = mode.value
  const values = { mode: selectedMode, prompt: props.prompt, selections: { ...props.selections }, reuse_id: props.reuseId, requested_count: count.value, feedback: feedback.value }
  // Serialize saves; an older response must not replace a newer input revision.
  const work = saveChain.catch(() => {}).then(async () => {
    let d = drafts.value[selectedMode]
    if (d && plans.value.some(p => p.draft_id === d!.id && p.confirmed_at)) d = undefined
    const result = await api.saveDraft({ ...values, id: d?.id, input_revision: d?.input_revision })
    drafts.value[selectedMode] = result
    if (mode.value === selectedMode && props.prompt === values.prompt && JSON.stringify(props.selections) === JSON.stringify(values.selections) && count.value === values.requested_count) dirty.value = false
  })
  saveChain = work
  await work
  return drafts.value[selectedMode]!
}
async function switchMode(next: api.Mode) {
  if (busy.value || mode.value === next) return
  if (!isDesktop()) { mode.value = next; return }
  // Flush feedback before switching; the debounce must not save it into the other draft.
  await act(async () => { await persist(); mode.value = next })
}
async function refresh() {
  const sequence = ++refreshSequence
  const data = await api.library()
  if (!data || !Array.isArray(data.jobs) || !Array.isArray(data.plans)) throw new Error('无法读取草稿与任务，请重试。')
  if (disposed || sequence !== refreshSequence) return
  all.value = data; plans.value = data.plans
  const p = data.planning_jobs.find(p => p.id === planningId.value)
  if (p && p.status !== 'running') {
    if (p.error) error.value = p.error
    if (p.output_plan_id) {
      const d = data.drafts.find(d => d.id === p.draft_id)
      if (d) { drafts.value.planned = d; feedback.value = d.feedback_draft }
    }
    planningId.value = null
  }
  if (job.value) {
    const selectedId = job.value.id
    const result = await api.snapshot(selectedId)
    if (!disposed && job.value?.id === selectedId) job.value = api.acceptSnapshot(job.value, result)
  }
}
async function act(action: () => Promise<unknown>) {
  if (pending.value) return
  pending.value = true; error.value = ''
  try { await action(); await refresh() } catch (e) { error.value = message(e) } finally { pending.value = false }
}
async function openJob(id: string) { job.value = await api.snapshot(id); showLibrary.value = false }
async function submit() {
  if (busy.value || props.locked || !props.prompt.trim() || !isDesktop()) return
  showLibrary.value = false
  await act(async () => {
    const d = await persist()
    if (mode.value === 'planned') { planningId.value = await api.planCreate(d); return }
    const signature = `${d.id}:${d.input_revision}`
    if (intentSignature !== signature) { intentSignature = signature; intentKey = crypto.randomUUID() }
    const j = await api.start(d, intentKey); await openJob(j.id)
    // A later deliberate click creates a new draft; retries after lost responses reuse the key.
    drafts.value.quick = undefined
  })
}
async function confirm() {
  if (busy.value || stale.value || feedback.value.trim() || !plan.value || props.locked) return
  const selected = plan.value
  await act(async () => {
    const d = await persist()
    const j = await api.start(d, `confirm-${selected.id}`, selected)
    await openJob(j.id)
  })
}
async function revise() {
  if (busy.value || !plan.value) return
  const base = plan.value.id
  await act(async () => { planningId.value = await api.planCreate(await persist(), base) })
}
function restore(d: api.Draft) {
  if (busy.value) return
  mode.value = d.mode; drafts.value[d.mode] = d; count.value = d.requested_count; feedback.value = d.feedback_draft
  emit('restore', d.prompt, Object.fromEntries(['style', 'layout', 'color'].flatMap(k => {
    const option = d.snapshot[k as keyof GenerationSelections]; return option ? [[k, option.id]] : []
  })))
  dirty.value = false; showLibrary.value = false
}
function photoSaved(_a: Artwork) { void refresh() }
function chooseVersion(id: string) { return act(async () => { await persist(); const p = await api.selectVersion(current.value!.id, id); plans.value.push(p); current.value!.current_plan_id = p.id }) }
function stopJob() { return act(() => api.stop(job.value!.id)) }
function resumeJob(item?: api.Item) { return act(() => api.resume(job.value!, item)) }
function saveItem(item: api.Item) { return act(() => api.retrySave(item.id)) }
function discardItem(item: api.Item) { return act(() => discardPending(item.id)) }
async function openLibrary() {
  showLibrary.value = true
  if (isDesktop()) await act(refresh)
}
defineExpose({ submit, refresh, photoSaved, openLibrary })
onMounted(async () => {
  mounted.value = true
  if (!isDesktop()) return
  try {
    // Subscribe before the initial snapshot. Polling remains a fallback if events are unavailable.
    try { unlisten = await listen<api.Notice>('creation-changed', ({ payload }) => {
      if (payload.job_id === job.value?.id && payload.revision <= job.value.revision) return
      void refresh().catch(e => { error.value = message(e) })
    }) } catch { /* snapshot polling works without the event bridge */ }
    try { closeListener = await listen<string>('creation-close-blocked', e => { error.value = e.payload }) } catch { /* optional notification */ }
    await refresh()
    if (disposed) { unlisten?.(); closeListener?.(); return }
    const latest = all.value.jobs[0]
    if (latest) await openJob(latest.id)
    timer = setInterval(() => { void refresh().catch(e => { error.value = message(e) }) }, 2500)
  } catch (e) { error.value = message(e) }
})
onBeforeUnmount(() => { disposed = true; clearTimeout(autosave); clearInterval(timer); unlisten?.(); closeListener?.() })
</script>
<template>
  <Teleport v-if="mounted && modeControl" :to="modeControl">
    <button type="button" class="mode-switch" role="switch" aria-label="快速生成" :aria-checked="mode === 'quick'" :disabled="busy"
      :title="mode === 'quick' ? '关闭快速生成，先规划再确认' : '开启快速生成，每次生成一张图片'" @click="switchMode(mode === 'quick' ? 'planned' : 'quick')">
      <span>{{ mode === 'quick' ? '快速生成' : '规划生成' }}</span><span class="switch-track" aria-hidden="true"></span>
    </button>
  </Teleport>
  <Teleport v-if="mounted && controls" :to="controls">
    <label v-if="mode === 'planned'">生成数量 <select v-model="count" :disabled="busy"><option :value="null">自动规划</option><option v-for="n in 12" :key="n" :value="n">{{ n }} 张</option></select></label>
    <button type="submit" :disabled="busy || locked || !prompt.trim() || !isDesktop()">{{ busy ? '正在处理…' : mode === 'quick' ? '生成图片 ↗' : '生成计划' }}</button>
    <p id="generation-hint" class="input-hint">{{ mode === 'quick' ? '每次生成 1 张图片' : count ? `本次按所选数量 ${count} 张规划` : '自动规划 1–12 张；确认计划后生成' }}</p>
  </Teleport>
  <p v-if="error" role="alert" class="error">{{ error }}</p>
  <button v-if="error" type="button" @click="act(refresh)">重新读取状态 / 重试升级</button>
  <button v-if="error" type="button" @click="act(openBackups)">打开备份位置</button>
  <section v-if="showLibrary" class="task-library"><h3>草稿与任务</h3>
    <p v-for="warning in all.warnings" :key="warning" role="status">{{ warning }}</p>
    <p v-if="!all.drafts.length && !all.jobs.length">还没有草稿或任务。</p>
    <button v-for="d in all.drafts" :key="d.id" type="button" :disabled="busy" @click="restore(d)">草稿 · {{ d.prompt.slice(0, 40) || '未命名' }} · {{ d.mode === 'quick' ? '快速' : '规划' }}</button>
    <button v-for="j in all.jobs" :key="j.id" type="button" @click="act(() => openJob(j.id))">任务 · {{ api.label(j.status) }} · {{ j.expected_count }} 张 · {{ j.updated_at }}</button>
    <p v-for="p in all.planning_jobs.filter(p => p.error)" :key="p.id">规划作业：{{ p.error }}</p>
  </section>
  <p v-if="planning" role="status">正在生成文字计划，尚未开始生图。</p>
  <component :is="plan?.confirmed_at ? 'details' : 'div'" v-if="mode === 'planned' && plan">
    <summary v-if="plan.confirmed_at">已执行计划与历史版本</summary>
    <PlanPanel :plan="plan" :versions="versions" v-model:feedback="feedback" :disabled="busy || locked" :stale="stale" @confirm="confirm" @revise="revise" @select="chooseVersion" />
  </component>
  <JobPreview v-if="job" :job="job" :disabled="pending || locked" @stop="stopJob" @resume="resumeJob()" @retry="resumeJob" @save="saveItem" @discard="discardItem" />
  <div v-else-if="!plan && !planning" class="preview-placeholder"><span aria-hidden="true">✳</span><p>生成的图片会逐张出现在这里</p></div>
</template>
<style scoped>
.task-library { display: flex; flex-wrap: wrap; gap: .5rem; }
.task-library { flex-direction: column; }
:deep(pre) { white-space: pre-wrap; overflow-wrap: anywhere; max-height: 24rem; overflow: auto; }
:deep(.photo-navigation) { display: flex; overflow-x: auto; gap: .4rem; }
:deep(.photo-navigation button) { flex-shrink: 0; }
:deep([aria-current="true"]), [aria-pressed="true"] { outline: 2px solid currentColor; }
:deep(.plan-panel li) { margin-block: 1rem; }
</style>
