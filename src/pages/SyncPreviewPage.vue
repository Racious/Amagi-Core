<template>
  <div>
    <div class="mb-6">
      <h1 class="text-2xl font-bold mb-1" style="color: #201b34;">同步預覽</h1>
      <p class="text-sm" style="color: #6f6883;">預覽即將同步到 Agent 檔案的內容差異，確認後再執行</p>
    </div>

    <div class="rounded-2xl p-4 border mb-4" style="background: white; border-color: #ded6f5;">
      <label class="text-sm font-bold mb-2 block" style="color: #2e2a3f;">選擇專案</label>
      <div class="flex gap-3">
        <select v-model="selectedId" class="flex-1 rounded-xl border px-3 py-2 text-sm" style="border-color: #ded6f5;">
          <option value="">— 請選擇 —</option>
          <option v-for="p in projectStore.projects" :key="p.id" :value="p.id">{{ p.name }}</option>
        </select>
        <button @click="loadPreview" :disabled="!selectedId || loading"
                class="px-4 py-2 rounded-xl text-sm font-bold text-white disabled:opacity-50"
                style="background: #5037c9;">預覽差異</button>
        <button @click="doSync" :disabled="!selectedId || loading || previews.length === 0"
                class="px-4 py-2 rounded-xl text-sm font-bold text-white disabled:opacity-50"
                style="background: #7c5cff;">執行同步</button>
      </div>
    </div>

    <div v-if="error" class="rounded-2xl p-4 mb-4 border" style="background: #fff0f0; border-color: #efb5b5;">
      <span class="text-sm" style="color: #ab3a3a;">{{ error }}</span>
    </div>

    <div v-if="syncResult" class="rounded-2xl p-4 mb-4 border" style="background: #eefaf4; border-color: #bde8d1;">
      <div class="font-bold text-sm mb-1" style="color: #1d7a51;">✅ 同步完成</div>
      <div v-for="f in syncResult.writtenFiles" :key="f" class="text-xs mt-0.5" style="color: #2e2a3f;">{{ f }}</div>
    </div>

    <div v-if="previews.length === 0 && !loading" class="text-center py-8" style="color: #6f6883;">
      <div class="text-3xl mb-2">📄</div>
      <div class="text-sm">選擇專案並點擊「預覽差異」以查看將寫入的檔案內容。</div>
      <div class="text-xs mt-1">需先在「審核佇列」接受候選項。</div>
    </div>

    <DiffPreview v-for="preview in previews" :key="preview.filePath" :preview="preview" class="mb-4" />
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
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

async function doSync() {
  if (!selectedId.value) return
  loading.value = true
  error.value = null
  try {
    syncResult.value = await api.syncAgentFiles(selectedId.value)
    previews.value = []
  } catch (e: any) {
    error.value = e?.message ?? String(e)
  } finally {
    loading.value = false
  }
}
</script>
