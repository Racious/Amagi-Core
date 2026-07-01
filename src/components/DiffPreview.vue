<template>
  <div class="card overflow-hidden">
    <div class="px-4 py-3 border-b border-border bg-surface-2 flex items-center justify-between">
      <div class="text-xs font-bold font-mono truncate text-fg">{{ preview.filePath }}</div>
      <span v-if="preview.isNewFile" class="pill tone-success ml-2 flex-shrink-0">新檔案</span>
      <span v-else-if="noChange" class="pill tone-muted ml-2 flex-shrink-0">無變更</span>
      <span v-else class="pill tone-warning ml-2 flex-shrink-0">更新</span>
    </div>
    <div class="overflow-x-auto bg-surface-2 text-fg">
      <div v-if="noChange" class="text-xs p-4 text-muted">（此檔內容無變更）</div>
      <pre v-else class="text-xs p-4 leading-5 font-mono" style="white-space: pre; min-height: 60px;"><span
        v-for="(d, idx) in diffLines"
        :key="idx"
        :style="lineStyle(d.t)"
        class="block"
      >{{ d.t }}{{ d.l }}</span></pre>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import type { FileDiffPreview } from '../api/tauriCommands'

const props = defineProps<{ preview: FileDiffPreview }>()

// 換行正規化：磁碟常為 CRLF、app 產生為 LF；不正規化會讓每行都被判定不同（假差異，
// 造成「一直有東西可同步」）。比對前統一成 LF，再去尾端多餘換行。
function norm(s: string): string {
  return s.replace(/\r\n/g, '\n').replace(/\r/g, '\n').replace(/\n+$/, '')
}

// 正規化後切行：空內容須回 [] 而非 ['']（否則空檔/全新增/全刪除會多一條假空白行，Codex #中）。
function toLines(s: string): string[] {
  const n = norm(s)
  return n === '' ? [] : n.split('\n')
}

// 真 line-diff（LCS）：比對「現在 vs 新內容」（換行正規化後），標出實際 +新增／−刪除／ 未變。
// 取代舊版「只 dump 新內容 + 拿行首 - 當刪除」的誤導顯示。
const diffLines = computed<{ t: string; l: string }[]>(() => {
  const next = toLines(props.preview.newContent)
  if (props.preview.isNewFile || props.preview.currentContent == null) {
    return next.map((l) => ({ t: '+', l }))
  }
  const cur = toLines(props.preview.currentContent)
  const m = cur.length, n = next.length
  const dp: number[][] = Array.from({ length: m + 1 }, () => new Array(n + 1).fill(0))
  for (let i = m - 1; i >= 0; i--)
    for (let j = n - 1; j >= 0; j--)
      dp[i][j] = cur[i] === next[j] ? dp[i + 1][j + 1] + 1 : Math.max(dp[i + 1][j], dp[i][j + 1])
  const out: { t: string; l: string }[] = []
  let i = 0, j = 0
  while (i < m && j < n) {
    if (cur[i] === next[j]) { out.push({ t: ' ', l: cur[i] }); i++; j++ }
    else if (dp[i + 1][j] >= dp[i][j + 1]) { out.push({ t: '-', l: cur[i] }); i++ }
    else { out.push({ t: '+', l: next[j] }); j++ }
  }
  while (i < m) { out.push({ t: '-', l: cur[i] }); i++ }
  while (j < n) { out.push({ t: '+', l: next[j] }); j++ }
  return out
})

const noChange = computed(() => !props.preview.isNewFile && diffLines.value.every((d) => d.t === ' '))

function lineStyle(t: string) {
  if (t === '+') return 'color: var(--c-success); background: var(--c-success-soft);'
  if (t === '-') return 'color: var(--c-danger); background: var(--c-danger-soft);'
  return 'color: var(--c-muted);'
}
</script>
