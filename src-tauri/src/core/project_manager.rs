use std::path::Path;
use chrono::Utc;
use uuid::Uuid;
use crate::AppError;
use crate::models::project::{Project, ProjectInfo, InitResult, ProjectsData};
use crate::utils::{fs_utils, json_store};

pub fn add_project(path: &str, data_dir: &Path) -> Result<Project, AppError> {
    let path = path.trim_end_matches(['/', '\\']);

    if !Path::new(path).exists() {
        return Err(AppError::InvalidPath(format!("路徑不存在：{}", path)));
    }
    // 註冊閘（2026-07-03 事故）：vault 根（或其內任何路徑）被註冊成專案後，
    // sync/init 會把 vault 根 CLAUDE.md（Wiki 規範源頭）整檔覆寫成專案指針。
    // canonical 比對吸收大小寫/斜線/symlink 變體。
    reject_path_inside_vault(path, data_dir)?;
    if !fs_utils::is_git_repo(path) {
        return Err(AppError::InvalidPath(format!("不是 Git 儲存庫：{}", path)));
    }

    let name = Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    let vault_folder = Some(format!("projects/{}", fs_utils::slugify(&name)));

    let project = Project {
        id: Uuid::new_v4().to_string(),
        name,
        path: path.to_string(),
        created_at: Utc::now(),
        last_scanned_at: None,
        initialized: false,
        vault_folder,
    };

    let store_path = data_dir.join("projects.json");
    let mut data: ProjectsData = json_store::read_json_or_default(&store_path);

    if data.projects.iter().any(|p| p.path == project.path) {
        return Err(AppError::InvalidPath(format!("專案已存在：{}", project.path)));
    }

    data.projects.push(project.clone());
    json_store::write_json(&store_path, &data)?;

    Ok(project)
}

/// 專案路徑等於 vault 根、或位於 vault 根之下 → 拒絕（vault 是知識庫，非專案）。
/// vault 未設定 → 放行（無從比對；本閘只防「把知識庫當專案」的誤用）。
fn reject_path_inside_vault(path: &str, data_dir: &Path) -> Result<(), AppError> {
    if let Some(vp) = crate::core::vault_manager::get_vault_config(data_dir).vault_path {
        if fs_utils::is_same_or_under(Path::new(&vp), Path::new(path)) {
            return Err(AppError::InvalidPath(format!(
                "此路徑位於 Amagi-Vault 知識庫內（vault：{vp}）。vault 是知識庫、非專案，\
                 不可註冊為專案——否則同步會覆寫 vault 根的 CLAUDE.md 等規範文件。"
            )));
        }
    }
    Ok(())
}

pub fn list_projects(data_dir: &Path) -> Vec<Project> {
    let store_path = data_dir.join("projects.json");
    let data: ProjectsData = json_store::read_json_or_default(&store_path);
    data.projects
}

pub fn get_project(project_id: &str, data_dir: &Path) -> Option<Project> {
    let store_path = data_dir.join("projects.json");
    let data: ProjectsData = json_store::read_json_or_default(&store_path);
    data.projects.into_iter().find(|p| p.id == project_id)
}

pub fn remove_project(project_id: &str, data_dir: &Path) -> Result<(), AppError> {
    let store_path = data_dir.join("projects.json");
    let mut data: ProjectsData = json_store::read_json_or_default(&store_path);
    let before = data.projects.len();
    data.projects.retain(|p| p.id != project_id);
    if data.projects.len() == before {
        return Err(AppError::ProjectNotFound(project_id.to_string()));
    }
    json_store::write_json(&store_path, &data)?;
    // 連帶清佇列殘項（2026-07-03 批測發現④）為 best-effort 資料衛生（r1 中風險裁決）：
    // 專案移除才是使用者意圖，projects.json 已成功寫入；即使佇列清理失敗也不讓整個操作回報失敗——
    // 否則專案已移除、UI 卻顯示失敗，且再移除同 id 會撞 ProjectNotFound 提早返回而永遠無法重試清理。
    // 罕見 IO 失敗下殘留的孤兒項不比本功能實作前更糟；失敗僅記錄，不阻斷。
    if let Err(e) = crate::core::review_queue::remove_items_of_project(data_dir, project_id) {
        eprintln!("[AMAGI] 專案 {project_id} 已移除，但佇列殘項清理失敗（孤兒項殘留）：{e}");
    }
    Ok(())
}

