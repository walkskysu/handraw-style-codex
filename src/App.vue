<script setup lang="ts">
import { ref } from 'vue'
import { greet, isDesktop } from './services/greeting'

const desktop = isDesktop()
const name = ref('')
const message = ref('')
const error = ref('')
const pending = ref(false)

async function sayHello() {
  if (pending.value) return

  error.value = ''
  message.value = ''
  pending.value = true

  try {
    message.value = await greet(name.value)
  } catch {
    error.value = '暂时无法完成问候，请再试一次。'
  } finally {
    pending.value = false
  }
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
        <p class="intro-copy">欢迎来到 Handraw，你的手绘风格工作台。<br />为下一个好点子，留一点自由生长的空间。</p>
        <div class="sketch" aria-hidden="true">
          <span class="sketch-star">✳</span>
          <span class="sketch-note">hello, idea!</span>
        </div>
      </section>

      <section class="welcome-card" aria-labelledby="greeting-title">
        <span class="tape" aria-hidden="true"></span>
        <p class="eyebrow">初次见面 / HELLO</p>
        <h2 id="greeting-title">怎么称呼你？</h2>
        <p>写下名字，向你的新工作台打个招呼。</p>
        <form :aria-busy="pending" @submit.prevent="sayHello">
          <label for="name">你的名字</label>
          <div class="form-row">
            <input id="name" v-model="name" name="name" autocomplete="given-name" placeholder="比如：小画家" required :disabled="pending" />
            <button type="submit" :disabled="pending || !name.trim()">
              {{ pending ? '问候中…' : '打个招呼 ↗' }}
            </button>
          </div>
        </form>
        <div class="response-area">
          <p v-if="error" role="alert" class="error">{{ error }}</p>
          <p v-else role="status" aria-live="polite">{{ message || '好点子，从一句「你好」开始。' }}</p>
        </div>
        <p v-if="!desktop" class="preview-hint">当前为浏览器预览，问候将在本地生成。</p>
      </section>
    </main>

    <footer class="app-footer">
      <span>给想象力一点留白。</span>
      <span>HANDRAW / 01</span>
    </footer>
  </div>
</template>
