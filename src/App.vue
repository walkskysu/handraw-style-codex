<script setup lang="ts">
import { computed, nextTick, ref } from 'vue'
import { isDesktop, MAX_PROMPT_LENGTH } from './services/imageGeneration'
import OptionPicker from './components/OptionPicker.vue'
import GalleryView from './components/GalleryView.vue'
import AboutSettings from './components/AboutSettings.vue'
import CreationFlow from './components/CreationFlow.vue'
import type { Artwork } from './services/gallery'
import { generationOptions, optionCategories, type GenerationSelections, type OptionCategory } from './services/generationOptions'
const desktop = isDesktop()
const updateLocked = ref(false)
const view = ref<'workspace' | 'gallery'>('workspace')
const pending = ref(false)
const hasUnsaved = ref(false)
const revision = ref('')
const flow = ref<InstanceType<typeof CreationFlow> | null>(null)
const actions = ref<HTMLElement | null>(null)
const modeControl = ref<HTMLElement | null>(null)
const prompt = ref('')
const selections = ref<GenerationSelections>({})
const reused = ref<Artwork | null>(null)
const replacement = ref<Artwork | null>(null)
const baseline = ref(JSON.stringify({ prompt: '', selections: {} }))
const draft = () => JSON.stringify({ prompt: prompt.value, selections: selections.value })
const reuseDialog = ref<HTMLDialogElement | null>(null)
const promptInput = ref<HTMLTextAreaElement | null>(null)
const activeCategory = ref<OptionCategory>('style')
const categoryLabels = { style: '风格', layout: '排版', color: '配色' }
const categoryIcons = { style: '✳', layout: '▤', color: '◐' }
const picker = ref<InstanceType<typeof OptionPicker> | null>(null)
const previewDialog = ref<HTMLDialogElement | null>(null)
const previewOpen = ref(false)
let previewTrigger: HTMLElement | null = null
const selectedOptions = computed(() => Object.fromEntries(optionCategories.map(({ key }) =>
  [key, reused.value?.snapshot[key]?.id === selections.value[key] && reused.value?.snapshot[key]
    ? reused.value.snapshot[key]
    : generationOptions[key].find((option) => option.id === selections.value[key])],
)))
async function openCatalog(category: OptionCategory) {
  activeCategory.value = category; await nextTick(); await picker.value?.reveal(selections.value[category])
  document.getElementById(category + '-tab')?.focus()
}
function openPreview(event: Event) { previewTrigger = event.currentTarget as HTMLElement; previewDialog.value?.showModal(); previewOpen.value = true; void flow.value?.openLibrary() }
function closePreview() {
  previewOpen.value = false
  if (previewTrigger?.matches(':disabled')) document.querySelector<HTMLElement>('.open-preview')?.focus()
  else previewTrigger?.focus()
}
function moveTab(event: KeyboardEvent) {
  const keys = optionCategories.map(({ key }) => key), index = keys.indexOf(activeCategory.value)
  const next = event.key === 'Home' ? 0 : event.key === 'End' ? keys.length - 1 : event.key === 'ArrowRight' ? (index + 1) % keys.length : event.key === 'ArrowLeft' ? (index + keys.length - 1) % keys.length : -1
  if (next < 0) return
  event.preventDefault(); activeCategory.value = keys[next]!
  nextTick(() => document.getElementById(activeCategory.value + '-tab')?.focus())
}
function clearSelection(category: OptionCategory) { if (!pending.value) delete selections.value[category] }
async function generate() {
  if (pending.value || updateLocked.value || !prompt.value.trim() || !desktop) return
  previewTrigger = actions.value?.querySelector('button[type="submit"]') ?? null
  previewDialog.value?.showModal(); previewOpen.value = true
  await flow.value?.submit()
}
function requestReuse(artwork: Artwork) {
  if (pending.value) return
  replacement.value = artwork
  if (draft() !== baseline.value || hasUnsaved.value) reuseDialog.value?.showModal()
  else applyReuse()
}
function restore(text: string, config: GenerationSelections) { prompt.value = text; selections.value = config; baseline.value = draft() }
function applyReuse() {
  if (!replacement.value) return
  reused.value = replacement.value
  restore(reused.value.prompt, Object.fromEntries(optionCategories.flatMap(({ key }) => { const option = reused.value!.snapshot[key]; return option ? [[key, option.id]] : [] })))
  view.value = 'workspace'; reuseDialog.value?.close(); nextTick(() => promptInput.value?.focus())
}
function changeView(next: 'workspace' | 'gallery') { previewDialog.value?.close(); view.value = next; if (next === 'workspace' && desktop) void flow.value?.refresh() }
</script>

