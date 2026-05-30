<template>
  <div>
    <div class="mb-6">
      <h1 class="text-2xl font-bold mb-1" style="color: #201b34;">設定</h1>
      <p class="text-sm" style="color: #6f6883;">調整 AMAGI Core 的通知模式與行為</p>
    </div>

    <div class="rounded-2xl p-5 border mb-4" style="background: white; border-color: #ded6f5;">
      <div class="font-bold mb-3" style="color: #2e2a3f;">通知模式</div>
      <div class="space-y-2">
        <label v-for="mode in modes" :key="mode.value"
               class="flex items-start gap-3 p-3 rounded-xl cursor-pointer border transition-colors"
               :style="settingsStore.notificationMode === mode.value
                 ? 'background: #eee8ff; border-color: #7c5cff;'
                 : 'background: #f9f7ff; border-color: #ded6f5;'">
          <input type="radio" :value="mode.value" v-model="settingsStore.notificationMode" class="mt-0.5" />
          <div>
            <div class="text-sm font-bold" style="color: #2e2a3f;">{{ mode.label }}</div>
            <div class="text-xs mt-0.5" style="color: #6f6883;">{{ mode.desc }}</div>
          </div>
        </label>
      </div>
    </div>

    <div class="rounded-2xl p-5 border" style="background: white; border-color: #ded6f5;">
      <div class="font-bold mb-2" style="color: #2e2a3f;">關於 AMAGI Core</div>
      <div class="text-sm" style="color: #6f6883;">
        <div>版本：0.1.0 MVP</div>
        <div class="mt-1">技術棧：Tauri 2 + Rust + Vue 3</div>
        <div class="mt-1">儲存位置：%APPDATA%\AMAGI Core\</div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { useSettingsStore } from '../stores/settingsStore'

const settingsStore = useSettingsStore()

const modes = [
  { value: 'quiet', label: '低干擾模式', desc: '只在系統匣顯示待審核數，不立即彈窗。適合日常開發。' },
  { value: 'normal', label: '一般模式', desc: '偵測到變更時顯示系統匣通知。' },
  { value: 'active', label: '主動模式', desc: '任務結束後自動彈出審核視窗。' },
]
</script>
