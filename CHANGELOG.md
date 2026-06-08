# Changelog

AMAGI Core 的所有重要變更記錄於此。版本遵循語意化版號（major.minor.patch）。

## v0.1.1

品牌視覺更新：全新應用圖標與正式 README。

### 介面
- 全新應用圖標（深色圓角方塊、雙 A 標誌與青色發光核心、雙橢圓軌道），統一覆蓋安裝圖標、系統匣、視窗標題列與瀏覽器 favicon 全尺寸。
- 自動去除底圖黑色四角為透明圓角，並裁切透明邊距使圖標占滿畫布，與系統其他圖標大小一致。

### 文件
- README 由 Vite / Tauri 範本改寫為正式專案說明（簡介、主要功能、技術棧、開發與發版流程、圖標工具）。

### 內部
- 新增圖標產生工具：`scripts/gen-icons.mjs`（一鍵產生 Tauri 全尺寸 PNG + `.ico` / `.icns` + favicon）、`scripts/round-corners.mjs`（去除底圖黑色四角並裁切）。

## v0.1.0

首個內部版本。AI 記憶與技能同步管家：在日常使用 Codex / Claude 後，偵測 git 變更、
產生候選記憶與技能草稿，審核通過後同步至 Agent 設定檔。

### 功能
- **專案管理**：加入本機 Git 專案、初始化 `.amagi/` 工作目錄骨架（memory / pending / skills / history / artifacts / state）。
- **學習變更**：規則式掃描 git diff（README、依賴設定、CI/CD、Tauri 設定、Agent 規則），產生候選記憶。
- **審核佇列**：逐項接受／忽略／編輯候選記憶與技能草稿。
- **原生技能同步**：技能以原生 Skills 格式（含 `name` / `description` / `when_to_use` frontmatter）同步至 `.claude/skills/`、`.codex/skills/` 與 `.amagi/skills/`，支援自動觸發。
- **記憶同步**：審核通過的記憶寫入 `AGENTS.md`、`CLAUDE.md`（覆寫前自動備份 `.bak`）。
- **引導式執行（File Bridge）**：分步驟導引任務，逐步留下執行軌跡。

### 安全
- **安全過濾**：偵測疑似密碼／API key／token，封鎖含機密的候選，不允許自動保存。
- **衝突卡控**：同步前偵測違反規則的內容（如 `git config --local user.*`、`Co-Authored-By`、`git push --force` 等），預設擋下，需修正或明確放行。

### 介面
- Linear / Vercel 風格重設計：語意化設計 token、淺色／深色雙主題可切換、側欄分組、克制的靛藍強調色。

### 內部
- Rust 後端單元測試 + 端對端整合測試（對真實檔案系統驗證完整流水線）。
- 自動發版流程：`set-version.mjs` 版本三處同步、`extract-changelog.mjs` 產生 release 內容、GitHub Actions（`tauri-action`）自動打包並發佈安裝器與攜帶版。
