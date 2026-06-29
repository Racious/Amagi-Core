<template>
  <div class="space-y-6">
    <!-- 頁首 -->
    <div class="flex items-center justify-between">
      <div>
        <h1 class="page-title mb-1">引導式執行</h1>
        <p class="page-sub">AMAGI 分步驅動 AI，一步一回報，不跳步</p>
      </div>
    </div>

    <!-- 專案選擇 -->
    <div class="card p-4">
      <label class="text-xs font-bold uppercase tracking-wider text-muted">專案</label>
      <select v-model="selectedId" @change="onProjectChange"
              class="select mt-2">
        <option :value="null" disabled>請選擇專案…</option>
        <option v-for="p in initializedProjects" :key="p.id" :value="p.id">{{ p.name }}</option>
      </select>
    </div>

    <div v-if="error" class="alert tone-danger">
      {{ error }}
    </div>

    <!-- ① 沒有進行中的流程 → 開始表單 -->
    <div v-if="selectedId && !activeRun"
         class="card p-5 space-y-4">
      <div>
        <label class="text-xs font-bold uppercase tracking-wider text-muted">流程類型</label>
        <div class="flex gap-2 mt-2">
          <button v-for="wf in workflowTypes" :key="wf.id"
                  @click="chosenWorkflow = wf.id"
                  class="btn btn-ghost btn-sm"
                  :class="chosenWorkflow === wf.id ? 'border-accent' : ''"
                  :style="chosenWorkflow === wf.id ? 'background: var(--c-accent-soft)' : ''">
            {{ wf.label }}
          </button>
        </div>
      </div>
      <div>
        <label class="text-xs font-bold uppercase tracking-wider text-muted">任務描述</label>
        <textarea v-model="taskInput" rows="3"
                  placeholder="例如：新增悔棋功能，讓玩家可以撤回上一步棋"
                  class="input mt-2"></textarea>
      </div>
      <button @click="startRun" :disabled="!canStart || busy"
              class="btn btn-primary disabled:opacity-50">
        {{ busy ? '建立中…' : '▶ 開始流程' }}
      </button>
    </div>

    <!-- ② 進行中 / 已完成 -->
    <div v-if="activeRun" class="space-y-4">
      <!-- 流程標頭 -->
      <div class="card p-4">
        <div class="flex items-center justify-between">
          <div>
            <span class="font-semibold text-fg">{{ activeRun.workflowName }}</span>
            <span class="pill ml-2" :class="statusTone(activeRun.status)">{{ statusLabel(activeRun.status) }}</span>
          </div>
          <button @click="cancelRun" :disabled="busy"
                  class="btn btn-danger btn-sm">中止</button>
        </div>
        <div class="text-sm mt-1 text-muted">{{ activeRun.task }}</div>
      </div>

      <!-- 步驟時間軸 -->
      <div class="card p-4 space-y-2">
        <div v-for="(step, i) in activeRun.steps" :key="step.id"
             class="flex items-start gap-3 p-2 rounded-xl"
             :style="i === activeRun.currentStep && activeRun.status === 'awaitingResult'
               ? 'background: var(--c-accent-soft)' : ''">
          <span class="text-base mt-0.5">{{ stepIcon(step.status) }}</span>
          <div class="flex-1">
            <div class="text-sm font-medium text-fg">
              步驟 {{ i + 1 }}：{{ step.name }}
            </div>
            <div v-if="step.result" class="text-xs mt-1 p-2 rounded-lg whitespace-pre-wrap bg-surface-2 text-muted"
                 style="max-height:120px; overflow:auto;">{{ step.result }}</div>
          </div>
        </div>
      </div>

      <!-- 當前步驟操作 -->
      <div v-if="activeRun.status === 'awaitingResult'"
           class="card p-5 space-y-3 border-accent">
        <div class="text-sm font-bold" style="color: var(--c-accent);">
          目前步驟：{{ currentStep?.name }}
        </div>
        <div class="text-sm text-fg">{{ currentStep?.instruction }}</div>

        <div class="alert tone-warning">
          <div class="font-bold mb-1">👉 請對 Claude / Codex 說：</div>
          <code class="block p-2 rounded-lg text-xs font-mono bg-surface-2 text-fg">{{ instructionLine }}</code>
          <button @click="copyInstruction" class="btn btn-ghost btn-sm mt-2">
            {{ copied ? '✓ 已複製' : '複製指令' }}
          </button>
        </div>

        <button @click="advance" :disabled="busy"
                class="btn btn-primary w-full disabled:opacity-50">
          {{ busy ? '讀取中…' : '✅ AI 已完成，讀取結果並推進' }}
        </button>
        <div class="text-xs text-center text-subtle">
          AMAGI 會讀取 <code>.amagi/state/result.md</code>，記錄後推進到下一步
        </div>
      </div>

      <!-- 完成 -->
      <div v-else-if="activeRun.status === 'done'"
           class="card p-5 text-center alert tone-success">
        <div class="text-3xl mb-2">🎉</div>
        <div class="font-bold" style="color: var(--c-success);">流程完成</div>
        <div class="text-sm mt-1 text-muted">所有步驟都已執行完畢</div>
        <div class="flex gap-2 justify-center mt-4">
          <RouterLink to="/review" class="btn btn-primary btn-sm">前往審核 →</RouterLink>
          <button @click="resetRun" class="btn btn-ghost btn-sm">開始新流程</button>
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
function statusTone(s: BridgeRunStatus) {
  if (s === 'done') return 'tone-success'
  if (s === 'cancelled') return 'tone-danger'
  return 'tone-warning'
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
    error.value = (e as any)?.message ?? String(e)
  }
}

async function startRun() {
  if (!selectedId.value || !canStart.value) return
  busy.value = true
  error.value = null
  try {
    activeRun.value = await api.startBridgeRun(selectedId.value, chosenWorkflow.value, taskInput.value.trim())
  } catch (e) {
    error.value = (e as any)?.message ?? String(e)
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
    error.value = (e as any)?.message ?? String(e)
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
    error.value = (e as any)?.message ?? String(e)
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
