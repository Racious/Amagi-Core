<template>
  <div class="card card-hover p-4" @click="$emit('select')">
    <div class="flex items-center gap-3 mb-3">
      <div class="w-9 h-9 rounded-md flex items-center justify-center text-white font-semibold text-sm flex-shrink-0"
           style="background: var(--c-accent);">
        {{ project.name.charAt(0).toUpperCase() }}
      </div>
      <div class="flex-1 min-w-0">
        <div class="font-semibold text-sm truncate text-fg">{{ project.name }}</div>
        <div class="text-xs truncate text-muted">{{ project.path }}</div>
      </div>
    </div>
    <div class="flex flex-wrap gap-1.5">
      <span v-if="project.currentBranch" class="pill tone-accent">{{ project.currentBranch }}</span>
      <span v-if="project.initialized" class="pill tone-success">已初始化</span>
      <span v-if="project.pendingReviewCount > 0" class="pill tone-warning">
        {{ project.pendingReviewCount }} 待審核
      </span>
    </div>
  </div>
</template>

<script setup lang="ts">
import type { ProjectInfo } from '../api/tauriCommands'

defineProps<{ project: ProjectInfo }>()
defineEmits<{ select: [] }>()
</script>