pub fn init_project(project: &Project, vault_root: Option<&Path>) -> Result<InitResult, AppError> {
    // 防守深度（2026-07-03 事故）：專案路徑落在 vault 內 → 拒建，
    // 否則會在 vault 根生成 AGENTS.md、覆寫規範文件（第一道閘在 add_project，此處擋存量資料）。
    if let Some(vroot) = vault_root {
        if fs_utils::is_same_or_under(vroot, Path::new(&project.path)) {
            return Err(AppError::InvalidPath(format!(
                "專案路徑「{}」位於 Amagi-Vault 知識庫內，拒絕初始化——\
                 vault 是知識庫、非專案，請自專案清單移除該項目。",
                project.path
            )));
        }
    }
    let base = Path::new(&project.path).join(".amagi");
    let dirs = ["memory", "pending", "skills", "history", "artifacts", "state"];

    let mut created_dirs = Vec::new();
    let mut created_files = Vec::new();

    for dir in &dirs {
        let d = base.join(dir);
        if !d.exists() {
            std::fs::create_dir_all(&d)
                .map_err(|e| AppError::Io(e.to_string()))?;
            created_dirs.push(d.to_string_lossy().to_string());
        }
    }

    let config_path = base.join("config.json");
    if !config_path.exists() {
        let config = serde_json::json!({
            "projectId": project.id,
            "projectName": project.name,
            "createdAt": project.created_at,
        });
        json_store::write_json(&config_path, &config)?;
        created_files.push(config_path.to_string_lossy().to_string());
    }

    // ── 寫入 Agent 投遞指引（技能＋記憶雙通道）─────────
    // 註：既有檔案不覆寫（尊重使用者自訂）→ **既有專案不會自動拿到記憶通道說明**，
    // 需手動更新或刪檔重 init。更新策略為產品決策，未納入本輪。
    let agent_guide_path = base.join("pending").join("AGENT_INSTRUCTIONS.md");
    if !agent_guide_path.exists() {
        let guide = AGENT_PENDING_INSTRUCTIONS;
        std::fs::write(&agent_guide_path, guide)
            .map_err(|e| AppError::Io(e.to_string()))?;
        created_files.push(agent_guide_path.to_string_lossy().to_string());
    }

    // ── 寫入 workflow-state.md（任務狀態檔）─────────────
    let workflow_state_path = base.join("workflow-state.md");
    if !workflow_state_path.exists() {
        std::fs::write(&workflow_state_path, WORKFLOW_STATE_TEMPLATE)
            .map_err(|e| AppError::Io(e.to_string()))?;
        created_files.push(workflow_state_path.to_string_lossy().to_string());
    }

    // ── 寫入 after-task-review.md（任務回顧清單）────────
    let review_path = base.join("after-task-review.md");
    if !review_path.exists() {
        std::fs::write(&review_path, AFTER_TASK_REVIEW_TEMPLATE)
            .map_err(|e| AppError::Io(e.to_string()))?;
        created_files.push(review_path.to_string_lossy().to_string());
    }

    // ── 寫入 AGENTS.md / CLAUDE.md（含技能記錄規範 + vault 指針）──────
    // 權威來源：Project.vault_folder；缺時才退回 slug(name)。
    let vault_folder = project.vault_folder.clone()
        .unwrap_or_else(|| crate::core::agent_exporter::project_vault_folder(&project.path));
    let agents_md = Path::new(&project.path).join("AGENTS.md");
    if !agents_md.exists() {
        std::fs::write(&agents_md, crate::utils::markdown::build_agents_md(&vault_folder, ""))
            .map_err(|e| AppError::Io(e.to_string()))?;
        created_files.push(agents_md.to_string_lossy().to_string());
    }

    // ── 寫入 CLAUDE.md（含技能記錄規範）─────────────
    let claude_md = Path::new(&project.path).join("CLAUDE.md");
    if !claude_md.exists() {
        std::fs::write(&claude_md, crate::utils::markdown::build_claude_md(Some(&vault_folder), ""))
            .map_err(|e| AppError::Io(e.to_string()))?;
        created_files.push(claude_md.to_string_lossy().to_string());
    }

    // ── 補 .gitignore 派生物規則（冪等、非破壞，Phase 1b）──────
    if ensure_gitignore_rules(&project.path).map_err(|e| AppError::Io(e.to_string()))? {
        created_files.push(
            Path::new(&project.path).join(".gitignore").to_string_lossy().to_string(),
        );
    }

    Ok(InitResult {
        project_id: project.id.clone(),
        created_dirs,
        created_files,
    })
}

