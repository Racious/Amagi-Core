<template>
  <div class="space-y-6">
    <!-- 頁首 -->
    <div class="flex items-center justify-between">
      <div>
        <h1 class="text-2xl font-bold" style="color: #2e2a3f;">工作流程</h1>
        <p class="text-sm mt-1" style="color: #6f6883;">AI 輔助開發流程管理器</p>
      </div>
      <button
        @click="loadWorkflows"
        :disabled="loading"
        class="px-4 py-2 rounded-xl text-sm font-medium text-white transition-opacity"
        style="background: linear-gradient(135deg, #7c5cff, #9b7fff);"
        :class="loading ? 'opacity-50' : 'hover:opacity-90'"
      >
        {{ loading ? '載入中…' : '🔄 重新掃描' }}
      </button>
    </div>

    <!-- 錯誤訊息 -->
    <div v-if="error" class="p-4 rounded-xl border text-sm" style="background:#fff0f0; border-color:#ffb3b3; color:#c0392b;">
      {{ error }}
    </div>

    <!-- 無專案提示 -->
    <div v-if="!loading && allProjectWorkflows.length === 0 && !error"
         class="p-8 rounded-2xl border-2 border-dashed text-center"
         style="border-color: #ded6f5; color: #6f6883;">
      <div class="text-4xl mb-3">🔧</div>
      <div class="font-medium mb-1">尚未偵測到工作流程</div>
      <div class="text-sm">請先在專案管理頁加入專案，或在專案根目錄建立 <code class="px-1 rounded" style="background:#eee8ff;">.workflow/workflow.yaml</code></div>
    </div>

    <!-- 各專案工作流程 -->
    <template v-for="pw in allProjectWorkflows" :key="pw.projectId">
      <div class="rounded-2xl border shadow-sm overflow-hidden" style="background:white; border-color:#ded6f5;">
        <!-- 專案標頭 -->
        <div class="px-5 py-3 flex items-center justify-between border-b" style="background:#f4f0ff; border-color:#ded6f5;">
          <div class="flex items-center gap-2">
            <span class="text-base">📁</span>
            <span class="font-semibold text-sm" style="color:#2e2a3f;">{{ pw.projectName }}</span>
            <span class="text-xs px-2 py-0.5 rounded-full" style="background:#eee8ff; color:#5037c9;">
              {{ pw.workflows.length }} 個流程
            </span>
          </div>
          <span class="text-xs font-mono truncate max-w-xs" style="color:#9f97b5;">{{ pw.projectPath }}</span>
        </div>

        <!-- 無 runner 警告 -->
        <div v-if="!pw.runnerPath" class="px-5 py-3 text-sm" style="color:#b08a00; background:#fffbea;">
          ⚠️ 未偵測到 <code>workflow-runner.js</code>，指令產生功能將不可用
        </div>

        <!-- 工作流程列表 -->
        <div class="divide-y" style="border-color:#f0ebff;">
          <div
            v-for="wf in pw.workflows"
            :key="wf.id"
            class="p-5"
          >
            <!-- 流程標題列 -->
            <div class="flex items-start justify-between gap-3 mb-3">
              <div>
                <div class="font-semibold" style="color:#2e2a3f;">{{ wf.name }}</div>
                <div class="text-sm mt-0.5" style="color:#6f6883;">{{ wf.description }}</div>
              </div>
              <button
                @click="selectWorkflow(pw, wf)"
                class="flex-shrink-0 px-3 py-1.5 rounded-lg text-xs font-medium text-white transition-opacity"
                style="background: linear-gradient(135deg, #7c5cff, #9b7fff);"
                :class="'hover:opacity-90'"
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
                    class="w-5 h-5 rounded-full flex items-center justify-center text-xs font-bold text-white flex-shrink-0"
                    :style="step.requiresStop
                      ? 'background: linear-gradient(135deg,#ff6b6b,#ffa94d);'
                      : 'background: linear-gradient(135deg,#7c5cff,#b197ff);'"
                  >
                    {{ idx + 1 }}
                  </div>
                  <div
                    v-if="idx < wf.steps.length - 1"
                    class="w-px flex-1 mt-1"
                    style="background:#ded6f5; min-height:12px;"
                  ></div>
                </div>

                <!-- 步驟內容 -->
                <div class="flex-1 pb-1">
                  <div class="flex items-center gap-2 flex-wrap">
                    <span class="text-sm font-medium" style="color:#2e2a3f;">{{ step.name }}</span>
                    <span
                      v-if="step.badge"
                      class="text-xs px-2 py-0.5 rounded-full font-medium"
                      :style="step.requiresStop
                        ? 'background:#ffe8e8; color:#c0392b;'
                        : 'background:#eee8ff; color:#5037c9;'"
                    >{{ step.badge }}</span>
                    <span
                      v-if="step.requiresStop"
                      class="text-xs px-2 py-0.5 rounded-full font-medium"
                      style="background:#fff3e0; color:#e65100;"
                    >⏸ 需人工確認</span>
                  </div>
                  <div class="text-xs mt-0.5" style="color:#9f97b5;">{{ step.description }}</div>
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
        class="fixed inset-0 z-50 flex items-center justify-center p-4"
        style="background: rgba(46,42,63,0.5);"
        @click.self="closePanel"
      >
        <div class="w-full max-w-xl rounded-2xl shadow-2xl overflow-hidden" style="background:white;">
          <!-- 面板標題 -->
          <div class="px-6 py-4 border-b flex items-center justify-between" style="border-color:#ded6f5; background:#f4f0ff;">
            <div>
              <div class="font-bold" style="color:#2e2a3f;">{{ selectedWorkflow.wf.name }}</div>
              <div class="text-xs mt-0.5" style="color:#6f6883;">{{ selectedWorkflow.pw.projectName }}</div>
            </div>
            <button @click="closePanel" class="text-xl leading-none" style="color:#6f6883;">✕</button>
          </div>

          <div class="p-6 space-y-5">
            <!-- 輸入欄位 -->
            <div v-if="selectedWorkflow.wf.inputs.length > 0" class="space-y-3">
              <div class="text-sm font-medium" style="color:#2e2a3f;">流程參數</div>
              <div
                v-for="inp in selectedWorkflow.wf.inputs"
                :key="inp.key"
                class="space-y-1"
              >
                <label class="text-xs font-medium" style="color:#5037c9;">
                  {{ inp.label }}
                  <span v-if="inp.required" style="color:#c0392b;">*</span>
                </label>
                <input
                  v-model="inputValues[inp.key]"
                  :placeholder="inp.defaultValue ?? ''"
                  class="w-full px-3 py-2 rounded-lg border text-sm outline-none transition-colors"
                  style="border-color:#ded6f5; color:#2e2a3f;"
                  @focus="(e: FocusEvent) => (e.target as HTMLInputElement).style.borderColor='#7c5cff'"
                  @blur="(e: FocusEvent) => (e.target as HTMLInputElement).style.borderColor='#ded6f5'"
                />
              </div>
            </div>
            <div v-else class="text-sm py-2" style="color:#6f6883;">此工作流程不需要額外參數。</div>

            <!-- 模式選擇 -->
            <div class="space-y-2">
              <div class="text-sm font-medium" style="color:#2e2a3f;">執行模式</div>
              <div class="flex gap-2">
                <button
                  v-for="m in modes"
                  :key="m.value"
                  @click="selectedMode = m.value"
                  class="flex-1 px-3 py-2 rounded-xl text-xs font-medium border transition-colors"
                  :style="selectedMode === m.value
                    ? 'background:#7c5cff; color:white; border-color:#7c5cff;'
                    : 'background:white; color:#2e2a3f; border-color:#ded6f5;'"
                >
                  {{ m.label }}
                </button>
              </div>
            </div>

            <!-- 產生指令 -->
            <div v-if="generatedCommand" class="space-y-2">
              <div class="text-sm font-medium" style="color:#2e2a3f;">執行指令</div>
              <div class="relative">
                <pre
                  class="p-3 rounded-xl text-xs overflow-x-auto"
                  style="background:#1e1a2e; color:#c9b8ff; font-family: 'Cascadia Code', monospace; white-space:pre-wrap; word-break:break-all;"
                >{{ generatedCommand }}</pre>
                <button
                  @click="copyCommand"
                  class="absolute top-2 right-2 px-2 py-1 rounded-lg text-xs font-medium transition-colors"
                  style="background:#7c5cff; color:white;"
                >
                  {{ copied ? '✓ 已複製' : '複製' }}
                </button>
              </div>
              <p class="text-xs" style="color:#6f6883;">
                請在專案目錄下開啟終端機，貼上上方指令後執行。
              </p>
            </div>

            <!-- 按鈕列 -->
            <div class="flex gap-3 pt-2">
              <button
                @click="generate"
                :disabled="generating"
                class="flex-1 px-4 py-2.5 rounded-xl text-sm font-semibold text-white transition-opacity"
                style="background: linear-gradient(135deg, #7c5cff, #9b7fff);"
                :class="generating ? 'opacity-50' : 'hover:opacity-90'"
              >
                {{ generating ? '產生中…' : '🚀 產生指令' }}
              </button>
              <button
                @click="closePanel"
                class="px-4 py-2.5 rounded-xl text-sm font-medium border transition-colors"
                style="border-color:#ded6f5; color:#6f6883;"
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
    error.value = e instanceof Error ? e.message : String(e)
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
    error.value = e instanceof Error ? e.message : String(e)
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
