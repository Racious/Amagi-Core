<template>
  <div>
    <div class="flex items-center justify-between mb-6">
      <div>
        <h1 class="page-title mb-1">專案管理</h1>
        <p class="page-sub">新增、初始化與管理受監控的 Git 專案</p>
      </div>
      <button @click="openFolderPicker" class="btn btn-primary">
        + 新增專案
      </button>
    </div>

    <!-- 錯誤訊息 -->
    <div v-if="error" class="alert tone-danger mb-4">
      <span class="text-sm">{{ error }}</span>
    </div>

    <!-- 成功訊息 -->
    <div v-if="successMsg" class="alert tone-success mb-4">
      <span class="text-sm">{{ successMsg }}</span>
    </div>

    <div v-if="projectStore.projects.length === 0"
         class="card card-dashed p-8 text-center">
      <div class="text-4xl mb-3">📂</div>
      <p class="text-sm text-muted">點擊「新增專案」選擇本機 Git 專案資料夾。</p>
    </div>

    <div v-else class="space-y-3">
      <div
        v-for="project in projectStore.projects"
        :key="project.id"
        class="card p-4 flex items-center gap-4"
      >
        <div class="flex-1 min-w-0">
          <div class="font-bold truncate text-fg">{{ project.name }}</div>
          <div class="text-xs truncate mt-0.5 text-muted">{{ project.path }}</div>
          <div class="text-xs truncate mt-0.5 text-muted">
            Vault：<span class="text-fg">{{ project.vaultFolder || '—' }}</span>
          </div>
          <div class="flex items-center gap-2 mt-1.5">
            <span v-if="project.currentBranch" class="pill tone-accent">
              {{ project.currentBranch }}
            </span>
            <span v-if="project.initialized" class="pill tone-success">
              已初始化
            </span>
            <span v-else class="pill tone-warning">
              未初始化
            </span>
          </div>
        </div>
        <div class="flex items-center gap-2 flex-shrink-0">
          <button
            v-if="!project.initialized"
            @click="initProject(project.id)"
            :disabled="projectStore.loading"
            class="btn btn-primary btn-sm"
          >初始化</button>
          <button
            @click="initVault(project.id)"
            :disabled="vaultBusy"
            class="btn btn-ghost btn-sm"
          >vault 資料夾</button>
          <button
            @click="removeProject(project.id)"
            class="btn btn-danger btn-sm"
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
import { api } from '../api/tauriCommands'

const projectStore = useProjectStore()
const error = ref<string | null>(null)
const successMsg = ref<string | null>(null)
const vaultBusy = ref(false)

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

async function initVault(projectId: string) {
  clearMessages()
  vaultBusy.value = true
  try {
    const r = await api.initProjectVault(projectId)
    successMsg.value = `已在 vault 建立知識資料夾（新增 ${r.createdDirs.length} 目錄、${r.createdFiles.length} 檔）。`
    setTimeout(() => { successMsg.value = null }, 4000)
  } catch (e: any) {
    error.value = e?.message ?? String(e)
  } finally {
    vaultBusy.value = false
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
