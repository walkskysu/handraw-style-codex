<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { isDesktop } from '../services/imageGeneration'
import { message } from '../services/gallery'
import { checkUpdate, downloadUpdate, getAbout, installUpdate, openManualUpdate, RELEASE_PAGE, type Release, type UpdateProgress } from '../services/updates'

const props = defineProps<{ working: boolean; unsaved: boolean }>()
const emit = defineEmits<{ lock: [value: boolean] }>()
const desktop = isDesktop()
const dialog = ref<HTMLDialogElement | null>(null)
const trigger = ref<HTMLButtonElement | null>(null)
const version = ref('')
const release = ref<Release | null>(null)
const phase = ref<'idle' | 'checking' | 'available' | 'latest' | 'downloading' | 'ready' | 'waiting' | 'installing' | 'error'>('idle')
const error = ref('')
const manualError = ref('')
const progress = ref<UpdateProgress>({ downloaded: 0, total: null })
const reminder = ref(false)
const busy = computed(() => ['checking', 'downloading', 'waiting', 'installing'].includes(phase.value))
const percent = computed(() => progress.value.total ? Math.min(100, Math.round(progress.value.downloaded / progress.value.total * 100)) : undefined)
const status = computed(() => ({
  idle: '检查 GitHub Release 中的新版本。', checking: '正在检查更新…', available: '有新版本可用。', latest: '当前已是最新版本。',
  downloading: `正在下载并校验签名… ${percent.value === undefined ? `${(progress.value.downloaded / 1024 / 1024).toFixed(1)} MB` : `${percent.value}%`}`,
  ready: '下载完成，签名校验通过。', waiting: props.unsaved ? '作品尚未保存。请返回作品预览重试保存，保存成功后自动安装。' : '等待当前任务结束并保存作品后，将自动重启并安装。',
  installing: '正在启动安装程序…', error: '更新未完成，可以重试或手动安装。',
})[phase.value])
let startupTimer: ReturnType<typeof setTimeout> | undefined
let reminderTimer: ReturnType<typeof setTimeout> | undefined

async function check(automatic = false) {
  if (!desktop || busy.value || phase.value === 'ready') return
  phase.value = 'checking'; error.value = ''; manualError.value = ''; reminder.value = false
  release.value = null
  try {
    const about = await getAbout()
    version.value = about.current_version
    error.value = about.install_error ?? ''
    release.value = await checkUpdate()
    phase.value = error.value ? 'error' : release.value ? 'available' : 'latest'
    if (automatic && (release.value || error.value)) reminder.value = true
  } catch (cause) {
    error.value = message(cause); phase.value = 'error'
    // Startup network failures stay in Settings, without interrupting creation.
  }
}

function open() {
  clearTimeout(startupTimer)
  reminder.value = false
  dialog.value?.showModal()
  if (phase.value === 'idle') void check()
}

function later() {
  reminder.value = false
  dialog.value?.close()
  clearTimeout(reminderTimer)
  // Session-scoped: remind again in one hour, or on the next launch.
  reminderTimer = setTimeout(() => { if (release.value && phase.value !== 'installing') reminder.value = true }, 60 * 60 * 1000)
}

async function download() {
  if (!release.value || busy.value) return
  phase.value = 'downloading'; error.value = ''; progress.value = { downloaded: 0, total: null }
  try {
    await downloadUpdate(value => { progress.value = value })
    phase.value = 'ready'
  } catch (cause) { error.value = message(cause); phase.value = 'error' }
}

async function installWhenSafe() {
  if (phase.value !== 'waiting' || props.working || props.unsaved) return
  phase.value = 'installing'
  try { await installUpdate() }
  catch (cause) { error.value = message(cause); phase.value = 'error'; emit('lock', false) }
}

function requestInstall() {
  if (phase.value !== 'ready') return
  phase.value = 'waiting'
  emit('lock', true)
  void installWhenSafe()
}

function cancelInstall() {
  if (phase.value !== 'waiting') return
  phase.value = 'ready'
  emit('lock', false)
}

async function manual() {
  manualError.value = ''
  try { await openManualUpdate() } catch (cause) { manualError.value = message(cause) }
}

watch(() => [props.working, props.unsaved], () => { void installWhenSafe() })
onMounted(() => { if (desktop) startupTimer = setTimeout(() => { void check(true) }, 5000) })
onBeforeUnmount(() => { clearTimeout(startupTimer); clearTimeout(reminderTimer) })
</script>

<template>
  <button ref="trigger" type="button" class="settings-trigger" aria-haspopup="dialog" aria-controls="settings-dialog" @click="open">设置</button>
  <aside v-if="reminder" class="update-reminder" aria-label="更新提醒">
    <span>{{ release ? `发现新版本：${release.version}` : '上次更新未完成' }}</span>
    <button type="button" @click="open">查看更新</button>
    <button type="button" @click="later">稍后提醒</button>
  </aside>
  <dialog id="settings-dialog" ref="dialog" class="settings-dialog" aria-labelledby="settings-title" @close="trigger?.focus()">
    <div class="section-heading">
      <h2 id="settings-title">设置</h2>
      <button type="button" aria-label="关闭设置" @click="dialog?.close()">×</button>
    </div>
    <section aria-labelledby="about-title">
      <h3 id="about-title">关于 Handraw</h3>
      <p>当前版本：{{ version || (desktop ? '读取中…' : '浏览器预览') }}</p>
      <template v-if="desktop">
        <p v-if="release">发现新版本：{{ release.version }}</p>
        <div v-if="release" class="release-notes">
          <h4>更新内容：</h4>
          <p>{{ release.notes || '此版本未提供更新说明。' }}</p>
        </div>
        <p class="update-status" aria-live="polite">{{ status }}</p>
        <progress v-if="phase === 'downloading'" :value="percent" max="100" aria-label="更新下载进度" />
        <p v-if="error" role="alert" class="error">{{ error }}</p>
        <div class="update-actions">
          <button v-if="release && !['waiting', 'installing'].includes(phase)" type="button" @click="later">稍后提醒</button>
          <button v-if="release && ['available', 'error'].includes(phase)" type="button" @click="download">{{ phase === 'error' ? '重新下载' : '下载更新' }}</button>
          <button v-if="phase === 'ready'" type="button" @click="requestInstall">重启并安装</button>
          <button v-if="phase === 'waiting'" type="button" @click="cancelInstall">取消自动安装</button>
          <button v-if="!busy && phase !== 'ready'" type="button" @click="check()">{{ phase === 'error' ? '重新检查' : '检查更新' }}</button>
          <button type="button" :disabled="phase === 'installing'" @click="manual">手动安装</button>
        </div>
        <p class="input-hint">升级前会等待图片生成和保存完成。手动安装时，请先保存作品并关闭应用。</p>
        <p v-if="manualError" role="alert" class="error">{{ manualError }}<br />{{ RELEASE_PAGE }}</p>
      </template>
      <p v-else>请在桌面应用中检查和安装更新。</p>
    </section>
  </dialog>
</template>
