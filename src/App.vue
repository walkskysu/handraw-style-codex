<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref } from 'vue'
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
const prompt = ref('')
const selections = ref<GenerationSelections>({})
const reused = ref<Artwork | null>(null)
const replacement = ref<Artwork | null>(null)
const baseline = ref(JSON.stringify({ prompt: '', selections: {} }))
const draft = () => JSON.stringify({ prompt: prompt.value, selections: selections.value })
const reuseDialog = ref<HTMLDialogElement | null>(null)
const promptInput = ref<HTMLTextAreaElement | null>(null)
const activeCategory = ref<OptionCategory>('style')
const categoryLabels = { style: '风格', layout: '排版', color: '主题色' }
const picker = ref<InstanceType<typeof OptionPicker> | null>(null)
const previewDialog = ref<HTMLDialogElement | null>(null)
const previewOpen = ref(false)
const compactMedia = window.matchMedia?.('(max-width: 1199px)')
const compact = ref(compactMedia?.matches ?? false)
let previewTrigger: HTMLElement | null = null
const selectedOptions = computed(() => Object.fromEntries(optionCategories.map(({ key }) =>
  [key, reused.value?.snapshot[key]?.id === selections.value[key] && reused.value?.snapshot[key]
    ? { ...reused.value.snapshot[key], image: generationOptions[key].find(o => o.id === selections.value[key])?.image }
    : generationOptions[key].find((option) => option.id === selections.value[key])],
)))
function resizeWorkspace(event: MediaQueryListEvent) {
  if (!event.matches) previewDialog.value?.close()
  compact.value = event.matches
}
compactMedia?.addEventListener('change', resizeWorkspace)
onBeforeUnmount(() => compactMedia?.removeEventListener('change', resizeWorkspace))
async function openCatalog(category: OptionCategory) {
  activeCategory.value = category; await nextTick(); await picker.value?.reveal(selections.value[category])
  document.getElementById(category + '-tab')?.focus()
}
function openPreview(event: Event) { previewTrigger = event.currentTarget as HTMLElement; previewDialog.value?.showModal(); previewOpen.value = true }
function closePreview() { previewOpen.value = false; previewTrigger?.focus() }
function moveTab(event: KeyboardEvent) {
  const keys = optionCategories.map(({ key }) => key), index = keys.indexOf(activeCategory.value)
  const next = event.key === 'Home' ? 0 : event.key === 'End' ? keys.length - 1 : event.key === 'ArrowRight' ? (index + 1) % keys.length : event.key === 'ArrowLeft' ? (index + keys.length - 1) % keys.length : -1
  if (next < 0) return
  event.preventDefault(); activeCategory.value = keys[next]!
  nextTick(() => document.getElementById(activeCategory.value + '-tab')?.focus())
}
function clearSelection(category: OptionCategory) { if (!pending.value) delete selections.value[category] }
async function generate() { await flow.value?.submit(); if (compact.value) { previewDialog.value?.showModal(); previewOpen.value = true } }
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
function changeView(next: 'workspace' | 'gallery') { previewDialog.value?.close(); view.value = next; if (next === 'workspace') void flow.value?.refresh() }
</script>

