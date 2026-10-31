<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import * as api from '../services/creation'
import { creationTasks, filterTasks, taskTime, type TaskFilter, type CreationTask } from '../services/creationTasks'
import { isDesktop } from '../services/imageGeneration'
import { discardPending, openBackups, message, type Artwork } from '../services/gallery'
import { generationOptions, optionCategories, type GenerationSelections, type OptionCategory } from '../services/generationOptions'
import OptionPicker from './OptionPicker.vue'
import PlanPanel from './PlanPanel.vue'
import JobPreview from './JobPreview.vue'
const props = defineProps<{ prompt: string; selections: GenerationSelections; reuseId?: string; locked: boolean; controls: HTMLElement | null; modeControl: HTMLElement | null }>()
const emit = defineEmits<{ busy: [boolean]; frozen: [boolean]; unsaved: [boolean]; revision: [string]; restore: [string, GenerationSelections]; gallery: [string]; resetSource: [] }>()
const mode = ref<api.Mode>(localStorage.getItem('creation-mode') === 'planned' ? 'planned' : 'quick')
const count = ref<number | null>(null)
const current = ref<api.Draft>()
const feedback = ref('')
const all = ref<api.Library>({ drafts: [], plans: [], planning_jobs: [], jobs: [] })
const job = ref<api.JobSnapshot | null>(null)
const pending = ref(false)
const error = ref('')
const showLibrary = ref(false)
const mounted = ref(false)
const dirty = ref(false)
const saveState = ref<'idle' | 'saving' | 'saved' | 'failed'>('idle')
const saveError = ref('')
const search = ref('')
const filter = ref<TaskFilter>('all')
const filters: [TaskFilter, string][] = [['all', '全部'], ['draft', '草稿'], ['active', '进行中'], ['attention', '待处理'], ['completed', '已完成']]
const category = ref<OptionCategory | null>(null)
const detail = ref<HTMLElement | null>(null)
const categoryLabels = { style: '风格', layout: '排版', color: '配色' }
const copiedJob = ref<string>()
const frozen = computed(() => !!job.value || all.value.jobs.some(j => j.draft_id === current.value?.id))
const planning = computed(() => all.value.planning_jobs.some(j => api.active(j.status)))
const currentPlanning = computed(() => all.value.planning_jobs.find(j => j.draft_id === current.value?.id))
const editorStatus = computed(() => currentPlanning.value && api.active(currentPlanning.value.status) ? '规划中' : currentPlanning.value?.error ? '失败' : plan.value ? '待确认' : '草稿')
const busy = computed(() => pending.value || planning.value || all.value.jobs.some(j => api.active(j.status)) || !!job.value && api.active(job.value.status))
const plan = computed(() => all.value.plans.find(p => p.id === current.value?.current_plan_id))
const versions = computed(() => all.value.plans.filter(p => p.draft_id === current.value?.id))
const stale = computed(() => dirty.value || mode.value !== 'planned' || plan.value?.input_revision !== current.value?.input_revision)
const tasks = computed(() => filterTasks(creationTasks(all.value), search.value, filter.value))
const saveLabel = computed(() => ({ idle: '填写后自动保存', saving: '保存中…', saved: '已自动保存', failed: '保存失败' })[saveState.value])
let autosave: ReturnType<typeof setTimeout> | undefined
let timer: ReturnType<typeof setInterval> | undefined
let unlisten: UnlistenFn | undefined
let closeListener: UnlistenFn | undefined
let disposed = false
let restoring = false
let saveChain = Promise.resolve()
let refreshSequence = 0
let selectionSequence = 0
watch(busy, value => emit('busy', value), { immediate: true })
watch(frozen, value => emit('frozen', value), { immediate: true })
watch(() => job.value, j => emit('revision', `${j?.id}-${j?.revision}`))
watch(() => [saveState.value, all.value.jobs, job.value], () => emit('unsaved', saveState.value === 'failed' || saveState.value === 'saving' || all.value.jobs.some(j => j.pending_save_count > 0) || !!job.value?.pending_save_count))
watch(mode, () => localStorage.setItem('creation-mode', mode.value))
watch(() => [props.prompt, JSON.stringify(props.selections), props.reuseId, count.value, mode.value], () => {
  if (restoring || frozen.value) return
  dirty.value = true; queueSave()
})
watch(feedback, () => { if (!restoring) queueSave() })
function queueSave() {
  clearTimeout(autosave)
  if (!mounted.value || !isDesktop() || frozen.value || (!current.value && !props.prompt.trim())) return
  saveState.value = 'saving'
  autosave = setTimeout(() => { void persist().catch(() => {}) }, 350)
}
async function persist() {
  clearTimeout(autosave)
  if (frozen.value || !isDesktop()) return current.value
  if (!current.value && !props.prompt.trim()) { saveState.value = 'idle'; return }
  const epoch = selectionSequence
  const values = { mode: mode.value, prompt: props.prompt, selections: { ...props.selections }, reuse_id: props.reuseId, reuse_job_id: copiedJob.value, requested_count: mode.value === 'quick' ? 1 : count.value, feedback: feedback.value }
  saveState.value = 'saving'
  const work = saveChain.catch(() => {}).then(async () => {
    if (epoch !== selectionSequence) return
    const d = current.value
    try {
      const result = await api.saveDraft({ ...values, id: d?.id, input_revision: d?.input_revision })
      if (epoch !== selectionSequence || disposed) return
      current.value = result
      all.value.drafts = [result, ...all.value.drafts.filter(d => d.id !== result.id)]
      if (mode.value === values.mode && props.prompt === values.prompt && JSON.stringify(props.selections) === JSON.stringify(values.selections) && (mode.value === 'quick' ? 1 : count.value) === values.requested_count && feedback.value === values.feedback) {
        dirty.value = false; saveState.value = 'saved'; saveError.value = ''
      }
    } catch (e) {
      if (epoch === selectionSequence) { saveState.value = 'failed'; saveError.value = message(e) }
      throw e
    }
  })
  saveChain = work
  await work
  return current.value
}
async function switchMode(next: api.Mode) {
  if (busy.value || frozen.value || mode.value === next) return
  await act(async () => {
    mode.value = next
    await nextTick()
    await persist()
  })
}
async function refresh() {
  const sequence = ++refreshSequence
  const data = await api.library()
  if (!data || !Array.isArray(data.jobs) || !Array.isArray(data.plans)) throw new Error('无法读取创作任务，请重试。')
  if (disposed || sequence !== refreshSequence) return
  all.value = data
  // A start response can be lost after admission. Recover the original run from
  // the library rather than permitting another execution or editing its input.
  const admitted = !pending.value && !job.value && data.jobs.find(j => j.draft_id === current.value?.id)
  if (admitted) { await openJob(admitted.id); return }
  // Only adopt planner output for the selected, unchanged task. Never overwrite an editor.
  const d = data.drafts.find(d => d.id === current.value?.id)
  if (d && !dirty.value && saveState.value !== 'saving' && saveState.value !== 'failed' && d.current_plan_id !== current.value?.current_plan_id) {
    restoring = true; current.value = d; feedback.value = d.feedback_draft
    await nextTick(); restoring = false
  }
  if (job.value) {
    const selectedId = job.value.id
    const result = await api.snapshot(selectedId)
    if (!disposed && sequence === refreshSequence && job.value?.id === selectedId) job.value = api.acceptSnapshot(job.value, result)
  }
}
async function act(action: () => Promise<unknown>) {
  if (pending.value) return
  pending.value = true; error.value = ''
  try { await action(); if (isDesktop()) await refresh() } catch (e) { error.value = message(e) } finally { pending.value = false }
}
async function flushEditor() {
  if (!frozen.value && isDesktop() && (dirty.value || saveState.value === 'saving' || saveState.value === 'failed')) await persist()
  await saveChain
}
function selectionsOf(d: api.Draft): GenerationSelections {
  return Object.fromEntries(optionCategories.flatMap(({ key }) => d.snapshot[key] ? [[key, d.snapshot[key]!.id]] : []))
}
async function loadDraft(d?: api.Draft) {
  clearTimeout(autosave); selectionSequence++; restoring = true
  job.value = null; current.value = d; category.value = null; copiedJob.value = undefined
  emit('resetSource')
  mode.value = d?.mode ?? mode.value; count.value = d?.requested_count ?? null; feedback.value = d?.feedback_draft ?? ''
  emit('restore', d?.prompt ?? '', d ? selectionsOf(d) : {})
  dirty.value = false; saveState.value = d ? 'saved' : 'idle'; saveError.value = ''; error.value = ''; showLibrary.value = false
  await nextTick(); restoring = false
  const panel = detail.value?.closest('.preview-panel')
  if (panel) panel.scrollTop = 0
}
async function openJob(id: string, onlyIfPristine = false) {
  const epoch = selectionSequence
  const result = await api.snapshot(id)
  if (disposed || epoch !== selectionSequence) return
  if (onlyIfPristine && (current.value || props.prompt.trim() || dirty.value || showLibrary.value)) return
  await loadDraft(result.input ?? all.value.drafts.find(d => d.id === result.draft_id))
  job.value = result
}
async function openTask(task: CreationTask) {
  await act(async () => { await flushEditor(); if (task.job) await openJob(task.job.id); else await loadDraft(task.draft) })
}
async function newTask(copy = false) {
  await flushEditor()
  const source = copy ? job.value?.input ?? current.value : undefined
  const sourceJob = copy ? job.value?.id : undefined
  await loadDraft()
  if (source) {
    restoring = true
    mode.value = source.mode; count.value = source.requested_count
    emit('restore', source.prompt, selectionsOf(source))
    await nextTick(); restoring = false; copiedJob.value = sourceJob
    dirty.value = true; await persist()
  }
}
async function submit() {
  if (busy.value || frozen.value || props.locked || !props.prompt.trim() || !isDesktop()) return
  showLibrary.value = false
  await act(async () => {
    const d = await persist()
    if (!d) return
    if (mode.value === 'planned') { await api.planCreate(d); return }
    const j = await api.start(d, `quick-${d.id}`); await openJob(j.id)
  })
}
async function confirm() {
  if (busy.value || frozen.value || stale.value || feedback.value.trim() || !plan.value || props.locked) return
  const selected = plan.value
  await act(async () => {
    const d = await persist()
    if (!d) return
    const j = await api.start(d, `confirm-${selected.id}`, selected)
    await openJob(j.id)
  })
}
async function revise() {
  if (busy.value || frozen.value || !plan.value) return
  const base = plan.value.id
  await act(async () => { const d = await persist(); if (d) await api.planCreate(d, base) })
}
function chooseVersion(id: string) {
  if (frozen.value) return
  return act(async () => {
    const d = await persist(); if (!d) return
    const p = await api.selectVersion(d.id, id)
    restoring = true; current.value!.current_plan_id = p.id; current.value!.feedback_draft = ''; feedback.value = ''
    await nextTick(); restoring = false
  })
}
function editPrompt(event: Event) { emit('restore', (event.target as HTMLTextAreaElement).value, props.selections) }
function editSelection(key: OptionCategory, value?: string) { emit('restore', props.prompt, { ...props.selections, [key]: value }) }
function photoSaved(_a: Artwork) { void refresh() }
function stopJob() { return act(() => api.stop(job.value!.id)) }
function resumeJob(item?: api.Item) { return act(() => api.resume(job.value!, item)) }
function saveItem(item: api.Item) { return act(() => api.retrySave(item.id)) }
function discardItem(item: api.Item) { return act(() => discardPending(item.id)) }
async function openLibrary() {
  await act(async () => { await flushEditor(); showLibrary.value = true; category.value = null })
}
defineExpose({ submit, refresh, photoSaved, openLibrary, newTask })
onMounted(async () => {
  mounted.value = true
  if (!isDesktop()) return
  try {
    try { unlisten = await listen<api.Notice>('creation-changed', ({ payload }) => {
      if (payload.job_id === job.value?.id && payload.revision <= job.value.revision) return
      void refresh().catch(e => { error.value = message(e) })
    }) } catch { /* Polling remains available. */ }
    try { closeListener = await listen<string>('creation-close-blocked', e => { error.value = e.payload }) } catch { /* optional */ }
    await refresh()
    if (disposed) { unlisten?.(); closeListener?.(); return }
    // Startup I/O may finish after the user has begun typing or opened a task.
    // Restoring a historical execution must never replace those newer edits.
    const latest = all.value.jobs[0]
    if (latest && !current.value && !props.prompt.trim() && !dirty.value && !showLibrary.value) await openJob(latest.id, true)
    timer = setInterval(() => { void refresh().catch(e => { error.value = message(e) }) }, 2500)
  } catch (e) { error.value = message(e) }
})
onBeforeUnmount(() => { disposed = true; clearTimeout(autosave); clearInterval(timer); unlisten?.(); closeListener?.() })
</script>
<template>
  <Teleport v-if="mounted && modeControl" :to="modeControl">
    <button type="button" class="mode-switch" role="switch" aria-label="快速生成" :aria-checked="mode === 'quick'" :disabled="busy || frozen"
      @click="switchMode(mode === 'quick' ? 'planned' : 'quick')">
      <span>{{ mode === 'quick' ? '快速生成' : '规划生成' }}</span><span class="switch-track" aria-hidden="true"></span>
    </button>
  </Teleport>
  <Teleport v-if="mounted && controls" :to="controls">
    <template v-if="!frozen">
      <label v-if="mode === 'planned'">生成数量 <select v-model="count" :disabled="busy"><option :value="null">自动规划</option><option v-for="n in 12" :key="n" :value="n">{{ n }} 张</option></select></label>
      <button type="submit" :disabled="busy || locked || !prompt.trim() || !isDesktop()">{{ busy ? '正在处理…' : mode === 'quick' ? '生成图片 ↗' : '生成计划' }}</button>
      <p id="generation-hint" class="input-hint">{{ mode === 'quick' ? '每次生成 1 张图片' : count ? `本次按所选数量 ${count} 张规划` : '自动规划 1–12 张；确认计划后生成' }}</p>
    </template>
    <template v-else><p class="input-hint">本次输入已固定，修改内容请以此新建。</p><button type="button" :disabled="pending || locked" @click="act(() => newTask(true))">以此新建</button></template>
  </Teleport>
  <p v-if="error" role="alert" class="error">{{ error }}</p>
  <div v-if="error" class="task-actions"><button type="button" @click="act(refresh)">重新读取状态 / 重试升级</button><button type="button" @click="act(openBackups)">打开备份位置</button></div>
  <section v-if="showLibrary" class="task-library" aria-label="创作任务列表">
    <p v-for="warning in all.warnings" :key="warning" role="status">{{ warning }}</p>
    <input v-model="search" type="search" aria-label="搜索创作任务" placeholder="搜索标题或创作描述…" />
    <nav class="task-filters" aria-label="任务状态筛选"><button v-for="[key, name] in filters" :key="key" type="button" :aria-pressed="filter === key" @click="filter = key">{{ name }}</button></nav>
    <p v-if="!tasks.length">没有符合条件的创作任务。</p>
    <button v-for="task in tasks" :key="task.id" type="button" class="task-card" :disabled="pending" @click="openTask(task)">
      <img v-if="task.job?.thumbnail" :src="task.job.thumbnail" alt="" />
      <span v-else class="task-document" aria-label="尚无作品">▤</span>
      <span class="task-card-content"><strong>{{ task.title }}</strong><span class="task-badge" :data-state="task.filter">{{ task.status }}</span>
        <small>{{ task.mode === 'quick' ? '快速生成' : '规划生成' }} · {{ task.count === null ? '自动数量' : task.count + ' 张' }}</small>
        <template v-if="task.job && task.filter === 'active'"><small>已生成 {{ task.job.generated_count }} / {{ task.count }} 张</small><progress :value="task.job.generated_count" :max="task.count ?? 1" aria-label="生成进度" /></template>
        <time :datetime="task.updatedAt">{{ taskTime(task.updatedAt) }}</time>
      </span>
    </button>
    <footer class="task-actions"><span>共 {{ tasks.length }} 个任务</span><button type="button" class="primary" :disabled="pending || locked" @click="act(() => newTask())">＋ 新建创作</button></footer>
  </section>
  <section v-else ref="detail" class="task-detail">
    <button type="button" class="back-tasks" :disabled="pending" @click="openLibrary">← 全部任务</button>
    <template v-if="!job">
      <div class="task-heading"><h3>{{ plan?.content.title || prompt.slice(0, 60) || '新建创作' }}</h3><span class="task-badge">{{ editorStatus }}</span><small role="status">{{ saveLabel }}</small></div>
      <p v-if="saveState === 'failed'" role="alert" class="error">{{ saveError }} <button type="button" @click="act(persist)">重试保存</button></p>
      <form v-if="!plan" class="task-editor" @submit.prevent="submit">
        <div class="task-modes"><button type="button" :aria-pressed="mode === 'quick'" :disabled="busy" @click="switchMode('quick')">快速生成</button><button type="button" :aria-pressed="mode === 'planned'" :disabled="busy" @click="switchMode('planned')">规划生成</button></div>
        <label for="task-prompt">创作描述</label><textarea id="task-prompt" :value="prompt" :disabled="busy || locked" maxlength="4000" @input="editPrompt" />
        <label>画面偏好</label>
        <div v-for="option in optionCategories" :key="option.key" class="task-preference"><button type="button" :disabled="busy || locked" @click="category = category === option.key ? null : option.key">{{ categoryLabels[option.key] }} <span>{{ generationOptions[option.key].find(o => o.id === selections[option.key])?.name ?? selections[option.key] ?? '自动' }}</span> ›</button><button v-if="selections[option.key]" type="button" :aria-label="'清除任务' + categoryLabels[option.key]" :disabled="busy || locked" @click="editSelection(option.key)">×</button></div>
        <section v-if="category" class="catalog-panel task-catalog"><h4>{{ categoryLabels[category] }}图鉴</h4><OptionPicker id-prefix="task-" :category="category" :label="categoryLabels[category]" :options="generationOptions[category]" :model-value="selections[category]" :disabled="busy || locked" @update:model-value="editSelection(category!, $event)" /></section>
        <label>生成数量 <select v-if="mode === 'planned'" v-model="count" :disabled="busy"><option :value="null">自动规划</option><option v-for="n in 12" :key="n" :value="n">{{ n }} 张</option></select><span v-else>1 张</span></label>
        <button class="primary" type="submit" :disabled="busy || locked || !prompt.trim() || !isDesktop()">{{ mode === 'quick' ? '生成图片' : '生成计划' }}</button>
      </form>
      <template v-if="plan">
        <p v-if="stale" role="status">输入或模式已变化，旧计划需要更新；请重新生成计划。</p>
        <details><summary>修改创作输入与配置</summary>
          <label>创作描述<textarea :value="prompt" :disabled="busy || locked" maxlength="4000" @input="editPrompt" /></label>
          <div class="task-modes"><button type="button" :disabled="busy" @click="switchMode('quick')">快速生成</button><button type="button" :disabled="busy" @click="switchMode('planned')">规划生成</button></div>
          <label v-for="option in optionCategories" :key="option.key">{{ categoryLabels[option.key] }}<select :value="selections[option.key] ?? ''" :disabled="busy || locked" @change="editSelection(option.key, ($event.target as HTMLSelectElement).value || undefined)"><option value="">自动</option><option v-for="o in generationOptions[option.key]" :key="o.id" :value="o.id">{{ o.id }} · {{ o.name }}</option></select></label>
          <label v-if="mode === 'planned'">生成数量<select v-model="count" :disabled="busy"><option :value="null">自动规划</option><option v-for="n in 12" :key="n" :value="n">{{ n }} 张</option></select></label>
        </details>
        <PlanPanel v-if="mode === 'planned'" :plan="plan" :versions="versions" v-model:feedback="feedback" :disabled="busy || locked" :stale="stale" @confirm="confirm" @revise="revise" @select="chooseVersion" />
        <button type="button" :disabled="busy || locked || !prompt.trim()" @click="submit">{{ mode === 'planned' ? '重新生成计划' : '生成图片' }}</button>
      </template>
      <p v-if="currentPlanning && api.active(currentPlanning.status)" role="status">正在生成文字计划，尚未开始生图。</p>
      <p v-if="currentPlanning?.error" role="alert">{{ currentPlanning.error }}</p>
    </template>
    <template v-else>
      <JobPreview :job="job" :disabled="pending || locked" @stop="stopJob" @resume="resumeJob()" @retry="resumeJob" @save="saveItem" @discard="discardItem" @gallery="emit('gallery', job.group_id)" />
      <footer class="task-actions"><small>复制输入，开启新的创作</small><button type="button" class="primary" :disabled="pending || locked" @click="act(() => newTask(true))">以此新建</button></footer>
    </template>
  </section>
