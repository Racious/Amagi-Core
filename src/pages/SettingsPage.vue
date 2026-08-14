<template>
  <div>
    <div class="mb-6">
      <h1 class="page-title mb-1">設定</h1>
      <p class="page-sub">調整 AMAGI Core 的外觀、通知模式與行為</p>
    </div>

    <!-- 外觀主題 -->
    <div class="card p-5 mb-4">
      <div class="font-semibold text-sm mb-3 text-fg">外觀主題</div>

      <!-- 跟隨系統 -->
      <button
        class="w-full flex items-center gap-3 p-3 rounded-lg border text-left transition-colors mb-3"
        :class="pref === 'system' ? 'border-accent' : 'border-border'"
        :style="pref === 'system' ? 'background: var(--c-accent-soft);' : 'background: var(--c-surface-2);'"
        @click="set('system')"
      >
        <span class="text-lg">🖥️</span>
        <div>
          <div class="text-sm font-medium text-fg">跟隨系統</div>
          <div class="text-xs text-muted">依作業系統的明暗設定自動切換</div>
        </div>
      </button>

      <!-- 淺色 -->
      <div class="text-[11px] font-semibold uppercase tracking-wider mb-1.5" style="color: var(--c-subtle);">淺色</div>
      <div class="grid grid-cols-2 gap-2 mb-3">
        <button
          v-for="t in lightThemes" :key="t.id"
          class="flex items-center gap-2.5 p-2.5 rounded-lg border text-left transition-colors"
          :class="pref === t.id ? 'border-accent' : 'border-border'"
          :style="pref === t.id ? 'background: var(--c-accent-soft);' : 'background: var(--c-surface-2);'"
          @click="set(t.id)"
        >
          <span class="flex rounded-md overflow-hidden border border-border flex-shrink-0" style="width: 38px; height: 26px;">
            <span class="flex-1" :style="{ background: t.swatch.bg }"></span>
            <span class="flex-1" :style="{ background: t.swatch.surface }"></span>
            <span class="flex-1" :style="{ background: t.swatch.accent }"></span>
          </span>
          <span class="text-sm font-medium text-fg leading-tight min-w-0 break-words">{{ t.label }}</span>
        </button>
      </div>

      <!-- 深色 -->
      <div class="text-[11px] font-semibold uppercase tracking-wider mb-1.5" style="color: var(--c-subtle);">深色</div>
      <div class="grid grid-cols-2 gap-2">
        <button
          v-for="t in darkThemes" :key="t.id"
          class="flex items-center gap-2.5 p-2.5 rounded-lg border text-left transition-colors"
          :class="pref === t.id ? 'border-accent' : 'border-border'"
          :style="pref === t.id ? 'background: var(--c-accent-soft);' : 'background: var(--c-surface-2);'"
          @click="set(t.id)"
        >
          <span class="flex rounded-md overflow-hidden border border-border flex-shrink-0" style="width: 38px; height: 26px;">
            <span class="flex-1" :style="{ background: t.swatch.bg }"></span>
            <span class="flex-1" :style="{ background: t.swatch.surface }"></span>
            <span class="flex-1" :style="{ background: t.swatch.accent }"></span>
          </span>
          <span class="text-sm font-medium text-fg leading-tight min-w-0 break-words">{{ t.label }}</span>
        </button>
      </div>
    </div>

    <!-- 通知模式 -->
    <div class="card p-5 mb-4">
      <div class="font-semibold text-sm mb-3 text-fg">通知模式</div>
      <div class="space-y-2">
        <label v-for="mode in modes" :key="mode.value"
               class="flex items-start gap-3 p-3 rounded-lg cursor-pointer border transition-colors"
               :class="settingsStore.notificationMode === mode.value ? 'border-accent' : 'border-border'"
               :style="settingsStore.notificationMode === mode.value ? 'background: var(--c-accent-soft);' : 'background: var(--c-surface-2);'">
          <input type="radio" :value="mode.value" v-model="settingsStore.notificationMode" class="mt-0.5" />
          <div>
            <div class="text-sm font-medium text-fg">{{ mode.label }}</div>
            <div class="text-xs mt-0.5 text-muted">{{ mode.desc }}</div>
          </div>
        </label>
      </div>
    </div>

    <!-- 知識庫（Vault） -->
    <div class="card p-5 mb-4">
      <div class="font-semibold text-sm mb-1 text-fg">知識庫（Vault）</div>
      <p class="text-xs text-muted mb-3">
        設定本機 vault 資料夾。套用後，AMAGI Core 會在全局 ~/.claude/CLAUDE.md 寫入受管區塊，
        讓 Claude 對話開始時自動指向本機 vault（僅替換受管區塊，原內容不動，並先備份 .bak）。
      </p>
      <div class="text-sm mb-3">
        <span class="text-muted">目前路徑：</span>
        <span class="text-fg break-all">{{ vaultPath || '（未設定）' }}</span>
      </div>
      <div class="flex items-center gap-3 flex-wrap">
        <button class="btn btn-primary btn-sm" :disabled="vaultBusy || deployBusy" @click="chooseVault">
          {{ vaultBusy ? '套用中…' : '選擇 vault 資料夾並套用' }}
        </button>
        <button class="btn btn-ghost btn-sm" :disabled="gitBusy || !vaultPath" @click="gitPull">Pull</button>
        <button class="btn btn-ghost btn-sm" :disabled="gitBusy || !vaultPath" @click="gitSync">提交並推送</button>
        <button class="btn btn-ghost btn-sm" :disabled="gitBusy || !vaultPath" @click="gitStatus">狀態</button>
      </div>
      <div class="mt-3 pt-3" style="border-top: 1px solid var(--c-border, #e5e7eb);">
        <p class="text-xs text-muted mb-2">
          同步全域 doctrine：把 vault 的 <code>general/_meta/global-agent-config.md</code>（人格／Git／工作流）
          <strong>整檔覆蓋</strong>本機 ~/.claude/CLAUDE.md 與 ~/.codex/AGENTS.md（首次保留原始 <code>.predeploy.bak</code>、每次留 <code>.bak</code>）。跨機只需 pull vault 後按此。
        </p>
        <button class="btn btn-primary btn-sm" :disabled="deployBusy || !vaultPath" @click="deployDoctrine">
          {{ deployBusy ? '部署中…' : '同步全域 doctrine' }}
        </button>
      </div>
      <p v-if="vaultMsg" class="text-xs mt-2 whitespace-pre-line break-all" style="color: var(--c-accent);">{{ vaultMsg }}</p>
      <p v-if="vaultWarn" class="text-xs mt-2 whitespace-pre-line" style="color: var(--c-warn, #b45309);">{{ vaultWarn }}</p>
      <pre v-if="gitMsg" class="text-xs mt-2 whitespace-pre-wrap text-muted">{{ gitMsg }}</pre>
    </div>

    <!-- Output Style 分發 -->
    <div class="card p-5 mb-4">
      <div class="font-semibold text-sm mb-1 text-fg">Output Style 分發</div>
      <p class="text-xs text-muted mb-3">
        把 vault <code>_output-styles/*.md</code>（天城應對模式正本）<strong>覆蓋分發</strong>到本機
        <code>~/.claude/output-styles/</code>；若 <code>~/.claude/settings.json</code> 尚未設定
        <code>outputStyle</code>，自動補預設「天城」（已有值則一位元組不動）。開全新視窗生效。
      </p>
      <button class="btn btn-primary btn-sm" :disabled="styleBusy || !vaultPath" @click="distributeStyles">
        {{ styleBusy ? '分發中…' : '分發 output styles' }}
      </button>
      <p v-if="styleMsg" class="text-xs mt-2 whitespace-pre-line break-all" style="color: var(--c-accent);">{{ styleMsg }}</p>
      <p v-if="styleWarn" class="text-xs mt-2 whitespace-pre-line" style="color: var(--c-warn, #b45309);">{{ styleWarn }}</p>
    </div>

    <!-- 技能分發（已移至「技能管理」頁）-->
    <div class="card p-5 mb-4">
      <div class="font-semibold text-sm mb-1 text-fg">技能分發</div>
      <p class="text-xs text-muted">
        技能分發已移至「技能管理」頁，改用<b>選擇性分發矩陣</b>（勾選技能 × 目標），避免一鍵誤分發。
      </p>
    </div>

    <!-- 軟體更新 -->
    <div class="card p-5 mb-4">
      <div class="font-semibold text-sm mb-3 text-fg">軟體更新</div>
      <div class="flex items-center gap-3 flex-wrap">
        <button class="btn btn-primary btn-sm"
                :disabled="updateStatus === 'checking' || updateStatus === 'downloading'"
                @click="checkForUpdate(false)">
          {{ updateStatus === 'checking' ? '檢查中…' : '檢查更新' }}
        </button>
        <button v-if="updateStatus === 'available'" class="btn btn-ghost btn-sm" @click="installUpdate">
          ⬇️ 立即更新 v{{ newVersion }}
        </button>
        <span class="text-xs text-muted">{{ updateText }}</span>
      </div>
    </div>

    <!-- 關於 -->
    <div class="card p-5">
      <div class="font-semibold text-sm mb-2 text-fg">關於 AMAGI Core</div>
      <div class="text-sm text-muted space-y-1">
        <div>版本：{{ appVersion }}</div>
        <div>技術棧：Tauri 2 + Rust + Vue 3</div>
        <div>儲存位置：%APPDATA%\AMAGI Core\</div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { getVersion } from '@tauri-apps/api/app'
import { useSettingsStore } from '../stores/settingsStore'
import { useTheme, THEMES } from '../composables/useTheme'
import { useUpdater } from '../composables/useUpdater'
import { open, ask } from '@tauri-apps/plugin-dialog'
import { api } from '../api/tauriCommands'

const settingsStore = useSettingsStore()
const { pref, set } = useTheme()
const { status: updateStatus, newVersion, errorMsg, progress, checkForUpdate, installUpdate } = useUpdater()

const appVersion = ref('—')

// ── Vault 知識庫設定 ──────────────────────────────
const vaultPath = ref<string | null>(null)
const vaultBusy = ref(false)
const vaultMsg = ref('')
const vaultWarn = ref('')

async function loadVault() {
  try {
    const cfg = await api.getVaultConfig()
    vaultPath.value = cfg.vaultPath
  } catch { /* 非 Tauri 環境 */ }
}

