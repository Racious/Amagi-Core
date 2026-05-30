<template>
  <div>
    <div class="mb-6">
      <h1 class="text-2xl font-bold mb-1" style="color: #201b34;">學習變更</h1>
      <p class="text-sm" style="color: #6f6883;">掃描選取專案的 git diff，產生候選記憶與技能</p>
    </div>

    <!-- 選擇專案 -->
    <div class="rounded-2xl p-4 border mb-4" style="background: white; border-color: #ded6f5;">
      <label class="text-sm font-bold mb-2 block" style="color: #2e2a3f;">選擇專案</label>
      <select v-model="selectedId"
              class="w-full rounded-xl border px-3 py-2 text-sm"
              style="border-color: #ded6f5; color: #2e2a3f;">
        <option value="">— 請選擇 —</option>
        <option v-for="p in projectStore.projects" :key="p.id" :value="p.id">{{ p.name }}</option>
      </select>
    </div>

    <div class="flex gap-3 mb-4">
      <button @click="doScan" :disabled="!selectedId || loading"
              class="px-4 py-2 rounded-xl text-sm font-bold text-white disabled:opacity-50"
              style="background: #5037c9;">掃描 Git 變更</button>
      <button @click="doLearn" :disabled="!selectedId || loading"
              class="px-4 py-2 rounded-xl text-sm font-bold text-white disabled:opacity-50"
              style="background: #7c5cff;">產生候選記憶</button>
    </div>

    <div v-if="error" class="rounded-2xl p-4 mb-4 border" style="background: #fff0f0; border-color: #efb5b5;">
      <span class="text-sm" style="color: #ab3a3a;">{{ error }}</span>
    </div>

    <div v-if="learnResult" class="rounded-2xl p-4 mb-4 border" style="background: #eefaf4; border-color: #bde8d1;">
      <div class="font-bold text-sm" style="color: #1d7a51;">
        產生 {{ learnResult.candidatesGenerated }} 個候選項
        <span v-if="learnResult.blockedCount > 0">（{{ learnResult.blockedCount }} 個已封鎖）</span>
      </div>
      <RouterLink to="/review" class="inline-block mt-2 text-xs font-bold" style="color: #5037c9;">前往審核 →</RouterLink>
    </div>

    <!-- Diff 摘要 -->
    <div v-if="scanResult" class="rounded-2xl border overflow-hidden" style="border-color: #ded6f5;">
      <div class="p-4 border-b" style="border-color: #ded6f5; background: #f9f7ff;">
        <div class="font-bold text-sm" style="color: #2e2a3f;">
          分支：<span style="color: #7c5cff;">{{ scanResult.branch }}</span>
        </div>
      </div>

      <div class="p-4 border-b" style="border-color: #ded6f5;">
        <div class="text-xs font-bold mb-2" style="color: #6f6883;">變更概要</div>
        <pre class="text-xs whitespace-pre-wrap" style="color: #2e2a3f; font-family: 'Cascadia Code', Consolas, monospace;">{{ scanResult.diffStat || '（無變更）' }}</pre>
      </div>

      <div class="p-4">
        <div class="text-xs font-bold mb-2" style="color: #6f6883;">最近提交</div>
        <pre class="text-xs" style="color: #6f6883; font-family: 'Cascadia Code', Consolas, monospace;">{{ scanResult.recentLog || '（無記錄）' }}</pre>
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
