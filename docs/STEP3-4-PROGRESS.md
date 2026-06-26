# Vault × Core 整合 — Step 3+4 開發進度交接

> 本檔隨程式碼提交，記錄 adr-002 整合的 Step 3/4 進度，供換機（如公司電腦）續開發。
> 設計依據：amagi-vault 的 `projects/amagi-core/pages/adr/adr-002-vault-core-integration.md`（D1–D10）。
> 狀態（2026-06-26）：**已實機驗測 T1~T7 通過、3 缺陷修復、`feat/vault-path-managed-pointer` 已併入 `main`**（merge `e50b430`）。續開發 `git pull` `main` 即可。

## 已完成

### Step 2 — vault 路徑設定 + 受管指針 + 專案映射 ✅（commit a86a2c9）
- `core/vault_manager`：寫全局 ~/.claude/CLAUDE.md 受管區塊（冪等、.bak）。
- `Project.vault_folder` 映射 + `init_project_vault`（建專案知識資料夾骨架）。
- 設定頁 Vault 卡片、專案頁 vault 按鈕。

### Step 3a — wiki 候選管線（核心）✅
- `models/review`：`ReviewItemType::Wiki` 新變體。
- `core/wiki_exporter`：`write_wiki_pages`（依層/型別寫入 vault、非破壞）+ `build_wiki_md`（frontmatter）。
  - 路徑：專案層 `projects/<name>/pages/<category>/<slug>.md`（spec→specs）；general/shared `<layer>/pages/<slug>.md`。
- `commands/wiki_commands`：`ingest_wiki_page`（建草稿進審核佇列）、`write_wiki_pages`（接受並寫入 vault、標記 synced）。
- 前端：新 `IngestPage`（/ingest「知識匯入」）+ 導覽列；`ReviewQueuePage`/`reviewStore` 排除 wiki 型別避免衝突。
- 測試：wiki_exporter 3 項；全 Rust 測試通過、vue-tsc 綠燈。

### Step 3b — 檔案匯入 + 原始來源保存 ✅
- `ingest_wiki_from_file`：讀檔 → 存原文到 vault `sources/imported/` → 建草稿，source 回指。
- `build_wiki_md` 依 `source_pending_file` 輸出 `source:` frontmatter。
- 前端：IngestPage「從檔案匯入…」按鈕（檔案選擇器）。

### Step 3c — sources/clips 掃描 + 分類器 ✅
- `clip_scanner::scan_clips`：掃 vault `sources/clips/*.md` → 去重 → 產 wiki 候選；含 frontmatter 解析。
- 規則式 `classify`：troubleshooting / adr / spec / concept。
- 命令 `scan_vault_clips`、IngestPage「掃描 sources/clips」按鈕。
- 註：採掃描式（非常駐監看）；live file watcher 留作後續增強。

## 待辦（依序）

### Step 4a — 技能庫分發 ✅
- `skill_library`：list_library_skills + distribute（vault `_skills/` → 各 repo .claude/skills、.codex/skills，覆寫受管副本）。
- 命令 list_library_skills / distribute_skill_library；設定頁「技能庫」卡片。
- 註：技能庫 `_skills/` 採**原生目錄式** `_skills/<slug>/SKILL.md`（與 Claude/Codex 慣例及分發輸出一致；亦相容舊扁平式 `<slug>.md`）；未來可由審核佇列 Skill 項目升級。

### Step 4b — Core 自管 git ✅
- `vault_git`：status_short / pull(--ff-only) / sync(add -A → commit 作者 あまぎ → push)。
- 命令 vault_git_status / vault_git_pull / vault_git_sync；設定頁 Vault 卡片加 狀態 / Pull / 提交並推送。

---

## 🎉 全部完成 + 已實機驗測（2026-06-26 換機）
Step 2 + Step 3(a/b/c) + Step 4(a/b) 皆完成，功能完整可用（adr-002 MVP 終點）。
依 `docs/STEP3-4-VERIFICATION.md` 跑完 **T1~T7：T1/T2/T3/T4/T5/T7 通過**，T6 揪出格式缺陷。
共發現 3 缺陷，當日全修並重驗通過（66 單元測試全綠），提交 `db425de`、併入 `main`（`e50b430`）。

### 驗測中發現並修正的缺陷
- **#3（高）技能庫格式判讀**：讀取僅認扁平 `_skills/<slug>.md`，與原生/文件/輸出的目錄式不一致 → 照文件擺技能掃到 0。修法：`skill_library::collect_skills` 優先目錄式、相容扁平式。
- **#1（中）略過草稿落入 UI 死角**：目標已存在而略過時停在 `accepted`，前端只顯示 pending/synced → 草稿消失。修法：`review_queue::mark_pending`，`wiki_commands` 略過者退回 pending。
- **#2（低）檔案匯入重複 H1**：匯出固定前置 `# title`，原檔已自帶 H1。修法：`wiki_exporter::build_wiki_md` 偵測內容首行為 H1 則不前置。

### dev 環境備註（非產品 bug）
- dev 模式分發技能到自身 Amagi-Core repo，會寫進 `.claude`/`.codex` 觸發 vite reload（畫面捲頂、訊息閃失）；打包正式版無此問題。

Phase 3（lint/consolidate）依 adr-002 延後至內容累積後再開。

### 後續可選增強
- 技能庫 `_skills/` 的「升級」入口（審核佇列 Skill → 技能庫）。
- sources/clips 的常駐 file watcher（目前為掃描式）。
- 啟動時自動 vault git pull（目前為手動按鈕）。

## 待確認（adr-002）
- `troubleshooting` 模板欄位最終定版。
- 技能庫單一來源實體位置（vault `_skills/` vs Core 獨立 store）。
- Core 自管 git 與 Obsidian Git 主從關係。

## 關鍵接點備忘
- 審核佇列儲存：`%APPDATA%/AMAGI Core/review-queue/queue.json`，`core/review_queue` 管理。
- vault 設定：`%APPDATA%/AMAGI Core/vault.json`，`core/vault_manager::get_vault_config`。
- 寫檔備份：`utils/markdown::write_with_backup`（.bak = stem.bak）。
- git 呼叫：`utils/proc::command("git").args([...]).current_dir(p)`。
