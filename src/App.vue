<script setup lang="ts">
import { computed, nextTick, ref } from 'vue'
import { generateImage, isDesktop, MAX_PROMPT_LENGTH } from './services/imageGeneration'

const desktop = isDesktop()
const prompt = ref('')
const image = ref('')
const imagePrompt = ref('')
const error = ref('')
const pending = ref(false)
const errorDialog = ref<HTMLDialogElement | null>(null)
const promptInput = ref<HTMLTextAreaElement | null>(null)
const needsCodexUpgrade = computed(() => /requires a newer version of Codex/i.test(error.value))

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
    const submittedPrompt = prompt.value.trim()
    const result = await generateImage(submittedPrompt)
    image.value = result
    imagePrompt.value = submittedPrompt
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
        <span class="brand-mark" aria-hidden="true">h.</span>
        <span>handraw<span class="brand-dot">.</span></span>
      </a>
      <span class="mode-badge">{{ desktop ? '桌面应用' : '浏览器预览' }}</span>
    </header>

    <main id="main">
      <section class="intro" aria-labelledby="welcome-title">
        <p class="eyebrow">一张白纸，无限可能</p>
        <h1 id="welcome-title">让灵感，<br /><span>从这里开始。</span></h1>
        <p class="intro-copy">写下脑海中的画面，让 Codex 帮你画出来。</p>
        <form :aria-busy="pending" @submit.prevent="generate">
          <label for="prompt">描述你想生成的画面</label>
          <textarea id="prompt" ref="promptInput" v-model="prompt" name="prompt" rows="6"
            placeholder="比如：一只在窗边喝咖啡的小猫，温暖的阳光，手绘水彩风格"
            :maxlength="MAX_PROMPT_LENGTH" required :disabled="pending" aria-describedby="generation-hint" />
          <p id="generation-hint" class="input-hint">生成可能需要几分钟。每次生成一张图片。</p>
          <button type="submit" :disabled="!desktop || pending || !prompt.trim()">
            {{ pending ? '正在生成…' : '生成图片 ↗' }}
          </button>
        </form>
        <div class="response-area">
          <p v-if="error" role="alert" class="error">{{ error }}</p>
          <p role="status" aria-live="polite">{{ pending ? 'Codex 正在绘制，请稍候…' : image ? '图片已生成。可以修改提示词，再画一张。' : '你的下一幅作品，从一句描述开始。' }}</p>
        </div>
        <p v-if="!desktop" class="preview-hint">当前为浏览器预览。请启动桌面应用，并安装、登录本地 Codex 后生成图片。</p>
      </section>

      <section class="welcome-card" aria-labelledby="preview-title" :aria-busy="pending">
        <span class="tape" aria-hidden="true"></span>
        <p class="eyebrow">灵感显影 / PREVIEW</p>
        <h2 id="preview-title">你的画面</h2>
        <figure v-if="image" class="image-preview">
          <img :src="image" :alt="imagePrompt" @error="previewFailed" />
          <figcaption>{{ imagePrompt }}</figcaption>
        </figure>
        <div v-else class="preview-placeholder">
          <span aria-hidden="true">✳</span>
          <p>{{ pending ? '画面正在慢慢成形…' : '生成的图片会出现在这里' }}</p>
        </div>
        <p v-if="pending && image" class="input-hint">新画面正在生成，暂时保留上一张作品。</p>
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