/// AMAGI Core 派生物的 gitignore 規則（受管副本/工作區，不進版控；根 AGENTS.md/CLAUDE.md 不在內，須保留版控）。
const MANAGED_GITIGNORE: &[&str] = &[".amagi/", ".codex/skills/", ".claude/skills/"];

/// 冪等、非破壞地把派生物規則補進專案 `.gitignore`：只追加「缺少」的規則行、
/// 保留既有內容；無檔則建立。回傳是否實際寫入。
pub(crate) fn ensure_gitignore_rules(project_path: &str) -> std::io::Result<bool> {
    let path = Path::new(project_path).join(".gitignore");
    // 非破壞：僅「檔案不存在」視為空檔；其他讀取錯誤回傳 Err，絕不覆寫未知內容。
    let existing = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e),
    };
    let have: std::collections::HashSet<String> =
        existing.lines().map(|l| l.trim().to_string()).collect();
    let missing: Vec<&str> = MANAGED_GITIGNORE
        .iter()
        .copied()
        .filter(|r| !have.contains(*r))
        .collect();
    if missing.is_empty() {
        return Ok(false);
    }
    let mut content = existing;
    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }
    content.push_str("\n# AMAGI Core 派生物（受管副本/工作區，不進版控）\n");
    for r in &missing {
        content.push_str(r);
        content.push('\n');
    }
    std::fs::write(&path, content)?;
    Ok(true)
}

/// 在 vault 內為專案建立知識資料夾骨架（adr-002 D5）。
/// 非破壞：所有目錄與檔案僅在不存在時建立，不覆寫既有手做內容（D7）。
pub fn init_project_vault(project: &Project, vault_root: &Path) -> Result<InitResult, AppError> {
    if !vault_root.is_dir() {
        return Err(AppError::InvalidPath(format!(
            "vault 路徑不存在：{}",
            vault_root.display()
        )));
    }

    let folder = project
        .vault_folder
        .clone()
        .unwrap_or_else(|| crate::core::agent_exporter::project_vault_folder(&project.path));
    let base = vault_root.join(&folder);

    let mut created_dirs = Vec::new();
    let mut created_files = Vec::new();

    // 三桶結構（adr-004 D3）：knowledge/、reports/ 主動建；agent/ 按需（記憶/指針寫入時才生）。
    // 資料夾按「用途」分（固定），內容種類靠 frontmatter `type` 細分。
    for sub in ["knowledge", "reports"] {
        let d = base.join(sub);
        if !d.exists() {
            std::fs::create_dir_all(&d).map_err(|e| AppError::Io(e.to_string()))?;
            created_dirs.push(d.to_string_lossy().to_string());
        }
        let keep = d.join(".gitkeep");
        if !keep.exists() {
            std::fs::write(&keep, "").map_err(|e| AppError::Io(e.to_string()))?;
        }
    }

    let index_md = base.join("index.md");
    if !index_md.exists() {
        std::fs::write(&index_md, build_project_index_md(&project.name))
            .map_err(|e| AppError::Io(e.to_string()))?;
        created_files.push(index_md.to_string_lossy().to_string());
    }

    Ok(InitResult {
        project_id: project.id.clone(),
        created_dirs,
        created_files,
    })
}

