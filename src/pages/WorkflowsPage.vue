<template>
  <div class="space-y-6">
    <!-- 頁首 -->
    <div class="flex items-center justify-between">
      <div>
        <h1 class="page-title mb-1">工作流程</h1>
        <p class="page-sub">AI 輔助開發流程管理器</p>
      </div>
      <button
        @click="loadWorkflows"
        :disabled="loading"
        class="btn btn-primary"
        :class="loading ? 'opacity-50' : ''"
      >
        {{ loading ? '載入中…' : '🔄 重新掃描' }}
      </button>
    </div>

    <!-- 錯誤訊息 -->
    <div v-if="error" class="alert tone-danger">
      {{ error }}
    </div>

    <!-- 無專案提示 -->
    <div v-if="!loading && allProjectWorkflows.length === 0 && !error"
         class="card card-dashed p-8 text-center text-muted">
      <div class="text-4xl mb-3">🔧</div>
      <div class="font-medium mb-1 text-fg">尚未偵測到工作流程</div>
      <div class="text-sm">請先在專案管理頁加入專案，或在專案根目錄建立 <code class="px-1 rounded bg-surface-2 text-fg">.workflow/workflow.yaml</code></div>
    </div>

    <!-- 各專案工作流程 -->
    <template v-for="pw in allProjectWorkflows" :key="pw.projectId">
      <div class="card overflow-hidden">
        <!-- 專案標頭 -->
        <div class="px-5 py-3 flex items-center justify-between border-b border-border bg-surface-2">
          <div class="flex items-center gap-2">
            <span class="text-base">📁</span>
            <span class="font-semibold text-sm text-fg">{{ pw.projectName }}</span>
            <span class="pill tone-accent">
              {{ pw.workflows.length }} 個流程
            </span>
          </div>
          <span class="text-xs font-mono truncate max-w-xs text-subtle">{{ pw.projectPath }}</span>
        </div>

        <!-- 無 runner 警告 -->
        <div v-if="!pw.runnerPath" class="alert tone-warning rounded-none border-x-0 border-t-0">
          ⚠️ 未偵測到 <code>workflow-runner.js</code>，指令產生功能將不可用
        </div>

        <!-- 工作流程列表 -->
        <div class="divide-y divide-border">
          <div
            v-for="wf in pw.workflows"
            :key="wf.id"
            class="p-5"
          >
            <!-- 流程標題列 -->
            <div class="flex items-start justify-between gap-3 mb-3">
              <div>
                <div class="font-semibold text-fg">{{ wf.name }}</div>
                <div class="text-sm mt-0.5 text-muted">{{ wf.description }}</div>
              </div>
              <button
                @click="selectWorkflow(pw, wf)"
                class="btn btn-primary btn-sm flex-shrink-0"
              >
                啟動
              </button>
            </div>

            <!-- 步驟時間軸 -->
            <div class="relative ml-3 space-y-0">
              <div
                v-for="(step, idx) in wf.steps"
                :key="step.id"
                class="flex items-start gap-3 py-2"
              >
                <!-- 連線 + 圓點 -->
                <div class="flex flex-col items-center flex-shrink-0" style="width:20px;">
                  <div
                    class="w-5 h-5 rounded-full flex items-center justify-center text-xs font-bold flex-shrink-0 bg-surface-2 text-muted"
                  >
                    {{ idx + 1 }}
                  </div>
                  <div
                    v-if="idx < wf.steps.length - 1"
                    class="w-px flex-1 mt-1 bg-border"
                    style="min-height:12px;"
                  ></div>
                </div>

                <!-- 步驟內容 -->
                <div class="flex-1 pb-1">
                  <div class="flex items-center gap-2 flex-wrap">
                    <span class="text-sm font-medium text-fg">{{ step.name }}</span>
                    <span
                      v-if="step.badge"
                      class="pill"
                      :class="step.requiresStop ? 'tone-danger' : 'tone-accent'"
                    >{{ step.badge }}</span>
                    <span
                      v-if="step.requiresStop"
                      class="pill tone-warning"
                    >⏸ 需人工確認</span>
                  </div>
                  <div class="text-xs mt-0.5 text-subtle">{{ step.description }}</div>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </template>

    <!-- ── 啟動面板（Modal 樣式） ── -->
    <Transition name="modal">
      <div
        v-if="selectedWorkflow"
        class="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/50"
        @click.self="closePanel"
      >
        <div class="card w-full max-w-xl shadow-2xl overflow-hidden">
          <!-- 面板標題 -->
          <div class="px-6 py-4 border-b border-border flex items-center justify-between bg-surface-2">
            <div>
              <div class="font-bold text-fg">{{ selectedWorkflow.wf.name }}</div>
              <div class="text-xs mt-0.5 text-muted">{{ selectedWorkflow.pw.projectName }}</div>
            </div>
            <button @click="closePanel" class="text-xl leading-none text-muted">✕</button>
          </div>

          <div class="p-6 space-y-5">
            <!-- 輸入欄位 -->
            <div v-if="selectedWorkflow.wf.inputs.length > 0" class="space-y-3">
              <div class="text-sm font-medium text-fg">流程參數</div>
              <div
                v-for="inp in selectedWorkflow.wf.inputs"
                :key="inp.key"
                class="space-y-1"
              >
                <label class="text-xs font-medium" style="color: var(--c-accent)">
                  {{ inp.label }}
                  <span v-if="inp.required" style="color: var(--c-danger)">*</span>
                </label>
                <input
                  v-model="inputValues[inp.key]"
                  :placeholder="inp.defaultValue ?? ''"
                  class="input"
                />
              </div>
            </div>
            <div v-else class="text-sm py-2 text-muted">此工作流程不需要額外參數。</div>

            <!-- 模式選擇 -->
            <div class="space-y-2">
              <div class="text-sm font-medium text-fg">執行模式</div>
              <div class="flex gap-2">
                <button
                  v-for="m in modes"
                  :key="m.value"
                  @click="selectedMode = m.value"
                  class="btn btn-sm flex-1"
                  :class="selectedMode === m.value ? 'btn-primary' : 'btn-ghost'"
                >
                  {{ m.label }}
                </button>
              </div>
            </div>

            <!-- 產生指令 -->
            <div v-if="generatedCommand" class="space-y-2">
              <div class="text-sm font-medium text-fg">執行指令</div>
              <div class="relative">
                <pre
                  class="card bg-surface-2 text-fg p-3 text-xs overflow-x-auto font-mono"
                  style="white-space:pre-wrap; word-break:break-all;"
                >{{ generatedCommand }}</pre>
                <button
                  @click="copyCommand"
                  class="btn btn-primary btn-sm absolute top-2 right-2"
                >
                  {{ copied ? '✓ 已複製' : '複製' }}
                </button>
              </div>
              <p class="text-xs text-muted">
                請在專案目錄下開啟終端機，貼上上方指令後執行。
              </p>
            </div>

            <!-- 按鈕列 -->
            <div class="flex gap-3 pt-2">
              <button
                @click="generate"
                :disabled="generating"
                class="btn btn-primary flex-1"
                :class="generating ? 'opacity-50' : ''"
              >
                {{ generating ? '產生中…' : '🚀 產生指令' }}
              </button>
              <button
                @click="closePanel"
                class="btn btn-ghost"
              >
                取消
              </button>
            </div>
          </div>
        </div>
      </div>
    </Transition>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, reactive } from 'vue'
