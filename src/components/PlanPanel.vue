<script setup lang="ts">
import type { Plan } from '../services/creation'
defineProps<{ plan: Plan; versions: Plan[]; feedback: string; disabled: boolean; stale: boolean }>()
defineEmits<{ 'update:feedback': [string]; revise: []; confirm: []; select: [string] }>()
</script>
<template>
  <section class="plan-panel" aria-labelledby="plan-title">
    <h3 id="plan-title">创作计划 · 第 {{ plan.version }} 版 · 共 {{ plan.content.items.length }} 张</h3>
    <h4>{{ plan.content.title }}</h4><p>{{ plan.content.goal }}</p>
    <p>{{ plan.content.kind === 'variants' ? '同一主题的多个变体' : '系列内容' }} · 受众：{{ plan.content.audience }}</p>
    <p>统一视觉：{{ plan.content.visual }} · {{ plan.content.dimensions }}</p>
    <p class="input-hint">核查说明：{{ plan.content.verification }}</p>
    <ol>
      <li v-for="item in plan.content.items" :key="item.id"><strong>{{ item.position + 1 }} · {{ item.title }}</strong>
        <p class="preserve-text">图片文案：{{ item.copy }}</p><p>画面说明：{{ item.description }}</p><p>特殊要求：{{ item.requirements }}</p>
      </li>
    </ol>
    <p>本版修改：{{ plan.content.change_summary || '初始计划' }}</p>
    <details><summary>完整内容与视觉快照</summary><pre>{{ JSON.stringify(plan, null, 2) }}</pre></details>
    <details><summary>查看历史版本</summary>
      <details v-for="version in versions" :key="version.id"><summary>第 {{ version.version }} 版 · {{ version.content.title }}</summary>
        <pre>{{ JSON.stringify(version.content, null, 2) }}</pre>
        <button type="button" :disabled="disabled" @click="$emit('select', version.id)">复制第 {{ version.version }} 版为当前计划</button>
      </details>
    </details>
    <p v-if="stale" role="status">输入或配置已变化，请更新计划。</p>
    <label>修改意见<textarea :value="feedback" :disabled="disabled" maxlength="4000" @input="$emit('update:feedback', ($event.target as HTMLTextAreaElement).value)" /></label>
    <button type="button" :disabled="disabled || !feedback.trim()" @click="$emit('revise')">按意见修改计划</button>
    <button type="button" :disabled="disabled || stale || !!feedback.trim() || !!plan.confirmed_at" @click="$emit('confirm')">确认计划，生成 {{ plan.content.items.length }} 张</button>
    <p v-if="feedback.trim()" class="input-hint">请先应用或清空修改意见，再确认计划。</p>
  </section>
</template>