fn build_project_index_md(name: &str) -> String {
    format!(
        r#"# {name} — 專案知識目錄

> 由 Amagi Core 建立。對話開始時，涉本專案先讀 [handoff.md](handoff.md)（當前狀態活頁），再讀本檔了解知識目錄與背景。

---

## 知識頁面

> 三桶結構（adr-004 D3）：資料夾按「用途」分（固定），內容種類靠 frontmatter `type`（可增）。

| 桶 | 目錄 | 裝什麼 |
|------|------|------|
| 知識 | [knowledge/](knowledge/) | 人看、可發布 docs/（`type`: adr/spec/business/concept/troubleshooting）|
| 報告 | [reports/](reports/) | 稽核紀錄（`type`: test-report/review）|
| 交接 | [handoff.md](handoff.md) | 當前狀態活頁（`type`: handoff；覆寫式快照、單一真實來源）|
| AI 私有 | agent/ | 長期記憶、指針（按需建立）|

（交接 handoff → 本專案活頁 [handoff.md](handoff.md)，覆寫式快照、單一真實來源；不再落頂層 daily/。）

### 頁面清單

| 頁面 | 型別 | 重要性 | 更新日 |
|------|------|--------|--------|
| （尚無頁面） | | | |

---

*新增頁面時請更新本清單。*
"#,
        name = name
    )
}

// ── Agent 投遞格式說明（寫入 .amagi/pending/AGENT_INSTRUCTIONS.md）──
// 涵蓋技能與記憶雙通道（P1，2026-08-17）：原僅技能，AI 無記憶投遞入口 → vault 記憶區長期為空。
// 內容須與 `pending_scanner::{SKILL_KIND, MEMORY_KIND}` 的檔名前綴與 frontmatter 解析一致。

const AGENT_PENDING_INSTRUCTIONS: &str = r#"# AMAGI 投遞指引（技能 ＋ 記憶）

這個目錄是 **AI → AMAGI Core** 的投遞口。你（Codex 或 Claude）在此建立草稿，
AMAGI 會在使用者按下「學習變更」時掃入審核佇列，經使用者核可後寫入 vault 知識庫。

兩種投遞類型，**依檔名前綴區分**：

| 前綴 | 用途 | 例 |
|---|---|---|
| `skill-` | 可重複執行的**做法／流程** | `skill-add-api-endpoint.md` |
| `memory-` | 值得長期記住的**事實／踩坑／限制** | `memory-ps51-bom-trap.md` |

> 兩者都會經過安全過濾。含 token、密碼、金鑰等敏感內容的檔案**不會入列**，
> 但會在學習結果中列出檔名與命中規則，請修正後再次學習。

---

## 何時投遞

**任務收尾的 after-task-review 階段**——這是最自然的時機。剛做完一件事，你才知道：

- 哪個坑會再踩一次 → 投 `memory-`
- 哪個流程下次還會走一遍 → 投 `skill-`

不必每個任務都投。**沒有可重複價值的就別投**，噪音比空白更糟。

---

## 記憶投遞：`memory-<主題>.md`

### 格式

```markdown
---
title: PS5.1 寫 JSON 帶 BOM 會讓 app 靜默清空資料
category: gotcha
scope: shared
---

PowerShell 5.1 的 `Out-File -Encoding utf8` 會寫入 BOM，serde 解析失敗後
`read_or_default` 靜默回傳預設值 → 使用者資料被清空且無錯誤訊息。

寫 app 要讀的 JSON 一律用 `[System.IO.File]::WriteAllText()`（無 BOM）。
```

### 欄位

| 欄位 | 必要 | 說明 |
|---|:--:|---|
| `title` | 建議 | 一句話講完那件事。留空會 fallback 成檔名（可讀性差，請自己寫） |
| `category` | 選填 | 自由分類字串，如 `gotcha`／`workflow`／`constraint`／`decision`。未給則為 `agent_note` |
| `scope` | 選填 | `project`（預設）／`shared`／`global`。**不確定就用 project** |

正文即記憶內容。**寫給未來的 AI 看**：講清楚現象、原因、正解，而不是「這裡要注意」。

### scope 怎麼選

| scope | 落點 | 用在什麼記憶 |
|---|---|---|
| `project` | `projects/<專案>/agent/memory` | 只有這個專案成立的事（預設，最安全） |
| `shared` | `shared/agent/memory` | 跨專案通用的技術踩坑、工具行為 |
| `global` | `general/agent/memory` | **每次對話都會載入**：使用者偏好、通用紀律 |

⚠ **`global` 請節制**。它會進使用者的全域 `CLAUDE.md`／`AGENTS.md` 錨點，
所有專案的所有 AI 每次對話都讀到。錯的全域記憶不只是一個檔案錯，
是**已經污染了後續每一次判斷**。不確定時投 `project`，使用者可在 UI 升級。

### 寫得好 vs 寫不好

