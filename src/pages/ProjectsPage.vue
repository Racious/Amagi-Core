<template>
  <div>
    <div class="flex items-center justify-between mb-6">
      <div>
        <h1 class="text-2xl font-bold mb-1" style="color: #201b34;">專案管理</h1>
        <p class="text-sm" style="color: #6f6883;">新增、初始化與管理受監控的 Git 專案</p>
      </div>
      <button @click="openFolderPicker"
              class="px-4 py-2 rounded-xl text-sm font-bold text-white transition-opacity hover:opacity-90"
              style="background: #7c5cff;">
        + 新增專案
      </button>
    </div>

    <!-- 錯誤訊息 -->
    <div v-if="error" class="rounded-2xl p-4 mb-4 border" style="background: #fff0f0; border-color: #efb5b5;">
      <span class="text-sm" style="color: #ab3a3a;">{{ error }}</span>
    </div>

    <!-- 成功訊息 -->
    <div v-if="successMsg" class="rounded-2xl p-4 mb-4 border" style="background: #eefaf4; border-color: #bde8d1;">
      <span class="text-sm" style="color: #1d7a51;">{{ successMsg }}</span>
    </div>

    <div v-if="projectStore.projects.length === 0"
         class="rounded-2xl p-8 text-center border"
         style="background: white; border-color: #ded6f5; border-style: dashed;">
      <div class="text-4xl mb-3">📂</div>
      <p class="text-sm" style="color: #6f6883;">點擊「新增專案」選擇本機 Git 專案資料夾。</p>
    </div>

    <div v-else class="space-y-3">
      <div
        v-for="project in projectStore.projects"
        :key="project.id"
        class="rounded-2xl p-4 border flex items-center gap-4"
        style="background: white; border-color: #ded6f5;"
      >
        <div class="flex-1 min-w-0">
          <div class="font-bold truncate" style="color: #2e2a3f;">{{ project.name }}</div>
          <div class="text-xs truncate mt-0.5" style="color: #6f6883;">{{ project.path }}</div>
          <div class="flex items-center gap-2 mt-1.5">
            <span v-if="project.currentBranch"
                  class="text-xs px-2 py-0.5 rounded-full"
                  style="background: #eee8ff; color: #5037c9;">
              {{ project.currentBranch }}
            </span>
            <span v-if="project.initialized"
                  class="text-xs px-2 py-0.5 rounded-full"
                  style="background: #eefaf4; color: #1d7a51;">
              已初始化
            </span>
            <span v-else
                  class="text-xs px-2 py-0.5 rounded-full"
                  style="background: #fff7e8; color: #916216;">
              未初始化
            </span>
          </div>
        </div>
        <div class="flex items-center gap-2 flex-shrink-0">
          <button
            v-if="!project.initialized"
            @click="initProject(project.id)"
            :disabled="projectStore.loading"
            class="px-3 py-1.5 rounded-xl text-xs font-bold text-white"
            style="background: #3cae78;"
          >初始化</button>
          <button
            @click="removeProject(project.id)"
            class="px-3 py-1.5 rounded-xl text-xs font-bold"
            style="background: #fff0f0; color: #d85c5c;"
          >移除</button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { open } from '@tauri-apps/plugin-dialog'
import { useProjectStore } from '../stores/projectStore'

const projectStore = useProjectStore()
const error = ref<string | null>(null)
const successMsg = ref<string | null>(null)

function clearMessages() {
  error.value = null
  successMsg.value = null
}

async function openFolderPicker() {
  clearMessages()
  const selected = await open({ directory: true, multiple: false })
  if (!selected || Array.isArray(selected)) return
  try {
    const info = await projectStore.addProject(selected)
    successMsg.value = `已成功加入專案「${info.name}」。`
    setTimeout(() => { successMsg.value = null }, 4000)
  } catch (e: any) {
    error.value = e?.message ?? String(e)
  }
}

async function initProject(projectId: string) {
  clearMessages()
  try {
    const result = await projectStore.initProject(projectId)
    successMsg.value = `初始化完成，建立了 ${result.createdDirs.length} 個目錄。`
    setTimeout(() => { successMsg.value = null }, 4000)
  } catch (e: any) {
    error.value = e?.message ?? String(e)
  }
}

async function removeProject(projectId: string) {
  clearMessages()
  if (!confirm('確定要移除此專案？（只移除監控，不刪除專案檔案）')) return
  try {
    await projectStore.removeProject(projectId)
  } catch (e: any) {
    error.value = e?.message ?? String(e)
  }
}
</script>
