# Vault × Core 整合 — Step 3+4 開發進度交接

> 本檔隨程式碼提交，記錄 adr-002 整合的 Step 3/4 進度，供換機（如公司電腦）續開發。
> 設計依據：amagi-vault 的 `projects/amagi-core/pages/adr/adr-002-vault-core-integration.md`（D1–D10）。
> 分支：`feat/vault-path-managed-pointer`。續開發前先 `git pull` 此分支。

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

## 待辦（依序）

### Step 3b — ingest 來源擴充
- 從「檔案路徑」匯入（讀檔 → 草稿），對應 vault `sources/clips/` 慣例。
- ingest 時保留原始來源到 vault `sources/`，正式頁 frontmatter 加 `source:` 回指。

### Step 3c — sources/clips 自動監看 + 分類器
- 掃 vault `sources/clips/*.md`（Web Clipper 落點）→ 自動產 wiki 候選進佇列。
- 規則式分類器：判定型別（adr/spec/troubleshooting/concept…）與歸層。

### Step 4 — 技能庫分發 + Core 自管 git
- 技能庫單一來源（vault `_skills/`）→ Core 分發原生格式到各 repo 的 .claude/skills、.codex/skills。
- Core 自管 vault git：啟動 pull、變更 commit、收工 push（用 utils/proc::command 組 git，無現成封裝）。

## 待確認（adr-002）
- `troubleshooting` 模板欄位最終定版。
- 技能庫單一來源實體位置（vault `_skills/` vs Core 獨立 store）。
- Core 自管 git 與 Obsidian Git 主從關係。

## 關鍵接點備忘
- 審核佇列儲存：`%APPDATA%/AMAGI Core/review-queue/queue.json`，`core/review_queue` 管理。
- vault 設定：`%APPDATA%/AMAGI Core/vault.json`，`core/vault_manager::get_vault_config`。
- 寫檔備份：`utils/markdown::write_with_backup`（.bak = stem.bak）。
- git 呼叫：`utils/proc::command("git").args([...]).current_dir(p)`。
