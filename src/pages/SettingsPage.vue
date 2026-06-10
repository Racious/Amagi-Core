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

const settingsStore = useSettingsStore()
const { pref, set } = useTheme()
const { status: updateStatus, newVersion, errorMsg, progress, checkForUpdate, installUpdate } = useUpdater()

const appVersion = ref('—')
onMounted(async () => {
  try { appVersion.value = await getVersion() } catch { /* 非 Tauri 環境 */ }
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
