<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref } from 'vue'
import { generateImage, isDesktop, MAX_PROMPT_LENGTH } from './services/imageGeneration'
import OptionPicker from './components/OptionPicker.vue'
import GalleryView from './components/GalleryView.vue'
import AboutSettings from './components/AboutSettings.vue'
import { discardPending, exportArtwork, message, retrySave, retryGroupSave, type Artwork, type GeneratedGroup } from './services/gallery'
import { generationOptions, optionCategories, type GenerationSelections, type OptionCategory } from './services/generationOptions'

const desktop = isDesktop()
const updateLocked = ref(false)
const view = ref<'workspace' | 'gallery'>('workspace')
const generatedGroup = ref<GeneratedGroup | null>(null)
const photoIndex = ref(0)
const count = ref(1)
const generated = computed(() => generatedGroup.value?.photos[photoIndex.value] ?? null)
const hasUnsaved = computed(() => !!generatedGroup.value?.photos.some(p => p.save_error) || !!generatedGroup.value?.group_save_error)
function selectPreview(index: number) {
  photoIndex.value = index
  image.value = generated.value?.image ?? ''
  saveNotice.value = generated.value?.save_error ? '' : '已保存到图库'
  exportError.value = ''
}
function photoSaved(id: string) {
  const photo = generatedGroup.value?.photos.find(p => p.id === id)
  if (photo) { photo.save_error = null; generatedGroup.value!.group_save_error = null }
}
async function saveGroupAgain() {
  if (!generatedGroup.value || saving.value) return
  saving.value = true
  try {
    await retryGroupSave(generatedGroup.value.group_id)
    generatedGroup.value.group_save_error = null
    if (!hasUnsaved.value) saveNotice.value = '已保存到图库'
  } catch (cause) { generatedGroup.value.group_save_error = message(cause) }
  finally { saving.value = false }
}
const saving = ref(false)
const exportNotes = ref(true)
const saveNotice = ref('')
const exportError = ref('')
const reused = ref<Artwork | null>(null)
const baseline = ref(JSON.stringify({ prompt: '', selections: {} }))
const reuseDialog = ref<HTMLDialogElement | null>(null)
const replacement = ref<Artwork | null>(null)
const draft = () => JSON.stringify({ prompt: prompt.value, selections: selections.value })
const prompt = ref('')
const image = ref('')
const imagePrompt = ref('')
const submittedPrompt = ref('')
const error = ref('')
const pending = ref(false)
const selections = ref<GenerationSelections>({})
const errorDialog = ref<HTMLDialogElement | null>(null)
const promptInput = ref<HTMLTextAreaElement | null>(null)
const needsCodexUpgrade = computed(() => /requires a newer version of Codex/i.test(error.value))
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
const status = computed(() => pending.value ? '生成中' : error.value ? '生成失败' : image.value ? '已完成' : '待创作')

function resizeWorkspace(event: MediaQueryListEvent) {
  if (!event.matches) previewDialog.value?.close()
  compact.value = event.matches
}
compactMedia?.addEventListener('change', resizeWorkspace)
onBeforeUnmount(() => compactMedia?.removeEventListener('change', resizeWorkspace))

async function openCatalog(category: OptionCategory) {
  activeCategory.value = category
  await nextTick()
  await picker.value?.reveal(selections.value[category])
  document.getElementById(`${category}-tab`)?.focus()
}

function openPreview(event: Event) {
  previewTrigger = event.currentTarget as HTMLElement
  previewDialog.value?.showModal()
  previewOpen.value = true
}

function closePreview() {
  previewOpen.value = false
  previewTrigger?.focus()
}

function moveTab(event: KeyboardEvent) {
  const keys = optionCategories.map(({ key }) => key)
  const index = keys.indexOf(activeCategory.value)
  const next = event.key === 'Home' ? 0 : event.key === 'End' ? keys.length - 1
    : event.key === 'ArrowRight' ? (index + 1) % keys.length
      : event.key === 'ArrowLeft' ? (index + keys.length - 1) % keys.length : -1
  if (next < 0) return
  event.preventDefault()
  activeCategory.value = keys[next]!
  nextTick(() => document.getElementById(`${activeCategory.value}-tab`)?.focus())
}

function clearSelection(category: OptionCategory) {
  if (!pending.value) delete selections.value[category]
}

async function showError(cause: unknown) {
  const message = typeof cause === 'string' ? cause : cause instanceof Error ? cause.message : ''
  error.value = message.trim() ? message : '暂时无法生成图片，请再试一次。'
  await nextTick()
  if (!errorDialog.value?.open) errorDialog.value?.showModal()
}