async function chooseVault() {
  vaultMsg.value = ''
  vaultWarn.value = ''
  vaultBusy.value = true
  try {
    // open 也納入 try：dialog plugin reject 時才會顯示提示，不致無聲中斷（與 OnboardingVault 一致）
    const picked = await open({ directory: true, multiple: false, title: '選擇 vault 資料夾' })
    if (typeof picked !== 'string') return // 使用者取消
    const r = await api.setVaultPath(picked)
    vaultPath.value = r.vaultPath
    const verb = r.pointerAction === 'replaced' ? '更新' : '寫入'
    vaultMsg.value = `已${verb} ~/.claude/CLAUDE.md 與 ~/.codex/AGENTS.md 受管區塊${r.backupMade ? '（已備份 .bak）' : ''}`
    if (!r.looksLikeVault) {
      vaultWarn.value = '提醒：該資料夾未偵測到 CLAUDE.md / index.md，可能尚未初始化為 vault。'
    }
    // 步驟5 自動化（A 案）：偵測到 vault 內有全域 doctrine 源檔時，提議順手部署，
    // 省去手動再按「同步全域」一步。破壞性整檔覆蓋，仍以確認對話把關、老爺可婉拒。
    if (r.hasDoctrineSource) {
      const go = await ask(
        '偵測到此 vault 內有全域 doctrine（general/_meta/global-agent-config.md）。\n'
          + '要現在「整檔覆蓋」部署到 ~/.claude/CLAUDE.md 與 ~/.codex/AGENTS.md 嗎？\n'
          + '（首次保留 .predeploy.bak、每次留 .bak，可還原；亦可稍後手動按「同步全域 doctrine」）',
        { title: '一併部署全域 doctrine？', kind: 'info' }
      )
      if (go) {
        // 獨立 try/catch（r1 低風險）：vault 已設定成功，失敗的僅是後續一併部署，
        // 訊息須如實區分，不可落入外層 catch 誤報「設定失敗」。
        // 期間設 deployBusy 防止手動「同步全域」按鈕重入同一破壞性覆寫流程。
        deployBusy.value = true
        try {
          await applyDoctrineDeploy()
        } catch (e: any) {
          vaultWarn.value = `vault 已設定，但一併部署全域 doctrine 失敗（未寫入）：${e?.message ?? e}`
        } finally {
          deployBusy.value = false
        }
      }
    }
  } catch (e: any) {
    vaultWarn.value = `設定失敗：${e?.message ?? e}`
  } finally {
    vaultBusy.value = false
  }
}

