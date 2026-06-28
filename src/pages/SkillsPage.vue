<template>
  <div>
    <div class="mb-6">
      <h1 class="page-title mb-1">技能管理</h1>
      <p class="page-sub">已接受的技能候選項，將同步為原生 Skills（.claude/skills 與 .codex/skills），AI 會自動觸發</p>
    </div>

    <div v-if="skills.length === 0"
         class="card card-dashed p-8 text-center">
      <div class="text-4xl mb-3">⚡</div>
      <div class="font-bold mb-1 text-fg">尚無技能</div>
      <div class="text-sm mb-3 text-muted">
        在「審核佇列」接受技能候選項後，技能會出現在這裡。
      </div>
      <RouterLink to="/review"
        class="btn btn-primary btn-sm">前往審核佇列</RouterLink>
    </div>

    <div v-else class="space-y-3">
      <div v-for="skill in skills" :key="skill.id"
           class="card p-4">
        <div class="flex items-start justify-between gap-3">
          <div class="flex-1 min-w-0">
            <div class="flex items-center gap-2 mb-1">
              <span class="text-sm font-bold text-fg">{{ skill.title }}</span>
              <StatusBadge :status="skill.status" />
            </div>
            <div class="text-xs mb-2 text-muted">{{ skill.content }}</div>
            <div class="flex flex-wrap gap-1">
              <span v-for="target in skill.syncTargets" :key="target"
                    class="pill tone-accent">{{ target }}</span>
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- 選擇性分發 ─ 每技能一卡，全域獨立開關＋專案可搜尋 -->
    <div class="card p-5 mt-8">
      <div class="flex items-center justify-between gap-3 mb-1">
        <div class="font-semibold text-sm text-fg">技能分發（選擇性）</div>
        <div class="flex items-center gap-2">
          <span v-if="totalSelected > 0" class="pill tone-accent">已選 {{ totalSelected }} 項</span>
          <button v-if="totalSelected > 0" class="btn btn-ghost btn-sm" @click="clearAll">
            全部清除
          </button>
        </div>
      </div>
      <p class="text-xs text-muted mb-3">
        為每個技能選擇分發目標再按套用。<b>全域</b>＝本機所有專案共用
        （<code>~/.codex/skills</code>、<code>~/.claude/skills</code>）；或點選個別專案。
        逐項挑選，避免一鍵誤分發。
      </p>

      <div v-if="library.length === 0" class="text-sm text-muted">
        vault <code>_skills/</code> 尚無技能（請先設定 vault 路徑並在技能庫放入技能）。
      </div>

      <template v-else>
        <!-- 專案多時才出現的搜尋框，跨所有技能卡片同步過濾 -->
        <div v-if="showProjectSearch" class="mb-3">
          <input v-model="projectQuery" class="input" type="search"
                 placeholder="搜尋專案（名稱或路徑）…" />
        </div>

        <div class="space-y-3">
          <div v-for="s in library" :key="s.slug" class="card p-4">
            <!-- 技能標題列 -->
            <div class="flex items-start justify-between gap-3 mb-3">
              <div class="min-w-0">
                <div class="text-sm font-bold text-fg truncate">{{ s.name }}</div>
                <div class="text-xs text-muted truncate">{{ s.slug }}</div>
              </div>
              <span class="pill shrink-0"
                    :class="selectedCount(s.slug) > 0 ? 'tone-accent' : 'tone-muted'">
                {{ selectedCount(s.slug) }} 個目標
              </span>
            </div>

            <!-- 全域：獨立明顯開關 -->
            <label class="global-row rounded-[var(--radius-sm)] bg-[var(--c-surface-2)] px-3 py-2 mb-3">
              <span class="min-w-0">
                <span class="text-sm font-medium text-fg">全域（本機共用）</span>
                <span class="block text-xs text-muted truncate">~/.codex/skills、~/.claude/skills</span>
              </span>
              <input type="checkbox" class="sr-only" v-model="checked[s.slug].global" />
              <span class="switch shrink-0" :class="{ on: checked[s.slug].global }">
                <span class="knob"></span>
              </span>
            </label>

            <!-- 專案：可切換 chips -->
            <div>
              <div class="flex items-center justify-between mb-2">
                <span class="text-xs text-muted">專案</span>
                <button v-if="projectSelectedCount(s.slug) > 0"
                        class="text-xs text-muted hover:text-fg"
                        @click="clearProjectTargets(s.slug)">
                  清除專案
                </button>
              </div>

              <div v-if="projects.length === 0" class="text-xs text-muted">
                尚無已加入的專案。
              </div>
              <div v-else-if="visibleProjects(s.slug).length === 0" class="text-xs text-muted">
                找不到符合「{{ projectQuery }}」的專案。
              </div>
              <div v-else class="flex flex-wrap gap-1.5">
                <button v-for="p in visibleProjects(s.slug)" :key="p.path"
                        type="button"
                        class="pill chip-toggle"
                        :class="!p.pathExists ? 'tone-muted' : (checked[s.slug][p.path] ? 'tone-accent' : 'tone-muted')"
                        :disabled="!p.pathExists"
                        :title="p.pathExists ? p.path : p.path + '（目錄不存在，無法分發）'"
                        :aria-pressed="checked[s.slug][p.path] ? 'true' : 'false'"
                        :aria-label="(checked[s.slug][p.path] ? '取消分發到 ' : '分發到 ') + p.name"
                        @click="toggle(s.slug, p.path)">
                  <span v-if="checked[s.slug][p.path] && p.pathExists">✓</span>{{ p.name }}<span v-if="!p.pathExists"> ⚠</span>
                </button>
              </div>
            </div>
          </div>
        </div>
      </template>

      <div class="mt-4 flex items-center gap-3">
        <button class="btn btn-primary btn-sm"
                :disabled="busy || library.length === 0"
                @click="applyDistribution">
          {{ busy ? '分發中…' : '套用分發' }}
        </button>
        <span v-if="distMsg" class="text-xs" style="color: var(--c-accent);">{{ distMsg }}</span>
      </div>

      <!-- 幽靈專案清理：projects.json 有記錄但磁碟目錄已不存在 -->
      <div v-if="ghostProjects.length"
           class="mt-4 pt-3 border-t border-[var(--c-border)]">
        <div class="text-xs font-semibold mb-1" style="color: var(--c-danger, #c0392b);">
          偵測到 {{ ghostProjects.length }} 個幽靈專案（記錄存在，但磁碟目錄已不存在）
        </div>
        <p class="text-xs text-muted mb-2">
          這些目標已無法分發（上方已停用 ⚠）。可移除其記錄以清理清單。
        </p>
        <div v-for="g in ghostProjects" :key="g.id"
             class="flex items-center justify-between gap-3 py-1">
          <span class="text-xs text-fg min-w-0">
            {{ g.name }} <code class="text-muted break-all">{{ g.path }}</code>
          </span>
          <button class="btn btn-ghost btn-sm shrink-0"
                  :disabled="busy"
                  @click="removeGhost(g)">移除記錄</button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, reactive, ref, onMounted } from 'vue'
