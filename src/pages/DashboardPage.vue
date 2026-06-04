<template>
  <div>
    <div class="mb-6">
      <h1 class="page-title mb-1">總覽</h1>
      <p class="page-sub">所有受監控專案的狀態一覽</p>
    </div>

    <!-- 統計卡片 -->
    <div class="grid grid-cols-3 gap-4 mb-6">
      <div class="card p-4">
        <div class="text-xs font-medium mb-1.5 text-muted">已監控專案</div>
        <div class="text-2xl font-semibold text-fg">{{ projectStore.projects.length }}</div>
      </div>
      <div class="card p-4">
        <div class="text-xs font-medium mb-1.5 text-muted">待審核候選</div>
        <div class="text-2xl font-semibold" style="color: var(--c-warning);">{{ reviewStore.pendingCount }}</div>
      </div>
      <div class="card p-4">
        <div class="text-xs font-medium mb-1.5 text-muted">已接受記憶</div>
        <div class="text-2xl font-semibold" style="color: var(--c-success);">{{ reviewStore.acceptedItems.length }}</div>
      </div>
    </div>

    <!-- 待審核提示 -->
    <div v-if="reviewStore.pendingCount > 0" class="alert tone-warning mb-6">
      <div class="font-semibold text-sm mb-1">有 {{ reviewStore.pendingCount }} 個候選記憶待審核</div>
      <div class="text-sm text-muted mb-2.5">前往「審核佇列」確認並接受或忽略候選項目。</div>
      <RouterLink to="/review" class="btn btn-primary btn-sm">前往審核</RouterLink>
    </div>

    <!-- 空狀態 -->
    <div v-if="projectStore.projects.length === 0" class="card card-dashed p-8 text-center">
      <div class="text-3xl mb-3 opacity-70">📁</div>
      <div class="font-semibold mb-1.5 text-fg">尚未加入任何專案</div>
      <div class="text-sm mb-4 text-muted">前往「專案管理」加入您的 Git 專案。</div>
      <RouterLink to="/projects" class="btn btn-primary">新增第一個專案</RouterLink>
    </div>

    <div v-else class="grid grid-cols-2 gap-4">
      <ProjectCard
        v-for="project in projectStore.projects"
        :key="project.id"
        :project="project"
        @select="projectStore.selectedProjectId = project.id"
      />
    </div>
  </div>
</template>

<script setup lang="ts">
import { RouterLink } from 'vue-router'
import { useProjectStore } from '../stores/projectStore'
import { useReviewStore } from '../stores/reviewStore'
import ProjectCard from '../components/ProjectCard.vue'

const projectStore = useProjectStore()
const reviewStore = useReviewStore()
</script>
