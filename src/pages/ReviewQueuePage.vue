<template>
  <div>
    <div class="flex items-center justify-between mb-6">
      <div>
        <h1 class="page-title mb-1">審核佇列</h1>
        <p class="page-sub">接受、忽略或編輯候選記憶與技能</p>
      </div>
      <div class="flex gap-2">
        <button @click="acceptAll" :disabled="pendingItems.length === 0 || reviewStore.loading"
                class="btn btn-primary btn-sm">全部接受</button>
        <button @click="ignoreAll" :disabled="pendingItems.length === 0 || reviewStore.loading"
                class="btn btn-danger btn-sm">全部忽略</button>
      </div>
    </div>

    <div v-if="reviewStore.loading" class="text-center py-8 text-muted">載入中…</div>

    <template v-else>
      <!-- 封鎖項（安全過濾）：唯讀，僅能確認丟棄——不提供編輯/接受，避免敏感內容被洗白後同步 -->
      <div v-if="blockedItems.length > 0" class="mb-6">
        <div class="text-xs font-bold uppercase tracking-wider mb-3" style="color: var(--c-danger)">
          ⛔ 已封鎖（{{ blockedItems.length }}）— 疑似敏感內容，唯讀
        </div>
        <div class="space-y-3">
          <div v-for="item in blockedItems" :key="item.id"
               class="card p-4" style="border-color: var(--c-danger)">
            <div class="flex items-center gap-2 flex-wrap mb-2">
              <StatusBadge status="blocked_type" />
              <StatusBadge :status="item.risk" />
              <!-- 舊版殘留的 accepted/ignored 封鎖項也導入此區，標示現況供辨識 -->
              <StatusBadge v-if="item.status !== 'pending'" :status="item.status" />
              <span class="font-bold text-sm text-fg">{{ item.title }}</span>
            </div>
            <!-- 命中檔行（📄 前綴，與 learn_engine::blocked_item 輸出格式耦合）直接渲染為可點連結
                 → reveal_in_explorer 開檔案總管並選中（後端已含相對路徑安全驗證）；其餘行照舊文字 -->
            <div class="text-sm text-muted">
              <template v-for="(line, i) in item.content.split('\n')" :key="i">
                <button v-if="fileLineOf(line)" type="button" class="file-link"
                        :title="'在檔案總管中開啟 ' + fileLineOf(line)"
                        @click="openBlockedFile(item, fileLineOf(line)!)">📄 {{ fileLineOf(line) }}</button>
                <div v-else class="whitespace-pre-wrap content-line">{{ line }}</div>
              </template>
            </div>
            <div v-if="revealMsg" class="text-xs mt-2" style="color: var(--c-danger);">{{ revealMsg }}</div>

            <!-- hit 級靜音勾選面板（adr-007 D1）：預設全選、可取消個別值；
                 全選→整卡出列，部分→卡就地更新殘餘。靜音的是「值」不是檔案。 -->
            <div v-if="muteOpenId === item.id" class="mt-3 p-3 rounded border border-border" style="background: var(--c-bg-soft, rgba(0,0,0,0.03));">
              <div class="text-xs font-bold mb-2 text-fg">勾選要靜音的誤判值（靜音的是這些值，不是檔案；同檔新出現的值仍會被擋）：</div>
              <label v-for="(h, hi) in hitsOf(item)" :key="hi" class="flex items-start gap-2 text-xs mb-1 cursor-pointer">
                <input type="checkbox" v-model="muteChecks[hi]" class="mt-0.5" />
                <span class="text-muted">
                  <template v-if="h.filePath">📄 {{ h.filePath }}｜</template>{{ h.ruleLabel }}｜{{ h.masked }}
                </span>
              </label>
              <div v-if="muteMsg" class="text-xs mt-1" style="color: var(--c-danger);">{{ muteMsg }}</div>
              <div class="flex justify-end gap-2 mt-2">
                <button class="btn btn-sm" :disabled="muteBusy" @click="muteOpenId = null">取消</button>
                <button class="btn btn-primary btn-sm" :disabled="reviewStore.loading || muteBusy || !muteChecks.some(Boolean)"
                        @click="confirmMute(item)">{{ muteBusy ? '靜音中…' : '🔕 靜音勾選值' }}</button>
              </div>
            </div>

            <div class="flex justify-end gap-2 mt-3 pt-3 border-t border-border">
              <button v-if="hitsOf(item).length" @click="toggleMute(item)" :disabled="reviewStore.loading"
                      class="btn btn-sm" title="誤判（如 commit SHA）勾選靜音，之後學習不再提醒；可於下方灰名單區解除">
                🔕 誤判，不再提醒</button>
              <button v-else disabled class="btn btn-sm opacity-50"
                      title="舊版卡片無結構化命中，無法靜音；請用「確認丟棄」，重新學習產生的新卡即可靜音">
                🔕 誤判，不再提醒</button>
              <button @click="discardBlocked(item)" :disabled="reviewStore.loading"
                      class="btn btn-danger btn-sm">🗑 確認丟棄</button>
            </div>
          </div>
        </div>
      </div>

      <!-- 已靜音誤判（灰名單，adr-007 D5）：可檢視、可解除——無救回 UI 不上線的硬約束。
           v1 已知限制：無批次解除/篩選/分頁（規模實證後另案）。 -->
      <details class="mb-6 card p-4" @toggle="onGreylistToggle">
        <summary class="cursor-pointer text-xs font-bold uppercase tracking-wider text-muted">
          🔕 已靜音誤判（灰名單）
        </summary>
        <div class="mt-3">
          <div class="flex items-center gap-2 mb-3">
            <span class="text-xs text-muted">專案：</span>
            <select v-model="glProjectId" class="input input-sm text-xs" @change="loadGreylist">
              <option v-for="p in glProjects" :key="p.id" :value="p.id">{{ p.name }}</option>
            </select>
          </div>
          <div v-if="glError" class="text-xs mb-2" style="color: var(--c-warn, #b45309);">
            ⚠ 灰名單讀取失敗，靜音效果已暫停（該值會重新出卡）：{{ glError }}
          </div>
          <div v-else-if="glLoading" class="text-xs text-muted">載入中…</div>
          <div v-else-if="glEntries.length === 0" class="text-xs text-muted">
            尚無靜音項。封鎖卡按「誤判，不再提醒」勾選的值會列在這裡，可隨時解除恢復提醒。
          </div>
          <div v-else class="space-y-1">
            <div v-for="(e, i) in glEntries" :key="i" class="flex items-center gap-2 text-xs py-1 border-b border-border">
              <span class="flex-1 text-muted truncate" :title="`${e.filePath ?? '（無檔案）'}｜${e.ruleLabel}`">
                <template v-if="e.filePath">📄 {{ e.filePath }}｜</template>{{ e.ruleLabel }}｜{{ e.masked }}
              </span>
              <span class="text-muted opacity-60">{{ e.createdAt.slice(0, 10) }}</span>
              <button class="btn btn-sm" :disabled="glLoading" @click="unmute(e)">解除</button>
            </div>
          </div>
        </div>
      </details>

      <div v-if="pendingItems.length === 0 && blockedItems.length === 0"
           class="card card-dashed p-8 text-center">
        <div class="text-4xl mb-3">✅</div>
        <div class="font-bold mb-1 text-fg">沒有待審核的候選項</div>
        <div class="text-sm text-muted">前往「學習變更」頁面掃描專案以產生候選記憶。</div>
      </div>

      <div v-else class="space-y-3">
        <ReviewItemCard
          v-for="item in pendingItems"
          :key="item.id"
          :item="item"
          @accept="accept(item.id)"
          @ignore="ignore(item.id)"
          @save="save"
          @save-and-accept="saveAndAccept"
        />
      </div>
    </template>

    <!-- 已接受待同步 -->
    <div v-if="acceptedItems.length > 0" class="mt-8">
      <div class="flex items-center gap-2 mb-3">
        <div class="text-xs font-bold uppercase tracking-wider" style="color: var(--c-info)">待同步（{{ acceptedItems.length }}）</div>
        <RouterLink to="/sync" class="pill tone-info font-medium">前往同步 →</RouterLink>
      </div>
      <div class="space-y-2">
        <div v-for="item in acceptedItems" :key="item.id"
             class="card p-3 flex items-center gap-3">
          <span class="text-sm">🔄</span>
          <span class="text-sm flex-1 truncate text-fg">{{ item.title }}</span>
          <StatusBadge :status="item.status" />
        </div>
      </div>
    </div>

    <!-- 已忽略（Phase 3 vault-first：已同步項入庫即出列，不再留佇列；此區常態只剩忽略項。
         篩選仍容忍 synced 以顯示極端殘留（migration 前的舊資料），不承擔權威資料管理。 -->
    <div v-if="doneItems.length > 0" class="mt-6">
      <div class="text-xs font-bold uppercase tracking-wider mb-3 text-muted">已忽略（{{ doneItems.length }}）</div>
      <div class="space-y-2">
        <div v-for="item in doneItems" :key="item.id"
             class="card p-3 flex items-center gap-3 opacity-60">
          <span class="text-sm">{{ item.status === 'synced' ? '✅' : '🚫' }}</span>
          <span class="text-sm flex-1 truncate text-muted">{{ item.title }}</span>
          <StatusBadge :status="item.status" />
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import { ask } from '@tauri-apps/plugin-dialog'
import { useReviewStore } from '../stores/reviewStore'
import { api, type BlockedHit, type GreylistEntry, type ProjectInfo, type ReviewItem } from '../api/tauriCommands'
import ReviewItemCard from '../components/ReviewItemCard.vue'
import StatusBadge from '../components/StatusBadge.vue'