| 沒有資訊量 | 未來的 AI 用得上 |
|---|---|
| 「注意 JSON 編碼問題」 | 「PS5.1 `Out-File -Encoding utf8` 帶 BOM → serde 失敗 → `read_or_default` 靜默清空資料；改用 `WriteAllText`」 |
| 「記得跑測試」 | 「`cargo test` 綠不代表新行為被測到——UI 改動一律走實機驗證」 |
| 「git 要小心」 | 「`pull --rebase --autostash` 套回衝突時 exit 0 卻留 UU 標記；成功路徑也必須查 unmerged」 |

---

## 技能投遞：`skill-<任務類型>.md`

### 格式

```markdown
---
title: 技能的簡短名稱
scope: project
---

## 描述
這個技能解決什麼問題、適用於什麼情境。

## 何時使用
會用到的情境或觸發關鍵字（例：新增 API、修 TypeScript 型別）。
這段讓未來的 AI 知道何時該套用，請務必填寫。

## 步驟
1. 第一步
2. 第二步

## 注意事項
- 這個專案特有的注意點
- 常見錯誤或陷阱
```

### scope 說明

技能經審核後一律收進 vault `_skills/`（單一來源）；是否分發到 `.codex`/`.claude`
由 Skills 頁決定。`scope` 表達傾向：`project`（預設）或 `global`（所有專案通用）。

### 什麼情況適合建立技能

- 完成了一個之前沒有標準流程的任務
- 發現了這個專案特有的最佳實踐
- 解決了一個需要特定步驟的問題

### 什麼情況不需要

- 簡單的一次性修改
- 已有對應技能的任務

---

## 投遞後會發生什麼

1. 使用者按「學習變更」→ 你的檔案被掃入審核佇列
2. 審核頁可**編輯內容、改 scope**後才核可（你寫的不是最終版，使用者是最後一關）
3. 核可 → 同步 → 寫入 vault，索引與 `CLAUDE.md`／`AGENTS.md` 內聯自動重建
4. 你的原始檔移入 `.amagi/history/` 歸檔（不會重複入列）

被安全過濾擋下的檔案**留在原處**，每次學習都會再提醒一次，直到你修好或刪掉。
"#;

// ── workflow-state.md 初始模板（任務狀態檔）──────────────────────

const WORKFLOW_STATE_TEMPLATE: &str = r#"# Workflow State

> 任務進行中的軌跡檔。接到非簡單任務時，先在此列「計畫步驟」，逐步執行、每步寫「步驟結果」
> （不跳步、不併步、寫完該步才前進）。完成並 commit 後歸檔 `.amagi/history/`、本檔重置。

## Current Task
（無進行中的任務）

## Workflow Type
（feature-dev / bug-fix / quick-task / commit-pr）

## 計畫步驟（分析後列出，不跳步不併步；完成打勾）
- [ ] 1. 分析：拆問題、查現況、定位檔案與風險
- [ ] 2. 制訂計畫（複雜時待老爺核可）
- [ ] 3. 實作（逐步，每步補下方「步驟結果」）
- [ ] 4. 自我驗證（build / test 綠燈；完成＝有客觀證據）
- [ ] 4.5 交叉審查（實質程式碼變更：交另一方 AI 審，含完成度稽核）
- [ ] 5. after-task-review（依 after-task-review.md 回顧）
- [ ] 6. commit 預覽（待老爺確認才 commit）

## 步驟結果（逐步追加，寫完一步才前進；證據＝改了哪些檔／build·test 結果）
### 步驟 N：<步驟名>
- 做了什麼：
- 改了哪些檔／產出：
- build·test 結果（證據）：

## Blockers
（卡住的原因，無則留空）

## Next Step
（下一步要做什麼）
"#;

// ── after-task-review.md 模板（任務完成回顧清單）──────────────────

const AFTER_TASK_REVIEW_TEMPLATE: &str = r#"# After Task Review（任務完成回顧）

完成一項有意義的任務後，依序確認以下項目：

## 1. 是否需要新的 skill？
若這類任務未來可能再發生，在 `.amagi/pending/` 建立 `skill-<任務類型>.md`。
（這是本專案的硬性規範，見 CLAUDE.md / AGENTS.md）

## 2. 是否需要更新既有 skill？
若使用了某個 skill，並發現更好的做法：
- 補充步驟
- 補充常見錯誤
- 補充範例

