<template>
  <div class="flex h-screen overflow-hidden bg-canvas">
    <OnboardingVault v-if="showOnboarding" @done="showOnboarding = false" @skip="showOnboarding = false" />
    <aside class="w-60 flex-shrink-0 flex flex-col bg-surface border-r border-border">
      <!-- 品牌 -->
      <div class="px-4 h-14 flex items-center gap-2.5 border-b border-border">
        <div class="w-7 h-7 rounded-md flex items-center justify-center text-white font-bold text-xs"
             style="background: var(--c-accent);">A</div>
        <div class="leading-tight">
          <div class="font-semibold text-sm text-fg">AMAGI Core</div>
          <div class="text-[11px] text-subtle">記憶與技能同步器</div>
        </div>
      </div>

      <!-- 導覽 -->
      <nav class="flex-1 px-2.5 py-3 overflow-y-auto">
        <template v-for="(group, gi) in navGroups" :key="gi">
          <div class="text-[10px] font-semibold uppercase tracking-wider px-2 mb-1"
               :class="gi > 0 ? 'mt-4' : ''" style="color: var(--c-subtle);">{{ group.title }}</div>
          <RouterLink
            v-for="item in group.items"
            :key="item.to"
            :to="item.to"
            class="nav-link mb-0.5"
            active-class="nav-active"
          >
            <span class="text-[15px] leading-none w-5 text-center opacity-80">{{ item.icon }}</span>
            <span class="flex-1">{{ item.label }}</span>
            <span v-if="item.badge && reviewStore.pendingCount > 0" class="pill tone-accent">
              {{ reviewStore.pendingCount }}
            </span>
          </RouterLink>
        </template>
      </nav>

      <!-- 頁尾：主題切換 + 版本 -->
      <div class="px-2.5 py-2.5 border-t border-border flex items-center justify-between">
        <button class="btn btn-ghost btn-sm" @click="toggle" :title="activeBase === 'dark' ? '切換為淺色' : '切換為深色'">
          <span>{{ activeBase === 'dark' ? '🌙' : '☀️' }}</span>
          <span>{{ activeBase === 'dark' ? '深色' : '淺色' }}</span>
        </button>
        <span class="text-[11px] text-subtle pr-1">v{{ appVersion }}</span>
      </div>
    </aside>

    <main class="flex-1 overflow-y-auto">
      <!-- 更新橫幅 -->
      <div v-if="updateStatus === 'available' || updateStatus === 'downloading'" class="px-7 pt-4">
        <div class="alert tone-accent flex items-center gap-3">
          <span class="text-sm flex-1" style="color: var(--c-accent);">
            <template v-if="updateStatus === 'available'">🎉 發現新版本 v{{ newVersion }}，要立即更新嗎？</template>
            <template v-else>下載並安裝中… {{ progress }}%（完成後將自動重啟）</template>
          </span>
          <template v-if="updateStatus === 'available'">
            <button class="btn btn-primary btn-sm" @click="installUpdate">立即更新</button>
            <button class="btn btn-ghost btn-sm" @click="dismiss">稍後</button>
          </template>
        </div>
      </div>
      <div class="max-w-5xl mx-auto px-7 py-6">
        <RouterView />
      </div>
    </main>
  </div>
</template>

<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { getVersion } from '@tauri-apps/api/app'
import { RouterLink, RouterView } from 'vue-router'
import { useReviewStore } from './stores/reviewStore'
import { useProjectStore } from './stores/projectStore'
import { useSkillStore } from './stores/skillStore'
import { useTheme } from './composables/useTheme'
import { useUpdater } from './composables/useUpdater'
import { api } from './api/tauriCommands'
import OnboardingVault from './components/OnboardingVault.vue'

const reviewStore = useReviewStore()
const projectStore = useProjectStore()
const skillStore = useSkillStore()
const { activeBase, toggle } = useTheme()

const appVersion = ref('0.1.0')
const showOnboarding = ref(false)
const { status: updateStatus, newVersion, progress, checkForUpdate, installUpdate, dismiss } = useUpdater()

const navGroups = [
  {
    title: '工作區',
    items: [
      { to: '/dashboard', icon: '🏠', label: '總覽', badge: false },
      { to: '/projects', icon: '📁', label: '專案管理', badge: false },
      { to: '/ingest', icon: '📥', label: '知識匯入', badge: false },
    ],
  },
  {
    title: '任務',
    items: [
      { to: '/run', icon: '▶️', label: '引導式執行', badge: false },
      { to: '/learn', icon: '🔍', label: '學習變更', badge: false },
      { to: '/review', icon: '📋', label: '審核佇列', badge: true },
      { to: '/skills', icon: '⚡', label: '技能管理', badge: false },
      { to: '/sync', icon: '🔄', label: '同步預覽', badge: false },
      { to: '/diff-export', icon: '🧾', label: '差異匯出', badge: false },
      { to: '/workflows', icon: '🔧', label: '工作流程', badge: false },
    ],
  },
  {
    title: '系統',
    items: [
      { to: '/settings', icon: '⚙️', label: '設定', badge: false },
    ],
  },
]

onMounted(async () => {
  try { appVersion.value = await getVersion() } catch { /* 非 Tauri 環境 */ }
  // 首次啟動引導（2c）：未設定 vault → 顯示引導 overlay
  try {
    const st = await api.getVaultStatus()
    showOnboarding.value = !st.configured
  } catch { /* 非 Tauri 環境 */ }
  await projectStore.fetchProjects()
  await reviewStore.fetchItems()
  // 預熱技能庫快取，讓首次進「技能管理」也免載入殘影（非阻塞、失敗不打擾）
  skillStore.fetchLibrary().catch(() => { /* 未設 vault 等情況 → 進頁時再抓 */ })
  // 啟動時靜默檢查更新（失敗不打擾；有新版才顯示橫幅）
  checkForUpdate(true)
})
</script>
