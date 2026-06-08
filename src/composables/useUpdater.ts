import { ref } from 'vue'
import { check } from '@tauri-apps/plugin-updater'
import { relaunch } from '@tauri-apps/plugin-process'

type UpdateStatus = 'idle' | 'checking' | 'available' | 'downloading' | 'uptodate' | 'error'

// 模組層級單例：橫幅與設定頁共用同一份狀態
const status = ref<UpdateStatus>('idle')
const newVersion = ref<string | null>(null)
const errorMsg = ref<string | null>(null)
const progress = ref(0) // 0–100（盡力而為）
let pending: Awaited<ReturnType<typeof check>> = null

export function useUpdater() {
  async function checkForUpdate(silent = false) {
    status.value = 'checking'
    errorMsg.value = null
    try {
      const update = await check()
      if (update) {
        pending = update
        newVersion.value = update.version
        status.value = 'available'
      } else {
        pending = null
        newVersion.value = null
        status.value = 'uptodate'
      }
    } catch (e: any) {
      // 靜默檢查（啟動時）失敗不干擾使用者；手動檢查才顯示錯誤
      status.value = silent ? 'idle' : 'error'
      errorMsg.value = e?.message ?? String(e)
    }
  }

  async function installUpdate() {
    if (!pending) return
    status.value = 'downloading'
    progress.value = 0
    let total = 0
    let downloaded = 0
    try {
      await pending.downloadAndInstall((event: any) => {
        switch (event.event) {
          case 'Started':
            total = event.data?.contentLength ?? 0
            break
          case 'Progress':
            downloaded += event.data?.chunkLength ?? 0
            if (total > 0) progress.value = Math.round((downloaded / total) * 100)
            break
          case 'Finished':
            progress.value = 100
            break
        }
      })
      await relaunch()
    } catch (e: any) {
      status.value = 'error'
      errorMsg.value = e?.message ?? String(e)
    }
  }

  function dismiss() {
    status.value = 'idle'
  }

  return { status, newVersion, errorMsg, progress, checkForUpdate, installUpdate, dismiss }
}
