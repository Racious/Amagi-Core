<template>
  <div>
    <div class="mb-6">
      <h1 class="page-title mb-1">差異匯出</h1>
      <p class="page-sub">列出專案異動檔，勾選後產生可複製的 diff 文字（相對上次 commit）</p>
    </div>

    <!-- 專案選擇 + 動作 -->
    <div class="card p-4 mb-4">
      <label class="text-sm font-medium mb-2 block text-fg">選擇專案</label>
      <div class="flex gap-3">
        <select v-model="selectedId" class="select flex-1" @change="reset">
          <option value="">— 請選擇 —</option>
          <option v-for="p in projectStore.projects" :key="p.id" :value="p.id">{{ p.name }}</option>
        </select>
        <button @click="scan" :disabled="!selectedId || loading" class="btn btn-ghost">掃描異動</button>
        <button @click="generate" :disabled="!selectedId || loading || selectedPaths.length === 0" class="btn btn-primary">
          產生 Diff（{{ selectedPaths.length }}）
        </button>
      </div>
    </div>

    <div v-if="error" class="alert tone-danger mb-4">
      <span class="text-sm" style="color: var(--c-danger);">{{ error }}</span>
    </div>

    <!-- 異動檔清單（兩組） -->
    <div v-if="scanned && files.length" class="grid grid-cols-2 gap-4 mb-4">
      <div v-for="grp in groups" :key="grp.key" class="card p-4">
        <div class="flex items-center justify-between mb-3">
          <span class="font-semibold text-sm text-fg">{{ grp.title }}（{{ grp.files.length }}）</span>
          <button v-if="grp.files.length" class="btn btn-ghost btn-sm"
                  @click="toggleGroup(grp.files, !allChecked(grp.files))">
            {{ allChecked(grp.files) ? '全不選' : '全選' }}
          </button>
        </div>
        <div v-if="!grp.files.length" class="text-xs text-subtle py-2">（無）</div>
        <div v-else class="space-y-1">
          <label v-for="f in grp.files" :key="f.path" class="flex items-center gap-2 py-1 cursor-pointer">
            <input type="checkbox" v-model="checked[f.path]" />
            <span class="pill" :class="statusTone(f.status)">{{ statusLabel(f.status) }}</span>
            <span class="text-xs text-fg truncate flex-1" :title="f.path">{{ f.path }}</span>
          </label>
        </div>
      </div>
    </div>

    <div v-else-if="scanned && !files.length" class="card card-dashed p-8 text-center mb-4">
      <div class="text-3xl mb-2 opacity-70">✨</div>
      <div class="text-sm text-fg">這個專案目前沒有未提交的異動。</div>
    </div>

    <div v-else-if="!scanned" class="card card-dashed p-8 text-center mb-4">
      <div class="text-3xl mb-2 opacity-70">🧾</div>
      <div class="text-sm text-fg">選擇專案並點「掃描異動」以列出變更檔案。</div>
    </div>

    <!-- 略過 / 截斷提示 -->
    <div v-if="bundle && bundle.skipped.length" class="alert tone-warning mb-4">
      <div class="font-semibold text-sm mb-1">略過 {{ bundle.skipped.length }} 個檔案</div>
      <div v-for="s in bundle.skipped" :key="s" class="text-xs mt-0.5" style="color: var(--c-warning);">{{ s }}</div>
    </div>
    <div v-if="bundle && bundle.truncated" class="alert tone-warning mb-4">
      <span class="text-sm" style="color: var(--c-warning);">⚠️ 內容過長已截斷，建議減少勾選的檔案數。</span>
    </div>

    <!-- 兩個結果框 -->
    <div v-if="bundle" class="space-y-4">
      <div class="card p-4">
        <div class="flex items-center justify-between mb-2">
          <span class="font-semibold text-sm text-fg">異動（框 1）</span>
          <button class="btn btn-ghost btn-sm" :disabled="!bundle.editedPatch" @click="copy(bundle.editedPatch, 'edited')">
            {{ copied === 'edited' ? '已複製 ✓' : '複製' }}
          </button>
        </div>
        <textarea v-if="bundle.editedPatch" class="input font-mono" readonly :rows="14"
                  :value="bundle.editedPatch" style="white-space: pre; overflow-x: auto;"></textarea>
        <div v-else class="text-xs text-subtle py-2">沒有勾選任何修改／改名檔。</div>
      </div>

      <div class="card p-4">
        <div class="flex items-center justify-between mb-2">
          <span class="font-semibold text-sm text-fg">新增／刪除（框 2）</span>
          <button class="btn btn-ghost btn-sm" :disabled="!bundle.addedDeletedPatch" @click="copy(bundle.addedDeletedPatch, 'addel')">
            {{ copied === 'addel' ? '已複製 ✓' : '複製' }}
          </button>
        </div>
        <textarea v-if="bundle.addedDeletedPatch" class="input font-mono" readonly :rows="14"
                  :value="bundle.addedDeletedPatch" style="white-space: pre; overflow-x: auto;"></textarea>
        <div v-else class="text-xs text-subtle py-2">沒有勾選任何新增／刪除檔。</div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { useProjectStore } from '../stores/projectStore'
