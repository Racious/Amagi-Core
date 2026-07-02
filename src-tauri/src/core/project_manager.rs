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
    json_store::write_json(&store_path, &data)
}

pub fn init_project(project: &Project) -> Result<InitResult, AppError> {
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

    // ── 寫入 Agent 技能記錄指引 ───────────────────────
    let agent_guide_path = base.join("pending").join("AGENT_INSTRUCTIONS.md");
    if !agent_guide_path.exists() {
        let guide = AGENT_SKILL_INSTRUCTIONS;
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

// ── Agent 技能記錄格式說明（寫入 .amagi/pending/AGENT_INSTRUCTIONS.md）──

const AGENT_SKILL_INSTRUCTIONS: &str = r#"# AMAGI 技能記錄指引

當你（Codex 或 Claude）完成一項**有意義、可重複**的任務後，
請在這個目錄建立一個技能草稿，讓 AMAGI 整合進技能庫。

## 檔名規則
skill-<任務類型>.md
例如：skill-add-api-endpoint.md、skill-fix-typescript-types.md

## 檔案格式
```
---
title: 技能的簡短名稱
scope: project
---

## 描述
這個技能解決什麼問題、適用於什麼情境。

## 何時使用
列出會用到這個技能的情境或觸發關鍵字（例：新增 API、修 TypeScript 型別）。
這段讓未來的 AI 知道何時該套用此技能，請務必填寫。

## 步驟
1. 第一步
2. 第二步
3. ...

## 注意事項
- 這個專案特有的注意點
- 常見錯誤或陷阱
```

## scope 說明
- 技能經審核後一律收進 vault `_skills/`（單一來源）；是否分發到 `.codex`/`.claude`、
  及範圍（本專案或全域）由 Skills 頁選擇性分發決定。
- `project`：傾向只在這個專案使用（預設）
- `global`：傾向所有專案通用

## 什麼情況適合建立技能
- 完成了一個之前沒有標準流程的任務
- 發現了這個專案特有的最佳實踐
- 解決了一個需要特定步驟的問題
- 建立了一個可重複使用的工作流程

## 什麼情況不需要建立技能
- 簡單的一次性修改
- 已有對應技能的任務
- 含有敏感資訊（token、密碼等）的內容
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
        init_project(&project).unwrap();
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

        let first = init_project(&project).unwrap();
        assert!(root.join(".amagi").join("config.json").is_file());
        assert!(!first.created_dirs.iter().any(|d| d.ends_with("pending")), "既有 pending 目錄不應重建");
        assert_eq!(std::fs::read_to_string(&draft).unwrap(), "既有草稿內容", "init 不得覆寫既有草稿");

        let second = init_project(&project).unwrap();
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
