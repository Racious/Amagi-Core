import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { api, type ReviewItem, type ReviewApplyResult } from '../api/tauriCommands'

export const useReviewStore = defineStore('review', () => {
  const items = ref<ReviewItem[]>([])
  const loading = ref(false)
  const error = ref<string | null>(null)

  // pendingItems 含 blocked（徽章＝待老爺處理的總數）；審核頁自行把 blocked 拆到獨立區塊
  const pendingItems = computed(() => items.value.filter(i => i.status === 'pending' && i.itemType !== 'wiki'))
  const acceptedItems = computed(() => items.value.filter(i => i.status === 'accepted'))
  const pendingCount = computed(() => pendingItems.value.length)

  async function fetchItems(projectId?: string) {
    loading.value = true
    error.value = null
    try {
      items.value = await api.listReviewItems(projectId)
    } catch (e) {
      error.value = (e as any)?.message ?? String(e)
    } finally {
      loading.value = false
    }
  }

  async function accept(ids: string[]): Promise<ReviewApplyResult> {
    loading.value = true
    error.value = null
    try {
      const result = await api.acceptReviewItems(ids)
      // 以後端實際接受清單為準（Blocked 等會被後端跳過），不得用送出的 ids 樂觀覆蓋
      const acceptedIds = new Set(result.acceptedIds)
      items.value = items.value.map(item =>
        acceptedIds.has(item.id) ? { ...item, status: 'accepted' as const } : item
      )
      return result
    } catch (e) {
      error.value = (e as any)?.message ?? String(e)
      throw e
    } finally {
      loading.value = false
    }
  }

  async function ignore(ids: string[]) {
    loading.value = true
    error.value = null
    try {
      await api.ignoreReviewItems(ids)
      items.value = items.value.map(item =>
        ids.includes(item.id) ? { ...item, status: 'ignored' as const } : item
      )
    } catch (e) {
      error.value = (e as any)?.message ?? String(e)
      throw e
    } finally {
      loading.value = false
    }
  }

  /** 「確認丟棄」封鎖項：後端實體出列（僅 Blocked 型別），成功後同步移除本地清單 */
  async function discardBlocked(ids: string[]) {
    loading.value = true
    error.value = null
    try {
      await api.discardBlockedItems(ids)
      items.value = items.value.filter(item => !(ids.includes(item.id) && item.itemType === 'blocked'))
    } catch (e) {
      error.value = (e as any)?.message ?? String(e)
      throw e
    } finally {
      loading.value = false
    }
  }

  async function update(item: ReviewItem): Promise<ReviewItem> {
    loading.value = true
    error.value = null
    try {
      const updated = await api.updateReviewItem(item)
      const idx = items.value.findIndex(i => i.id === updated.id)
      if (idx !== -1) items.value[idx] = updated
      return updated
    } catch (e) {
      error.value = (e as any)?.message ?? String(e)
      throw e
    } finally {
      loading.value = false
    }
  }

  return { items, loading, error, pendingItems, acceptedItems, pendingCount, fetchItems, accept, ignore, discardBlocked, update }
})