import { api, type ChangedFile, type ChangedStatus, type DiffBundle } from '../api/tauriCommands'

const projectStore = useProjectStore()
const selectedId = ref('')
const loading = ref(false)
const error = ref<string | null>(null)
const scanned = ref(false)
const files = ref<ChangedFile[]>([])
const checked = ref<Record<string, boolean>>({})
const bundle = ref<DiffBundle | null>(null)
const copied = ref<string | null>(null)

const editedFiles = computed(() => files.value.filter((f) => f.group === 'edited'))
const addedDeletedFiles = computed(() => files.value.filter((f) => f.group === 'addedDeleted'))
const groups = computed(() => [
  { key: 'edited', title: '異動（修改／改名）', files: editedFiles.value },
  { key: 'addedDeleted', title: '新增／刪除', files: addedDeletedFiles.value },
])
const selectedPaths = computed(() => files.value.filter((f) => checked.value[f.path]).map((f) => f.path))

const statusMeta: Record<ChangedStatus, { label: string; tone: string }> = {
  modified: { label: '改', tone: 'tone-warning' },
  renamed: { label: '改名', tone: 'tone-info' },
  added: { label: '增', tone: 'tone-success' },
  untracked: { label: '新', tone: 'tone-success' },
  deleted: { label: '刪', tone: 'tone-danger' },
}
const statusLabel = (s: ChangedStatus) => statusMeta[s]?.label ?? s
const statusTone = (s: ChangedStatus) => statusMeta[s]?.tone ?? 'tone-muted'

function reset() {
  scanned.value = false
  files.value = []
  checked.value = {}
  bundle.value = null
  error.value = null
}

function allChecked(list: ChangedFile[]) {
  return list.length > 0 && list.every((f) => checked.value[f.path])
}

function toggleGroup(list: ChangedFile[], value: boolean) {
  const next = { ...checked.value }
  list.forEach((f) => (next[f.path] = value))
  checked.value = next
}

async function scan() {
  if (!selectedId.value) return
  loading.value = true
  error.value = null
  bundle.value = null
  try {
    const result = await api.listChangedFiles(selectedId.value)
    files.value = result
    const map: Record<string, boolean> = {}
    result.forEach((f) => (map[f.path] = true)) // 預設全選
    checked.value = map
    scanned.value = true
  } catch (e: any) {
    error.value = e?.message ?? String(e)
  } finally {
    loading.value = false
  }
}

async function generate() {
  if (!selectedId.value || selectedPaths.value.length === 0) return
  loading.value = true
  error.value = null
  try {
    bundle.value = await api.generateDiffText(selectedId.value, selectedPaths.value)
  } catch (e: any) {
    error.value = e?.message ?? String(e)
  } finally {
    loading.value = false
  }
}

async function copy(text: string, key: string) {
  if (!text) return
  try {
    await navigator.clipboard.writeText(text)
    copied.value = key
    setTimeout(() => {
      if (copied.value === key) copied.value = null
    }, 1500)
  } catch {
    /* 忽略剪貼簿失敗 */
  }
}
</script>