<template>
  <div class="app-shell">
    <header class="app-header">
      <a class="brand" href="#main" aria-label="Handraw 首页">
        <span>handraw.</span>
      </a>
      <h1>灵感工作台</h1>
      <nav class="app-navigation" aria-label="工作区">
        <button type="button" :aria-current="view === 'workspace' ? 'page' : undefined" @click="changeView('workspace')">创作工作台</button>
        <button type="button" :aria-current="view === 'gallery' ? 'page' : undefined" @click="changeView('gallery')">我的图库</button>
      </nav>
      <span class="mode-badge">{{ desktop ? '桌面应用' : '浏览器预览' }}</span>
      <AboutSettings :working="pending" :unsaved="hasUnsaved" @lock="updateLocked = $event" />
    </header>

    <dialog id="preview-dialog" ref="previewDialog" class="preview-dialog" aria-labelledby="preview-title"
      @close="closePreview" @click="($event.target === previewDialog) && previewDialog?.close()"></dialog>

    <GalleryView v-if="view === 'gallery'" :generation-pending="pending" :revision="revision" @reuse="requestReuse" @saved="flow?.refresh()" />
    <main v-show="view === 'workspace'" id="main" :class="{ 'compact-workspace': compact }">
      <section class="creation-panel panel" aria-labelledby="creation-title">
        <form :aria-busy="pending" @submit.prevent="generate">
          <h2 id="creation-title"><label for="prompt">创作描述</label></h2>
          <textarea id="prompt" ref="promptInput" v-model="prompt" name="prompt" rows="6"
            placeholder="比如：一只在窗边喝咖啡的小猫，温暖的阳光，手绘水彩风格"
            :maxlength="MAX_PROMPT_LENGTH" required :disabled="pending" aria-describedby="prompt-count generation-hint" />
          <p id="prompt-count" class="prompt-count">{{ prompt.length }} / {{ MAX_PROMPT_LENGTH }}</p>
          <div class="generation-options">
            <div class="section-heading">
              <h2>已选配置</h2>
            </div>
            <div v-for="category in optionCategories" :key="category.key" class="selection-row" :data-selection="category.key">
              <button type="button" class="selection-link"
                :title="selectedOptions[category.key]?.id" @click="openCatalog(category.key)">
                <img v-if="selectedOptions[category.key]?.image" :src="selectedOptions[category.key]!.image" alt="" />
                <span v-else class="selection-placeholder" aria-hidden="true">＋</span>
                <span><small>{{ categoryLabels[category.key] }}</small>
                  <strong>{{ selectedOptions[category.key]?.name ?? '未选择 · 自由生成' }}</strong>
                </span>
              </button>
              <button v-if="selectedOptions[category.key]" type="button" class="clear-selection"
                :aria-label="`清除${categoryLabels[category.key]}选择`" :disabled="pending"
                @click="clearSelection(category.key)">×</button>
            </div>
            <p class="input-hint">未选类别将按描述自由生成</p>
          </div>
          <div id="creation-actions" ref="actions" class="generation-action"></div>
        </form>
        <p v-if="!desktop" class="preview-hint">当前为浏览器预览。请启动桌面应用，并安装、登录本地 Codex 后生成图片。</p>
      </section>

      <section class="catalog-panel panel" aria-labelledby="catalog-title">
        <div class="section-heading">
          <h2 id="catalog-title">灵感图鉴</h2>
          <button v-if="compact" type="button" class="open-preview" aria-haspopup="dialog"
            aria-controls="preview-dialog" :aria-expanded="previewOpen" @click="openPreview">查看计划与作品 · {{ pending ? '处理中' : '预览' }}</button>
        </div>
        <div class="catalog-tabs" role="tablist" aria-label="图鉴类别" @keydown="moveTab">
          <button v-for="category in optionCategories" :id="`${category.key}-tab`" :key="category.key"
            type="button" role="tab" :aria-selected="activeCategory === category.key"
            :aria-controls="activeCategory === category.key ? `${category.key}-panel` : undefined"
            :tabindex="activeCategory === category.key ? 0 : -1" @click="activeCategory = category.key">
            {{ categoryLabels[category.key] }} <small>{{ generationOptions[category.key].length }}</small>
          </button>
        </div>
        <OptionPicker ref="picker" v-model="selections[activeCategory]" :category="activeCategory"
          :label="categoryLabels[activeCategory]" :options="generationOptions[activeCategory]" :disabled="pending" />
      </section>

      <Teleport :to="previewDialog ?? 'body'" :disabled="!compact">
        <section class="preview-panel panel" aria-labelledby="preview-title">
          <div class="section-heading"><h2 id="preview-title">计划与作品预览</h2><button v-if="compact" type="button" class="close-preview" aria-label="关闭作品预览" @click="previewDialog?.close()">×</button></div>
          <CreationFlow ref="flow" :controls="actions" :prompt="prompt" :selections="selections" :reuse-id="reused?.id" :locked="updateLocked" @busy="pending = $event" @unsaved="hasUnsaved = $event" @revision="revision = $event" @restore="restore" />
        </section>
      </Teleport>    </main>

    <dialog ref="reuseDialog" class="confirmation-dialog reuse-dialog" aria-labelledby="reuse-title">
      <h2 id="reuse-title">替换当前编辑？</h2>
      <p>当前有未保存的编辑或尚未保存到图库的作品。复用将替换描述与配置，已有预览仍会保留。</p>
      <button type="button" @click="applyReuse">确认复用</button>
      <button type="button" @click="reuseDialog?.close()">取消</button>
    </dialog>



    <footer class="app-footer">
      <span>给想象力一点留白。</span>
      <span>HANDRAW / 01</span>
    </footer>
  </div>
</template>