import { RouterLink } from 'vue-router'
import { useReviewStore } from '../stores/reviewStore'
import StatusBadge from '../components/StatusBadge.vue'
import { api, type LibrarySkill, type ProjectInfo } from '../api/tauriCommands'

const reviewStore = useReviewStore()
const skills = computed(() =>
  reviewStore.items.filter(i => i.itemType === 'skill')
)

// ── 選擇性分發 ────────────────────────────────────
const library = ref<LibrarySkill[]>([])
const projects = ref<ProjectInfo[]>([])
const busy = ref(false)
const distMsg = ref('')
const projectQuery = ref('')
const checked = reactive<Record<string, Record<string, boolean>>>({})

// 專案多時才顯示搜尋框，少量專案搜尋反而是噪音
const showProjectSearch = computed(() => projects.value.length > 5)

// 幽靈專案：projects.json 有記錄但磁碟目錄已不存在（pathExists=false）
const ghostProjects = computed(() => projects.value.filter(p => !p.pathExists))

// 搜尋過濾後仍要保留「該技能已選的專案」，避免選好的目標被搜尋藏起來
function visibleProjects(slug: string): ProjectInfo[] {
  const q = projectQuery.value.trim().toLowerCase()
  const row = checked[slug] || {}
  return projects.value.filter(p => {
    if (row[p.path]) return true
    if (!q) return true
    return p.name.toLowerCase().includes(q) || p.path.toLowerCase().includes(q)
  })
}