// ── 步驟5：同步全域 doctrine（整檔部署）──────────────
const deployBusy = ref(false)

/** 實際呼叫部署並更新訊息（不含確認對話）；供手動鈕與設路徑後自動提議共用。 */
async function applyDoctrineDeploy() {
  const r = await api.deployGlobalDoctrine()
  // 累加不覆蓋：自動部署路徑上，vault 設定結果（受管區塊/備份）須與部署結果並陳
  const msg = `已部署全域 doctrine：\n・${r.claudePath}\n・${r.codexPath}`
    + (r.backupMade ? '\n（首次原始版存於 <檔>.predeploy.bak、前一版存於 <檔>.bak，可還原）' : '')
  vaultMsg.value = vaultMsg.value ? `${vaultMsg.value}\n${msg}` : msg
  if (r.warnings.length) {
    const w = r.warnings.join('\n')
    vaultWarn.value = vaultWarn.value ? `${vaultWarn.value}\n${w}` : w
  }
}

async function deployDoctrine() {
  vaultMsg.value = ''
  vaultWarn.value = ''
  const ok = await ask(
    '將用 vault 的 global-agent-config.md「整檔覆蓋」~/.claude/CLAUDE.md 與 ~/.codex/AGENTS.md。\n首次保留 .predeploy.bak、每次留 .bak，可還原。確定部署？',
    { title: '同步全域 doctrine', kind: 'warning' }
  )
  if (!ok) return
  deployBusy.value = true
  try {
    await applyDoctrineDeploy()
  } catch (e: any) {
    vaultWarn.value = `部署失敗（未寫入）：${e?.message ?? e}`
  } finally {
    deployBusy.value = false
  }
}

