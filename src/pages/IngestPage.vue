<template>
  <div>
    <div class="mb-6">
      <h1 class="page-title mb-1">知識匯入</h1>
      <p class="page-sub">把討論結論、文件或筆記匯入 vault 知識庫（經審核後寫入）</p>
    </div>

    <div v-if="error" class="alert tone-danger mb-4"><span class="text-sm">{{ error }}</span></div>
    <div v-if="successMsg" class="alert tone-success mb-4"><span class="text-sm">{{ successMsg }}</span></div>

    <!-- 匯入表單 -->
    <div class="card p-5 mb-6">
      <div class="grid grid-cols-2 gap-3 mb-3">
        <label class="block">
          <span class="text-xs text-muted">專案</span>
          <select v-model="form.projectId" :class="inputClass" :style="inputStyle" @change="onProjectChange">
            <option value="">（不指定 / general·shared）</option>
            <option v-for="p in projects" :key="p.id" :value="p.id">{{ p.name }}</option>
          </select>
        </label>
        <label class="block">
          <span class="text-xs text-muted">歸層</span>
          <select v-model="form.layer" :class="inputClass" :style="inputStyle">
            <option v-for="l in layerOptions" :key="l.value" :value="l.value">{{ l.label }}</option>
          </select>
        </label>
        <label class="block">
          <span class="text-xs text-muted">型別</span>
          <select v-model="form.pageType" :class="inputClass" :style="inputStyle">
            <option v-for="t in pageTypes" :key="t" :value="t">{{ t }}</option>
          </select>
        </label>
        <label class="block">
          <span class="text-xs text-muted">標題</span>
          <input v-model="form.title" :class="inputClass" :style="inputStyle" placeholder="頁面標題" />
        </label>
      </div>
      <label class="block mb-3">
        <span class="text-xs text-muted">內容（Markdown）</span>
        <textarea v-model="form.content" rows="8" :class="inputClass + ' font-mono'" :style="inputStyle"
                  placeholder="貼上內容…"></textarea>
      </label>
      <div class="flex gap-2">
        <button class="btn btn-primary btn-sm" :disabled="busy || !form.title.trim()" @click="createDraft">
          {{ busy ? '建立中…' : '建立草稿' }}
        </button>
        <button class="btn btn-ghost btn-sm" :disabled="busy" @click="importFromFile">
          從檔案匯入…
        </button>
      </div>
    </div>

    <!-- 待審核 wiki 草稿 -->
    <div class="flex items-center justify-between mb-3">
      <div class="text-xs font-bold uppercase tracking-wider text-muted">
        待審核知識草稿（{{ pendingWiki.length }}）
      </div>
      <button class="btn btn-ghost btn-sm" :disabled="busy" @click="scanClips">
        掃描 sources/clips
      </button>
    </div>
    <div v-if="pendingWiki.length === 0" class="card card-dashed p-6 text-center text-sm text-muted mb-6">
      尚無待審核的知識草稿。
    </div>
    <div v-else class="space-y-3 mb-6">
      <div v-for="item in pendingWiki" :key="item.id" class="card p-4">
        <div class="flex items-center gap-2 mb-2">
          <span class="pill tone-accent">{{ item.category }}</span>
          <span class="text-xs text-muted">{{ item.syncTargets[0] }}</span>
          <span class="font-bold text-fg flex-1 truncate">{{ item.title }}</span>
        </div>
        <pre class="text-xs text-muted whitespace-pre-wrap mb-3 max-h-32 overflow-auto">{{ item.content }}</pre>
        <div class="flex gap-2">
          <button class="btn btn-primary btn-sm" :disabled="busy" @click="acceptWrite(item.id)">
            接受並寫入 vault
          </button>
          <button class="btn btn-danger btn-sm" :disabled="busy" @click="ignoreItem(item.id)">忽略</button>
        </div>
      </div>
    </div>

    <!-- 已寫入 -->
    <div v-if="syncedWiki.length > 0">
      <div class="mb-3 text-xs font-bold uppercase tracking-wider text-muted">
        已寫入 vault（{{ syncedWiki.length }}）
      </div>
      <div class="space-y-2">
        <div v-for="item in syncedWiki" :key="item.id" class="card p-3 flex items-center gap-3 opacity-70">
          <span>✅</span>
          <span class="text-sm flex-1 truncate text-muted">{{ item.title }}</span>
          <span class="pill tone-success">{{ item.category }}</span>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { open } from '@tauri-apps/plugin-dialog'
