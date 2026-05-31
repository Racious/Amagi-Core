<template>
  <div>
    <div class="mb-6 flex items-start justify-between gap-3">
      <div>
        <h1 class="text-2xl font-bold mb-1" style="color: #201b34;">技能管理</h1>
        <p class="text-sm" style="color: #6f6883;">已接受的技能候選項，將同步至 .codex/skills 與 .claude/commands</p>
      </div>
      <div class="text-right">
        <button @click="rebuildIndex" :disabled="!selectedProjectId || rebuilding"
                class="px-3 py-1.5 rounded-xl text-xs font-bold disabled:opacity-50"
                style="background: #eee8ff; color: #5037c9;"
                title="依現有技能重寫 CLAUDE.md / AGENTS.md 的技能索引">
          {{ rebuilding ? '重建中…' : '🔄 重建技能索引' }}
        </button>
        <div v-if="rebuildMsg" class="text-xs mt-1" :style="rebuildMsg.startsWith('✅') ? 'color:#1d7a51' : 'color:#d85c5c'">
          {{ rebuildMsg }}
        </div>
      </div>
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
import { computed, ref } from 'vue'
import { RouterLink } from 'vue-router'
import { useReviewStore } from '../stores/reviewStore'
import { useProjectStore } from '../stores/projectStore'
import { api } from '../api/tauriCommands'
import StatusBadge from '../components/StatusBadge.vue'

const reviewStore = useReviewStore()
const projectStore = useProjectStore()
const skills = computed(() =>
  reviewStore.items.filter(i => i.itemType === 'skill')
)

const selectedProjectId = computed(() => projectStore.selectedProjectId)
const rebuilding = ref(false)
const rebuildMsg = ref('')

async function rebuildIndex() {
  if (!selectedProjectId.value) return
  rebuilding.value = true
  rebuildMsg.value = ''
  try {
    await api.rebuildSkillIndex(selectedProjectId.value)
    rebuildMsg.value = '✅ 索引已重建'
  } catch (e) {
    rebuildMsg.value = '✗ ' + String(e)
  } finally {
    rebuilding.value = false
    setTimeout(() => (rebuildMsg.value = ''), 4000)
  }
}
</script>
