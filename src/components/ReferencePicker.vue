<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { isDesktop } from '../services/imageGeneration'
import { message } from '../services/gallery'
import { loadReferences, uploadReferences, referenceImages, referenceError, referenceLoading, MAX_REFERENCES } from '../services/referenceImages'
const props = defineProps<{ modelValue?: string[]; disabled?: boolean; embedded?: boolean }>()
const emit = defineEmits<{ 'update:modelValue': [string[]] }>()
const fileInput = ref<HTMLInputElement>()
const query = ref('')
const uploading = ref(false)
const error = ref('')
const selected = computed(() => props.modelValue ?? [])
const filtered = computed(() => referenceImages.value.filter(r => r.name.toLocaleLowerCase().includes(query.value.trim().toLocaleLowerCase())))
onMounted(loadReferences)
function toggle(id: string) {
  if (props.disabled || uploading.value) return
  error.value = ''
  if (selected.value.includes(id)) emit('update:modelValue', selected.value.filter(v => v !== id))
  else if (selected.value.length >= MAX_REFERENCES) error.value = `最多选择 ${MAX_REFERENCES} 张参考图像。`
  else emit('update:modelValue', [...selected.value, id])
}
async function upload(event: Event) {
  const input = event.target as HTMLInputElement
  const files = Array.from(input.files ?? [])
  input.value = ''
  if (props.disabled || uploading.value) return
  uploading.value = true; error.value = ''
  try { await uploadReferences(files) }
  catch (e) { error.value = message(e) }
  finally { uploading.value = false }
}
</script>
<template>
  <section :id="embedded ? undefined : 'reference-panel'" class="reference-picker picker-content" :role="embedded ? 'region' : 'tabpanel'"
    :aria-labelledby="embedded ? undefined : 'reference-tab'" :aria-label="embedded ? '参考图像' : undefined">
    <div class="reference-tools">
      <input v-model="query" type="search" aria-label="搜索参考图像" placeholder="搜索参考图像名称" />
      <input ref="fileInput" class="visually-hidden" type="file" multiple accept="image/png,image/jpeg,image/webp" aria-label="上传人物或风格图片" :disabled="disabled || uploading || !isDesktop()" @change="upload" />
      <button type="button" :disabled="disabled || uploading || !isDesktop()" @click="fileInput?.click()">{{ uploading ? '上传中…' : '上传参考图像' }}</button>
    </div>
    <p class="input-hint reference-help">上传人物或风格图片后点击选择，可多选（最多 {{ MAX_REFERENCES }} 张）。在描述中说明如何使用参考图 1、参考图 2。支持 PNG、JPEG、WebP，每张不超过 20 MB。</p>
    <p v-if="!isDesktop()" class="input-hint reference-help">请在桌面应用中上传参考图像。</p>
    <p v-if="error || referenceError" class="error reference-help" role="alert">{{ error || referenceError }} <button v-if="referenceError" type="button" @click="loadReferences">重新加载</button></p>
    <div class="option-picker" tabindex="0" aria-label="参考图像列表">
      <p v-if="referenceLoading" role="status">正在读取参考图像…</p>
      <p v-else-if="!filtered.length" class="input-hint">{{ referenceImages.length ? '没有匹配的参考图像。' : '还没有参考图像，请先上传。' }}</p>
      <div class="option-grid">
        <button v-for="item in filtered" :key="item.id" type="button" class="option-card" :data-reference="item.id"
          :aria-pressed="selected.includes(item.id)" :disabled="disabled || uploading" @click="toggle(item.id)">
          <img :src="item.thumbnail" :alt="item.name" loading="lazy" />
          <span v-if="selected.includes(item.id)" class="selected-check">{{ selected.indexOf(item.id) + 1 }}</span>
          <strong :title="item.name">{{ item.name }}</strong>
        </button>
      </div>
    </div>
    <div class="picker-footer"><span role="status">已选择 {{ selected.length }} 张 / 共 {{ referenceImages.length }} 张</span><button type="button" :disabled="disabled || uploading || !selected.length" @click="emit('update:modelValue', [])">清除选择</button></div>
  </section>
</template>
<style scoped>
.reference-picker { overflow-y: auto; }
.reference-picker > .option-picker { flex: 1 0 180px; }
.reference-tools { display: flex; flex-wrap: wrap; gap: 8px; padding: 0 24px 12px; }
.reference-tools input[type="search"] { flex: 1; min-width: 120px; width: 100%; padding: 10px; border: 1px solid var(--border); border-radius: 8px; }
.reference-help { margin: 0 24px 12px; }
</style>