const reviewStore = useReviewStore()
const revealMsg = ref('')

// ── hit 級靜音（adr-007 D1）──────────────────────────────
const muteOpenId = ref<string | null>(null)
const muteChecks = ref<boolean[]>([])
const muteMsg = ref('')
const muteBusy = ref(false) // 送出中鎖：防同卡重複送出（impl-review 發現 2）

/** 舊卡（升級前產生）blockedHits 為空/缺欄 → 靜音鈕禁用（不解析 content 文案） */
function hitsOf(item: ReviewItem): BlockedHit[] {
  return item.blockedHits ?? []
}

function toggleMute(item: ReviewItem) {
  muteMsg.value = ''
  if (muteOpenId.value === item.id) {
    muteOpenId.value = null
    return
  }
  muteOpenId.value = item.id
  muteChecks.value = hitsOf(item).map(() => true) // 預設全選、可取消個別值
}

async function confirmMute(item: ReviewItem) {
  muteMsg.value = ''
  const hits = hitsOf(item)
  const selected = hits.filter((_, i) => muteChecks.value[i])
  if (selected.length === 0) return
  const ok = await ask(
    `將靜音 ${selected.length}/${hits.length} 個值：之後的學習不再對這些值出封鎖卡。\n` +
      `靜音的是「值」不是檔案——同檔新出現的疑似機密仍會被擋。\n可隨時在「已靜音誤判」區解除。`,
    { title: '誤判，不再提醒？', kind: 'warning' },
  )
  if (!ok) return
  if (muteBusy.value) return
  muteBusy.value = true
  try {
    await api.discardBlockedAsFalsePositive(
      item.projectId,
      item.id,
      selected.map(h => ({ filePath: h.filePath, ruleLabel: h.ruleLabel, valueDigest: h.valueDigest })),
    )
    muteOpenId.value = null
    // 後端已改卡（部分靜音）或出列（全選）：重載佇列反映實況
    await reviewStore.fetchItems()
    if (glProjectId.value === item.projectId) await loadGreylist()
  } catch (e: any) {
    muteMsg.value = `靜音失敗（卡未變動）：${e?.message ?? e}`
  } finally {
    muteBusy.value = false
  }
}

