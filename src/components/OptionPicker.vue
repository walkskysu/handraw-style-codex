<script setup lang="ts">
import { computed, nextTick, ref, watch } from 'vue'
import type { GenerationOption, OptionCategory } from '../services/generationOptions'

const props = defineProps<{
  category: OptionCategory
  label: string
  options: GenerationOption[]
  modelValue?: string
  disabled?: boolean
  idPrefix?: string
}>()
const emit = defineEmits<{ 'update:modelValue': [value: string | undefined] }>()
const query = ref('')
const scrollArea = ref<HTMLElement | null>(null)
const filtered = computed(() => {
  const term = query.value.trim().toLocaleLowerCase()
  return props.options.filter((option) => `${option.id} ${option.name} ${option.group}`.toLocaleLowerCase().includes(term))
})
watch([query, () => props.category], () => {
  if (scrollArea.value) scrollArea.value.scrollTop = 0
}, { flush: 'post' })

async function reveal(id?: string) {
  query.value = ''
  // Let the search/category watchers reset before locating a selected card.
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
        <label :for="`${idPrefix ?? ''}catalog-search`"><span class="visually-hidden">搜索编号、名称或分类</span>
          <input :id="`${idPrefix ?? ''}catalog-search`" v-model="query" :disabled="disabled" type="search" autocomplete="off" placeholder="搜索名称或编号" />
        </label>
      </div>
    <div :id="`${idPrefix ?? ''}${category}-panel`" ref="scrollArea" class="option-picker" :data-category="category"
      :role="idPrefix ? 'region' : 'tabpanel'" tabindex="0" :aria-label="idPrefix ? label : undefined" :aria-labelledby="idPrefix ? undefined : `${category}-tab`">
    <fieldset :disabled="disabled">
      <legend class="visually-hidden">{{ label }}</legend>
      <div class="option-grid">
        <button v-for="option in filtered" :key="option.id" type="button" class="option-card"
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
      <span role="status">共 {{ filtered.length }} 项</span>
    </div>
  </div>
</template>