import { api, type ProjectWorkflows, type WorkflowDefinition } from '../api/tauriCommands'

// ── 狀態 ──────────────────────────────────────────
const loading = ref(false)
const error = ref<string | null>(null)
const allProjectWorkflows = ref<ProjectWorkflows[]>([])

// ── 面板狀態 ──────────────────────────────────────
const selectedWorkflow = ref<{ pw: ProjectWorkflows; wf: WorkflowDefinition } | null>(null)
const inputValues = reactive<Record<string, string>>({})
const selectedMode = ref<'plan' | 'run' | 'command'>('command')
const generatedCommand = ref<string | null>(null)
const generating = ref(false)
const copied = ref(false)

const modes: { value: 'plan' | 'run' | 'command'; label: string }[] = [
  { value: 'command', label: '📋 產生指令' },
  { value: 'plan', label: '🗺️ Plan 模式' },
  { value: 'run', label: '▶️ Run 模式' },
]

// ── 載入 ──────────────────────────────────────────
async function loadWorkflows() {
  loading.value = true
  error.value = null
  try {
    allProjectWorkflows.value = await api.listAllWorkflows()
  } catch (e: unknown) {
    error.value = (e as any)?.message ?? String(e)
  } finally {
    loading.value = false
  }
}

// ── 選擇工作流程 ──────────────────────────────────
function selectWorkflow(pw: ProjectWorkflows, wf: WorkflowDefinition) {
  selectedWorkflow.value = { pw, wf }
  generatedCommand.value = null
  copied.value = false

  // 預填預設值
  Object.keys(inputValues).forEach(k => delete inputValues[k])
  for (const inp of wf.inputs) {
    if (inp.defaultValue) inputValues[inp.key] = inp.defaultValue
  }
}

function closePanel() {
  selectedWorkflow.value = null
  generatedCommand.value = null
  copied.value = false
}

// ── 產生指令 ──────────────────────────────────────
async function generate() {
  if (!selectedWorkflow.value) return

  const { pw, wf } = selectedWorkflow.value

  // 檢查必填欄位
  for (const inp of wf.inputs) {
    if (inp.required && !inputValues[inp.key]?.trim()) {
      error.value = `「${inp.label}」為必填欄位`
      return
    }
  }
  error.value = null

  if (!pw.runnerPath) {
    generatedCommand.value = `# 未找到 workflow-runner.js\n# 請確認專案根目錄或 .workflow/ 資料夾中存在 workflow-runner.js`
    return
  }

  generating.value = true
  try {
    // 合併輸入（帶入預設值）
    const merged: Record<string, string> = {}
    for (const inp of wf.inputs) {
      merged[inp.key] = inputValues[inp.key] ?? inp.defaultValue ?? ''
    }

    generatedCommand.value = await api.generateWorkflowCommand(
      pw.runnerPath,
      wf.id,
      merged,
      selectedMode.value,
    )
  } catch (e: unknown) {
    error.value = (e as any)?.message ?? String(e)
  } finally {
    generating.value = false
  }
}

// ── 複製指令 ──────────────────────────────────────
async function copyCommand() {
  if (!generatedCommand.value) return
  try {
    await navigator.clipboard.writeText(generatedCommand.value)
    copied.value = true
    setTimeout(() => { copied.value = false }, 2000)
  } catch {
    // 瀏覽器不支援 clipboard API 時靜默處理
  }
}

onMounted(loadWorkflows)
</script>

<style scoped>
.modal-enter-active,
.modal-leave-active {
  transition: opacity 0.2s ease;
}
.modal-enter-from,
.modal-leave-to {
  opacity: 0;
}
</style>
