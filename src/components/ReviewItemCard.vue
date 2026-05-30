<template>
  <div class="rounded-2xl border overflow-hidden" style="background: white; border-color: #ded6f5;">
    <div class="p-4">
      <!-- 標題列 -->
      <div class="flex items-start justify-between gap-3 mb-2">
        <div class="flex-1 min-w-0">
          <div class="flex items-center gap-2 flex-wrap mb-1">
            <StatusBadge :status="item.itemType" />
            <StatusBadge :status="item.risk" />
            <!-- Agent 寫入標記 -->
            <span v-if="item.sourcePendingFile"
                  class="text-xs px-2 py-0.5 rounded-full font-medium"
                  style="background:#e8f5e9; color:#2e7d32;"
                  title="由 Agent 自動建立，來自 .amagi/pending/">
              🤖 Agent 草稿
            </span>
          </div>
          <!-- 標題：檢視 / 編輯 -->
          <div v-if="!editing" class="font-bold text-sm" style="color: #2e2a3f;">{{ item.title }}</div>
          <input v-else v-model="editTitle"
                 class="w-full rounded-xl border px-3 py-1.5 text-sm font-bold"
                 style="border-color: #7c5cff; color: #2e2a3f;" />
        </div>
        <button @click="startEdit"
                class="text-xs px-2 py-1 rounded-lg flex-shrink-0"
                style="color: #6f6883; background: #f5f5f5;">
          {{ editing ? '取消' : '編輯' }}
        </button>
      </div>

      <!-- 內容：一般檢視 -->
      <div v-if="!editing" class="text-sm" style="color: #6f6883; white-space: pre-wrap;">{{ item.content }}</div>

      <!-- 內容：編輯模式 -->
      <template v-else>
        <!-- 技能用大型 Markdown 編輯器 -->
        <div v-if="item.itemType === 'skill'" class="space-y-1">
          <div class="flex items-center justify-between mb-1">
            <span class="text-xs font-medium" style="color: #5037c9;">技能內容（Markdown）</span>
            <span class="text-xs" style="color: #9f97b5;">{{ editContent.split('\n').length }} 行</span>
          </div>
          <textarea
            v-model="editContent"
            :rows="editorRows"
            class="w-full rounded-xl border px-3 py-2.5 text-xs leading-relaxed outline-none transition-colors"
            style="border-color: #ded6f5; color: #2e2a3f; resize: vertical;
                   font-family: 'Cascadia Code', 'Consolas', 'Monaco', monospace;
                   background: #faf8ff; tab-size: 2;"
            placeholder="## 描述&#10;這個技能解決什麼問題&#10;&#10;## 步驟&#10;1. 第一步&#10;2. 第二步&#10;&#10;## 注意事項&#10;- 注意點"
            @focus="(e: FocusEvent) => (e.target as HTMLTextAreaElement).style.borderColor='#7c5cff'"
            @blur="(e: FocusEvent) => (e.target as HTMLTextAreaElement).style.borderColor='#ded6f5'"
          />
        </div>
        <!-- 一般記憶用小型編輯器 -->
        <textarea v-else
                  v-model="editContent"
                  rows="4"
                  class="w-full rounded-xl border px-3 py-2 text-sm outline-none"
                  style="border-color: #ded6f5; color: #2e2a3f; resize: vertical;"
                  @focus="(e: FocusEvent) => (e.target as HTMLTextAreaElement).style.borderColor='#7c5cff'"
                  @blur="(e: FocusEvent) => (e.target as HTMLTextAreaElement).style.borderColor='#ded6f5'"
        />
      </template>

      <!-- 同步目標 + 範圍切換 -->
      <div class="flex items-center flex-wrap gap-1 mt-2">
        <span v-for="t in item.syncTargets" :key="t"
              class="text-xs px-2 py-0.5 rounded-full"
              style="background: #eee8ff; color: #5037c9;">{{ t }}</span>

        <button
          v-if="item.itemType === 'skill'"
          @click="toggleScope"
          class="text-xs px-2 py-0.5 rounded-full border font-medium transition-colors"
          :style="currentScope === 'global'
            ? 'background:#fff3e0; color:#e65100; border-color:#ffcc80;'
            : 'background:#f0f0f0; color:#555; border-color:#ddd;'"
          :title="currentScope === 'global'
            ? '目前：全域（所有專案共用）→ 點擊切換為此專案'
            : '目前：此專案 → 點擊切換為全域（~/.codex/skills）'"
        >
          {{ currentScope === 'global' ? '🌐 全域' : '📁 此專案' }}
        </button>
      </div>
    </div>

    <!-- 操作按鈕列 -->
    <div class="flex border-t" style="border-color: #ded6f5;">
      <template v-if="!editing">
        <button @click="$emit('accept')"
                class="flex-1 py-2.5 text-xs font-bold transition-colors hover:opacity-80"
                style="color: #1d7a51; background: #eefaf4;">✅ 接受</button>
        <button @click="$emit('ignore')"
                class="flex-1 py-2.5 text-xs font-bold border-l transition-colors hover:opacity-80"
                style="color: #d85c5c; background: #fff0f0; border-color: #ded6f5;">🚫 忽略</button>
      </template>

      <!-- 編輯模式按鈕：技能有「儲存並接受」快捷鍵 -->
      <template v-else>
        <button @click="saveEdit"
                class="flex-1 py-2.5 text-xs font-bold border-r transition-colors hover:opacity-80"
                style="color: #5037c9; background: #eee8ff; border-color: #ded6f5;">
          儲存草稿
        </button>
        <button v-if="item.itemType === 'skill'"
                @click="saveAndAccept"
                class="flex-1 py-2.5 text-xs font-bold text-white transition-opacity hover:opacity-90"
                style="background: linear-gradient(135deg, #7c5cff, #9b7fff);">
          ✅ 儲存並接受
        </button>
        <button v-else @click="saveEdit"
                class="flex-1 py-2.5 text-xs font-bold text-white"
                style="background: #7c5cff;">
          儲存變更
        </button>
      </template>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import type { ReviewItem, SyncScope } from '../api/tauriCommands'
import StatusBadge from './StatusBadge.vue'

const props = defineProps<{ item: ReviewItem }>()
const emit = defineEmits<{
  accept: []
  ignore: []
  save: [item: ReviewItem]
  saveAndAccept: [item: ReviewItem]
}>()

const editing = ref(false)
const editTitle = ref(props.item.title)
const editContent = ref(props.item.content)
const currentScope = ref<SyncScope>(props.item.syncScope ?? 'project')

// 技能的編輯器高度：依內容行數動態調整，最少 10 行，最多 24 行
const editorRows = computed(() => {
  const lines = editContent.value.split('\n').length
  return Math.min(Math.max(lines + 2, 10), 24)
})

function startEdit() {
  if (editing.value) {
    // 取消：還原
    editTitle.value = props.item.title
    editContent.value = props.item.content
    editing.value = false
  } else {
    editing.value = true
  }
}

function buildUpdated(): ReviewItem {
  return {
    ...props.item,
    title: editTitle.value,
    content: editContent.value,
    syncScope: currentScope.value,
  }
}

function saveEdit() {
  emit('save', buildUpdated())
  editing.value = false
}

function saveAndAccept() {
  emit('saveAndAccept', buildUpdated())
  editing.value = false
}

function toggleScope() {
  currentScope.value = currentScope.value === 'global' ? 'project' : 'global'
  emit('save', { ...props.item, syncScope: currentScope.value })
}
</script>
