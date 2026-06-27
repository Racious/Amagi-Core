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
        std::fs::write(&agents_md, build_initial_agents_md(&project.name, &vault_folder))
            .map_err(|e| AppError::Io(e.to_string()))?;
        created_files.push(agents_md.to_string_lossy().to_string());
    }

    // ── 寫入 CLAUDE.md（含技能記錄規範）─────────────
    let claude_md = Path::new(&project.path).join("CLAUDE.md");
    if !claude_md.exists() {
        std::fs::write(&claude_md, build_initial_claude_md(&project.name, &vault_folder))
            .map_err(|e| AppError::Io(e.to_string()))?;
        created_files.push(claude_md.to_string_lossy().to_string());
    }

    Ok(InitResult {
        project_id: project.id.clone(),
        created_dirs,
        created_files,
    })
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

    for sub in ["pages/adr", "pages/specs", "pages/business"] {
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

> 由 Amagi Core 建立。對話開始時，天城先閱讀本檔了解專案背景。

---

## 知識頁面

| 分類 | 目錄 | 內容 |
|------|------|------|
| 架構決策 | pages/adr | 技術選型、設計決策紀錄 |
| 介面規格 | pages/specs | command 與 API 規格 |
| 商業邏輯 | pages/business | 功能流程說明 |

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
- `project`：只在這個專案使用（預設）
- `global`：所有專案通用，AMAGI 會寫入 ~/.codex/skills 和 ~/.claude/commands

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

> 任務進行中的狀態檔。接到非簡單任務時，由 AI 更新此檔；
> 完成並 commit 後可清空回到此初始狀態。

## Current Task
（無進行中的任務）

## Workflow Type
（feature-dev / bug-fix / quick-task / commit-pr）

## Current Step
（目前進行到哪一步）

## Completed Steps
- [ ] classify-task（任務分類）
- [ ] search-existing-code（搜尋既有程式碼）
- [ ] check-skill（查閱 .claude/commands/ 是否有規範）
- [ ] create-skill-draft（建立 .amagi/pending/ 技能草稿）
- [ ] implement（實作）
- [ ] verify（build / test 驗證）
- [ ] after-task-review（依 after-task-review.md 回顧）
- [ ] commit-check（詢問是否 commit）

## Blockers
（卡住的原因，無則留空）

## Artifacts
（本次任務產出的檔案，例如 .amagi/artifacts/xxx.md）

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

fn build_initial_agents_md(project_name: &str, vault_folder: &str) -> String {
    let pointer = crate::utils::markdown::build_vault_pointer_block(vault_folder, true);
    format!(r#"# {project_name} — Agent 工作記憶

> 由 AMAGI Core 自動建立。本檔案記錄此專案的規則、技術棧與注意事項。

{pointer}

## 執行方式：自我分步 + 留下軌跡（重要）

接到任何任務，先做以下事，再開始動工：

1. **拆步驟**：把任務拆成數個明確步驟，先在 `.amagi/state/step-results.md` 寫出「步驟清單」。
2. **逐步執行**：一次只做一步。
3. **每步寫 result**：每完成一步，立刻在 `.amagi/state/step-results.md` 追加一段：
   ```
   ## 步驟 N：<步驟名>
   - 做了什麼
   - 產出/修改的檔案
   - 結果
   ```
4. **寫完才前進**：該步的 result 寫好，才能做下一步。
5. **不可跳步、不可合併步驟。**

> 這份逐步軌跡讓使用者事後能完整看到你每一步怎麼走的。

---

## 標準步驟（拆步驟時務必包含這些）

1. **確認可用技能**：相關技能會自動載入（原生 Skills，位於 `.claude/skills/`）。有對應技能 → 照它做；無 → 按工程判斷。
2. **實作功能**。
3. **驗證**：跑 build / test，確認沒錯。
4. **建立 skill 草稿**：在 `.amagi/pending/` 建 `skill-<任務類型>.md`，記錄這次的可重複流程（格式見下）。
5. **詢問是否 commit**：向使用者報告，⛔ **使用者確認前不得自行執行 git commit**。

### skill 草稿格式
```
---
title: 簡短描述這個技能
scope: project
---

## 描述
這個技能解決什麼問題、適用於什麼情境。

## 何時使用
列出會用到這個技能的情境或觸發關鍵字（例：新增 API、四則運算）。
這段讓未來的 AI 知道何時該套用此技能，請務必填寫。

## 步驟
1. ...

## 注意事項
- 常見錯誤或陷阱
```

---

<!-- AMAGI 會在這裡自動插入審核通過的記憶與規則 -->
"#, project_name = project_name, pointer = pointer)
}

fn build_initial_claude_md(project_name: &str, vault_folder: &str) -> String {
    let pointer = crate::utils::markdown::build_vault_pointer_block(vault_folder, false);
    format!(r#"# {project_name} — Claude 工作規則

> 由 AMAGI Core 自動建立。本檔案記錄 Claude 在此專案應遵守的規則。

{pointer}

## 執行方式：自我分步 + 留下軌跡（重要）

接到任何任務，先做以下事，再開始動工：

1. **拆步驟**：把任務拆成數個明確步驟，先在 `.amagi/state/step-results.md` 寫出「步驟清單」。
2. **逐步執行**：一次只做一步。
3. **每步寫 result**：每完成一步，立刻在 `.amagi/state/step-results.md` 追加一段：
   ```
   ## 步驟 N：<步驟名>
   - 做了什麼
   - 產出/修改的檔案
   - 結果
   ```
4. **寫完才前進**：該步的 result 寫好，才能做下一步。
5. **不可跳步、不可合併步驟。**

> 這份逐步軌跡讓老爺事後能完整看到你每一步怎麼走的。

---

## 標準步驟（拆步驟時務必包含這些）

1. **確認可用技能**：相關技能會自動載入（原生 Skills，位於 `.claude/skills/`）。有對應技能 → 照它做；無 → 按工程判斷。
2. **實作功能**。
3. **驗證**：跑 build / test，確認沒錯。
4. **建立 skill 草稿**：在 `.amagi/pending/` 建 `skill-<任務類型>.md`，記錄這次的可重複流程（格式見下）。
5. **詢問是否 commit**：向老爺報告，⛔ **老爺確認前不得自行執行 git commit**。

### skill 草稿格式
```
---
title: 簡短描述這個技能
scope: project
---

## 描述
這個技能解決什麼問題、適用於什麼情境。

## 何時使用
列出會用到這個技能的情境或觸發關鍵字（例：新增 API、四則運算）。
這段讓未來的 AI 知道何時該套用此技能，請務必填寫。

## 步驟
1. ...

## 注意事項
- 常見錯誤或陷阱
```

---

<!-- AMAGI 會在這裡自動插入審核通過的規則 -->
"#, project_name = project_name, pointer = pointer)
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
        initialized: Path::new(&project.path).join(".amagi").exists(),
        pending_review_count: 0,
        vault_folder: project.vault_folder.clone()
            .or_else(|| Some(crate::core::agent_exporter::project_vault_folder(&project.path))),
    }
}
