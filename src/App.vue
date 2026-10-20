<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref } from 'vue'
import { generateImage, isDesktop, MAX_PROMPT_LENGTH } from './services/imageGeneration'
import OptionPicker from './components/OptionPicker.vue'
import { generationOptions, optionCategories, type GenerationSelections, type OptionCategory } from './services/generationOptions'

const desktop = isDesktop()
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
const catalogDialog = ref<HTMLDialogElement | null>(null)
const catalogOpen = ref(false)
const compactMedia = window.matchMedia?.('(max-width: 1199px)')
const compact = ref(compactMedia?.matches ?? false)
let catalogTrigger: HTMLElement | null = null
const selectedOptions = computed(() => Object.fromEntries(optionCategories.map(({ key }) =>
  [key, generationOptions[key].find((option) => option.id === selections.value[key])],
)))
const status = computed(() => pending.value ? '生成中' : error.value ? '生成失败' : image.value ? '已完成' : '待创作')

function resizeCatalog(event: MediaQueryListEvent) {
  if (!event.matches) catalogDialog.value?.close()
  compact.value = event.matches
}
compactMedia?.addEventListener('change', resizeCatalog)
onBeforeUnmount(() => compactMedia?.removeEventListener('change', resizeCatalog))

async function openCatalog(category: OptionCategory, event: Event) {
  catalogTrigger = event.currentTarget as HTMLElement
  activeCategory.value = category
  await nextTick()
  await picker.value?.reveal(selections.value[category])
  if (compact.value && !catalogDialog.value?.open) {
    catalogDialog.value?.showModal()
    catalogOpen.value = true
  }
  document.getElementById(`${category}-tab`)?.focus()
}

function closeCatalog() {
  catalogOpen.value = false
  catalogTrigger?.focus()
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
  if (pending.value || !desktop) return

  error.value = ''
  errorDialog.value?.close()
  pending.value = true

  try {
    submittedPrompt.value = prompt.value.trim()
    const result = await generateImage(submittedPrompt.value, { ...selections.value })
    image.value = result
    imagePrompt.value = submittedPrompt.value
  } catch (cause) {
    await showError(cause)
  } finally {
    pending.value = false
  }
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
      <span class="mode-badge">{{ desktop ? '桌面应用' : '浏览器预览' }}</span>
    </header>

    <dialog id="catalog-dialog" ref="catalogDialog" class="catalog-dialog" aria-labelledby="catalog-title"
      @close="closeCatalog" @click="($event.target === catalogDialog) && catalogDialog?.close()"></dialog>

    <main id="main" :class="{ 'compact-workspace': compact }">
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
              <button v-if="compact" type="button" class="open-catalog" aria-haspopup="dialog"
                aria-controls="catalog-dialog" :aria-expanded="catalogOpen" @click="openCatalog('style', $event)">选择风格</button>
            </div>
            <div v-for="category in optionCategories" :key="category.key" class="selection-row" :data-selection="category.key">
              <button type="button" class="selection-link"
                :title="selectedOptions[category.key]?.id" @click="openCatalog(category.key, $event)">
                <img v-if="selectedOptions[category.key]" :src="selectedOptions[category.key]!.image" alt="" />
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
            <button type="submit" :disabled="!desktop || pending || !prompt.trim()">
              {{ pending ? '正在生成…' : '生成图片 ↗' }}
            </button>
            <p id="generation-hint" class="input-hint">每次生成一张 · 通常需要几分钟</p>
          </div>
        </form>
        <p v-if="!desktop" class="preview-hint">当前为浏览器预览。请启动桌面应用，并安装、登录本地 Codex 后生成图片。</p>
      </section>

      <Teleport :to="catalogDialog ?? 'body'" :disabled="!compact">
        <section class="catalog-panel panel" aria-labelledby="catalog-title">
          <div class="section-heading">
            <h2 id="catalog-title">灵感图鉴</h2>
            <button v-if="compact" type="button" class="close-catalog" aria-label="关闭图鉴"
              @click="catalogDialog?.close()">×</button>
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
      </Teleport>

      <section class="preview-panel panel" aria-labelledby="preview-title" :aria-busy="pending">
        <div class="section-heading">
          <h2 id="preview-title">作品预览</h2>
          <span class="generation-status" :data-state="pending ? 'pending' : error ? 'error' : 'ready'">{{ status }}</span>
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
          <p v-if="(pending || error) && submittedPrompt && submittedPrompt !== imagePrompt" class="submitted-prompt">本次描述：{{ submittedPrompt }}</p>
          <p v-if="error" role="alert" class="error">{{ error }}</p>
          <p role="status" aria-live="polite">{{ pending ? 'Codex 正在绘制，请稍候…' : error ? '请重试，描述和已选配置均已保留。' : image ? '图片已生成。可以修改描述，再画一张。' : '你的下一幅作品，从一句描述开始。' }}</p>
          <p v-if="image" class="input-hint">生成新图时保留上一张作品</p>
        </div>
      </section>
    </main>

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
