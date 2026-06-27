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

    <!-- 選擇性分發（訂閱矩陣）─ 取代「一鍵全撒」 -->
    <div class="card p-5 mt-8">
      <div class="font-semibold text-sm mb-1 text-fg">技能分發（選擇性）</div>
      <p class="text-xs text-muted mb-3">
        勾選「哪個技能 → 去哪個目標」再按套用。<b>全域</b>＝本機所有專案共用
        （<code>~/.codex/skills</code>、<code>~/.claude/skills</code>）；或指定個別專案。
        逐項勾選，避免一鍵誤分發。
      </p>

      <div v-if="library.length === 0" class="text-sm text-muted">
        vault <code>_skills/</code> 尚無技能（請先設定 vault 路徑並在技能庫放入技能）。
      </div>

      <div v-else class="overflow-x-auto">
        <table class="text-sm w-full">
          <thead>
            <tr class="text-muted border-b border-[var(--c-border)]">
              <th class="text-left py-2 pr-3">技能</th>
              <th v-for="t in targets" :key="t.key"
                  class="px-2 py-2 text-center whitespace-nowrap">{{ t.label }}</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="s in library" :key="s.slug"
                class="border-b border-[var(--c-border)]">
              <td class="py-2 pr-3 text-fg whitespace-nowrap">
                {{ s.name }} <span class="text-muted text-xs">({{ s.slug }})</span>
              </td>
              <td v-for="t in targets" :key="t.key" class="px-2 py-2 text-center">
                <input type="checkbox" v-model="checked[s.slug][t.key]" />
              </td>
            </tr>
          </tbody>
        </table>
      </div>

      <div class="mt-3 flex items-center gap-3">
        <button class="btn btn-primary btn-sm"
                :disabled="busy || library.length === 0"
                @click="applyDistribution">
          {{ busy ? '分發中…' : '套用分發' }}
        </button>
        <span v-if="distMsg" class="text-xs" style="color: var(--c-accent);">{{ distMsg }}</span>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, reactive, ref, onMounted } from 'vue'
import { RouterLink } from 'vue-router'
import { useReviewStore } from '../stores/reviewStore'
import StatusBadge from '../components/StatusBadge.vue'
import { api, type LibrarySkill, type ProjectInfo } from '../api/tauriCommands'

const reviewStore = useReviewStore()
const skills = computed(() =>
  reviewStore.items.filter(i => i.itemType === 'skill')
)

// ── 選擇性分發 ────────────────────────────────────
const library = ref<LibrarySkill[]>([])
const projects = ref<ProjectInfo[]>([])
const busy = ref(false)
const distMsg = ref('')
const checked = reactive<Record<string, Record<string, boolean>>>({})

const targets = computed(() => [
  { key: 'global', label: '全域（本機共用）' },
  ...projects.value.map(p => ({ key: p.path, label: p.name })),
])

onMounted(async () => {
  try {
    const [libs, projs] = await Promise.all([api.listLibrarySkills(), api.listProjects()])
    // 先建好勾選矩陣，再賦值給 reactive，避免渲染時 checked 尚未就緒
    const targetKeys = ['global', ...projs.map(p => p.path)]
    for (const s of libs) {
      checked[s.slug] = {}
      for (const k of targetKeys) checked[s.slug][k] = false
    }
    library.value = libs
    projects.value = projs
  } catch (e: any) {
    distMsg.value = `載入失敗：${e?.message ?? e}`
  }
})

async function applyDistribution() {
  const selections: { skillSlug: string; target: string }[] = []
  for (const s of library.value) {
    const row = checked[s.slug] || {}
    for (const t of targets.value) {
      if (row[t.key]) selections.push({ skillSlug: s.slug, target: t.key })
    }
  }
  if (selections.length === 0) {
    distMsg.value = '請先勾選要分發的項目'
    return
  }
  busy.value = true
  distMsg.value = ''
  try {
    const r = await api.distributeSkillsSelective(selections)
    distMsg.value = `已分發 ${r.skillCount} 個技能到 ${r.repoCount} 個目標（寫入 ${r.writtenCount} 檔）。`
  } catch (e: any) {
    distMsg.value = `分發失敗：${e?.message ?? e}`
  } finally {
    busy.value = false
  }
}
</script>