// ── Output Style 分發（A-3）────────────────────────
const styleBusy = ref(false)
const styleMsg = ref('')
const styleWarn = ref('')

/** settings 動作 → 人話回報（情境窮舉，缺 case 會被 TS never 檢查抓到） */
function settingsActionText(a: import('../api/tauriCommands').OutputStyleSettingsAction): string {
  switch (a) {
    case 'created_with_default': return 'settings.json 不存在，已建立並設預設「天城」'
    case 'added_default': return 'settings.json 已補預設 outputStyle「天城」（其他欄位未動）'
    case 'already_set': return 'settings.json 已有 outputStyle，原值未動'
    case 'parse_failed_skipped': return '⚠ settings.json 解析失敗，未寫入（請手動檢查該檔）'
    case 'skipped_no_styles': return 'settings.json 未動'
  }
}

async function distributeStyles() {
  styleMsg.value = ''
  styleWarn.value = ''
  const ok = await ask(
    '將把 vault _output-styles/ 的 style「覆蓋」到 ~/.claude/output-styles/ 同名副本，\n'
      + '並在 settings.json 缺 outputStyle 時補預設「天城」（已有值不動）。確定分發？',
    { title: '分發 output styles', kind: 'warning' }
  )
  if (!ok) return
  styleBusy.value = true
  try {
    const r = await api.distributeOutputStyles()
    if (r.distributed.length === 0) {
      styleWarn.value = 'vault _output-styles/ 沒有可分發的 style（README 與 dot-prefixed 不算），未寫入任何檔案。'
      return
    }
    styleMsg.value = `已分發 ${r.distributed.length} 款：${r.distributed.join('、')}\n`
      + settingsActionText(r.settingsAction)
      + '\n（切換預設款後需開全新視窗才生效）'
    if (r.missingName.length) {
      styleWarn.value = `以下檔案缺 frontmatter name:（切換時無法以名稱選用，建議補上）：${r.missingName.join('、')}`
    }
    if (r.settingsAction === 'parse_failed_skipped') {
      styleWarn.value = (styleWarn.value ? styleWarn.value + '\n' : '')
        + 'settings.json 解析失敗未寫入——style 檔已照常分發，settings 請手動處理。'
    }
  } catch (e: any) {
    styleWarn.value = `分發失敗：${e?.message ?? e}`
  } finally {
    styleBusy.value = false
  }
}

