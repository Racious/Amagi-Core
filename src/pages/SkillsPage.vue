<template>
  <div>
    <div class="mb-6">
      <h1 class="text-2xl font-bold mb-1" style="color: #201b34;">技能管理</h1>
      <p class="text-sm" style="color: #6f6883;">已接受的技能候選項，將同步為原生 Skills（.claude/skills 與 .codex/skills），AI 會自動觸發</p>
    </div>

    <div v-if="skills.length === 0"
         class="rounded-2xl p-8 text-center border"
         style="background: white; border-color: #ded6f5; border-style: dashed;">
      <div class="text-4xl mb-3">⚡</div>
      <div class="font-bold mb-1" style="color: #2e2a3f;">尚無技能</div>
      <div class="text-sm mb-3" style="color: #6f6883;">
        在「審核佇列」接受技能候選項後，技能會出現在這裡。
      </div>
      <RouterLink to="/review"
        class="inline-block px-4 py-2 rounded-xl text-sm font-bold text-white"
        style="background: #7c5cff;">前往審核佇列</RouterLink>
    </div>

    <div v-else class="space-y-3">
      <div v-for="skill in skills" :key="skill.id"
           class="rounded-2xl p-4 border"
           style="background: white; border-color: #ded6f5;">
        <div class="flex items-start justify-between gap-3">
          <div class="flex-1 min-w-0">
            <div class="flex items-center gap-2 mb-1">
              <span class="text-sm font-bold" style="color: #2e2a3f;">{{ skill.title }}</span>
              <StatusBadge :status="skill.status" />
            </div>
            <div class="text-xs mb-2" style="color: #6f6883;">{{ skill.content }}</div>
            <div class="flex flex-wrap gap-1">
              <span v-for="target in skill.syncTargets" :key="target"
                    class="text-xs px-2 py-0.5 rounded-full"
                    style="background: #eee8ff; color: #5037c9;">{{ target }}</span>
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
