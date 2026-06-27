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
        <button class="btn btn-primary btn-sm" :disabled="vaultBusy" @click="chooseVault">
          {{ vaultBusy ? '套用中…' : '選擇 vault 資料夾並套用' }}
        </button>
        <button class="btn btn-ghost btn-sm" :disabled="gitBusy || !vaultPath" @click="gitPull">Pull</button>
        <button class="btn btn-ghost btn-sm" :disabled="gitBusy || !vaultPath" @click="gitSync">提交並推送</button>
        <button class="btn btn-ghost btn-sm" :disabled="gitBusy || !vaultPath" @click="gitStatus">狀態</button>
      </div>
      <p v-if="vaultMsg" class="text-xs mt-2" style="color: var(--c-accent);">{{ vaultMsg }}</p>
      <p v-if="vaultWarn" class="text-xs mt-2" style="color: var(--c-warn, #b45309);">{{ vaultWarn }}</p>
      <pre v-if="gitMsg" class="text-xs mt-2 whitespace-pre-wrap text-muted">{{ gitMsg }}</pre>
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
import { open } from '@tauri-apps/plugin-dialog'
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
  const picked = await open({ directory: true, multiple: false, title: '選擇 vault 資料夾' })
  if (typeof picked !== 'string') return
  vaultBusy.value = true
  try {
    const r = await api.setVaultPath(picked)
    vaultPath.value = r.vaultPath
    const verb = r.pointerAction === 'replaced' ? '更新' : '寫入'
    vaultMsg.value = `已${verb} ~/.claude/CLAUDE.md 受管區塊${r.backupMade ? '（已備份 .bak）' : ''}`
    if (!r.looksLikeVault) {
      vaultWarn.value = '提醒：該資料夾未偵測到 CLAUDE.md / index.md，可能尚未初始化為 vault。'
    }
  } catch (e: any) {
    vaultWarn.value = `設定失敗：${e?.message ?? e}`
  } finally {
    vaultBusy.value = false
  }
}

// ── vault git 自管 ────────────────────────────────
const gitBusy = ref(false)
const gitMsg = ref('')

async function gitStatus() {
  gitBusy.value = true
  try {
    const s = await api.vaultGitStatus()
    gitMsg.value = s.trim() ? s : '工作區乾淨，無變更。'
  } catch (e: any) { gitMsg.value = `失敗：${e?.message ?? e}` }
  finally { gitBusy.value = false }
}

async function gitPull() {
  gitBusy.value = true
  try { gitMsg.value = await api.vaultGitPull() || '已是最新。' }
  catch (e: any) { gitMsg.value = `失敗：${e?.message ?? e}` }
  finally { gitBusy.value = false }
}

async function gitSync() {
  gitBusy.value = true
  try { gitMsg.value = await api.vaultGitSync() }
  catch (e: any) { gitMsg.value = `失敗：${e?.message ?? e}` }
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
