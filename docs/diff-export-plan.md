# 差異匯出（Diff Export）— 實作計畫

> 作者：天城（AMAGI）　狀態：待老爺確認後動工
> 目標：在指定專案底下，列出異動檔、勾選想要的，產生可複製的 diff 文字（分兩框）。

## 1. 需求（已與老爺確認鎖定）

- 針對**指定專案**，列出底下所有**異動檔**（修改 / 新增 / 刪除 / 改名 / 未追蹤）。
- 每個檔可**勾選**；只處理勾選的檔案，沒勾的不碰。
- 按鈕產生 diff 文字，分**兩個複製框**：
  - **框 1：異動** — 修改（M）、改名（R），局部 unified diff。
  - **框 2：新增/刪除** — 新增（A/??）、刪除（D），整檔。
- 文字採 **B patch 格式**（JetBrains 風 `Index:` / `===` / `(revision)` / `(date)`），新增整排 `+`、刪除整排 `-`，格式統一。
- 框 2 的個別新檔，額外提供「複製純內容」小鈕（輔助）。
- 比較基準：**工作區 vs HEAD**（`git diff HEAD`，含已暫存與未暫存）。

## 2. 後端設計（Rust / Tauri）

### 2.1 資料模型（models/diff.rs，新增）
```rust
enum ChangedStatus { Modified, Added, Deleted, Renamed, Untracked }
enum DiffGroup { Edited, AddedDeleted }   // 框1 / 框2

struct ChangedFile {
    path: String,
    status: ChangedStatus,
    group: DiffGroup,
    staged: bool,
    is_binary: bool,
    size: Option<u64>,
}

struct DiffBundle {
    edited_patch: String,         // 框1
    added_deleted_patch: String,  // 框2
    skipped: Vec<String>,         // 二進位/過大而略過
    truncated: bool,
}
```
（IPC 結構一律 `#[serde(rename_all = "camelCase")]`。）

### 2.2 指令
```rust
list_changed_files(project_id) -> Vec<ChangedFile>
generate_diff_text(project_id, paths: Vec<String>) -> DiffBundle
```
- `list_changed_files`：`git status --porcelain` → 解析 XY 碼 → 分組
  （M/R→Edited；A/??/D→AddedDeleted）。**未追蹤檔在此就能列出**。
- `generate_diff_text`：對每個勾選路徑，依現況分類後路由：
  - **M / R** → `git diff HEAD -- <path>` → 包成框1。
  - **D** → `git diff HEAD -- <path>`（整排 `-`）→ 框2。
  - **A / ??** → **自合成**（讀檔內容，整排 `+` + 表頭）→ 框2（不呼叫 git、不動 index）。

### 2.3 git_scanner 變更（安全關鍵，謹慎）
- 固定白名單**新增一條**：`["rev-parse", "HEAD"]`（取基準 SHA 供 `(revision)`）。
- **新增「帶路徑」的受控執行函式**（不破壞既有精確比對白名單）：
  - 只建構 `["diff", "HEAD", "--", p1, p2, ...]` 這一種帶路徑指令。
  - 執行前**逐一驗證每個路徑**，任一不過就整批拒絕：
    - 必須相對路徑（拒絕開頭 `/`、磁碟機 `X:`、含 `:`）
    - 不得含 `..`（防跳脫）
    - 不得以 `-` 開頭（防旗標注入）
    - **必須存在於當下 `git status` 回報的集合**（最強保險：前端傳的路徑一定是掃描列出的）
- 維持**唯讀**：只用 `status` / `diff` / `rev-parse`，不碰任何寫入或 index。

### 2.4 格式規格（B patch）
每檔：
```
Index: <path>
===================================================================
<git diff 主體，其中：>
--- a/<path>\t(revision <HEAD SHA>)
+++ b/<path>\t(date <epoch_ms>)
<@@ 區塊…>
```
- 新增檔（自合成）：`--- /dev/null` + `+++ b/<path>\t(date …)` + `@@ -0,0 +1,N @@` + 整排 `+`。
- 刪除檔：git 原生即為 `+++ /dev/null` + 整排 `-`。

### 2.5 邊角處理
- **二進位**（含 NUL / git 標記 binary）：不輸出內容，列入 `skipped`，框內標「Binary file (N bytes)」。
- **過大**：沿用 512KB 上限；超過則截斷並 `truncated=true` + 提示。
- **改名（R）**：歸框1，保留 git 的 rename 表示。
- **無變更 / 檔已不在磁碟**：友善訊息，不報錯。

## 3. 前端設計（Vue）