// ── vault git 自管 ────────────────────────────────
const gitBusy = ref(false)
const gitMsg = ref('')

// AppError 結構化變體（GitConflict / GitSyncUnpushed）的 message 是物件（含人話 message 欄）；
// 其餘變體 message 為字串。依 kind 加前綴讓錯誤性質一眼可辨（adr-008）。
function gitErrText(e: any): string {
  const m = e?.message
  const text = (m && typeof m === 'object') ? (m.message ?? JSON.stringify(m)) : String(m ?? e)
  if (e?.kind === 'GitConflict') return `衝突：${text}`
  if (e?.kind === 'GitSyncUnpushed') return `未推送：${text}`
  return `失敗：${text}`
}

async function gitStatus() {
  gitBusy.value = true
  try {
    const s = await api.vaultGitStatus()
    gitMsg.value = s.trim() ? s : '工作區乾淨，無變更。'
  } catch (e: any) { gitMsg.value = gitErrText(e) }
  finally { gitBusy.value = false }
}

async function gitPull() {
  gitBusy.value = true
  try { gitMsg.value = await api.vaultGitPull() || '已是最新。' }
  catch (e: any) { gitMsg.value = gitErrText(e) }
  finally { gitBusy.value = false }
}

async function gitSync() {
  gitBusy.value = true
  try { gitMsg.value = await api.vaultGitSync() }
  catch (e: any) { gitMsg.value = gitErrText(e) }
  finally { gitBusy.value = false }
}

onMounted(async () => {
  try { appVersion.value = await getVersion() } catch { /* 非 Tauri 環境 */ }
  loadVault()
})

const updateText = computed(() => {
  switch (updateStatus.value) {
    case 'uptodate': return '已是最新版本'
    case 'available': return `發現新版本 v${newVersion.value}`
    case 'downloading': return `下載中… ${progress.value}%`
    case 'error': return `檢查失敗：${errorMsg.value ?? ''}`
    default: return ''
  }
})

const lightThemes = THEMES.filter((t) => t.base === 'light')
const darkThemes = THEMES.filter((t) => t.base === 'dark')

const modes = [
  { value: 'quiet', label: '低干擾模式', desc: '只在系統匣顯示待審核數，不立即彈窗。適合日常開發。' },
  { value: 'normal', label: '一般模式', desc: '偵測到變更時顯示系統匣通知。' },
  { value: 'active', label: '主動模式', desc: '任務結束後自動彈出審核視窗。' },
]
</script>
