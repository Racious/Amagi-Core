<template>
  <div>
    <div class="mb-6">
      <h1 class="page-title mb-1">記憶庫</h1>
      <p class="page-sub">瀏覽已同步進 vault 的記憶（專案 / 共用 / 全域三層），可把專案記憶升級為共用。</p>
    </div>

    <div v-if="reviewStore.error" class="alert tone-danger mb-4"><span class="text-sm">{{ reviewStore.error }}</span></div>

    <div v-if="reviewStore.loading && memories.length === 0" class="card p-8 text-center text-sm text-muted">
      載入記憶庫…
    </div>

    <div v-else-if="memories.length === 0" class="card card-dashed p-8 text-center">
      <div class="text-4xl mb-3">🧠</div>
      <div class="font-bold mb-1 text-fg">記憶庫尚無已同步記憶</div>
      <div class="text-sm text-muted">
        於「學習 → 審核 → 同步」核可並同步記憶後，會寫入 vault <code>agent/memory</code> 並出現在這裡。
      </div>
    </div>

    <template v-else>
      <!-- 透鏡 + 搜尋 -->
      <div class="flex gap-2 mb-2">
        <select v-model="lens" class="input" style="width: auto; max-width: 50%;">
          <option value="all">全部（{{ memories.length }}）</option>
          <option value="shared">共用（{{ counts.shared }}）</option>
          <option value="global">全域（{{ counts.global }}）</option>
          <option v-for="p in projectsWithMemory" :key="p.id" :value="p.id">
            {{ p.name }}（{{ counts.project[p.id] }}）
          </option>
        </select>
        <input v-model="query" class="input flex-1" type="search" placeholder="搜尋記憶標題或內容" />
      </div>

      <div class="text-xs text-muted mb-3">
        共 {{ memories.length }} 筆 · 專案 {{ counts.projectTotal }} · 共用 {{ counts.shared }} · 全域 {{ counts.global }}
      </div>

      <div class="card p-0 overflow-hidden">
        <button v-for="m in filtered" :key="m.id"
                type="button" class="mem-row" @click="open(m)">
          <span class="text-sm font-bold text-fg truncate" style="max-width: 36%; flex: none;">{{ m.title }}</span>
          <span class="text-xs text-muted truncate flex-1">{{ summary(m.content) }}</span>
          <span class="pill shrink-0" :class="scopeTone(m.syncScope)">{{ scopeLabel(m) }}</span>
          <span class="text-muted shrink-0" aria-hidden="true">›</span>
        </button>
        <div v-if="filtered.length === 0" class="p-4 text-sm text-muted text-center">
          找不到符合「{{ query }}」的記憶。
        </div>
      </div>
    </template>

    <!-- 詳情 / 升級跳窗 -->
    <div v-if="detail" class="modal-overlay" @click.self="close">
      <div class="card modal-card p-0">
        <div class="modal-head">
          <span class="text-sm font-bold text-fg flex-1 truncate">{{ detail.title }}</span>
          <span class="pill shrink-0" :class="scopeTone(detail.syncScope)">{{ scopeLabel(detail) }}</span>
          <button class="btn btn-ghost btn-sm shrink-0" aria-label="關閉" @click="close">✕</button>
        </div>
        <div class="modal-body">
          <div class="text-xs text-muted mb-2">
            分類：{{ detail.category || '—' }} · 來源：{{ scopePath(detail) }}
          </div>
          <pre class="text-xs text-muted whitespace-pre-wrap rounded p-2"
               style="background: var(--c-surface-2);">{{ detail.content }}</pre>
        </div>
        <div class="modal-foot">
          <template v-if="detail.syncScope === 'project'">
            <template v-if="!confirmingPromote">
              <button class="btn btn-primary btn-sm" :disabled="busy" @click="confirmingPromote = true">
                升級為共用
              </button>
            </template>
            <template v-else>
              <span class="text-xs text-muted flex-1 self-center">
                移到 <code>shared/agent/memory</code>，所有專案可見。確認？
              </span>
              <button class="btn btn-primary btn-sm" :disabled="busy" @click="doPromote">
                {{ busy ? '升級中…' : '確認升級' }}
              </button>
              <button class="btn btn-ghost btn-sm" :disabled="busy" @click="confirmingPromote = false">取消</button>
            </template>
          </template>
          <button v-if="!confirmingPromote" class="btn btn-ghost btn-sm" @click="close">關閉</button>
        </div>
      </div>
    </div>

    <div v-if="msg" class="mt-3 text-xs" style="color: var(--c-accent);">{{ msg }}</div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import { api, type ReviewItem, type SyncScope } from '../api/tauriCommands'
import { useReviewStore } from '../stores/reviewStore'
import { useProjectStore } from '../stores/projectStore'