// ── 灰名單檢視/解除（adr-007 D5）────────────────────────
const glProjects = ref<ProjectInfo[]>([])
const glProjectId = ref('')
const glEntries = ref<GreylistEntry[]>([])
const glError = ref('')
const glLoading = ref(false)

async function onGreylistToggle(e: Event) {
  if (!(e.target as HTMLDetailsElement).open) return
  if (glProjects.value.length === 0) {
    try {
      glProjects.value = await api.listProjects()
      if (!glProjectId.value && glProjects.value.length > 0) glProjectId.value = glProjects.value[0].id
    } catch (err: any) {
      glError.value = err?.message ?? String(err)
      return
    }
  }
  await loadGreylist()
}

async function loadGreylist() {
  if (!glProjectId.value) return
  glLoading.value = true
  glError.value = ''
  try {
    const data = await api.listBlockedGreylist(glProjectId.value)
    glEntries.value = data.entries
  } catch (e: any) {
    // 讀取失敗＝靜音效果已暫停（產卡端 fail-open）：明確警示，不靜默顯示空清單
    glEntries.value = []
    glError.value = e?.message ?? String(e)
  } finally {
    glLoading.value = false
  }
}

async function unmute(entry: GreylistEntry) {
  glLoading.value = true
  glError.value = ''
  try {
    await api.removeGreylistEntries(glProjectId.value, [
      { filePath: entry.filePath, ruleLabel: entry.ruleLabel, valueDigest: entry.valueDigest },
    ])
    await loadGreylist()
  } catch (e: any) {
    glError.value = e?.message ?? String(e)
    glLoading.value = false
  }
}

