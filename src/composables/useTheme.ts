import { ref, watch } from 'vue'

export type Theme = 'light' | 'dark'

const STORAGE_KEY = 'amagi-theme'

function initialTheme(): Theme {
  const saved = localStorage.getItem(STORAGE_KEY)
  if (saved === 'light' || saved === 'dark') return saved
  // 跟隨系統偏好
  return window.matchMedia?.('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'
}

function apply(t: Theme) {
  document.documentElement.setAttribute('data-theme', t)
}

// 模組層級單例：全 App 共用同一份主題狀態
const theme = ref<Theme>(initialTheme())
apply(theme.value)

watch(theme, (t) => {
  localStorage.setItem(STORAGE_KEY, t)
  apply(t)
})

export function useTheme() {
  function toggle() {
    theme.value = theme.value === 'dark' ? 'light' : 'dark'
  }
  function set(t: Theme) {
    theme.value = t
  }
  return { theme, toggle, set }
}
