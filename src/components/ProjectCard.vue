<template>
  <div class="rounded-2xl p-4 border cursor-pointer transition-shadow hover:shadow-md"
       style="background: white; border-color: #ded6f5;"
       @click="$emit('select')">
    <div class="flex items-center gap-3 mb-3">
      <div class="w-9 h-9 rounded-xl flex items-center justify-center text-white font-bold text-sm"
           style="background: linear-gradient(135deg, #7c5cff, #b197ff);">
        {{ project.name.charAt(0).toUpperCase() }}
      </div>
      <div class="flex-1 min-w-0">
        <div class="font-bold text-sm truncate" style="color: #2e2a3f;">{{ project.name }}</div>
        <div class="text-xs truncate" style="color: #6f6883;">{{ project.path }}</div>
      </div>
    </div>
    <div class="flex flex-wrap gap-1.5">
      <span v-if="project.currentBranch"
            class="text-xs px-2 py-0.5 rounded-full"
            style="background: #eee8ff; color: #5037c9;">{{ project.currentBranch }}</span>
      <span v-if="project.initialized"
            class="text-xs px-2 py-0.5 rounded-full"
            style="background: #eefaf4; color: #1d7a51;">已初始化</span>
      <span v-if="project.pendingReviewCount > 0"
            class="text-xs px-2 py-0.5 rounded-full"
            style="background: #fff4db; color: #916216;">{{ project.pendingReviewCount }} 待審核</span>
    </div>
  </div>
</template>

<script setup lang="ts">
import type { ProjectInfo } from '../api/tauriCommands'

defineProps<{ project: ProjectInfo }>()
defineEmits<{ select: [] }>()
</script>