async function generate() {
  if (pending.value || saving.value || hasUnsaved.value || updateLocked.value || !desktop) return

  error.value = ''
  errorDialog.value?.close()
  pending.value = true

  try {
    submittedPrompt.value = prompt.value.trim()
    const result = await generateImage(submittedPrompt.value, { ...selections.value }, reused.value?.id, count.value)
    generatedGroup.value = result
    photoIndex.value = 0
    image.value = generated.value?.image ?? ''
    imagePrompt.value = submittedPrompt.value
    baseline.value = draft()
    saveNotice.value = hasUnsaved.value ? '' : '已保存到图库'
    exportError.value = ''
  } catch (cause) {
    await showError(cause)
  } finally {
    pending.value = false
  }
}

async function saveAgain() {
  if (!generated.value || saving.value) return
  saving.value = true
  try {
    await retrySave(generated.value.id)
    generated.value.save_error = null
    generatedGroup.value!.group_save_error = null
    saveNotice.value = '已保存到图库'
  } catch (cause) { generated.value.save_error = message(cause) }
  finally { saving.value = false }
}

async function exportPreview() {
  if (!generated.value || saving.value) return
  saving.value = true; exportError.value = ''
  try {
    if (await exportArtwork(generated.value.id, exportNotes.value)) saveNotice.value = '已导出图片'
  } catch (cause) { exportError.value = message(cause) }
  finally { saving.value = false }
}

async function discardExported() {
  if (!generated.value?.save_error || saving.value || saveNotice.value !== '已导出图片') return
  saving.value = true
  try {
    await discardPending(generated.value.id)
    generatedGroup.value!.photos.splice(photoIndex.value, 1)
    selectPreview(0)
    saveNotice.value = ''
  } catch (cause) { exportError.value = message(cause) }
  finally { saving.value = false }
}

function requestReuse(artwork: Artwork) {
  if (pending.value || saving.value) return
  replacement.value = artwork
  if (draft() !== baseline.value || hasUnsaved.value) reuseDialog.value?.showModal()
  else applyReuse()
}

function applyReuse() {
  if (!replacement.value) return
  reused.value = replacement.value
  prompt.value = replacement.value.prompt
  selections.value = Object.fromEntries(optionCategories.flatMap(({ key }) => {
    const option = replacement.value!.snapshot[key]
    return option ? [[key, option.id]] : []
  }))
  baseline.value = draft()
  // The failed preview remains available for retry/export after refilling the form.
  view.value = 'workspace'
  reuseDialog.value?.close()
  nextTick(() => promptInput.value?.focus())
}

function changeView(next: 'workspace' | 'gallery') {
  previewDialog.value?.close()
  view.value = next
}

