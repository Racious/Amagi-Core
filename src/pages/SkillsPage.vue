<template>
  <div>
    <div class="mb-6">
      <h1 class="page-title mb-1">技能管理</h1>
      <p class="page-sub">vault <code>_skills</code> 技能庫——瀏覽查詢、看細節，或分發到全域/專案。</p>
    </div>

    <!-- 頁籤 -->
    <div class="tabs mb-4">
      <button type="button" class="tab" :class="{ active: tab === 'library' }" @click="tab = 'library'">技能庫</button>
      <button type="button" class="tab" :class="{ active: tab === 'distribute' }" @click="tab = 'distribute'">技能分發</button>
    </div>

    <div v-if="loadErr" class="alert tone-danger mb-4"><span class="text-sm">{{ loadErr }}</span></div>

    <!-- ── Tab 1：技能庫（精簡條列，點列開詳情跳窗）── -->
    <div v-show="tab === 'library'">
      <div v-if="loading" class="card p-8 text-center text-sm text-muted">載入技能庫…</div>
      <div v-else-if="library.length === 0" class="card card-dashed p-8 text-center">
        <div class="text-4xl mb-3">⚡</div>
        <div class="font-bold mb-1 text-fg">技能庫尚無技能</div>
        <div class="text-sm text-muted">
          在 vault <code>_skills/</code> 放入技能（或於審核佇列接受、收編散落技能）後會出現在這裡。
        </div>
      </div>
      <template v-else>
        <div class="flex gap-2 mb-3">
          <input v-model="query" class="input flex-1" type="search" placeholder="搜尋技能名稱或內容" />
          <select v-model="distFilter" class="input" style="width: auto;">
            <option value="all">全部</option>
            <option value="distributed">已分發（全域）</option>
            <option value="undistributed">未分發</option>
          </select>
        </div>

        <div class="card p-0 overflow-hidden">
          <button v-for="s in filteredLibrary" :key="s.slug"
                  type="button" class="skill-row" @click="detail = s">
            <span class="skill-name text-sm font-bold text-fg truncate">{{ s.name }}</span>
            <span class="text-xs text-muted truncate flex-1">{{ summary(s.content) }}</span>
            <span class="pill shrink-0" :class="s.distributedGlobal ? 'tone-accent' : 'tone-muted'">
              {{ s.distributedGlobal ? '全域' : '未分發' }}
            </span>
            <span class="text-muted shrink-0" aria-hidden="true">›</span>
          </button>
          <div v-if="filteredLibrary.length === 0" class="p-4 text-sm text-muted text-center">
            找不到符合「{{ query }}」的技能。
          </div>
        </div>
      </template>
    </div>

    <!-- ── Tab 2：技能分發（透鏡式：選目標 → 開關 → 套用前列差異）── -->
    <div v-show="tab === 'distribute'" class="card p-5">
      <div class="flex items-center justify-between gap-3 mb-1">
        <div class="font-semibold text-sm text-fg">技能分發</div>
        <div class="flex items-center gap-2">
          <span v-if="pendingCount > 0" class="pill tone-accent">{{ pendingCount }} 項待套用</span>
          <button v-if="pendingCount > 0" class="btn btn-ghost btn-sm" @click="resetDesired">還原</button>
        </div>
      </div>
      <p class="text-xs text-muted mb-3">
        選一個<b>目標</b>當視角：上方是已分發到該目標的技能、下方是未分發。切換開關後按「套用變更」會先列出
        <b style="color: var(--c-accent);">＋新增</b> /
        <b style="color: var(--c-danger, #c0392b);">−移除</b> 差異，確認才執行。
        <b>全域</b>＝本機所有專案共用（含日後新增的專案）。
      </p>

      <div v-if="loading" class="text-sm text-muted">載入中…</div>
      <div v-else-if="library.length === 0" class="text-sm text-muted">
        vault <code>_skills/</code> 尚無技能（請先設定 vault 路徑並在技能庫放入技能）。
      </div>

      <template v-else>
        <!-- 目標透鏡 + 技能搜尋 -->
        <div class="flex gap-2 mb-2">
          <select v-model="lens" class="input" style="width: auto; max-width: 50%;">
            <option value="global">全域（所有專案）</option>
            <option v-for="p in projects" :key="p.path" :value="p.path" :disabled="!p.pathExists">
              {{ p.name }}{{ p.pathExists ? '' : '（目錄不存在）' }}
            </option>
          </select>
          <input v-model="distQuery" class="input flex-1" type="search" placeholder="搜尋技能名稱或 slug" />
        </div>

        <div class="text-xs text-muted mb-3">
          視角：<b class="text-fg">{{ lensLabel }}</b>
          <span v-if="lensIsProject"> · 全域技能標
            <span class="pill tone-accent">全域</span> 並鎖定（要改請切到「全域」視角）</span>
        </div>

        <!-- 已分發到此目標 -->
        <div class="card p-0 overflow-hidden mb-3">
          <div class="list-head">已分發到此目標（{{ onSkills.length }}）</div>
          <div v-if="onSkills.length === 0" class="p-3 text-xs text-muted">尚無技能分發到此目標。</div>
          <div v-for="s in onSkills" :key="s.slug" class="dist-row">
            <button type="button" class="row-main" @click="detail = s">
              <span class="text-sm font-bold text-fg truncate">{{ s.name }}</span>
              <span class="text-xs text-muted truncate">已分發：{{ whereSummary(s) }}</span>
            </button>
            <span v-if="coveredByGlobal(s)" class="pill tone-accent shrink-0"
                  title="此技能已全域分發，所有專案皆可用；要移除請切到「全域」視角">全域</span>
            <button v-else type="button" class="switch shrink-0" :class="{ on: rowOn(s) }"
                    :aria-pressed="rowOn(s) ? 'true' : 'false'"
                    :aria-label="(rowOn(s) ? '取消分發 ' : '分發 ') + s.name"
                    @click="toggleLens(s)">
              <span class="knob"></span>
            </button>
          </div>
        </div>

        <!-- 未分發 -->
        <div class="card p-0 overflow-hidden">
          <div class="list-head">未分發（{{ offSkills.length }}）</div>
          <div v-if="offSkills.length === 0" class="p-3 text-xs text-muted">所有技能都已分發到此目標。</div>
          <div v-for="s in offSkills" :key="s.slug" class="dist-row">
            <button type="button" class="row-main" @click="detail = s">
              <span class="text-sm font-bold text-fg truncate">{{ s.name }}</span>
              <span class="text-xs text-muted truncate">已分發：{{ whereSummary(s) }}</span>
            </button>
            <span v-if="coveredByGlobal(s)" class="pill tone-accent shrink-0"
                  title="此技能已全域分發，所有專案皆可用；要移除請切到「全域」視角">全域</span>
            <button v-else type="button" class="switch shrink-0" :class="{ on: rowOn(s) }"
                    :aria-pressed="rowOn(s) ? 'true' : 'false'"
                    :aria-label="(rowOn(s) ? '取消分發 ' : '分發 ') + s.name"
                    @click="toggleLens(s)">
              <span class="knob"></span>
            </button>
          </div>
        </div>
      </template>

      <div class="mt-4 flex items-center gap-3">
        <button class="btn btn-primary btn-sm"
                :disabled="busy || pendingCount === 0"
                @click="openDiff">
          {{ busy ? '套用中…' : (pendingCount > 0 ? `套用變更（${pendingCount}）` : '無待套用變更') }}
        </button>
        <span v-if="distMsg" class="text-xs" style="color: var(--c-accent);">{{ distMsg }}</span>
      </div>

      <div v-if="ghostProjects.length"
           class="mt-4 pt-3 border-t border-[var(--c-border)]">
        <div class="text-xs font-semibold mb-1" style="color: var(--c-danger, #c0392b);">
          偵測到 {{ ghostProjects.length }} 個幽靈專案（記錄存在，但磁碟目錄已不存在）
        </div>
        <p class="text-xs text-muted mb-2">這些目標已無法分發（透鏡下拉中已停用）。可移除其記錄以清理清單。</p>
        <div v-for="g in ghostProjects" :key="g.id"
             class="flex items-center justify-between gap-3 py-1">
          <span class="text-xs text-fg min-w-0">
            {{ g.name }} <code class="text-muted break-all">{{ g.path }}</code>
          </span>
          <button class="btn btn-ghost btn-sm shrink-0" :disabled="busy" @click="removeGhost(g)">移除記錄</button>
        </div>
      </div>
    </div>

    <!-- ── 套用差異確認跳窗 ── -->
    <div v-if="showDiff" class="modal-overlay" @click.self="showDiff = false">
      <div class="card modal-card p-0">
        <div class="modal-head">
          <span class="text-sm font-bold text-fg flex-1">確認分發變更</span>
          <button class="btn btn-ghost btn-sm shrink-0" aria-label="關閉" @click="showDiff = false">✕</button>
        </div>
        <div class="modal-body">
          <div v-if="diff.toAdd.length" class="mb-4">
            <div class="text-xs font-semibold mb-2" style="color: var(--c-accent);">
              ＋ 新增分發（{{ diff.toAdd.length }}）
            </div>
            <div v-for="d in diff.toAdd" :key="'a-' + d.slug + '-' + d.target" class="diff-line">
              <span class="text-fg">{{ d.name }}</span>
              <span class="text-muted">→ {{ d.targetLabel }}</span>
            </div>
          </div>
          <div v-if="diff.toRemove.length">
            <div class="text-xs font-semibold mb-2" style="color: var(--c-danger, #c0392b);">
              − 移除分發（{{ diff.toRemove.length }}）
            </div>
            <div v-for="d in diff.toRemove" :key="'r-' + d.slug + '-' + d.target" class="diff-line">
              <span class="text-fg">{{ d.name }}</span>
              <span class="text-muted">→ {{ d.targetLabel }}</span>
            </div>
          </div>
          <div v-if="!diff.toAdd.length && !diff.toRemove.length" class="text-sm text-muted">無變更。</div>
        </div>
        <div class="modal-foot">
          <button class="btn btn-primary btn-sm" :disabled="busy" @click="confirmApply">
            {{ busy ? '套用中…' : '確認套用' }}
          </button>
          <button class="btn btn-ghost btn-sm" :disabled="busy" @click="showDiff = false">取消</button>
        </div>
      </div>
    </div>

    <!-- ── 技能詳情跳窗 ── -->
    <div v-if="detail" class="modal-overlay" @click.self="detail = null">
      <div class="card modal-card p-0">
        <div class="modal-head">
          <span class="text-sm font-bold text-fg flex-1 truncate">{{ detail.name }}</span>
          <span class="pill shrink-0" :class="detail.distributedGlobal ? 'tone-accent' : 'tone-muted'">
            {{ detail.distributedGlobal ? '全域已分發' : '未分發' }}
          </span>
          <button class="btn btn-ghost btn-sm shrink-0" aria-label="關閉" @click="detail = null">✕</button>
        </div>
        <div class="modal-body">
          <div class="text-xs text-muted mb-2" style="font-family: var(--font-mono, monospace);">{{ detail.slug }}</div>
          <div class="text-xs text-muted mb-2">目前分發：{{ whereSummary(detail) }}</div>
          <pre class="text-xs text-muted whitespace-pre-wrap rounded p-2"
               style="background: var(--c-surface-2);">{{ detail.content }}</pre>
        </div>
        <div class="modal-foot">
          <button class="btn btn-primary btn-sm" @click="goDistribute()">前往分發</button>
          <button class="btn btn-ghost btn-sm" @click="detail = null">關閉</button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, reactive, ref, onMounted } from 'vue'
