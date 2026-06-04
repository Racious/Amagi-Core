/**
 * extract-changelog.mjs
 * CI 工具：從 CHANGELOG.md 提取指定版本的更新說明，
 * 合併下載表格後輸出至 release-body.md（無 AI，純確定性）。
 *
 * Usage: node scripts/extract-changelog.mjs <version>
 * Example: node scripts/extract-changelog.mjs v0.1.0
 */

import { readFileSync, writeFileSync } from 'fs'

const rawVersion = process.argv[2] ?? ''
const version = rawVersion.replace(/^v/, '')

const downloadTable = `## 下載

| 類型 | 說明 | 檔案 |
|------|------|------|
| 安裝版（MSI，推薦） | Windows Installer，自動整合捷徑與解除安裝 | \`AMAGI Core_${version}_x64_en-US.msi\` |
| 安裝版（NSIS） | 輕量安裝程式 | \`AMAGI Core_${version}_x64-setup.exe\` |
| 攜帶版（免安裝） | 單一 .exe，無需安裝，直接執行 | \`AMAGI_Core_${version}_portable.exe\` |

> AMAGI Core — AI 記憶與技能同步管家：偵測專案變更、產生候選記憶與技能草稿，審核後同步至 AGENTS.md / CLAUDE.md / .claude/skills / .codex/skills。

---

`

let changelog = ''
try {
  changelog = readFileSync('CHANGELOG.md', 'utf-8')
} catch {
  writeFileSync('release-body.md', downloadTable)
  process.exit(0)
}

const versionHeader = `## v${version}`
const start = changelog.indexOf(versionHeader)

if (start === -1) {
  writeFileSync('release-body.md', downloadTable)
  process.exit(0)
}

const afterHeader = start + versionHeader.length
const nextVersion = changelog.indexOf('\n## v', afterHeader)
const versionSection = nextVersion === -1
  ? changelog.slice(start)
  : changelog.slice(start, nextVersion)

writeFileSync('release-body.md', downloadTable + versionSection.trim() + '\n')
console.log(`Release body generated for v${version}`)
