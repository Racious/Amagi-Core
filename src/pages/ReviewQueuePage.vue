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

    <div v-else-if="pendingItems.length === 0"
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
import { computed } from 'vue'
import { useReviewStore } from '../stores/reviewStore'
import type { ReviewItem } from '../api/tauriCommands'
import ReviewItemCard from '../components/ReviewItemCard.vue'
import StatusBadge from '../components/StatusBadge.vue'

const reviewStore = useReviewStore()

const pendingItems = computed(() => reviewStore.items.filter(i => i.status === 'pending' && i.itemType !== 'wiki'))
const acceptedItems = computed(() => reviewStore.items.filter(i => i.status === 'accepted' && i.itemType !== 'wiki'))
const doneItems = computed(() => reviewStore.items.filter(i => (i.status === 'ignored' || i.status === 'synced') && i.itemType !== 'wiki'))

async function accept(id: string) {
  await reviewStore.accept([id])
}

async function ignore(id: string) {
  await reviewStore.ignore([id])
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
