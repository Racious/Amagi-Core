<template>
  <div class="onboarding-overlay">
    <div class="card onboarding-card">
      <div class="text-2xl mb-1">📚 設定知識庫（Vault）</div>
      <p class="text-sm text-muted mb-4">
        AMAGI Core 的知識、技能、長期記憶都存在「知識庫（vault）」這個資料夾裡。
        首次使用請先指定一個資料夾作為本機 vault——之後所有產出都會歸入此處，跨機器也靠它同步。
      </p>

      <!-- 步驟一：選資料夾 -->
      <template v-if="!done">
        <div class="flex items-center gap-3 flex-wrap">
          <button class="btn btn-primary btn-sm" :disabled="busy" @click="chooseVault">
            {{ busy ? '套用中…' : '選擇 vault 資料夾' }}
          </button>
          <button class="btn btn-ghost btn-sm" :disabled="busy" @click="$emit('skip')">稍後再說</button>
        </div>
        <p v-if="warn" class="text-xs mt-3" style="color: var(--c-danger, #c0392b);">{{ warn }}</p>
        <p class="text-xs text-muted mt-3">
          選定後，AMAGI 會在全局 <code>~/.claude/CLAUDE.md</code>、<code>~/.codex/AGENTS.md</code>
          寫入受管區塊（僅替換標記區、原內容不動、先備份 .bak），讓 AI 對話自動指向本機 vault。
        </p>
      </template>

      <!-- 步驟二：設定完成 + git 保命建議 -->
      <template v-else>
        <p class="text-sm mb-3">
          <span style="color: var(--c-accent);">✓ 已設定：</span>
          <span class="text-fg break-all">{{ vaultPath }}</span>
        </p>
        <div v-if="!isGitRepo" class="alert tone-warning mb-3">
          <div class="text-sm font-semibold mb-1" style="color: var(--c-warning);">⚠ 強烈建議掛上 git（保命）</div>
          <p class="text-xs text-muted">
            vault 是所有知識的唯一家——一旦遺失無可挽回。請將此資料夾
            <code>git init</code> 並推送遠端，才能跨機器同步、避免單點故障。
            掛好後可到「設定 → 知識庫」用「提交並推送」。
          </p>
        </div>
        <div v-else class="text-sm mb-3" style="color: var(--c-accent);">
          ✓ 已偵測到 git，跨機同步就緒。
        </div>
        <button class="btn btn-primary btn-sm" @click="$emit('done')">開始使用</button>
      </template>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { open } from '@tauri-apps/plugin-dialog'
import { api } from '../api/tauriCommands'

defineEmits<{ done: []; skip: [] }>()

const busy = ref(false)
const done = ref(false)
const warn = ref('')
const vaultPath = ref<string | null>(null)
const isGitRepo = ref(false)

async function chooseVault() {
  warn.value = ''
  busy.value = true
  try {
    // open 也納入 try：dialog plugin reject 時才會顯示提示，不致無聲中斷
    const picked = await open({ directory: true, multiple: false, title: '選擇 vault 資料夾' })
    if (typeof picked !== 'string') return // 使用者取消
    const r = await api.setVaultPath(picked)
    vaultPath.value = r.vaultPath
    // 取最新狀態判斷是否已掛 git（保命建議用）
    const st = await api.getVaultStatus()
    isGitRepo.value = st.isGitRepo
    done.value = true
  } catch (e: any) {
    warn.value = `設定失敗：${e?.message ?? e}`
  } finally {
    busy.value = false
  }
}
</script>

<style scoped>
.onboarding-overlay {
  position: fixed;
  inset: 0;
  z-index: 50;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 1.5rem;
  background: color-mix(in srgb, var(--c-canvas) 80%, transparent);
  backdrop-filter: blur(2px);
}
.onboarding-card {
  max-width: 32rem;
  width: 100%;
  padding: 1.75rem;
}
</style>
