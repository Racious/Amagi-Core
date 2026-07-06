<template>
  <div class="onboarding-overlay">
    <div class="card onboarding-card">
      <div class="text-2xl mb-1">📚 設定知識庫（Vault）</div>
      <p class="text-sm text-muted mb-4">
        AMAGI Core 的知識、技能、長期記憶都存在「知識庫（vault）」這個資料夾裡。
        首次使用請先指定一個資料夾作為本機 vault——之後所有產出都會歸入此處，跨機器也靠它同步。
      </p>

      <!-- 步驟一：選資料夾 -->
      <template v-if="!done">
        <div class="flex items-center gap-3 flex-wrap">
          <button class="btn btn-primary btn-sm" :disabled="busy" @click="chooseVault">
            {{ busy ? '套用中…' : '選擇 vault 資料夾' }}
          </button>
          <button class="btn btn-ghost btn-sm" :disabled="busy" @click="$emit('skip')">稍後再說</button>
        </div>
        <p v-if="warn" class="text-xs mt-3" style="color: var(--c-danger, #c0392b);">{{ warn }}</p>
        <p class="text-xs text-muted mt-3">
          選定後，AMAGI 會在全局 <code>~/.claude/CLAUDE.md</code>、<code>~/.codex/AGENTS.md</code>
          寫入受管區塊（僅替換標記區、原內容不動、先備份 .bak），讓 AI 對話自動指向本機 vault。
        </p>
      </template>

      <!-- 步驟二：設定完成 + git 保命建議 -->
      <template v-else>
        <p class="text-sm mb-3">
          <span style="color: var(--c-accent);">✓ 已設定：</span>
          <span class="text-fg break-all">{{ vaultPath }}</span>
        </p>
        <div v-if="!isGitRepo" class="alert tone-warning mb-3">
          <div class="text-sm font-semibold mb-1" style="color: var(--c-warning);">⚠ 強烈建議掛上 git（保命）</div>
          <p class="text-xs text-muted">
            vault 是所有知識的唯一家——一旦遺失無可挽回。請將此資料夾
            <code>git init</code> 並推送遠端，才能跨機器同步、避免單點故障。
            掛好後可到「設定 → 知識庫」用「提交並推送」。
          </p>
        </div>
        <div v-else class="text-sm mb-3" style="color: var(--c-accent);">
          ✓ 已偵測到 git，跨機同步就緒。
        </div>

        <!-- 步驟三：部署全域 doctrine（偵測到 vault 內有源檔時才顯示；每機一次、可略過）-->
        <div v-if="hasDoctrineSource" class="doctrine-deploy mb-4">
          <div class="text-sm font-semibold mb-1">📜 部署全域 doctrine</div>
          <p class="text-xs text-muted mb-2">
            偵測到知識庫內有全域 doctrine（<code>general/_meta/global-agent-config.md</code>）。
            部署會「整檔覆蓋」本機 <code>~/.claude/CLAUDE.md</code>、<code>~/.codex/AGENTS.md</code>
            （首次留 .predeploy.bak、每次留 .bak，可還原），讓本機 AI 依知識庫統一規範運作。每台機器做一次。
          </p>
          <div v-if="!doctrineDone" class="flex items-center gap-3 flex-wrap">
            <button class="btn btn-primary btn-sm" :disabled="deployingDoctrine" @click="deployDoctrine">
              {{ deployingDoctrine ? '部署中…' : '部署全域 doctrine' }}
            </button>
            <span class="text-xs text-muted">可略過，日後在「設定 → 知識庫」再部署。</span>
          </div>
          <p v-else class="text-xs" style="color: var(--c-accent);">{{ doctrineMsg }}</p>
          <p v-if="doctrineErr" class="text-xs mt-1" style="color: var(--c-danger, #c0392b);">{{ doctrineErr }}</p>
        </div>

        <!-- 步驟四：技能分發到本機全域（每機各做一次，新環境一鍵就緒）-->
        <div class="skill-distribute mb-4">
          <template v-if="librarySkills.length > 0">
            <div class="text-sm font-semibold mb-1">🧩 技能分發到本機全域</div>
            <p class="text-xs text-muted mb-2">
              知識庫有 {{ librarySkills.length }} 個技能。分發到本機全域
              <code>~/.codex/skills</code>、<code>~/.claude/skills</code> 後，所有專案的 AI 對話即可使用。
              技能正本恆在 vault，分發每台機器各做一次；日後可到「技能」頁調整。
            </p>
            <div v-if="!distributeDone" class="flex items-center gap-3 flex-wrap">
              <button class="btn btn-primary btn-sm" :disabled="distributing" @click="distributeAll">
                {{ distributing ? '分發中…' : `分發 ${librarySkills.length} 個技能到全域` }}
              </button>
              <span class="text-xs text-muted">可略過，日後在「技能」頁再分發。</span>
            </div>
            <p v-else class="text-xs" style="color: var(--c-accent);">{{ distributeMsg }}</p>
            <p v-if="distributeErr" class="text-xs mt-1" style="color: var(--c-danger, #c0392b);">{{ distributeErr }}</p>
          </template>
          <p v-else-if="skillLoadWarn" class="text-xs" style="color: var(--c-danger, #c0392b);">{{ skillLoadWarn }}</p>
          <p v-else class="text-xs text-muted">
            知識庫目前沒有技能。日後可到「技能」頁收編技能後再分發到本機全域。
          </p>
        </div>

        <button class="btn btn-primary btn-sm" :disabled="distributing || deployingDoctrine" @click="$emit('done')">
          {{ distributing || deployingDoctrine ? '完成後開始使用' : '開始使用' }}
        </button>
      </template>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { open } from '@tauri-apps/plugin-dialog'