function previewFailed() {
  image.value = ''
  showError('图片无法显示，请重新生成。')
}
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
      <AboutSettings :working="pending || saving" :unsaved="hasUnsaved" @lock="updateLocked = $event" />
    </header>

    <dialog id="preview-dialog" ref="previewDialog" class="preview-dialog" aria-labelledby="preview-title"
      @close="closePreview" @click="($event.target === previewDialog) && previewDialog?.close()"></dialog>

    <GalleryView v-if="view === 'gallery'" :generation-pending="pending || saving" :revision="`${generatedGroup?.group_id}-${hasUnsaved}`" @reuse="requestReuse" @saved="photoSaved" />
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
          <div class="generation-action">
            <label>本组照片数量 <select v-model.number="count" :disabled="pending"><option v-for="n in 4" :key="n" :value="n">{{ n }} 张</option></select></label>
            <button type="submit" :disabled="!desktop || pending || saving || updateLocked || hasUnsaved || !prompt.trim()">
              {{ pending ? '正在生成…' : '生成图片 ↗' }}
            </button>
            <p id="generation-hint" class="input-hint">每次生成一个独立照片组 · 每张通常需要几分钟</p>
            <p v-if="updateLocked" class="input-hint">已预约更新，当前任务和作品保存完成后将安装。可在设置中取消。</p>
            <p v-if="hasUnsaved" class="input-hint">请先逐张重试保存，或导出后放弃该图的图库保存，再继续生成。</p>
          </div>
        </form>
        <p v-if="!desktop" class="preview-hint">当前为浏览器预览。请启动桌面应用，并安装、登录本地 Codex 后生成图片。</p>
      </section>

      <section class="catalog-panel panel" aria-labelledby="catalog-title">
        <div class="section-heading">
          <h2 id="catalog-title">灵感图鉴</h2>
          <button v-if="compact" type="button" class="open-preview" aria-haspopup="dialog"
            aria-controls="preview-dialog" :aria-expanded="previewOpen" @click="openPreview">查看作品 · {{ status }}</button>
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
        <section class="preview-panel panel" aria-labelledby="preview-title" :aria-busy="pending">
          <div class="section-heading">
            <h2 id="preview-title">作品预览</h2>
            <span class="generation-status" :data-state="pending ? 'pending' : error ? 'error' : 'ready'">{{ status }}</span>
            <button v-if="compact" type="button" class="close-preview" aria-label="关闭作品预览"
              @click="previewDialog?.close()">×</button>
          </div>
          <figure v-if="image" class="image-preview">
            <img :src="image" :alt="imagePrompt" @error="previewFailed" />
            <figcaption>{{ imagePrompt }}</figcaption>
          </figure>
          <div v-else class="preview-placeholder">
            <span aria-hidden="true">✳</span>
            <p>{{ pending ? '画面正在慢慢成形…' : '生成的图片会出现在这里' }}</p>
          </div>
          <div class="response-area">
            <template v-if="generatedGroup">
              <p v-if="generatedGroup.group_save_error" class="error" role="alert">照片组信息保存失败：{{ generatedGroup.group_save_error }}。预览已保留，关闭应用前请重试。</p>
              <button v-if="generatedGroup.group_save_error" type="button" :disabled="saving" @click="saveGroupAgain">重试保存组信息</button>
              <p class="group-status">{{ generatedGroup.photos.length }} / {{ generatedGroup.expected_count }} 张成功 · 已保存 {{ generatedGroup.photos.filter(p => !p.save_error).length }} 张 · {{ generatedGroup.photos.filter(p => p.save_error).length }} 张待保存</p>
              <div class="photo-navigation" aria-label="本次生成的照片">
                <button v-for="(photo, index) in generatedGroup.photos" :key="photo.id" type="button" :disabled="saving" :aria-pressed="photoIndex === index" @click="selectPreview(index)">第 {{ index + 1 }} 张{{ photo.save_error ? ' · 待保存' : '' }}</button>
              </div>
              <p v-for="failure in generatedGroup.generation_errors" :key="failure" class="error">{{ failure }}</p>
            </template>
            <div v-if="generated" class="save-controls">
              <p v-if="generated.save_error" role="alert" class="error">图库保存失败：{{ generated.save_error }}。预览已保留；关闭应用前请重试或导出。</p>
              <p class="save-status" aria-live="polite">{{ saving ? '正在处理…' : saveNotice }}</p>
              <button v-if="generated.save_error" type="button" :disabled="saving" @click="saveAgain">重试保存</button>
              <label><input v-model="exportNotes" type="checkbox" /> 同时导出提示词说明</label>
              <button type="button" :disabled="saving" @click="exportPreview">导出图片</button>
              <button v-if="generated.save_error && saveNotice === '已导出图片'" type="button" :disabled="saving" @click="discardExported">放弃本次图库保存</button>
              <p v-if="exportError" role="alert" class="error">{{ exportError }}</p>
            </div>
            <p v-if="(pending || error) && submittedPrompt && submittedPrompt !== imagePrompt" class="submitted-prompt">本次描述：{{ submittedPrompt }}</p>
            <p v-if="error" role="alert" class="error">{{ error }}</p>
            <p role="status" aria-live="polite">{{ pending ? 'Codex 正在绘制，请稍候…' : error ? '请重试，描述和已选配置均已保留。' : image ? '图片已生成。可以修改描述，再画一张。' : '你的下一幅作品，从一句描述开始。' }}</p>
            <p v-if="image" class="input-hint">生成新图时保留上一张作品</p>
          </div>
        </section>
      </Teleport>
    </main>

    <dialog ref="reuseDialog" class="confirmation-dialog reuse-dialog" aria-labelledby="reuse-title">
      <h2 id="reuse-title">替换当前编辑？</h2>
      <p>当前有未保存的编辑或尚未保存到图库的作品。复用将替换描述与配置，已有预览仍会保留。</p>
      <button type="button" @click="applyReuse">确认复用</button>
      <button type="button" @click="reuseDialog?.close()">取消</button>
    </dialog>

    <dialog ref="errorDialog" class="error-dialog" role="alertdialog" aria-modal="true"
      aria-labelledby="error-title" :aria-describedby="needsCodexUpgrade ? 'upgrade-hint error-detail' : 'error-detail'"
      @close="promptInput?.focus()">
      <h2 id="error-title">图片生成失败</h2>
      <div v-if="needsCodexUpgrade" id="upgrade-hint" class="upgrade-hint">
        <p>当前 Codex 版本过旧，无法使用所选模型。请在终端运行以下命令升级：</p>
        <pre><code>npm.cmd install -g @openai/codex@latest</code></pre>
        <p>升级完成后，重新启动 Handraw 并重试。</p>
      </div>
      <p id="error-detail" class="error">{{ error }}</p>
      <button type="button" autofocus @click="errorDialog?.close()">知道了</button>
    </dialog>

    <footer class="app-footer">
      <span>给想象力一点留白。</span>
      <span>HANDRAW / 01</span>
    </footer>
  </div>
</template>
