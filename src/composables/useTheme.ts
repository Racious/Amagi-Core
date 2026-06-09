import { ref, computed, watch } from 'vue'

export type ThemeBase = 'light' | 'dark'

export interface ThemeDef {
  id: string
  label: string
  base: ThemeBase
  /** 設定頁色板預覽用（canvas / surface / accent） */
  swatch: { bg: string; surface: string; accent: string }
}

/** 可選主題清單（順序即設定頁顯示順序） */
export const THEMES: ThemeDef[] = [
  { id: 'daylight',    label: 'Daylight',         base: 'light', swatch: { bg: '#f7f7f8', surface: '#ffffff', accent: '#5e6ad2' } },
  { id: 'latte',       label: 'Catppuccin Latte', base: 'light', swatch: { bg: '#eff1f5', surface: '#ffffff', accent: '#8839ef' } },
  { id: 'midnight',    label: 'Midnight',         base: 'dark',  swatch: { bg: '#0b0b0e', surface: '#141417', accent: '#7079e8' } },
  { id: 'tokyo-night', label: 'Tokyo Night',      base: 'dark',  swatch: { bg: '#1a1b26', surface: '#1f2335', accent: '#7aa2f7' } },
  { id: 'mocha',       label: 'Catppuccin Mocha', base: 'dark',  swatch: { bg: '#1e1e2e', surface: '#28283c', accent: '#cba6f7' } },
  { id: 'nord',        label: 'Nord',             base: 'dark',  swatch: { bg: '#2e3440', surface: '#353c4a', accent: '#88c0d0' } },
  { id: 'everforest',  label: 'Everforest',       base: 'dark',  swatch: { bg: '#2d353b', surface: '#343f44', accent: '#a7c080' } },
  { id: 'rose-pine',   label: 'Rosé Pine',        base: 'dark',  swatch: { bg: '#191724', surface: '#1f1d2e', accent: '#ebbcba' } },
]

/** 'system' 跟隨系統，或某個主題 id */
export type ThemePref = 'system' | string

const STORAGE_KEY = 'amagi-theme'
const DEFAULT_LIGHT = 'daylight'
const DEFAULT_DARK = 'midnight'

const ids = new Set(THEMES.map((t) => t.id))

/** 相容舊版儲存值（light / dark）並驗證合法性 */
function normalize(v: string | null): ThemePref | null {
  if (!v) return null
  if (v === 'system') return 'system'
  if (v === 'light') return DEFAULT_LIGHT
  if (v === 'dark') return DEFAULT_DARK
  return ids.has(v) ? v : null
}

function initialPref(): ThemePref {
  return normalize(localStorage.getItem(STORAGE_KEY)) ?? 'system'
}

const mql = window.matchMedia?.('(prefers-color-scheme: dark)')

/** 將偏好解析為實際套用的主題 id（system 依系統明暗） */
function resolveId(p: ThemePref): string {
  if (p === 'system') return mql?.matches ? DEFAULT_DARK : DEFAULT_LIGHT
  return p
}

function apply(p: ThemePref) {
  document.documentElement.setAttribute('data-theme', resolveId(p))
}

// 模組層級單例：全 App 共用同一份主題狀態
const pref = ref<ThemePref>(initialPref())
apply(pref.value)

watch(pref, (p) => {
  localStorage.setItem(STORAGE_KEY, p)
  apply(p)
})

// 系統明暗變化時，若為「跟隨系統」則即時重套
mql?.addEventListener?.('change', () => {
  if (pref.value === 'system') apply('system')
})

export function useTheme() {
  /** 目前實際套用的主題 id（system 已解析） */
  const activeId = computed(() => resolveId(pref.value))
  /** 目前實際基調 light / dark（供明暗圖示判斷） */
  const activeBase = computed<ThemeBase>(
    () => THEMES.find((t) => t.id === activeId.value)?.base ?? 'light',
  )

  function set(p: ThemePref) {
    pref.value = p
  }
  /** 頁尾快速鈕：在預設淺 / 深之間切換 */
  function toggle() {
    pref.value = activeBase.value === 'dark' ? DEFAULT_LIGHT : DEFAULT_DARK
  }

  return { pref, themes: THEMES, activeId, activeBase, set, toggle }
}