// 某技能已選目標數（含全域）
function selectedCount(slug: string): number {
  const row = checked[slug]
  if (!row) return 0
  return Object.values(row).filter(Boolean).length
}

// 某技能已選的「專案」數（不含全域）
function projectSelectedCount(slug: string): number {
  const row = checked[slug]
  if (!row) return 0
  return projects.value.reduce((n, p) => n + (row[p.path] ? 1 : 0), 0)
}

// 全部技能跨目標的已選總數
const totalSelected = computed(() => {
  let n = 0
  for (const s of library.value) n += selectedCount(s.slug)
  return n
})

function toggle(slug: string, key: string) {
  const row = checked[slug]
  if (row) row[key] = !row[key]
}

function clearProjectTargets(slug: string) {
  const row = checked[slug]
  if (!row) return
  for (const p of projects.value) row[p.path] = false
}

function clearAll() {
  for (const slug in checked) {
    const row = checked[slug]
    for (const k in row) row[k] = false
  }
}

// 載入技能庫與專案、重建勾選矩陣。重載時保留既有勾選（移除幽靈專案後不清掉使用者選擇）。
async function loadMatrix() {
  const [libs, projs] = await Promise.all([api.listLibrarySkills(), api.listProjects()])
  const targetKeys = ['global', ...projs.map(p => p.path)]
  for (const s of libs) {
    const prev = checked[s.slug] || {}
    const row: Record<string, boolean> = {}
    for (const k of targetKeys) row[k] = prev[k] ?? false
    checked[s.slug] = row
  }
  library.value = libs
  projects.value = projs
}

onMounted(async () => {
  try {
    await loadMatrix()
  } catch (e: any) {
    distMsg.value = `載入失敗：${e?.message ?? e}`
  }
})

// 移除幽靈專案記錄（projects.json）後重載清單
async function removeGhost(g: ProjectInfo) {
  busy.value = true
  distMsg.value = ''
  try {
    await api.removeProject(g.id)
    await loadMatrix()
    distMsg.value = `已移除幽靈專案記錄：${g.name}`
  } catch (e: any) {
    distMsg.value = `移除失敗：${e?.message ?? e}`
  } finally {
    busy.value = false
  }
}

async function applyDistribution() {
  const selections: { skillSlug: string; target: string }[] = []
  const targetKeys = ['global', ...projects.value.map(p => p.path)]
  for (const s of library.value) {
    const row = checked[s.slug] || {}
    for (const key of targetKeys) {
      if (row[key]) selections.push({ skillSlug: s.slug, target: key })
    }
  }
  if (selections.length === 0) {
    distMsg.value = '請先選擇要分發的目標'
    return
  }
  busy.value = true
  distMsg.value = ''
  try {
    const r = await api.distributeSkillsSelective(selections)
    let msg = `已分發 ${r.skillCount} 個技能到 ${r.repoCount} 個目標（寫入 ${r.writtenCount} 檔）。`
    if (r.invalidTargets?.length) {
      msg += ` ${r.invalidTargets.length} 個目標不存在已跳過：${r.invalidTargets.join('、')}`
    }
    distMsg.value = msg
  } catch (e: any) {
    distMsg.value = `分發失敗：${e?.message ?? e}`
  } finally {
    busy.value = false
  }
}
</script>

<style scoped>
/* 全域開關列 */
.global-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.75rem;
  cursor: pointer;
  /* 框住內部 .sr-only checkbox（position:absolute）：否則它停在文檔靜態位置、
     以 html 為包含塊，把 html scrollHeight 撐大而多長一條捲軸 */
  position: relative;
}

/* 開關 switch */
.switch {
  position: relative;
  width: 38px;
  height: 22px;
  border-radius: 999px;
  background: var(--c-border-strong);
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

/* 可切換的專案 chip */
.chip-toggle {
  cursor: pointer;
  border: 1px solid transparent;
  transition: filter 0.12s ease;
}
.chip-toggle:hover { filter: brightness(1.06); }
</style>
