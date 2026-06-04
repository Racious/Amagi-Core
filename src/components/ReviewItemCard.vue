<template>
  <div class="card overflow-hidden">
    <div class="p-4">
      <!-- 標題列 -->
      <div class="flex items-start justify-between gap-3 mb-2">
        <div class="flex-1 min-w-0">
          <div class="flex items-center gap-2 flex-wrap mb-1">
            <StatusBadge :status="item.itemType" />
            <StatusBadge :status="item.risk" />
            <!-- Agent 寫入標記 -->
            <span v-if="item.sourcePendingFile"
                  class="pill tone-success"
                  title="由 Agent 自動建立，來自 .amagi/pending/">
              🤖 Agent 草稿
            </span>
          </div>
          <!-- 標題：檢視 / 編輯 -->
          <div v-if="!editing" class="font-bold text-sm text-fg">{{ item.title }}</div>
          <input v-else v-model="editTitle"
                 class="input w-full text-sm font-bold" />
        </div>
        <button @click="startEdit"
                class="btn btn-ghost btn-sm flex-shrink-0">
          {{ editing ? '取消' : '編輯' }}
        </button>
      </div>

      <!-- 內容：一般檢視 -->
      <div v-if="!editing" class="text-sm text-muted whitespace-pre-wrap">{{ item.content }}</div>

      <!-- 內容：編輯模式 -->
      <template v-else>
        <!-- 技能用大型 Markdown 編輯器 -->
        <div v-if="item.itemType === 'skill'" class="space-y-1">
          <div class="flex items-center justify-between mb-1">
            <span class="text-xs font-medium" style="color: var(--c-accent)">技能內容（Markdown）</span>
            <span class="text-xs text-subtle">{{ editContent.split('\n').length }} 行</span>
          </div>
          <textarea
            v-model="editContent"
            :rows="editorRows"
            class="input w-full text-xs leading-relaxed font-mono resize-y"
            placeholder="## 描述&#10;這個技能解決什麼問題&#10;&#10;## 步驟&#10;1. 第一步&#10;2. 第二步&#10;&#10;## 注意事項&#10;- 注意點"
          />
        </div>
        <!-- 一般記憶用小型編輯器 -->
        <textarea v-else
                  v-model="editContent"
                  rows="4"
                  class="input w-full text-sm resize-y"
        />
      </template>

      <!-- 同步目標 + 範圍切換 -->
      <div class="flex items-center flex-wrap gap-1 mt-2">
        <span v-for="t in item.syncTargets" :key="t"
              class="pill tone-accent">{{ t }}</span>

        <button
          v-if="item.itemType === 'skill'"
          @click="toggleScope"
          class="pill font-medium transition-colors"
          :class="currentScope === 'global' ? 'tone-warning' : 'tone-muted'"
          :title="currentScope === 'global'
            ? '目前：全域（所有專案共用）→ 點擊切換為此專案'
            : '目前：此專案 → 點擊切換為全域（~/.codex/skills）'"
        >
          {{ currentScope === 'global' ? '🌐 全域' : '📁 此專案' }}
        </button>
      </div>
    </div>

    <!-- 操作按鈕列 -->
    <div class="flex gap-2 p-3 border-t border-border">
      <template v-if="!editing">
        <button @click="$emit('accept')"
                class="btn btn-primary btn-sm flex-1">✅ 接受</button>
        <button @click="$emit('ignore')"
                class="btn btn-danger btn-sm flex-1">🚫 忽略</button>
      </template>

      <!-- 編輯模式按鈕：技能有「儲存並接受」快捷鍵 -->
      <template v-else>
        <button @click="saveEdit"
                class="btn btn-ghost btn-sm flex-1">
          儲存草稿
        </button>
        <button v-if="item.itemType === 'skill'"
                @click="saveAndAccept"
                class="btn btn-primary btn-sm flex-1">
          ✅ 儲存並接受
        </button>
        <button v-else @click="saveEdit"
                class="btn btn-primary btn-sm flex-1">
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
