<template>
  <span class="inline-flex items-center px-2 py-0.5 rounded-full text-xs font-bold" :style="style">
    {{ label }}
  </span>
</template>

<script setup lang="ts">
import { computed } from 'vue'

const props = defineProps<{ status: string }>()

const map: Record<string, { label: string; bg: string; color: string }> = {
  pending:  { label: '待審核', bg: '#fff4db', color: '#916216' },
  accepted: { label: '已接受', bg: '#eefaf4', color: '#1d7a51' },
  ignored:  { label: '已忽略', bg: '#f5f5f5', color: '#6f6883' },
  synced:   { label: '已同步', bg: '#e8f4ff', color: '#1a6fc4' },
  blocked:  { label: '已封鎖', bg: '#fff0f0', color: '#d85c5c' },
  low:      { label: '低風險', bg: '#eefaf4', color: '#1d7a51' },
  medium:   { label: '中風險', bg: '#fff4db', color: '#916216' },
  high:     { label: '高風險', bg: '#fff0f0', color: '#d85c5c' },
  memory:   { label: '記憶', bg: '#eee8ff', color: '#5037c9' },
  skill:    { label: '技能', bg: '#e8f4ff', color: '#1a6fc4' },
  blocked_type: { label: '封鎖', bg: '#fff0f0', color: '#d85c5c' },
}

const info = computed(() => map[props.status] ?? { label: props.status, bg: '#f5f5f5', color: '#6f6883' })
const style = computed(() => `background: ${info.value.bg}; color: ${info.value.color};`)
const label = computed(() => info.value.label)
</script>