import { api, type ProjectInfo, type ReviewItem } from '../api/tauriCommands'

const inputClass = 'w-full mt-1 px-2.5 py-1.5 rounded-md border border-border text-sm text-fg'
const inputStyle = 'background: var(--c-surface-2);'

const projects = ref<ProjectInfo[]>([])
const items = ref<ReviewItem[]>([])
const busy = ref(false)
const error = ref<string | null>(null)
const successMsg = ref<string | null>(null)

// 對齊 amagi-conventions §5 知識桶型別（adr/spec/business/concept/troubleshooting → knowledge/）。
// reports 類（review/test-report）由 Codex/測試流程自動寫入、handoff 由天城寫各專案活頁 handoff.md，
// 皆不從「知識匯入」入口手工產出（老爺裁定，2026-06-28）。
const pageTypes = ['adr', 'spec', 'business', 'concept', 'troubleshooting']

const form = ref({
  projectId: '',
  layer: 'general',
  pageType: 'concept',
  title: '',
  content: '',
})

const selectedProject = computed(() => projects.value.find(p => p.id === form.value.projectId))

const layerOptions = computed(() => {
  const opts = [
    { value: 'general', label: 'general（個人通用）' },
    { value: 'shared', label: 'shared（跨專案共用）' },
  ]
  const vf = selectedProject.value?.vaultFolder
  if (vf) opts.unshift({ value: vf, label: `${selectedProject.value?.name}（專案知識）` })
  return opts
})

const wikiItems = computed(() => items.value.filter(i => i.itemType === 'wiki'))
const pendingWiki = computed(() => wikiItems.value.filter(i => i.status === 'pending'))
const syncedWiki = computed(() => wikiItems.value.filter(i => i.status === 'synced'))

function onProjectChange() {
  form.value.layer = selectedProject.value?.vaultFolder ?? 'general'
}

function clearMsg() { error.value = null; successMsg.value = null }

async function refresh() {
  try {
    items.value = await api.listReviewItems()
  } catch (e: any) { error.value = e?.message ?? String(e) }
}

async function createDraft() {
  clearMsg()
  busy.value = true
  try {
    await api.ingestWikiPage({
      projectId: form.value.projectId,
      layer: form.value.layer,
      pageType: form.value.pageType,
      title: form.value.title,
      content: form.value.content,
    })
    successMsg.value = `已建立草稿「${form.value.title}」，待審核。`
    form.value.title = ''
    form.value.content = ''
    await refresh()
  } catch (e: any) { error.value = e?.message ?? String(e) }
  finally { busy.value = false }
}

async function importFromFile() {
  clearMsg()
  const picked = await open({
    directory: false,
    multiple: false,
    title: '選擇要匯入的檔案',
    filters: [{ name: 'Markdown / 文字', extensions: ['md', 'markdown', 'txt'] }],
  })
  if (typeof picked !== 'string') return
  busy.value = true
  try {
    const item = await api.ingestWikiFromFile({
      projectId: form.value.projectId,
      layer: form.value.layer,
      pageType: form.value.pageType,
      filePath: picked,
    })
    successMsg.value = `已從檔案建立草稿「${item.title}」，原始來源已存入 sources/。`
    await refresh()
  } catch (e: any) { error.value = e?.message ?? String(e) }
  finally { busy.value = false }
}

async function scanClips() {
  clearMsg()
  busy.value = true
  try {
    const n = await api.scanVaultClips()
    successMsg.value = n > 0 ? `從 sources/clips 產生 ${n} 筆新候選。` : '沒有新的剪藏可匯入。'
    await refresh()
  } catch (e: any) { error.value = e?.message ?? String(e) }
  finally { busy.value = false }
}

async function acceptWrite(id: string) {
  clearMsg()
  busy.value = true
  try {
    const r = await api.writeWikiPages([id])
    if (r.written.length > 0) successMsg.value = `已寫入 vault：${r.written.join('、')}`
    else if (r.skipped.length > 0) error.value = `目標已存在，已略過：${r.skipped.join('、')}`
    await refresh()
  } catch (e: any) { error.value = e?.message ?? String(e) }
  finally { busy.value = false }
}

async function ignoreItem(id: string) {
  clearMsg()
  busy.value = true
  try {
    await api.ignoreReviewItems([id])
    await refresh()
  } catch (e: any) { error.value = e?.message ?? String(e) }
  finally { busy.value = false }
}

onMounted(async () => {
  try { projects.value = await api.listProjects() } catch { /* 非 Tauri 環境 */ }
  await refresh()
})
</script>