import { api, type LibrarySkill, type ProjectInfo } from '../api/tauriCommands'
import { useSkillStore } from '../stores/skillStore'
import { useProjectStore } from '../stores/projectStore'

// ── 共用載入狀態（讀 store 快取，消除切頁重抓的殘影）─────
const skillStore = useSkillStore()
const projectStore = useProjectStore()
const library = computed<LibrarySkill[]>(() => skillStore.library)
const projects = computed<ProjectInfo[]>(() => projectStore.projects)
// 已有快取 → 不顯示載入占位；首次進來才 true
const loading = ref(!skillStore.loaded)
const loadErr = ref('')
const busy = ref(false)
const distMsg = ref('')
const detail = ref<LibrarySkill | null>(null)

// ── 頁籤 ─────────────────────────────────────────
const tab = ref<'library' | 'distribute'>('library')

// ── Tab 1：技能庫一覽 ────────────────────────────
const query = ref('')
const distFilter = ref<'all' | 'distributed' | 'undistributed'>('all')

// 一行摘要：優先取 frontmatter 的 description；否則取 frontmatter 之後第一行
// 非空、非 Markdown 標題的正文（去清單符號）。避免把 frontmatter 的 name: 當摘要。
function summary(content: string): string {
  const lines = content.split('\n')
  let i = 0
  // 跳過開頭的 YAML frontmatter，同時撈出 description
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

const filteredLibrary = computed(() => {
  const q = query.value.trim().toLowerCase()
  return library.value.filter(s => {
    if (distFilter.value === 'distributed' && !s.distributedGlobal) return false
    if (distFilter.value === 'undistributed' && s.distributedGlobal) return false
    if (!q) return true
    return s.name.toLowerCase().includes(q) || s.content.toLowerCase().includes(q)
  })
})

function goDistribute() {
  detail.value = null
  tab.value = 'distribute'
}

// ── Tab 2：透鏡式分發 ────────────────────────────
const lens = ref<string>('global')        // 'global' | 專案路徑
const distQuery = ref('')
// desired[slug][target] = 使用者期望狀態；初始 = 目前實際（current）狀態
const desired = reactive<Record<string, Record<string, boolean>>>({})

const ghostProjects = computed(() => projects.value.filter(p => !p.pathExists))
const lensIsProject = computed(() => lens.value !== 'global')
const lensLabel = computed(() => targetLabel(lens.value))

function targetLabel(target: string): string {
  if (target === 'global') return '全域（所有專案）'
  const p = projects.value.find(x => x.path === target)
  return p?.name ?? target
}

// current：目前磁碟上的實際分發狀態（以伺服器回報的 library 為準）
function isCurrent(slug: string, target: string): boolean {
  const s = library.value.find(x => x.slug === slug)
  if (!s) return false
  return target === 'global' ? s.distributedGlobal : s.distributedProjects.includes(target)
}

// 在「專案視角」下，全域技能視為已涵蓋並鎖定（Option A）
function coveredByGlobal(s: LibrarySkill): boolean {
  return lensIsProject.value && s.distributedGlobal
}

// 開關顯示狀態（涵蓋者恆為開；其餘看 desired）
function rowOn(s: LibrarySkill): boolean {
  if (coveredByGlobal(s)) return true
  return !!desired[s.slug]?.[lens.value]
}

function toggleLens(s: LibrarySkill) {
  if (coveredByGlobal(s)) return
  const row = desired[s.slug]
  if (row) row[lens.value] = !row[lens.value]
}

// 排序用：以「目前實際狀態」分組（切換開關時列不跳動，套用 reload 後才重排）
function committedOn(s: LibrarySkill): boolean {
  if (coveredByGlobal(s)) return true
  return isCurrent(s.slug, lens.value)
}

const filteredDist = computed(() => {
  const q = distQuery.value.trim().toLowerCase()
  return library.value.filter(s => {
    if (!q) return true
    return s.name.toLowerCase().includes(q) || s.slug.toLowerCase().includes(q)
  })
})
const onSkills = computed(() => filteredDist.value.filter(committedOn))
const offSkills = computed(() => filteredDist.value.filter(s => !committedOn(s)))

// 此技能目前分發到哪（給每列摘要與詳情用）
function whereSummary(s: LibrarySkill): string {
  if (s.distributedGlobal) return '全域（所有專案）'
  if (s.distributedProjects.length) {
    return s.distributedProjects.map(p => targetLabel(p)).join('、')
  }
  return '未分發'
}

// desired vs current 全目標差異
const diff = computed(() => {
  const toAdd: { slug: string; name: string; target: string; targetLabel: string }[] = []
  const toRemove: { slug: string; name: string; target: string; targetLabel: string }[] = []
  const targets = ['global', ...projects.value.map(p => p.path)]
  for (const s of library.value) {
    const row = desired[s.slug] || {}
    for (const t of targets) {
      const cur = isCurrent(s.slug, t)
      const des = !!row[t]
      if (des && !cur) toAdd.push({ slug: s.slug, name: s.name, target: t, targetLabel: targetLabel(t) })
      else if (!des && cur) toRemove.push({ slug: s.slug, name: s.name, target: t, targetLabel: targetLabel(t) })
    }
  }
  return { toAdd, toRemove }
})
const pendingCount = computed(() => diff.value.toAdd.length + diff.value.toRemove.length)
const showDiff = ref(false)

function initDesired() {
  for (const s of library.value) {
    const row: Record<string, boolean> = { global: s.distributedGlobal }
    for (const p of projects.value) row[p.path] = s.distributedProjects.includes(p.path)
    desired[s.slug] = row
  }
}
function resetDesired() {
  initDesired()
}

async function load() {
  await Promise.all([skillStore.fetchLibrary(), projectStore.fetchProjects()])
  // 若目前透鏡指向已不存在的專案，退回全域
  if (lens.value !== 'global' && !projects.value.some(p => p.path === lens.value && p.pathExists)) {
    lens.value = 'global'
  }
  initDesired()
}

onMounted(async () => {
  if (skillStore.loaded) {
    // 已有快取：立即以快取初始化、渲染（零殘影），再背景靜默刷新
    initDesired()
    loading.value = false
    load().catch((e: any) => { loadErr.value = `刷新失敗：${e?.message ?? e}` })
  } else {
    // 首次進來：顯示一次載入占位，抓完才渲染
    try {
      await load()
    } catch (e: any) {
      loadErr.value = `載入失敗：${e?.message ?? e}`
    } finally {
      loading.value = false
    }
  }
})

function openDiff() {
  if (pendingCount.value === 0) return
  showDiff.value = true
}

async function confirmApply() {
  const { toAdd, toRemove } = diff.value
  busy.value = true
  let removed = 0
  let added = 0
  const notes: string[] = []
  try {
    if (toRemove.length) {
      const r = await api.undistributeSkills(toRemove.map(d => ({ skillSlug: d.slug, target: d.target })))
      removed = r.removedCount
    }
    if (toAdd.length) {
      const r = await api.distributeSkillsSelective(toAdd.map(d => ({ skillSlug: d.slug, target: d.target })))
      added = r.writtenCount
      if (r.invalidTargets?.length) notes.push(`部分目標不存在已跳過：${r.invalidTargets.join('、')}`)
    }
    notes.push(`已套用（新增寫入 ${added} 檔、移除 ${removed} 目錄）。`)
    showDiff.value = false
  } catch (e: any) {
    // 先移除後新增為非交易性批次：中途失敗時，已完成的部分不回滾。
    // 無論成敗都在 finally 重新載入，讓 UI 反映磁碟實際狀態（含部分完成）（Codex 備註）。
    notes.push(`套用過程發生錯誤：${e?.message ?? e}（已重新載入反映實際狀態）`)
  } finally {
    // reload 自身亦可能失敗（如 vault 路徑異動），保護避免覆蓋上方訊息。
    try {
      await load()
    } catch (e: any) {
      notes.push(`重新載入失敗：${e?.message ?? e}`)
    }
    distMsg.value = notes.join('　')
    busy.value = false
  }
}

async function removeGhost(g: ProjectInfo) {
  busy.value = true
  distMsg.value = ''
  try {
    await api.removeProject(g.id)
    await load()
    distMsg.value = `已移除幽靈專案記錄：${g.name}`
  } catch (e: any) {
    distMsg.value = `移除失敗：${e?.message ?? e}`
  } finally {
    busy.value = false
  }
}
</script>

<style scoped>
/* 頁籤 */
.tabs {
  display: flex;
  gap: 1.25rem;
  border-bottom: 1px solid var(--c-border);
}
.tab {
  padding: 8px 2px;
  font-size: 14px;
  color: var(--c-muted);
  background: none;
  border: none;
  border-bottom: 2px solid transparent;
  cursor: pointer;
}
.tab.active {
  color: var(--c-fg);
  font-weight: 600;
  border-bottom-color: var(--c-accent);
}

/* 技能庫條列 */
.skill-row {
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
.skill-row:last-child { border-bottom: none; }
.skill-row:hover { background: var(--c-surface-2); }
.skill-name { flex: none; max-width: 38%; }

/* 分發清單 */
.list-head {
  padding: 8px 14px;
  font-size: 12px;
  font-weight: 600;
  color: var(--c-muted);
  background: var(--c-surface-2);
  border-bottom: 1px solid var(--c-border);
}
.dist-row {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 9px 14px;
  border-bottom: 1px solid var(--c-border);
}
.dist-row:last-child { border-bottom: none; }
.dist-row:hover { background: var(--c-surface-2); }
.row-main {
  display: flex;
  flex-direction: column;
  gap: 1px;
  flex: 1;
  min-width: 0;
  text-align: left;
  border: none;
  background: none;
  cursor: pointer;
  padding: 0;
}

/* 差異列 */
.diff-line {
  display: flex;
  gap: 6px;
  font-size: 13px;
  padding: 2px 0;
}

/* 詳情/確認跳窗 */
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

/* 開關 switch（button 版） */
.switch {
  position: relative;
  width: 38px;
  height: 22px;
  border-radius: 999px;
  background: var(--c-border-strong);
  border: none;
  padding: 0;
  cursor: pointer;
  transition: background-color 0.15s ease;
}
.switch.on { background: var(--c-accent); }
.switch .knob {
  position: absolute;
  top: 2px;
  left: 2px;
  width: 18px;
  height: 18px;
  border-radius: 50%;
  background: #fff;
  box-shadow: var(--shadow-sm);
  transition: transform 0.15s ease;
}
.switch.on .knob { transform: translateX(16px); }
</style>