<template>
  <div class="app-shell">
    <header class="app-header">
      <a class="brand" href="#main" aria-label="IdeaCanvas 首页" @click="changeView('workspace')">
        <span>IdeaCanvas</span>
      </a>
      <div class="brand-message"><h1>把想法，变成画面。</h1><p>写下灵感，剩下的交给创作。</p></div>
      <nav class="app-navigation" aria-label="工作区">
        <button type="button" :aria-current="view === 'workspace' ? 'page' : undefined" @click="changeView('workspace')">创作</button>
        <button type="button" :aria-current="view === 'gallery' ? 'page' : undefined" @click="changeView('gallery')">我的图库</button>
        <button type="button" class="open-preview" aria-haspopup="dialog" aria-controls="preview-dialog" :aria-expanded="previewOpen" @click="openPreview">草稿与任务<span v-if="pending" class="task-indicator" role="status"> · 处理中</span></button>
      </nav>
      <AboutSettings :working="pending" :unsaved="hasUnsaved" @lock="updateLocked = $event" />
    </header>

    <dialog id="preview-dialog" ref="previewDialog" class="preview-dialog" aria-labelledby="preview-title"
      @close="closePreview" @click="($event.target === previewDialog) && previewDialog?.close()">
      <section class="preview-panel panel" aria-labelledby="preview-title">
        <div class="section-heading"><h2 id="preview-title">计划与作品预览</h2><button type="button" class="close-preview" aria-label="关闭作品预览" @click="previewDialog?.close()">×</button></div>
        <CreationFlow ref="flow" :controls="actions" :mode-control="modeControl" :prompt="prompt" :selections="selections" :reuse-id="reused?.id" :locked="updateLocked" @busy="pending = $event" @unsaved="hasUnsaved = $event" @revision="revision = $event" @restore="restore" />
      </section>
    </dialog>

    <GalleryView v-if="view === 'gallery'" :generation-pending="pending" :revision="revision" @reuse="requestReuse" @saved="flow?.refresh()" />
    <main v-show="view === 'workspace'" id="main">
      <section class="creation-panel panel" aria-labelledby="creation-title">
        <form :aria-busy="pending" @submit.prevent="generate">
          <div class="creation-heading"><h2 id="creation-title"><label for="prompt">你想画什么？</label></h2><div ref="modeControl"></div></div>
          <textarea id="prompt" ref="promptInput" v-model="prompt" name="prompt" rows="6"
            placeholder="例如：一只在窗边喝咖啡的小猫，午后阳光洒在桌上……"
            :maxlength="MAX_PROMPT_LENGTH" required :disabled="pending" aria-describedby="generation-hint" />
          <div class="generation-options">
            <div class="section-heading">
              <h2>画面偏好 <span class="optional-label">· 可选</span></h2>
            </div>
            <div v-for="category in optionCategories" :key="category.key" class="selection-row" :data-selection="category.key">
              <button type="button" class="selection-link"
                :title="selectedOptions[category.key]?.name ?? '按描述自动选择'" @click="openCatalog(category.key)">
                <span class="selection-placeholder" aria-hidden="true">{{ categoryIcons[category.key] }}</span>
                <span>{{ categoryLabels[category.key] }}</span>
                <span class="selection-value">{{ selectedOptions[category.key]?.id ?? '自动' }}</span><span aria-hidden="true">›</span>
              </button>
              <button v-if="selectedOptions[category.key]" type="button" class="clear-selection"
                :aria-label="`清除${categoryLabels[category.key]}选择`" :disabled="pending"
                @click="clearSelection(category.key)">×</button>
            </div>
          </div>
          <div id="creation-actions" ref="actions" class="generation-action"></div>
        </form>
        <p v-if="!desktop" class="preview-hint">当前为浏览器预览。请启动桌面应用，并安装、登录本地 Codex 后生成图片。</p>
      </section>

      <section class="catalog-panel panel" aria-labelledby="catalog-title">
        <div class="section-heading">
          <h2 id="catalog-title">灵感图鉴</h2>
          <span class="catalog-hint">点击卡片应用到画面偏好</span>
        </div>
        <div class="catalog-tabs" role="tablist" aria-label="图鉴类别" @keydown="moveTab">
          <button v-for="category in optionCategories" :id="`${category.key}-tab`" :key="category.key"
            type="button" role="tab" :aria-selected="activeCategory === category.key"
            :aria-controls="activeCategory === category.key ? `${category.key}-panel` : undefined"
            :tabindex="activeCategory === category.key ? 0 : -1" @click="activeCategory = category.key">
            {{ categoryLabels[category.key] }}
          </button>
        </div>
        <OptionPicker ref="picker" v-model="selections[activeCategory]" :category="activeCategory"
          :label="categoryLabels[activeCategory]" :options="generationOptions[activeCategory]" :disabled="pending" />
      </section>

    </main>

    <dialog ref="reuseDialog" class="confirmation-dialog reuse-dialog" aria-labelledby="reuse-title">
      <h2 id="reuse-title">替换当前编辑？</h2>
      <p>当前有未保存的编辑或尚未保存到图库的作品。复用将替换描述与配置，已有预览仍会保留。</p>
      <button type="button" @click="applyReuse">确认复用</button>
      <button type="button" @click="reuseDialog?.close()">取消</button>
    </dialog>

  </div>
</template>
