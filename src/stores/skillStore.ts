import { defineStore } from 'pinia'
import { ref } from 'vue'
import { api, type LibrarySkill } from '../api/tauriCommands'

/**
 * 技能庫快取。跨頁面導覽保留，讓 SkillsPage 重新掛載時能即時以快取渲染、
 * 背景靜默刷新，避免每次切頁都重抓而閃一下載入占位（殘影）。
 */
export const useSkillStore = defineStore('skill', () => {
  const library = ref<LibrarySkill[]>([])
  const loaded = ref(false)
  const error = ref<string | null>(null)

  async function fetchLibrary() {
    error.value = null
    try {
      library.value = await api.listLibrarySkills()
      loaded.value = true
    } catch (e) {
      error.value = String(e)
      throw e
    }
  }

  return { library, loaded, error, fetchLibrary }
})
