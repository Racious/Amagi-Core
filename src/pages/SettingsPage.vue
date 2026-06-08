<template>
  <div>
    <div class="mb-6">
      <h1 class="page-title mb-1">設定</h1>
      <p class="page-sub">調整 AMAGI Core 的外觀、通知模式與行為</p>
    </div>

    <!-- 外觀 -->
    <div class="card p-5 mb-4">
      <div class="font-semibold text-sm mb-3 text-fg">外觀主題</div>
      <div class="grid grid-cols-2 gap-2">
        <button
          v-for="opt in themeOptions" :key="opt.value"
          class="flex items-center gap-3 p-3 rounded-lg border text-left transition-colors"
          :class="theme === opt.value ? 'border-accent' : 'border-border'"
          :style="theme === opt.value ? 'background: var(--c-accent-soft);' : 'background: var(--c-surface-2);'"
          @click="set(opt.value)"
        >
          <span class="text-lg">{{ opt.icon }}</span>
          <span class="text-sm font-medium text-fg">{{ opt.label }}</span>
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
import { useTheme, type Theme } from '../composables/useTheme'
import { useUpdater } from '../composables/useUpdater'

const settingsStore = useSettingsStore()
const { theme, set } = useTheme()
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

const themeOptions: { value: Theme; label: string; icon: string }[] = [
  { value: 'light', label: '淺色', icon: '☀️' },
  { value: 'dark', label: '深色', icon: '🌙' },
]

const modes = [
  { value: 'quiet', label: '低干擾模式', desc: '只在系統匣顯示待審核數，不立即彈窗。適合日常開發。' },
  { value: 'normal', label: '一般模式', desc: '偵測到變更時顯示系統匣通知。' },
  { value: 'active', label: '主動模式', desc: '任務結束後自動彈出審核視窗。' },
]
</script>
