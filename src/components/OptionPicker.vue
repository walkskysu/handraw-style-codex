<script setup lang="ts">
import { computed, ref, watch } from 'vue'
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
const pageSize = 6
const selected = computed(() => props.options.find((option) => option.id === props.modelValue))
const filtered = computed(() => {
  const term = query.value.trim().toLocaleLowerCase()
  return props.options.filter((option) => `${option.id} ${option.name} ${option.group}`.toLocaleLowerCase().includes(term))
})
const pageCount = computed(() => Math.max(1, Math.ceil(filtered.value.length / pageSize)))
const visible = computed(() => filtered.value.slice((page.value - 1) * pageSize, page.value * pageSize))
watch(query, () => { page.value = 1 })

function choose(id?: string) {
  if (!props.disabled) emit('update:modelValue', id)
}
</script>

<template>
  <details class="option-picker" :data-category="category">
    <summary>
      <span>{{ label }} <small>（可选 · {{ options.length }} 种）</small></span>
      <span class="selection-summary">{{ selected ? `${selected.id} · ${selected.name}` : '未选择 · 自由生成' }}</span>
    </summary>
    <fieldset :disabled="disabled">
      <legend class="visually-hidden">{{ label }}</legend>
      <div class="picker-tools">
        <label :for="`${category}-search`">搜索编号、名称或分类
          <input :id="`${category}-search`" v-model="query" type="search" autocomplete="off" />
        </label>
        <button type="button" class="clear-selection" :disabled="!selected" @click="choose()">清除选择</button>
      </div>
      <div class="option-grid">
        <button v-for="option in visible" :key="option.id" type="button" class="option-card"
          :data-option="option.id" :aria-pressed="modelValue === option.id"
          :aria-label="`${option.id} ${option.name}`" @click="choose(option.id)">
          <img :src="option.image" :alt="`${option.id} ${option.name}预览`" loading="lazy" />
          <span>{{ option.id }} <span v-if="modelValue === option.id">✓ 已选</span></span>
          <strong>{{ option.name }}</strong>
          <small>{{ option.group }}</small>
        </button>
      </div>
      <p v-if="!filtered.length" class="input-hint">没有匹配的选项，请更换关键词。</p>
      <nav class="picker-pagination" :aria-label="`${label}分页`">
        <button type="button" :disabled="page === 1" @click="page--">上一页</button>
        <span aria-live="polite">第 {{ page }} / {{ pageCount }} 页 · {{ filtered.length }} 项</span>
        <button type="button" :disabled="page === pageCount" @click="page++">下一页</button>
      </nav>
    </fieldset>
  </details>
</template>
