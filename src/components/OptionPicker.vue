<script setup lang="ts">
import { computed, nextTick, ref, watch } from 'vue'
import type { GenerationOption, OptionCategory } from '../services/generationOptions'

const props = defineProps<{
  category: OptionCategory
  label: string
  options: GenerationOption[]
  modelValue?: string
  disabled?: boolean
}>()
const emit = defineEmits<{ 'update:modelValue': [value: string | undefined] }>()
const page = ref(1)
const query = ref('')
const scrollArea = ref<HTMLElement | null>(null)
const pageSize = 9
const filtered = computed(() => {
  const term = query.value.trim().toLocaleLowerCase()
  return props.options.filter((option) => `${option.id} ${option.name} ${option.group}`.toLocaleLowerCase().includes(term))
})
const pageCount = computed(() => Math.max(1, Math.ceil(filtered.value.length / pageSize)))
const visible = computed(() => filtered.value.slice((page.value - 1) * pageSize, page.value * pageSize))
watch([query, () => props.category], () => { page.value = 1 })
watch([page, query, () => props.category], () => {
  if (scrollArea.value) scrollArea.value.scrollTop = 0
}, { flush: 'post' })

async function reveal(id?: string) {
  query.value = ''
  // Let the search/category watchers reset before locating a selected card.
  await nextTick()
  const index = props.options.findIndex((option) => option.id === id)
  page.value = index < 0 ? 1 : Math.floor(index / pageSize) + 1
  await nextTick()
  if (scrollArea.value) scrollArea.value.scrollTop = 0
  scrollArea.value?.querySelector<HTMLElement>('[aria-pressed="true"]')?.scrollIntoView?.({ block: 'nearest' })
}
defineExpose({ reveal })

function choose(id?: string) {
  if (!props.disabled) emit('update:modelValue', id)
}
</script>

<template>
  <div :id="`${category}-panel`" ref="scrollArea" class="option-picker" :data-category="category"
    role="tabpanel" tabindex="0" :aria-labelledby="`${category}-tab`">
    <fieldset :disabled="disabled">
      <legend class="visually-hidden">{{ label }}</legend>
      <div class="picker-tools">
        <label for="catalog-search"><span class="visually-hidden">搜索编号、名称或分类</span>
          <input id="catalog-search" v-model="query" type="search" autocomplete="off" placeholder="搜索编号、名称或分类" />
        </label>
      </div>
      <div class="option-grid">
        <button v-for="option in visible" :key="option.id" type="button" class="option-card"
          :data-option="option.id" :aria-pressed="modelValue === option.id"
          :aria-label="`${option.id} ${option.name}`" @click="choose(option.id)">
          <img :src="option.image" :alt="`${option.id} ${option.name}预览`" loading="lazy" />
          <span v-if="modelValue === option.id" class="selected-check" aria-hidden="true">✓</span>
          <strong :title="`${option.id} · ${option.name} · ${option.group}`">{{ option.name }}</strong>
          <small>{{ option.id }}</small>
        </button>
      </div>
      <p v-if="!filtered.length" class="input-hint">没有匹配的选项，请更换关键词。</p>
      <nav class="picker-pagination" :aria-label="`${label}分页`">
        <button type="button" :disabled="page === 1" @click="page--">上一页</button>
        <span aria-live="polite">第 {{ page }} / {{ pageCount }} 页 · {{ filtered.length }} 项</span>
        <button type="button" :disabled="page === pageCount" @click="page++">下一页</button>
      </nav>
    </fieldset>
  </div>
</template>
