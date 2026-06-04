# AMAGI Core — 端對端整合測試報告

> 建立日期：2026-06-05　執行者：天城（AMAGI）
> 對應程式：`src-tauri/src/e2e_test.rs`（`#[cfg(test)]` 內部整合測試模組）

## 1. 目的

先前已有 33 個**單元測試**（零件級，各模組內 `#[cfg(test)]`），但缺少把整條
流水線串起來、對**真實檔案系統**落地的驗證。本次新增 6 條端對端測試，用
「假 Git 專案 + 假 AppData 目錄」實際走完：

```
add_project → init_project → learn(generate_candidates)
  → review_queue(add/list/accept) → conflict gate → agent_exporter::sync
  → 驗證真的落地的檔案內容 → mark_synced
```

## 2. 測試方式

- **沙盒**：`Sandbox` struct 在系統暫存區建立唯一目錄（`amagi-e2e-<tag>-<uuid>`），
  內含 `repo/`（假專案，含 `.git/` 讓 `is_git_repo` 通過）與 `appdata/`（假 `%APPDATA%`）。
  實作 `Drop`，測試結束自動整包刪除。**已確認測試後無殘留暫存目錄。**
- **路徑零硬編**：所有路徑由 `std::env::temp_dir()` + UUID 動態產生，
  不寫死任何個人路徑（符合跨機原則）。
- **不污染家目錄**：技能同步一律用 **Project scope** 測試，
  不測 Global scope（避免寫進老爺真實的 `~/.claude`、`~/.codex`）。

## 3. 涵蓋的測試

| # | 測試 | 驗證內容 |
|---|---|---|
| 1 | `e2e_add_and_init_project` | 專案註冊、重複加入被拒、list/get、init 建立 6 個 `.amagi` 子目錄 + 6 個模板檔、AGENTS/CLAUDE 含「自我分步／不可跳步」紀律、**init 冪等**（再跑不重建） |
| 2 | `e2e_learn_review_sync_memory` | learn 依 README/package/CLAUDE 變更產出 project_rule / tech_stack / agent_rule 候選 → 進佇列 → 跨專案過濾 → 接受 → 衝突閘門放行 → sync 落地 `AGENTS.md`（含分類標頭 + 備份）與 `CLAUDE.md`（只收 agent_rule）→ mark_synced |
| 3 | `e2e_skill_sync_native_format` | 技能同步同時落地 `.claude/skills/<slug>/SKILL.md`、`.codex/skills/<slug>/SKILL.md`、`.amagi/skills/<slug>.md`，且原生 frontmatter 帶 `name`/`description`/`when_to_use`（含觸發關鍵字）、內文保留、三份內容一致 |
| 4 | `e2e_conflict_gate_blocks_bad_memory` | 含 `git config --local` 的記憶被閘門擋下、正確 `--author` 用法放行、理由講得出來（真實 Gomoku 出包內容） |
| 5 | `e2e_safety_filter_blocks_secret_in_learn` | diff 含疑似金鑰時 learn 只產出單一 Blocked 候選（status=Ignored） |
| 6 | `e2e_remove_project` | 移除後清單變空、移除不存在 id 回 Err |

## 4. 遇到的問題與解法

### 問題 1：整合測試抓不到 crate 內部模組
`lib.rs` 的模組是私有的（`mod core;` 而非 `pub mod core;`）。放在 `tests/`
的外部整合測試只能用 public API，抓不到 `core::*`。

**解法**：不改模組可見性（避免擴大公開 API 面），改在 lib 內新增
`#[cfg(test)] mod e2e_test;`。它屬於 crate 本身，能存取私有模組，
且只在 `cargo test` 時編譯，不影響正式 build。

### 問題 2：`.bak` 備份檔名與預期不符（測試斷言錯，非程式錯）
測試原本斷言覆寫 `AGENTS.md` 後會產生 `AGENTS.md.bak`，實際失敗。
追查 `markdown::write_with_backup` 使用 `path.with_extension("bak")`，
而 `with_extension` 是**取代**副檔名，故 `AGENTS.md` 的備份是 `AGENTS.bak`。

**解法**：修正測試斷言為 `AGENTS.bak`。

> **附帶發現（非阻斷）**：此命名會丟掉原副檔名。若同目錄同時存在
> `AGENTS.md` 與假設的 `AGENTS.txt`，兩者備份都會落到 `AGENTS.bak` 互相覆蓋。
> 目前 AMAGI 寫入的檔名彼此不衝突，暫不影響；若日後要更穩，可改成附加式
> （`AGENTS.md.bak`）。已記錄，未修改。

## 5. 邊界與未涵蓋

- **Tauri command 層**（`sync_agent_files` 等）需要執行中的 App + `State<AppState>`，
  無法在純測試環境構造。故本次測「command 實際呼叫的底層函式」，等同其核心行為。
  command 的衝突卡控判斷另由 `commands::sync_commands::tests` 覆蓋
  （`test_gate_flags_conflicting_item` / `test_gate_passes_clean_items`）。
- **前端 / IPC 序列化**：未在此涵蓋（屬 `npm run type-check` 與人工 E2E 範疇）。
- **Global scope 同步**：刻意不測，以免污染真實家目錄。

## 6. 結果

```
cargo test
→ 39 passed; 0 failed（原 33 單元 + 新 6 端對端）
測試後暫存目錄無殘留。
```

## 7. 如何重跑

```powershell
cd "src-tauri"
cargo test            # 全部
cargo test e2e        # 只跑端對端
```
