<template>
  <div class="space-y-6">
    <!-- 頁首 -->
    <div class="flex items-center justify-between">
      <div>
        <h1 class="text-2xl font-bold" style="color: #2e2a3f;">引導式執行</h1>
        <p class="text-sm mt-1" style="color: #6f6883;">AMAGI 分步驅動 AI，一步一回報，不跳步</p>
      </div>
    </div>

    <!-- 專案選擇 -->
    <div class="rounded-2xl border p-4" style="background:white; border-color:#ded6f5;">
      <label class="text-xs font-bold uppercase tracking-wider" style="color:#6f6883;">專案</label>
      <select v-model="selectedId" @change="onProjectChange"
              class="mt-2 w-full px-3 py-2 rounded-xl border text-sm"
              style="border-color:#ded6f5; color:#2e2a3f;">
        <option :value="null" disabled>請選擇專案…</option>
        <option v-for="p in initializedProjects" :key="p.id" :value="p.id">{{ p.name }}</option>
      </select>
    </div>

    <div v-if="error" class="p-4 rounded-xl border text-sm" style="background:#fff0f0; border-color:#ffb3b3; color:#c0392b;">
      {{ error }}
    </div>

    <!-- ① 沒有進行中的流程 → 開始表單 -->
    <div v-if="selectedId && !activeRun"
         class="rounded-2xl border p-5 space-y-4" style="background:white; border-color:#ded6f5;">
      <div>
        <label class="text-xs font-bold uppercase tracking-wider" style="color:#6f6883;">流程類型</label>
        <div class="flex gap-2 mt-2">
          <button v-for="wf in workflowTypes" :key="wf.id"
                  @click="chosenWorkflow = wf.id"
                  class="px-4 py-2 rounded-xl text-sm font-medium border transition"
                  :style="chosenWorkflow === wf.id
                    ? 'background:#7c5cff; color:white; border-color:#7c5cff;'
                    : 'background:white; color:#6f6883; border-color:#ded6f5;'">
            {{ wf.label }}
          </button>
        </div>
      </div>
      <div>
        <label class="text-xs font-bold uppercase tracking-wider" style="color:#6f6883;">任務描述</label>
        <textarea v-model="taskInput" rows="3"
                  placeholder="例如：新增悔棋功能，讓玩家可以撤回上一步棋"
                  class="mt-2 w-full px-3 py-2 rounded-xl border text-sm"
                  style="border-color:#ded6f5; color:#2e2a3f;"></textarea>
      </div>
      <button @click="startRun" :disabled="!canStart || busy"
              class="px-4 py-2 rounded-xl text-sm font-bold text-white disabled:opacity-50"
              style="background: linear-gradient(135deg, #7c5cff, #9b7fff);">
        {{ busy ? '建立中…' : '▶ 開始流程' }}
      </button>
    </div>

    <!-- ② 進行中 / 已完成 -->
    <div v-if="activeRun" class="space-y-4">
      <!-- 流程標頭 -->
      <div class="rounded-2xl border p-4" style="background:#f4f0ff; border-color:#ded6f5;">
        <div class="flex items-center justify-between">
          <div>
            <span class="font-semibold" style="color:#2e2a3f;">{{ activeRun.workflowName }}</span>
            <span class="text-xs ml-2 px-2 py-0.5 rounded-full"
                  :style="statusStyle(activeRun.status)">{{ statusLabel(activeRun.status) }}</span>
          </div>
          <button @click="cancelRun" :disabled="busy"
                  class="text-xs px-2 py-1 rounded-lg" style="background:#fff0f0; color:#d85c5c;">中止</button>
        </div>
        <div class="text-sm mt-1" style="color:#6f6883;">{{ activeRun.task }}</div>
      </div>

      <!-- 步驟時間軸 -->
      <div class="rounded-2xl border p-4 space-y-2" style="background:white; border-color:#ded6f5;">
        <div v-for="(step, i) in activeRun.steps" :key="step.id"
             class="flex items-start gap-3 p-2 rounded-xl"
             :style="i === activeRun.currentStep && activeRun.status === 'awaitingResult'
               ? 'background:#f4f0ff;' : ''">
          <span class="text-base mt-0.5">{{ stepIcon(step.status) }}</span>
          <div class="flex-1">
            <div class="text-sm font-medium" style="color:#2e2a3f;">
              步驟 {{ i + 1 }}：{{ step.name }}
            </div>
            <div v-if="step.result" class="text-xs mt-1 p-2 rounded-lg whitespace-pre-wrap"
                 style="background:#f7f5ff; color:#6f6883; max-height:120px; overflow:auto;">{{ step.result }}</div>
          </div>
        </div>
      </div>

      <!-- 當前步驟操作 -->
      <div v-if="activeRun.status === 'awaitingResult'"
           class="rounded-2xl border p-5 space-y-3" style="background:white; border-color:#7c5cff;">
        <div class="text-sm font-bold" style="color:#5037c9;">
          目前步驟：{{ currentStep?.name }}
        </div>
        <div class="text-sm" style="color:#2e2a3f;">{{ currentStep?.instruction }}</div>

        <div class="p-3 rounded-xl text-sm" style="background:#fffbea; color:#705100;">
          <div class="font-bold mb-1">👉 請對 Claude / Codex 說：</div>
          <code class="block p-2 rounded-lg text-xs" style="background:#2b2736; color:#f7f4ee;">{{ instructionLine }}</code>
          <button @click="copyInstruction" class="mt-2 text-xs px-2 py-1 rounded-lg" style="background:#eee8ff; color:#5037c9;">
            {{ copied ? '✓ 已複製' : '複製指令' }}
          </button>
        </div>

        <button @click="advance" :disabled="busy"
                class="w-full px-4 py-2.5 rounded-xl text-sm font-bold text-white disabled:opacity-50"
                style="background: linear-gradient(135deg, #3cae78, #2d9d6a);">
          {{ busy ? '讀取中…' : '✅ AI 已完成，讀取結果並推進' }}
        </button>
        <div class="text-xs text-center" style="color:#9f97b5;">
          AMAGI 會讀取 <code>.amagi/state/result.md</code>，記錄後推進到下一步
        </div>
      </div>

      <!-- 完成 -->
      <div v-else-if="activeRun.status === 'done'"
           class="rounded-2xl border p-5 text-center" style="background:#ecf8ec; border-color:#cce8ce;">
        <div class="text-3xl mb-2">🎉</div>
        <div class="font-bold" style="color:#1f7a4d;">流程完成</div>
        <div class="text-sm mt-1" style="color:#6f6883;">所有步驟都已執行完畢</div>
        <div class="flex gap-2 justify-center mt-4">
          <RouterLink to="/review" class="text-xs px-3 py-1.5 rounded-lg font-medium text-white" style="background:#7c5cff;">前往審核 →</RouterLink>
          <button @click="resetRun" class="text-xs px-3 py-1.5 rounded-lg font-medium" style="background:#eee8ff; color:#5037c9;">開始新流程</button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { api, type BridgeRun, type BridgeRunStatus, type BridgeStepStatus } from '../api/tauriCommands'
