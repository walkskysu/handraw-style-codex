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
const query = ref('')
const scrollArea = ref<HTMLElement | null>(null)
const pageSize = 40
const visibleCount = ref(pageSize)
const filtered = computed(() => {
  const term = query.value.trim().toLocaleLowerCase()
  return props.options.filter((option) => `${option.id} ${option.name} ${option.group}`.toLocaleLowerCase().includes(term))
})
const visible = computed(() => filtered.value.slice(0, visibleCount.value))
watch([query, () => props.category], () => {
  visibleCount.value = pageSize
  if (scrollArea.value) scrollArea.value.scrollTop = 0
}, { flush: 'post' })

async function reveal(id?: string) {
  query.value = ''
  // Let the search/category watchers reset before locating a selected card.
  await nextTick()
  const index = filtered.value.findIndex(option => option.id === id)
  visibleCount.value = Math.max(pageSize, Math.ceil((index + 1) / pageSize) * pageSize)
  await nextTick()
  if (scrollArea.value) scrollArea.value.scrollTop = 0
  const card = Array.from(scrollArea.value?.querySelectorAll<HTMLElement>('[data-option]') ?? [])
    .find((element) => element.dataset.option === id)
  card?.scrollIntoView?.({ block: 'nearest' })
}
defineExpose({ reveal })

function choose(id?: string) {
  if (!props.disabled) emit('update:modelValue', id)
}
</script>

<template>
  <div class="picker-content">
      <div class="picker-tools">
        <label for="catalog-search"><span class="visually-hidden">搜索编号、名称或分类</span>
          <input id="catalog-search" v-model="query" :disabled="disabled" type="search" autocomplete="off" placeholder="搜索名称或编号" />
        </label>
      </div>
    <div :id="`${category}-panel`" ref="scrollArea" class="option-picker" :data-category="category"
      role="tabpanel" tabindex="0" :aria-labelledby="`${category}-tab`">
    <fieldset :disabled="disabled">
      <legend class="visually-hidden">{{ label }}</legend>
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
    </fieldset>
    </div>
    <div class="picker-footer">
      <span role="status">显示 {{ visible.length }} / {{ filtered.length }} 项</span>
      <button type="button" class="load-more" :disabled="visible.length >= filtered.length" @click="visibleCount += pageSize">{{ visible.length < filtered.length ? '加载更多' : '已全部加载' }}</button>
    </div>
  </div>
</template>
