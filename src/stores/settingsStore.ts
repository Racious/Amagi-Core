import { defineStore } from 'pinia'
import { ref } from 'vue'

export type NotificationMode = 'quiet' | 'normal' | 'active'

export const useSettingsStore = defineStore('settings', () => {
  const notificationMode = ref<NotificationMode>('quiet')

  function setNotificationMode(mode: NotificationMode) {
    notificationMode.value = mode
  }

  return { notificationMode, setNotificationMode }
})