## 3. 是否有值得保存的記憶？
專案脈絡、技術決策、踩坑解法，值得記錄的寫進 `.amagi/memory/`。

## 4. Commit 建議
若有程式碼或文件變更：
- 摘要變更的檔案
- 提出 commit message
- **詢問老爺是否 commit（確認前不得自行 commit）**
"#;

// 註：專案根 AGENTS.md／CLAUDE.md 的內容統一由 `markdown::build_agents_md`／`build_claude_md`
// 生成（指針＋記憶內聯＋工作流薄錨），init 與 sync 共用同一來源，杜絕「init 寫豐富版、
// sync 用薄版覆寫」的分歧。開發工作流 doctrine 全文置於全域錨點（~/.claude、~/.codex）。

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::project::Project;

    fn make_project(root: &Path) -> Project {
        Project {
            id: "test-id".to_string(),
            name: "test-project".to_string(),
            path: root.to_string_lossy().to_string(),
            created_at: Utc::now(),
            last_scanned_at: None,
            initialized: false,
            vault_folder: Some("projects/test-project".to_string()),
        }
    }

    /// 建 temp 沙盒：data_dir（含指向 vault 的 vault.json）+ vault git repo + 正常專案 git repo。
    fn vault_guard_sandbox() -> (std::path::PathBuf, std::path::PathBuf, std::path::PathBuf) {
        let base = std::env::temp_dir().join(format!("amagi-vguard-{}", Uuid::new_v4()));
        let data_dir = base.join("data");
        let vault = base.join("vault");
        let repo = base.join("repo");
        for d in [&data_dir, &vault.join(".git"), &repo.join(".git")] {
            std::fs::create_dir_all(d).unwrap();
        }
        json_store::write_json(
            &data_dir.join("vault.json"),
            &crate::core::vault_manager::VaultConfig {
                vault_path: Some(vault.to_string_lossy().to_string()),
                pointer_written: true,
            },
        ).unwrap();
        (base, data_dir, vault)
    }

    #[test]
    fn test_add_project_rejects_vault_root_and_children() {
        let (base, data_dir, vault) = vault_guard_sandbox();

        // vault 根本身 → 拒絕
        let err = add_project(&vault.to_string_lossy(), &data_dir).unwrap_err();
        assert!(format!("{err:?}").contains("知識庫"), "錯誤訊息應說明 vault 是知識庫非專案：{err:?}");

        // vault 根之下的子路徑（本身也是 git repo）→ 拒絕
        let sub = vault.join("projects").join("inner");
        std::fs::create_dir_all(sub.join(".git")).unwrap();
        assert!(add_project(&sub.to_string_lossy(), &data_dir).is_err(), "vault 子路徑應被拒");

        // 大小寫變體（Windows 同一目錄）→ 拒絕
        #[cfg(windows)]
        {
            let upper = vault.to_string_lossy().to_uppercase();
            assert!(add_project(&upper, &data_dir).is_err(), "vault 路徑大小寫變體應被拒");
        }

        // 尾斜線變體 → 拒絕
        let trailing = format!("{}\\", vault.to_string_lossy());
        assert!(add_project(&trailing, &data_dir).is_err(), "vault 路徑尾斜線變體應被拒");

        // 正常專案 → 通過
        let repo = base.join("repo");
        assert!(add_project(&repo.to_string_lossy(), &data_dir).is_ok(), "正常專案應可註冊");

        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn test_add_project_without_vault_config_unaffected() {
        // vault 未設定 → 閘不生效，正常註冊
        let base = std::env::temp_dir().join(format!("amagi-vguard-nocfg-{}", Uuid::new_v4()));
        let data_dir = base.join("data");
        let repo = base.join("repo");
        std::fs::create_dir_all(&data_dir).unwrap();
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        assert!(add_project(&repo.to_string_lossy(), &data_dir).is_ok());
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn test_init_project_rejects_path_inside_vault() {
        // 防守深度：專案路徑等於/位於 vault 根 → 拒建，vault 根不得長出 AGENTS.md/.amagi
        let base = std::env::temp_dir().join(format!("amagi-initguard-{}", Uuid::new_v4()));
        let vault = base.join("vault");
        std::fs::create_dir_all(&vault).unwrap();
        std::fs::write(vault.join("CLAUDE.md"), "# Wiki 規範源頭").unwrap();

        // vault 根本身
        let p_root = make_project(&vault);
        assert!(init_project(&p_root, Some(&vault)).is_err(), "vault 根應拒絕初始化");
        // vault 內子路徑
        let sub = vault.join("projects").join("x");
        std::fs::create_dir_all(&sub).unwrap();
        assert!(init_project(&make_project(&sub), Some(&vault)).is_err(), "vault 子路徑應拒絕初始化");
        // vault 未被污染
        assert!(!vault.join("AGENTS.md").exists(), "vault 根不得生成 AGENTS.md");
        assert!(!vault.join(".amagi").exists(), "vault 根不得生成 .amagi");
        assert_eq!(std::fs::read_to_string(vault.join("CLAUDE.md")).unwrap(), "# Wiki 規範源頭",
            "vault 根 CLAUDE.md 不得被覆寫");

        // vault 外正常專案 → 通過
        let repo = base.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        assert!(init_project(&make_project(&repo), Some(&vault)).is_ok(), "vault 外專案應可初始化");

        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn test_pending_without_config_is_not_initialized() {
        // agent 在 init 前自建 .amagi/pending/ 草稿，不得誤判為已初始化
        let root = std::env::temp_dir().join(format!("amagi-initjudge-{}", Uuid::new_v4()));
        std::fs::create_dir_all(root.join(".amagi").join("pending")).unwrap();
        std::fs::write(root.join(".amagi").join("pending").join("skill-draft.md"), "draft").unwrap();

        let info = get_project_info(&make_project(&root));
        assert!(!info.initialized, "只有 .amagi/pending/ 而無 config.json，應視為未初始化");

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn test_init_project_marks_initialized() {
        let root = std::env::temp_dir().join(format!("amagi-initdone-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let project = make_project(&root);

        assert!(!get_project_info(&project).initialized);
        init_project(&project, None).unwrap();
        assert!(get_project_info(&project).initialized, "init_project 後 config.json 存在，應視為已初始化");

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn test_init_project_idempotent_fills_missing_keeps_existing() {
        // 先有 agent 自建的 pending 草稿：init 應補齊骨架、不動既有內容；重跑無新產物
        let root = std::env::temp_dir().join(format!("amagi-initidem-{}", Uuid::new_v4()));
        let draft = root.join(".amagi").join("pending").join("skill-draft.md");
        std::fs::create_dir_all(draft.parent().unwrap()).unwrap();
        std::fs::write(&draft, "既有草稿內容").unwrap();
        let project = make_project(&root);

        let first = init_project(&project, None).unwrap();
        assert!(root.join(".amagi").join("config.json").is_file());
        assert!(!first.created_dirs.iter().any(|d| d.ends_with("pending")), "既有 pending 目錄不應重建");
        assert_eq!(std::fs::read_to_string(&draft).unwrap(), "既有草稿內容", "init 不得覆寫既有草稿");

        let second = init_project(&project, None).unwrap();
        assert!(second.created_dirs.is_empty(), "重跑 init 不應再建目錄");
        assert!(second.created_files.is_empty(), "重跑 init 不應再建檔案");

        std::fs::remove_dir_all(&root).ok();
    }
}

pub fn get_project_info(project: &Project) -> ProjectInfo {
    let branch = crate::utils::proc::command("git")
        .args(["branch", "--show-current"])
        .current_dir(&project.path)
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    ProjectInfo {
        id: project.id.clone(),
        name: project.name.clone(),
        path: project.path.clone(),
        is_git_repo: fs_utils::is_git_repo(&project.path),
        current_branch: branch,
        // 以 init 專屬產物 config.json 判定；agent 可能先自建 .amagi/pending/ 草稿，
        // 只看 .amagi/ 目錄會誤判已初始化（init_project 本身冪等，重跑安全）
        initialized: Path::new(&project.path).join(".amagi").join("config.json").is_file(),
        pending_review_count: 0,
        vault_folder: project.vault_folder.clone()
            .or_else(|| Some(crate::core::agent_exporter::project_vault_folder(&project.path))),
        // 與後端 distribute_selective 的 is_dir 判斷一致：路徑被同名檔案取代也算不可分發
        path_exists: Path::new(&project.path).is_dir(),
    }
}
