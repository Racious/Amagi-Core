<template>
  <div>
    <div class="mb-6">
      <h1 class="text-2xl font-bold mb-1" style="color: #201b34;">總覽</h1>
      <p class="text-sm" style="color: #6f6883;">所有受監控專案的狀態一覽</p>
    </div>

    <!-- 統計卡片 -->
    <div class="grid grid-cols-3 gap-4 mb-6">
      <div class="rounded-2xl p-4 border" style="background: white; border-color: #ded6f5;">
        <div class="text-xs font-bold mb-1" style="color: #6f6883;">已監控專案</div>
        <div class="text-2xl font-bold" style="color: #7c5cff;">{{ projectStore.projects.length }}</div>
      </div>
      <div class="rounded-2xl p-4 border" style="background: white; border-color: #ded6f5;">
        <div class="text-xs font-bold mb-1" style="color: #6f6883;">待審核候選</div>
        <div class="text-2xl font-bold" style="color: #d28b19;">{{ reviewStore.pendingCount }}</div>
      </div>
      <div class="rounded-2xl p-4 border" style="background: white; border-color: #ded6f5;">
        <div class="text-xs font-bold mb-1" style="color: #6f6883;">已接受記憶</div>
        <div class="text-2xl font-bold" style="color: #3cae78;">{{ reviewStore.acceptedItems.length }}</div>
      </div>
    </div>

    <!-- 待審核提示 -->
    <div v-if="reviewStore.pendingCount > 0"
         class="rounded-2xl p-4 mb-6 border"
         style="background: #fff7e8; border-color: #efd49b;">
      <div class="font-bold mb-1" style="color: #916216;">📋 有 {{ reviewStore.pendingCount }} 個候選記憶待審核</div>
      <div class="text-sm" style="color: #7b560e;">
        前往「審核佇列」頁面確認並接受或忽略候選項目。
      </div>
      <RouterLink to="/review"
        class="inline-block mt-2 px-3 py-1.5 rounded-xl text-xs font-bold text-white"
        style="background: #7c5cff;">前往審核</RouterLink>
    </div>

    <!-- 專案卡片 -->
    <div v-if="projectStore.projects.length === 0"
         class="rounded-2xl p-8 text-center border"
         style="background: white; border-color: #ded6f5; border-style: dashed;">
      <div class="text-4xl mb-3">📁</div>
      <div class="font-bold mb-2" style="color: #2e2a3f;">尚未加入任何專案</div>
      <div class="text-sm mb-4" style="color: #6f6883;">前往「專案管理」頁面加入您的 Git 專案。</div>
      <RouterLink to="/projects"
        class="inline-block px-4 py-2 rounded-xl text-sm font-bold text-white"
        style="background: #7c5cff;">新增第一個專案</RouterLink>
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
