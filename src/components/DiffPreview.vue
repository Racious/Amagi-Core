<template>
  <div class="card overflow-hidden">
    <div class="px-4 py-3 border-b border-border bg-surface-2 flex items-center justify-between">
      <div class="text-xs font-bold font-mono truncate text-fg">{{ preview.filePath }}</div>
      <span v-if="preview.isNewFile"
            class="pill tone-success ml-2 flex-shrink-0">新檔案</span>
      <span v-else
            class="pill tone-warning ml-2 flex-shrink-0">更新</span>
    </div>
    <div class="overflow-x-auto bg-surface-2 text-fg">
      <pre class="text-xs p-4 leading-5 font-mono" style="white-space: pre; min-height: 60px;"><span
        v-for="(line, idx) in lines"
        :key="idx"
        :style="lineStyle(line)"
        class="block"
      >{{ line }}</span></pre>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import type { FileDiffPreview } from '../api/tauriCommands'

const props = defineProps<{ preview: FileDiffPreview }>()

const lines = computed(() => props.preview.newContent.split('\n'))

function lineStyle(line: string) {
  if (line.startsWith('+')) return 'color: var(--c-success); background: var(--c-success-soft);'
  if (line.startsWith('-')) return 'color: var(--c-danger); background: var(--c-danger-soft);'
  if (line.startsWith('@@')) return 'color: var(--c-accent); background: var(--c-accent-soft);'
  return 'color: var(--c-muted);'
}
</script>
