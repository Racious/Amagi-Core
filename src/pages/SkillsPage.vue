<template>
  <div>
    <div class="mb-6">
      <h1 class="page-title mb-1">技能管理</h1>
      <p class="page-sub">已接受的技能候選項，將同步為原生 Skills（.claude/skills 與 .codex/skills），AI 會自動觸發</p>
    </div>

    <div v-if="skills.length === 0"
         class="card card-dashed p-8 text-center">
      <div class="text-4xl mb-3">⚡</div>
      <div class="font-bold mb-1 text-fg">尚無技能</div>
      <div class="text-sm mb-3 text-muted">
        在「審核佇列」接受技能候選項後，技能會出現在這裡。
      </div>
      <RouterLink to="/review"
        class="btn btn-primary btn-sm">前往審核佇列</RouterLink>
    </div>

    <div v-else class="space-y-3">
      <div v-for="skill in skills" :key="skill.id"
           class="card p-4">
        <div class="flex items-start justify-between gap-3">
          <div class="flex-1 min-w-0">
            <div class="flex items-center gap-2 mb-1">
              <span class="text-sm font-bold text-fg">{{ skill.title }}</span>
              <StatusBadge :status="skill.status" />
            </div>
            <div class="text-xs mb-2 text-muted">{{ skill.content }}</div>
            <div class="flex flex-wrap gap-1">
              <span v-for="target in skill.syncTargets" :key="target"
                    class="pill tone-accent">{{ target }}</span>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { RouterLink } from 'vue-router'
import { useReviewStore } from '../stores/reviewStore'
import StatusBadge from '../components/StatusBadge.vue'

const reviewStore = useReviewStore()
const skills = computed(() =>
  reviewStore.items.filter(i => i.itemType === 'skill')
)
</script>
