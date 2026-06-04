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

    <div class="flex gap-3 mb-4">
      <button @click="doScan" :disabled="!selectedId || loading"
              class="btn btn-ghost disabled:opacity-50">掃描 Git 變更</button>
      <button @click="doLearn" :disabled="!selectedId || loading"
              class="btn btn-primary disabled:opacity-50">產生候選記憶</button>
    </div>

    <div v-if="error" class="alert tone-danger mb-4">
      <span class="text-sm">{{ error }}</span>
    </div>

    <div v-if="learnResult" class="alert tone-success mb-4">
      <div class="font-bold text-sm">
        產生 {{ learnResult.candidatesGenerated }} 個候選項
        <span v-if="learnResult.blockedCount > 0">（{{ learnResult.blockedCount }} 個已封鎖）</span>
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
import { ref } from 'vue'
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

async function doScan() {
  if (!selectedId.value) return
  loading.value = true
  error.value = null
  try {
    scanResult.value = await api.scanProject(selectedId.value)
  } catch (e: any) {
    error.value = e?.message ?? String(e)
  } finally {
    loading.value = false
  }
}

async function doLearn() {
  if (!selectedId.value) return
  loading.value = true
  error.value = null
  try {
    learnResult.value = await api.learnFromProject(selectedId.value)
    await reviewStore.fetchItems()
  } catch (e: any) {
    error.value = e?.message ?? String(e)
  } finally {
    loading.value = false
  }
}
</script>