import { api, type LibrarySkill } from '../api/tauriCommands'

defineEmits<{ done: []; skip: [] }>()

const busy = ref(false)
const done = ref(false)
const warn = ref('')
const vaultPath = ref<string | null>(null)
const isGitRepo = ref(false)

// 全域 doctrine 部署引導（步驟三；偵測到 vault 內有源檔時才顯示）
const hasDoctrineSource = ref(false)
const deployingDoctrine = ref(false)
const doctrineDone = ref(false)
const doctrineMsg = ref('')
const doctrineErr = ref('')

// 技能全域分發引導（步驟三）
const librarySkills = ref<LibrarySkill[]>([])
const skillLoadWarn = ref('')
const distributing = ref(false)
const distributeDone = ref(false)
const distributeMsg = ref('')
const distributeErr = ref('')

async function chooseVault() {
  warn.value = ''
  busy.value = true
  try {
    // open 也納入 try：dialog plugin reject 時才會顯示提示，不致無聲中斷
    const picked = await open({ directory: true, multiple: false, title: '選擇 vault 資料夾' })
    if (typeof picked !== 'string') return // 使用者取消
    const r = await api.setVaultPath(picked)
    vaultPath.value = r.vaultPath
    // 是否有全域 doctrine 源檔 → 決定是否顯示「部署全域 doctrine」可選步驟（④ 新機主要場景）
    hasDoctrineSource.value = r.hasDoctrineSource
    // 取最新狀態判斷是否已掛 git（保命建議用）
    const st = await api.getVaultStatus()
    isGitRepo.value = st.isGitRepo
    // 讀技能庫供全域分發引導；失敗不擋流程，但區分「讀取失敗」與「確實無技能」（Codex 低）
    try {
      librarySkills.value = await api.listLibrarySkills()
    } catch {
      librarySkills.value = []
      skillLoadWarn.value = '無法讀取技能清單，可稍後到「技能」頁分發。'
    }
    done.value = true
  } catch (e: any) {
    warn.value = `設定失敗：${e?.message ?? e}`
  } finally {
    busy.value = false
  }
}

// 部署全域 doctrine：整檔覆蓋本機 ~/.claude/CLAUDE.md、~/.codex/AGENTS.md（後端 fail-closed＋備份＋原子寫入）。
// onboarding 為新機主要場景（④）；可略過，日後可到設定頁再部署。失敗不擋流程、訊息如實區分。
async function deployDoctrine() {
  doctrineErr.value = ''
  deployingDoctrine.value = true
  try {
    const r = await api.deployGlobalDoctrine()
    doctrineMsg.value = `✓ 已部署全域 doctrine 到 ~/.claude/CLAUDE.md 與 ~/.codex/AGENTS.md`
      + (r.backupMade ? '（原始版存於 .predeploy.bak、前一版存於 .bak，可還原）' : '') + '。'
    doctrineDone.value = true
    if (r.warnings.length) doctrineErr.value = r.warnings.join('\n')
  } catch (e: any) {
    doctrineErr.value = `部署失敗（未寫入）：${e?.message ?? e}`
  } finally {
    deployingDoctrine.value = false
  }
}

// 把知識庫所有技能分發到本機全域（所有技能 × global），複用選擇性分發指令。
async function distributeAll() {
  distributeErr.value = ''
  distributing.value = true
  try {
    const selections = librarySkills.value.map((s) => ({ skillSlug: s.slug, target: 'global' }))
    const r = await api.distributeSkillsSelective(selections)
    distributeMsg.value = `✓ 已分發 ${r.skillCount} 個技能到本機全域。`
    distributeDone.value = true
  } catch (e: any) {
    distributeErr.value = `分發失敗：${e?.message ?? e}`
  } finally {
    distributing.value = false
  }
}
</script>

<style scoped>
.onboarding-overlay {
  position: fixed;
  inset: 0;
  z-index: 50;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 1.5rem;
  background: color-mix(in srgb, var(--c-canvas) 80%, transparent);
  backdrop-filter: blur(2px);
}
.onboarding-card {
  max-width: 32rem;
  width: 100%;
  padding: 1.75rem;
}
</style>
