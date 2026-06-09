# Changelog

AMAGI Core 的所有重要變更記錄於此。版本遵循語意化版號（major.minor.patch）。

## v0.1.3

修正應用圖標小尺寸雜點。

### 修正
- 修正 `.ico` 小尺寸層（48px 等）的彩色雜點：改用 sharp 高品質縮放 + `png-to-ico` 組裝，取代會在小尺寸產生雜訊的 `png2icons`。Windows 桌面／開始選單常用的 48px 圖標不再破圖。

## v0.1.2

差異匯出功能、自動更新，以及多項體驗與安全強化。

### 功能
- **差異匯出**：列出專案異動檔（修改／新增／刪除／改名／未追蹤），勾選後產生可複製的 diff 文字，分「異動」與「新增刪除」兩框（JetBrains 風 patch 格式）。
- **自動更新**：整合 `tauri-plugin-updater`，啟動時靜默檢查、有新版顯示橫幅，設定頁亦可手動「檢查更新」；發版經簽章驗證。

### 體驗
- **單一實例**：再次啟動不再另開視窗，而是叫回既有（可能縮在系統匣的）視窗。
- **消除 CMD 黑窗**：呼叫 git／node 等子行程時不再閃出主控台視窗（Windows `CREATE_NO_WINDOW`）。
- **修正雙系統匣圖示**：移除設定檔重複的 tray 設定，右下角只剩一顆。

### 安全
- **敏感內容可檢視**：偵測到疑似機密時，不再「一次擋全部並自動忽略」；改為留一筆「待確認」項，列出觸發規則名與遮罩片段，由使用者判斷真偽（其餘正常候選照常產生）。
- 同步衝突卡控的理由附上命中片段，訊息更具體。

### 內部
- 清除全部編譯警告；新增差異匯出的單元與端對端測試。

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
