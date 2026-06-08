# AMAGI Core

> AI 記憶與技能同步管家 — 偵測專案變更、產生候選記憶與技能草稿，審核通過後同步至 `AGENTS.md` / `CLAUDE.md` / `.claude/skills` / `.codex/skills`。

## 簡介

在日常使用 Codex / Claude 開發後，AMAGI Core 會偵測 git 變更、以規則式分析產生候選記憶與技能草稿，經人工審核後同步至各 Agent 設定檔，讓 AI 助手持續累積專案知識，無需手動維護。

## 主要功能

- **專案管理**：加入本機 Git 專案，初始化 `.amagi/` 工作目錄骨架（memory / pending / skills / history / artifacts / state）。
- **學習變更**：規則式掃描 git diff（README、依賴設定、CI/CD、Tauri 設定、Agent 規則），產生候選記憶。
- **審核佇列**：逐項接受／忽略／編輯候選記憶與技能草稿。
- **原生技能同步**：技能以原生 Skills 格式（含 `name` / `description` / `when_to_use` frontmatter）同步至 `.claude/skills/`、`.codex/skills/` 與 `.amagi/skills/`，支援自動觸發。
- **記憶同步**：審核通過的記憶寫入 `AGENTS.md`、`CLAUDE.md`（覆寫前自動備份 `.bak`）。
- **引導式執行（File Bridge）**：分步驟導引任務，逐步留下執行軌跡。
- **安全防護**：偵測疑似密碼／API key／token 並封鎖；同步前對違反規則的內容（`git config --local user.*`、`Co-Authored-By`、`git push --force` 等）卡控放行。

## 技術棧

- **前端**：Vue 3 + TypeScript + Vite + Tailwind CSS + Pinia + Vue Router
- **後端**：Tauri 2（Rust）
- **平台**：Windows（MSI / NSIS / 攜帶版）

## 開發

前置需求：Node.js 20+、Rust stable。

```bash
npm install
npm run tauri:dev     # 開發模式（熱重載）
npm run tauri:build   # 本機打包
```

## 發版

版號於三處同步，CHANGELOG 驅動 Release 內容：

```bash
npm run version:set -- <semver>   # 同步 package.json / tauri.conf.json / Cargo.toml
# 於 CHANGELOG.md 新增對應 ## v<semver> 段落
git commit -am "chore: 發版 v<semver>" && git push
git tag v<semver> && git push origin v<semver>   # 觸發 GitHub Actions 自動打包發佈
```

`.github/workflows/release.yml` 會以 `tauri-action` 自動建置並發佈 GitHub Release，附 MSI、NSIS 安裝器與攜帶版 exe；Release 說明由 `scripts/extract-changelog.mjs` 依 tag 自動擷取對應 CHANGELOG 段落。

## 應用圖標

圖標來源與產生工具：

- 來源母版：`public/amagi-core-icon-rounded.png`（深色圓角、雙 A 標誌與發光核心，已去除黑色四角並裁切）。
- `node scripts/round-corners.mjs`：由 concept 原圖去除黑色四角並裁切透明邊距。
- `node scripts/gen-icons.mjs`：一鍵產生 `src-tauri/icons/` 全尺寸 PNG、`.ico` / `.icns` 與 `public/favicon.*`。
