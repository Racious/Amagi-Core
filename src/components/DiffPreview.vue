<template>
  <div class="rounded-2xl border overflow-hidden" style="border-color: #ded6f5;">
    <div class="px-4 py-3 border-b flex items-center justify-between"
         style="background: #f9f7ff; border-color: #ded6f5;">
      <div class="text-xs font-bold font-mono truncate" style="color: #2e2a3f;">{{ preview.filePath }}</div>
      <span v-if="preview.isNewFile"
            class="text-xs px-2 py-0.5 rounded-full ml-2 flex-shrink-0"
            style="background: #eefaf4; color: #1d7a51;">新檔案</span>
      <span v-else
            class="text-xs px-2 py-0.5 rounded-full ml-2 flex-shrink-0"
            style="background: #fff4db; color: #916216;">更新</span>
    </div>
    <div class="overflow-x-auto">
      <pre class="text-xs p-4 leading-5" style="font-family: 'Cascadia Code', Consolas, monospace; white-space: pre; min-height: 60px;"><span
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
  if (line.startsWith('+')) return 'color: #1d7a51; background: #eefaf4;'
  if (line.startsWith('-')) return 'color: #d85c5c; background: #fff0f0;'
  if (line.startsWith('@@')) return 'color: #5037c9; background: #f0edff;'
  return 'color: #6f6883;'
}
</script>
