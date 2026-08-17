<template>
  <div>
    <div class="mb-6">
      <h1 class="page-title mb-1">學習變更</h1>
      <p class="page-sub">掃描選取專案的 git diff，產生候選記憶與技能</p>
    </div>

    <!-- 選擇專案 -->
    <div class="card p-4 mb-4">
      <label class="text-sm font-bold mb-2 block text-fg">選擇專案</label>
      <select v-model="selectedId" class="select w-full">
        <option value="">— 請選擇 —</option>
        <option v-for="p in projectStore.projects" :key="p.id" :value="p.id">{{ p.name }}</option>
      </select>
    </div>

    <!-- 單一入口（2026-07-03 合併）：learn 後端內部本就自行掃 git，獨立「掃描」鈕為冗餘；
         變更概要改為學習後自動附上（見下方 Diff 摘要卡） -->
    <div class="flex gap-3 mb-4">
      <button @click="doLearn" :disabled="!selectedId || loading"
              class="btn btn-primary disabled:opacity-50">{{ loading ? '學習中…' : '開始學習' }}</button>
    </div>

    <div v-if="error" class="alert tone-danger mb-4">
      <span class="text-sm">{{ error }}</span>
    </div>

    <div v-if="learnResult" class="alert tone-success mb-4">
      <div class="font-bold text-sm">
        產生 {{ learnResult.candidatesGenerated }} 個候選項
        <span v-if="agentDraftNote">（{{ agentDraftNote }}）</span>
      </div>
      <div v-if="learnResult.blockedCount > 0" class="text-sm mt-1" style="color: var(--c-danger)">
        ⛔ 偵測到疑似敏感內容，已建立 {{ learnResult.blockedCount }} 筆封鎖項——請至審核頁檢視命中規則
      </div>
      <!-- N3：被安全過濾擋下的 pending 投遞檔。原先只印到 stderr，AI 以為已投遞、
           老爺以為沒有候選 → 該筆知識無聲消失。此處只列檔名與命中規則，不顯示命中值。 -->
      <div v-if="learnResult.pendingSkipped?.length" class="text-sm mt-1" style="color: var(--c-danger)">
        ⚠ 以下 {{ learnResult.pendingSkipped.length }} 個 Agent 投遞檔含疑似敏感內容，<strong>未</strong>加入候選：
        <ul class="mt-1 ml-4" style="list-style: disc">
          <li v-for="s in learnResult.pendingSkipped" :key="s.fileName">
            <code>{{ s.fileName }}</code>（{{ s.kind }}）— 命中：{{ s.labels.join('、') }}
          </li>
        </ul>
        <div class="text-xs mt-1">
          請修正 <code>.amagi/pending/</code> 內該檔（移除機密、改用環境變數）後再次學習；檔案仍留在原處。
        </div>
      </div>
      <RouterLink to="/review" class="inline-block mt-2 text-xs font-bold" style="color: var(--c-accent)">前往審核 →</RouterLink>
    </div>

    <!-- Diff 摘要 -->
    <div v-if="scanResult" class="card overflow-hidden">
      <div class="p-4 border-b border-border bg-surface-2">
        <div class="font-bold text-sm text-fg">
          分支：<span style="color: var(--c-accent)">{{ scanResult.branch }}</span>
        </div>
      </div>

      <div class="p-4 border-b border-border">
        <div class="text-xs font-bold mb-2 text-muted">變更概要</div>
        <pre class="text-xs whitespace-pre-wrap bg-surface-2 text-fg font-mono">{{ scanResult.diffStat || '（無變更）' }}</pre>
      </div>

      <div class="p-4">
        <div class="text-xs font-bold mb-2 text-muted">最近提交</div>
        <pre class="text-xs bg-surface-2 text-muted font-mono">{{ scanResult.recentLog || '（無記錄）' }}</pre>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import { RouterLink } from 'vue-router'
import { useProjectStore } from '../stores/projectStore'
import { useReviewStore } from '../stores/reviewStore'
import { api, type ScanResult, type LearnResult } from '../api/tauriCommands'

const projectStore = useProjectStore()
const reviewStore = useReviewStore()
const selectedId = ref('')
const loading = ref(false)
const error = ref<string | null>(null)
const scanResult = ref<ScanResult | null>(null)
const learnResult = ref<LearnResult | null>(null)

/** Agent 投遞草稿數的附註：技能／記憶各自有數才列出，兩者皆 0 則不顯示括號 */
const agentDraftNote = computed(() => {
  const r = learnResult.value
  if (!r) return ''
  const parts: string[] = []
  if (r.pendingSkillCount > 0) parts.push(`${r.pendingSkillCount} 個 Agent 技能草稿`)
  if (r.pendingMemoryCount > 0) parts.push(`${r.pendingMemoryCount} 個 Agent 記憶草稿`)
  return parts.length ? `含 ${parts.join('、')}` : ''
})

async function doLearn() {
  if (!selectedId.value) return
  loading.value = true
  error.value = null
  try {
    learnResult.value = await api.learnFromProject(selectedId.value)
    await reviewStore.fetchItems()
    // 變更概要（分支/diff 概要/最近提交）學習後自動附上——純顯示資訊，失敗不影響學習結果
    try {
      scanResult.value = await api.scanProject(selectedId.value)
    } catch { /* 概要卡缺席即可，不蓋掉學習成功訊息 */ }
  } catch (e: any) {
    error.value = e?.message ?? String(e)
  } finally {
    loading.value = false
  }
}
</script>