- **新路由** `/diff-export`，側欄「任務」組新增一項（emoji，如 🧾「差異匯出」）。
- 版面（沿用設計 token：card / btn / input / pill）：
  1. 專案下拉選擇 + 「掃描異動」鈕。
  2. 兩個分組清單（**異動** / **新增·刪除**），每項 checkbox + 狀態 pill（M/A/D/R/??），每組有「全選」。
  3. 「產生 Diff」鈕（對勾選項呼叫 `generate_diff_text`）。
  4. **兩個唯讀結果框**（框1 異動 / 框2 新增刪除），各一顆「複製」鈕（`navigator.clipboard`）。
  5. 框2 個別新檔：額外「複製純內容」小鈕。
  6. 狀態：載入中 / 無異動 / 錯誤 / 被略過清單提示。

## 4. 測試

- **單元**：
  - porcelain 解析（M/A/D/R/?? → 正確分組）
  - 路徑驗證（拒絕 `..`、絕對路徑、`-` 開頭；接受正常相對路徑）
  - 新檔自合成（內容 → 整排 `+`；二進位 → stub）
  - 格式包裝（`Index:` / `===` / `(revision)` / `(date)` 正確）
- **E2E**（暫存 git repo）：
  - 造出「改、增、刪」三檔 → `generate_diff_text(只勾部分)`：
    - `edited_patch` 含被改檔的 hunk
    - `added_deleted_patch` 含新檔 `+`、刪除檔 `-`
    - **未勾選的檔不出現**（驗證範圍卡控）

## 5. 工序與工量

| 順序 | 項目 | 工量 |
|---|---|---|
| 1 | models/diff.rs | 低 |
| 2 | git_scanner：rev-parse HEAD + 帶路徑受控函式 + 路徑驗證 | **中（安全關鍵）** |
| 3 | list_changed_files 指令 | 低 |
| 4 | generate_diff_text 指令（含新檔自合成、格式化） | 中 |
| 5 | 單元 + E2E 測試 | 中 |
| 6 | 前端頁面 + api 接線 | 低～中 |

整體：**中等工量，無技術障礙；最需謹慎者為步驟 2 的安全卡控。**

## 6. 已定的預設（可再改）

- 格式：**B，兩框皆是**。
- 分組：異動＝M/R；新增刪除＝A/??/D。
- 基準：`git diff HEAD`（含暫存＋未暫存）。
- 新檔：框2 主用 patch；另給「複製純內容」輔助鈕。

---

## 7. 實作完成紀錄（2026-06-05）

**狀態：已實作並驗證通過。**

### 新增／修改檔案
- 後端
  - `models/diff.rs`（新）：`ChangedStatus` / `DiffGroup` / `ChangedFile` / `DiffBundle`
  - `core/diff_export.rs`（新）：porcelain 解析、分組、路由、JetBrains 格式化、新檔自合成
  - `core/git_scanner.rs`：允許 `status --porcelain`、`rev-parse HEAD`；新增 `status_porcelain` / `head_sha` / `validate_rel_path` / `diff_one`
  - `commands/diff_commands.rs`（新）：`list_changed_files` / `generate_diff_text`
  - 各 `mod.rs` + `lib.rs` handler 註冊
- 前端
  - `pages/DiffExportPage.vue`（新）：選專案→掃描→兩組勾選→產生→兩框複製
  - `api/tauriCommands.ts`：型別 + `listChangedFiles` / `generateDiffText`
  - `router/index.ts`、`App.vue`：路由 `/diff-export` + 側欄「🧾 差異匯出」

### 測試結果
- Rust：**49 passed / 0 failed**
  - 單元：porcelain 解析分組、staged 旗標、格式改寫（含略過 /dev/null）、新檔合成（文字／二進位略過）、路徑驗證（拒 `..`／絕對／旗標注入）
  - E2E（真實 git repo）：`e2e_diff_export_real_git`——改/增/刪三檔，驗分組、兩框內容、**範圍卡控**（只勾的才出現）、安全（清單外路徑忽略、跳脫路徑報錯）
- 前端：`vue-tsc` exit 0、`vite build` 成功

### 安全卡控（實作確認）
- 路徑須在 `git status` 回報集合內，否則忽略
- `validate_rel_path` 擋 `..`／絕對路徑／磁碟機代號／`-` 開頭
- 全程唯讀（`status` / `diff` / `rev-parse`），不動 git index

### 已知延後項（可選，未做）
- 框2 個別新檔的「複製純內容」輔助鈕——本版先以 patch 格式為主，未實作純內容鈕。