// 讀 store 快取（App 啟動已 fetch），切頁無殘影、零新後端。
const reviewStore = useReviewStore()
const projectStore = useProjectStore()

const lens = ref<string>('all') // 'all' | 'shared' | 'global' | <projectId>
const query = ref('')
const detail = ref<ReviewItem | null>(null)
const confirmingPromote = ref(false)
const busy = ref(false)
const msg = ref('')

// 已同步進 vault 的記憶（記憶庫＝vault 實際內容，故只取 synced）
const memories = computed(() =>
  reviewStore.items.filter(i => i.itemType === 'memory' && i.status === 'synced'),
)

function projectName(id: string): string {
  return projectStore.projects.find(p => p.id === id)?.name ?? id
}

const projectsWithMemory = computed(() => {
  const ids = new Set(memories.value.filter(m => m.syncScope === 'project').map(m => m.projectId))
  return projectStore.projects.filter(p => ids.has(p.id))
})

const counts = computed(() => {
  const c = { shared: 0, global: 0, projectTotal: 0, project: {} as Record<string, number> }
  for (const m of memories.value) {
    if (m.syncScope === 'shared') c.shared++
    else if (m.syncScope === 'global') c.global++
    else if (m.syncScope === 'project') {
      c.projectTotal++
      c.project[m.projectId] = (c.project[m.projectId] ?? 0) + 1
    }
  }
  return c
})

const filtered = computed(() => {
  const q = query.value.trim().toLowerCase()
  return memories.value.filter(m => {
    if (lens.value === 'shared' && m.syncScope !== 'shared') return false
    if (lens.value === 'global' && m.syncScope !== 'global') return false
    if (lens.value !== 'all' && lens.value !== 'shared' && lens.value !== 'global') {
      // 透鏡為某專案 id
      if (!(m.syncScope === 'project' && m.projectId === lens.value)) return false
    }
    if (!q) return true
    return m.title.toLowerCase().includes(q) || m.content.toLowerCase().includes(q)
  })
})

// 一行摘要：跳過 frontmatter，取 description 或正文首行
function summary(content: string): string {
  const lines = content.split('\n')
  let i = 0
  if (lines[0]?.trim() === '---') {
    i = 1
    for (; i < lines.length; i++) {
      const t = lines[i].trim()
      if (t === '---') { i++; break }
      const m = t.match(/^description:\s*(.+)$/)
      if (m) return m[1].trim().replace(/^["']|["']$/g, '')
    }
  }
  for (; i < lines.length; i++) {
    const t = lines[i].trim()
    if (!t || t.startsWith('#') || t.startsWith('---')) continue
    return t.replace(/^[-*]\s*/, '')
  }
  return content.trim().slice(0, 80)
}

function scopeLabel(m: ReviewItem): string {
  if (m.syncScope === 'shared') return '共用'
  if (m.syncScope === 'global') return '全域'
  return projectName(m.projectId)
}
function scopeTone(scope: SyncScope): string {
  return scope === 'project' ? 'tone-muted' : 'tone-accent'
}
function scopePath(m: ReviewItem): string {
  if (m.syncScope === 'shared') return 'shared/agent/memory'
  if (m.syncScope === 'global') return 'general/agent/memory'
  return `projects/<${projectName(m.projectId)}>/agent/memory`
}

function open(m: ReviewItem) {
  detail.value = m
  confirmingPromote.value = false
}
function close() {
  detail.value = null
  confirmingPromote.value = false
}

async function doPromote() {
  if (!detail.value) return
  busy.value = true
  msg.value = ''
  try {
    await api.promoteMemory(detail.value.id)
    msg.value = `已將「${detail.value.title}」升級為共用記憶。`
    close()
    await reviewStore.fetchItems()
  } catch (e: any) {
    msg.value = `升級失敗：${e?.message ?? e}`
  } finally {
    busy.value = false
  }
}
</script>

<style scoped>
.mem-row {
  display: flex;
  align-items: center;
  gap: 10px;
  width: 100%;
  text-align: left;
  padding: 11px 14px;
  border: none;
  border-bottom: 1px solid var(--c-border);
  background: none;
  cursor: pointer;
}
.mem-row:last-child { border-bottom: none; }
.mem-row:hover { background: var(--c-surface-2); }

.modal-overlay {
  position: fixed;
  inset: 0;
  z-index: 50;
  background: rgba(0, 0, 0, 0.5);
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 1rem;
}
.modal-card {
  width: 100%;
  max-width: 560px;
  max-height: 80vh;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}
.modal-head {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 12px 16px;
  border-bottom: 1px solid var(--c-border);
}
.modal-body {
  padding: 14px 16px;
  overflow: auto;
}
.modal-body pre { max-height: 50vh; overflow: auto; }
.modal-foot {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  padding: 12px 16px;
  border-top: 1px solid var(--c-border);
}
</style>
