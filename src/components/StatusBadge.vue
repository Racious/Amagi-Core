<template>
  <span class="pill" :class="info.tone">{{ info.label }}</span>
</template>

<script setup lang="ts">
import { computed } from 'vue'

const props = defineProps<{ status: string }>()

const map: Record<string, { label: string; tone: string }> = {
  pending:      { label: '待審核', tone: 'tone-warning' },
  accepted:     { label: '已接受', tone: 'tone-success' },
  ignored:      { label: '已忽略', tone: 'tone-muted' },
  synced:       { label: '已同步', tone: 'tone-info' },
  blocked:      { label: '已封鎖', tone: 'tone-danger' },
  low:          { label: '低風險', tone: 'tone-success' },
  medium:       { label: '中風險', tone: 'tone-warning' },
  high:         { label: '高風險', tone: 'tone-danger' },
  memory:       { label: '記憶', tone: 'tone-accent' },
  skill:        { label: '技能', tone: 'tone-info' },
  blocked_type: { label: '封鎖', tone: 'tone-danger' },
}

const info = computed(() => map[props.status] ?? { label: props.status, tone: 'tone-muted' })
</script>