/// 命中檔行判定：「📄 <path>」行回傳路徑，否則 null（格式由 learn_engine 產生、確定性）
function fileLineOf(line: string): string | null {
  const t = line.trim()
  if (!t.startsWith('📄 ')) return null
  const p = t.slice(2).trim()
  return p || null
}

async function openBlockedFile(item: ReviewItem, relPath: string) {
  revealMsg.value = ''
  try {
    await api.revealInExplorer(item.projectId, relPath)
  } catch (e: any) {
    // 常見情境：檔案已被修正/刪除、或專案已移除 → 誠實顯示，不阻斷其他操作
    revealMsg.value = `開啟失敗：${e?.message ?? e}`
  }
}

// 封鎖項獨立成塊（唯讀 + 確認丟棄）；一般待審清單不含 blocked，「全部接受/忽略」也就碰不到它。
// 收「全部狀態」的 blocked（Codex R2）：舊版 queue.json 可能殘留 Accepted（G1 歷史殭屍）/
// Ignored 的封鎖項，一律導進此區給出丟棄出口，待同步/已忽略區則排除 blocked。
const blockedItems = computed(() => reviewStore.items.filter(i => i.itemType === 'blocked'))
const pendingItems = computed(() => reviewStore.items.filter(i => i.status === 'pending' && i.itemType !== 'wiki' && i.itemType !== 'blocked'))
const acceptedItems = computed(() => reviewStore.items.filter(i => i.status === 'accepted' && i.itemType !== 'wiki' && i.itemType !== 'blocked'))
const doneItems = computed(() => reviewStore.items.filter(i => (i.status === 'ignored' || i.status === 'synced') && i.itemType !== 'wiki' && i.itemType !== 'blocked'))

async function accept(id: string) {
  await reviewStore.accept([id])
}

async function ignore(id: string) {
  await reviewStore.ignore([id])
}

async function discardBlocked(item: ReviewItem) {
  const ok = await ask(
    `確定丟棄封鎖項「${item.title}」？\n\n丟棄不影響其他候選；若內容確為真實機密，請先至原始檔與 git 紀錄移除。`,
    { title: '確認丟棄封鎖項', kind: 'warning' },
  )
  if (!ok) return
  await reviewStore.discardBlocked([item.id])
}

async function save(item: ReviewItem) {
  await reviewStore.update(item)
}

async function saveAndAccept(item: ReviewItem) {
  await reviewStore.update(item)
  await reviewStore.accept([item.id])
}

async function acceptAll() {
  const ids = pendingItems.value.map(i => i.id)
  await reviewStore.accept(ids)
}

async function ignoreAll() {
  const ids = pendingItems.value.map(i => i.id)
  await reviewStore.ignore(ids)
}
</script>

<style scoped>
/* 命中檔連結：像檔案連結般可點（hover 底線＋accent 色），對齊卡片內文字排版 */
.file-link {
  display: block;
  background: none;
  border: none;
  padding: 0;
  font: inherit;
  text-align: left;
  cursor: pointer;
  color: var(--c-accent);
}
.file-link:hover { text-decoration: underline; }
/* 空行也要佔行高，維持原 pre-wrap 的段落間距 */
.content-line { min-height: 1.25em; }
</style>
