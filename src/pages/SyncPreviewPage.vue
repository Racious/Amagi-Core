<template>
  <div>
    <div class="mb-6">
      <h1 class="page-title mb-1">同步預覽</h1>
      <p class="page-sub">預覽即將同步到 Agent 檔案的內容差異，確認後再執行</p>
    </div>

    <div class="card p-4 mb-4">
      <label class="text-sm font-medium mb-2 block text-fg">選擇專案</label>
      <div class="flex gap-3">
        <select v-model="selectedId" class="select flex-1">
          <option value="">— 請選擇 —</option>
          <option v-for="p in projectStore.projects" :key="p.id" :value="p.id">{{ p.name }}</option>
        </select>
        <button @click="loadPreview" :disabled="!selectedId || loading" class="btn btn-ghost">預覽差異</button>
        <button @click="doSync()" :disabled="!selectedId || loading || previews.length === 0" class="btn btn-primary">執行同步</button>
      </div>
    </div>

    <div v-if="error" class="alert tone-danger mb-4">
      <span class="text-sm" style="color: var(--c-danger);">{{ error }}</span>
    </div>

    <!-- 衝突擋下 -->
    <div v-if="syncResult && syncResult.blockedConflicts.length" class="alert tone-warning mb-4">
      <div class="font-semibold text-sm mb-2">⛔ 偵測到衝突，同步已擋下（尚未寫入任何檔案）</div>
      <div v-for="c in syncResult.blockedConflicts" :key="c.itemId" class="card p-2.5 mb-2">
        <div class="text-sm font-medium text-fg">{{ c.itemTitle }}</div>
        <div v-for="(r, i) in c.reasons" :key="i" class="text-xs mt-0.5" style="color: var(--c-warning);">⚠️ {{ r }}</div>
      </div>
      <div class="flex gap-2 mt-2">
        <RouterLink to="/review" class="btn btn-primary btn-sm">前往審核去修</RouterLink>
        <button @click="doSync(true)" :disabled="loading" class="btn btn-danger btn-sm">我知道，仍要同步</button>
      </div>
    </div>

    <!-- 同步完成 -->
    <div v-if="syncResult && !syncResult.blockedConflicts.length && syncResult.writtenFiles.length" class="alert tone-success mb-4">
      <div class="font-semibold text-sm mb-1">✅ 同步完成</div>
      <div v-for="f in syncResult.writtenFiles" :key="f" class="text-xs mt-0.5 text-muted">{{ f }}</div>
    </div>

    <div v-if="previews.length === 0 && !loading" class="card card-dashed p-8 text-center">
      <div class="text-3xl mb-2 opacity-70">📄</div>
      <div class="text-sm text-fg">選擇專案並點擊「預覽差異」以查看將寫入的檔案內容。</div>
      <div class="text-xs mt-1 text-muted">需先在「審核佇列」接受候選項。</div>
    </div>

    <DiffPreview v-for="preview in previews" :key="preview.filePath" :preview="preview" class="mb-4" />
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { RouterLink } from 'vue-router'
import { useProjectStore } from '../stores/projectStore'
import { api, type FileDiffPreview, type SyncResult } from '../api/tauriCommands'
import DiffPreview from '../components/DiffPreview.vue'

const projectStore = useProjectStore()
const selectedId = ref('')
const loading = ref(false)
const error = ref<string | null>(null)
const previews = ref<FileDiffPreview[]>([])
const syncResult = ref<SyncResult | null>(null)

async function loadPreview() {
  if (!selectedId.value) return
  loading.value = true
  error.value = null
  syncResult.value = null
  try {
    previews.value = await api.previewSyncDiff(selectedId.value)
  } catch (e: any) {
    error.value = e?.message ?? String(e)
  } finally {
    loading.value = false
  }
}

async function doSync(force = false) {
  if (!selectedId.value) return
  loading.value = true
  error.value = null
  try {
    const result = await api.syncAgentFiles(selectedId.value, force)
    syncResult.value = result
    if (result.blockedConflicts.length === 0) {
      previews.value = []
    }
  } catch (e: any) {
    error.value = e?.message ?? String(e)
  } finally {
    loading.value = false
  }
}
</script>
