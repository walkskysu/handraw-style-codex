<script setup lang="ts">
import { referenceImages } from '../services/referenceImages'
defineProps<{ modelValue?: string[]; disabled?: boolean }>()
const emit = defineEmits<{ 'update:modelValue': [string[]]; open: [] }>()
const item = (id: string) => referenceImages.value.find(r => r.id === id)
</script>
<template>
  <div class="reference-selection" data-selection="reference">
    <button type="button" class="selection-link" :disabled="disabled" @click="emit('open')"><span class="selection-placeholder" aria-hidden="true">▧</span><span>参考图像</span><span class="selection-value">{{ modelValue?.length ? `已选 ${modelValue.length} 张` : '未选择' }}</span><span aria-hidden="true">›</span></button>
    <div v-for="(id, index) in modelValue" :key="id" class="reference-selected">
      <img v-if="item(id)" class="selection-thumbnail" :src="item(id)!.thumbnail" :alt="item(id)!.name" />
      <span>参考图 {{ index + 1 }} · {{ item(id)?.name ?? '图片不可用，请重新上传或移除' }}</span>
      <button type="button" class="clear-selection" :aria-label="`移除参考图 ${index + 1}`" :disabled="disabled" @click="emit('update:modelValue', modelValue!.filter(value => value !== id))">×</button>
    </div>
  </div>
</template>
<style scoped>
.reference-selection { border-bottom: 1px solid #ecefe7; }
.selection-link { width: 100%; }
.reference-selected { display: flex; align-items: center; gap: 8px; margin-bottom: 8px; font-size: 12px; }
.reference-selected span { flex: 1; min-width: 0; overflow-wrap: anywhere; }
</style>