import { useProjectStore } from '../stores/projectStore'

const projectStore = useProjectStore()

const selectedId = ref<string | null>(null)
const activeRun = ref<BridgeRun | null>(null)
const chosenWorkflow = ref('feature-dev')
const taskInput = ref('')
const busy = ref(false)
const error = ref<string | null>(null)
const copied = ref(false)

const workflowTypes = [
  { id: 'feature-dev', label: '功能開發' },
  { id: 'bug-fix', label: 'Bug 修復' },
]

const initializedProjects = computed(() => projectStore.projects.filter(p => p.initialized))
const canStart = computed(() => !!selectedId.value && taskInput.value.trim().length > 0)
const currentStep = computed(() =>
  activeRun.value ? activeRun.value.steps[activeRun.value.currentStep] : null)
const instructionLine = computed(() =>
  '讀取 .amagi/state/next-step.md 執行，完成後把結果寫進 .amagi/state/result.md')

function stepIcon(s: BridgeStepStatus) {
  return s === 'done' ? '✅' : s === 'active' ? '🔵' : '⚪'
}
function statusLabel(s: BridgeRunStatus) {
  return s === 'awaitingResult' ? '進行中' : s === 'done' ? '已完成' : '已中止'
}
function statusStyle(s: BridgeRunStatus) {
  if (s === 'done') return 'background:#ecf8ec; color:#1f7a4d;'
  if (s === 'cancelled') return 'background:#fff0f0; color:#d85c5c;'
  return 'background:#fff4db; color:#916216;'
}

async function onProjectChange() {
  activeRun.value = null
  error.value = null
  if (selectedId.value) await loadRun()
}

async function loadRun() {
  if (!selectedId.value) return
  try {
    const run = await api.getBridgeRun(selectedId.value)
    // 只有進行中的才顯示為 active；已完成的也顯示（讓使用者看到結果）
    activeRun.value = run && run.status !== 'cancelled' ? run : null
  } catch (e) {
    error.value = String(e)
  }
}

async function startRun() {
  if (!selectedId.value || !canStart.value) return
  busy.value = true
  error.value = null
  try {
    activeRun.value = await api.startBridgeRun(selectedId.value, chosenWorkflow.value, taskInput.value.trim())
  } catch (e) {
    error.value = String(e)
  } finally {
    busy.value = false
  }
}

async function advance() {
  if (!selectedId.value) return
  busy.value = true
  error.value = null
  try {
    activeRun.value = await api.advanceBridgeRun(selectedId.value)
  } catch (e) {
    error.value = String(e)
  } finally {
    busy.value = false
  }
}

async function cancelRun() {
  if (!selectedId.value) return
  busy.value = true
  try {
    await api.cancelBridgeRun(selectedId.value)
    activeRun.value = null
  } catch (e) {
    error.value = String(e)
  } finally {
    busy.value = false
  }
}

function resetRun() {
  activeRun.value = null
  taskInput.value = ''
}

async function copyInstruction() {
  try {
    await navigator.clipboard.writeText(instructionLine.value)
    copied.value = true
    setTimeout(() => (copied.value = false), 1500)
  } catch { /* ignore */ }
}

onMounted(async () => {
  if (projectStore.projects.length === 0) await projectStore.fetchProjects()
  selectedId.value = projectStore.selectedProjectId ?? initializedProjects.value[0]?.id ?? null
  if (selectedId.value) await loadRun()
})
</script>