</template>
<style scoped>
.task-library, .task-detail, .task-editor { display: flex; flex-direction: column; gap: 16px; min-width: 0; }
.task-library input { width: 100%; padding: 12px; border: 1px solid var(--border); border-radius: 9px; }
.task-filters, .task-modes { display: flex; gap: 4px; background: #f3f4f0; padding: 4px; border-radius: 9px; }
.task-filters button, .task-modes button { flex: 1; padding: 8px 4px; border: 0; font-size: 13px; background: transparent; }
[aria-pressed="true"] { background: #dfede2 !important; color: var(--green); }
.task-card { display: flex; gap: 16px; padding: 12px; text-align: left; background: white; }
.task-card img, .task-document { width: 96px; height: 96px; object-fit: cover; border-radius: 8px; flex-shrink: 0; }
.task-document { display: grid; place-items: center; background: #eef1ea; color: #7d9184; font-size: 40px; }
.task-card-content { display: flex; flex: 1; min-width: 0; flex-direction: column; align-items: flex-start; gap: 7px; }
.task-card strong { display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; overflow-wrap: anywhere; }
.task-card small, time, .task-heading small { color: var(--muted); font-size: 12px; }
.task-badge { display: inline-block; padding: 4px 10px; border-radius: 6px; background: #e9edf0; font-size: 12px; }
.task-badge[data-state="completed"] { color: #245c4b; background: #dceee0; }
.task-badge[data-state="attention"] { color: #805314; background: #fff0d4; }
.task-badge[data-state="active"] { color: #22638a; background: #e1f0fa; }
.task-heading, .task-actions { display: flex; gap: 12px; align-items: center; flex-wrap: wrap; }
.task-heading h3 { font-size: 19px; margin: 0; overflow-wrap: anywhere; }
.task-heading small, .task-actions button:last-child { margin-left: auto; }
.task-actions { border-top: 1px solid var(--border); padding-top: 16px; }
.back-tasks { align-self: flex-start; border: 0; padding: 4px 0; }
.primary { background: var(--green); color: white; }
.task-preference { display: flex; }
.task-preference > button:first-child { display: flex; flex: 1; text-align: left; gap: 12px; }
.task-preference span { flex: 1; }
.task-editor label { font-size: 14px; }
select { max-width: 100%; padding: 6px; font: inherit; }
.task-catalog { height: 430px; flex-shrink: 0; border: 1px solid var(--border); border-radius: 8px; }
.task-catalog h4 { margin: 16px; }
.task-catalog :deep(.picker-tools) { position: static; width: auto; margin: 0 16px 12px; }
.task-catalog :deep(.option-grid) { grid-template-columns: repeat(3, minmax(0, 1fr)); }
:deep(pre) { white-space: pre-wrap; overflow-wrap: anywhere; max-height: 24rem; overflow: auto; }
:deep(progress) { width: 100%; height: 6px; accent-color: var(--green); }
:deep(.photo-navigation) { display: flex; overflow-x: auto; gap: .4rem; }
:deep(.photo-navigation button) { flex-shrink: 0; }
:deep([aria-current="true"]) { outline: 2px solid var(--green); }
:deep(.plan-panel li) { margin-block: 1rem; }
@media (max-width: 480px) { .task-card img, .task-document { width: 64px; height: 80px; } .task-catalog :deep(.option-grid) { grid-template-columns: repeat(2, minmax(0, 1fr)); } }
</style>
