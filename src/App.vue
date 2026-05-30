<template>
  <div class="flex h-screen overflow-hidden" style="background-color: #f7f3ff;">
    <aside class="w-56 flex-shrink-0 flex flex-col border-r" style="background: rgba(255,255,255,0.92); border-color: #ded6f5;">
      <div class="p-4 border-b" style="border-color: #ded6f5;">
        <div class="flex items-center gap-3 p-3 rounded-2xl shadow-sm" style="background: linear-gradient(135deg, #fff, #eee8ff);">
          <div class="w-10 h-10 rounded-xl flex items-center justify-center text-white font-bold text-sm"
               style="background: linear-gradient(135deg, #7c5cff, #b197ff);">A</div>
          <div>
            <div class="font-bold text-sm" style="color: #2e2a3f;">AMAGI Core</div>
            <div class="text-xs" style="color: #6f6883;">記憶與技能同步器</div>
          </div>
        </div>
      </div>

      <nav class="flex-1 p-3 space-y-1 overflow-y-auto">
        <div class="text-xs font-bold uppercase tracking-wider px-2 pt-2 pb-1" style="color: #6f6883;">功能</div>
        <RouterLink
          v-for="item in navItems"
          :key="item.to"
          :to="item.to"
          class="flex items-center gap-3 px-3 py-2.5 rounded-xl text-sm transition-colors no-underline"
          style="color: #2e2a3f;"
          active-class="nav-active"
        >
          <span class="text-base leading-none">{{ item.icon }}</span>
          <span class="flex-1">{{ item.label }}</span>
          <span
            v-if="item.badge && reviewStore.pendingCount > 0"
            class="text-xs font-bold text-white rounded-full px-1.5 min-w-[20px] text-center"
            style="background: #7c5cff; padding-top: 2px; padding-bottom: 2px;"
          >{{ reviewStore.pendingCount }}</span>
        </RouterLink>
      </nav>

      <div class="p-3 text-xs text-center border-t" style="color: #6f6883; border-color: #ded6f5;">v0.1.0 MVP</div>
    </aside>

    <main class="flex-1 overflow-y-auto p-6">
      <RouterView />
    </main>
  </div>
</template>

<script setup lang="ts">
import { onMounted } from 'vue'
import { RouterLink, RouterView } from 'vue-router'
import { useReviewStore } from './stores/reviewStore'
import { useProjectStore } from './stores/projectStore'

const reviewStore = useReviewStore()
const projectStore = useProjectStore()

const navItems = [
  { to: '/dashboard', icon: '🏠', label: '總覽', badge: false },
  { to: '/projects', icon: '📁', label: '專案管理', badge: false },
  { to: '/run', icon: '▶️', label: '引導式執行', badge: false },
  { to: '/learn', icon: '🔍', label: '學習變更', badge: false },
  { to: '/review', icon: '📋', label: '審核佇列', badge: true },
  { to: '/skills', icon: '⚡', label: '技能管理', badge: false },
  { to: '/sync', icon: '🔄', label: '同步預覽', badge: false },
  { to: '/workflows', icon: '🔧', label: '工作流程', badge: false },
  { to: '/settings', icon: '⚙️', label: '設定', badge: false },
]

onMounted(async () => {
  await projectStore.fetchProjects()
  await reviewStore.fetchItems()
})
</script>

<style>
.nav-active {
  background-color: #eee8ff !important;
  color: #5037c9 !important;
  font-weight: 600;
}
a:hover {
  background-color: #eee8ff;
  text-decoration: none;
}
* { box-sizing: border-box; }
</style>