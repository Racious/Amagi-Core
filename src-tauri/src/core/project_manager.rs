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

    let project = Project {
        id: Uuid::new_v4().to_string(),
        name,
        path: path.to_string(),
        created_at: Utc::now(),
        last_scanned_at: None,
        initialized: false,
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

    // ── 寫入 AGENTS.md（含技能記錄規範）──────────────
    let agents_md = Path::new(&project.path).join("AGENTS.md");
    if !agents_md.exists() {
        std::fs::write(&agents_md, build_initial_agents_md(&project.name))
            .map_err(|e| AppError::Io(e.to_string()))?;
        created_files.push(agents_md.to_string_lossy().to_string());
    }

    // ── 寫入 CLAUDE.md（含技能記錄規範）─────────────
    let claude_md = Path::new(&project.path).join("CLAUDE.md");
    if !claude_md.exists() {
        std::fs::write(&claude_md, build_initial_claude_md(&project.name))
            .map_err(|e| AppError::Io(e.to_string()))?;
        created_files.push(claude_md.to_string_lossy().to_string());
    }

    Ok(InitResult {
        project_id: project.id.clone(),
        created_dirs,
        created_files,
    })
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

fn build_initial_agents_md(project_name: &str) -> String {
    format!(r#"# {project_name} — Agent 工作記憶

> 由 AMAGI Core 自動建立。本檔案記錄此專案的規則、技術棧與注意事項。

## 每次接到任務的標準流程（依序執行，不得跳過）

### 步驟一：查閱現有規範
查閱 `.claude/commands/` 目錄，確認是否有對應的技能規範：
- **有規範** → 按規範實作
- **無規範** → 按工程判斷實作

### 步驟二：建立 skill 草稿框架（實作之前）
在 `.amagi/pending/` 建立 `skill-<任務類型>.md`，先填入標題與預計步驟（可在實作後補充細節）：

```
---
title: 簡短描述這個技能
scope: project
---

## 描述
這個技能解決什麼問題、適用於什麼情境。

## 何時使用
列出會用到這個技能的情境或觸發關鍵字（例：新增 API、修 TypeScript 型別）。
這段讓未來的 AI 知道何時該套用此技能，請務必填寫。

## 步驟
1. （待實作後填入）

## 注意事項
- （待實作後填入）
```

### 步驟三：實作功能
完成程式碼修改，並將 skill 草稿的步驟與注意事項補充完整。

### 步驟四：詢問是否 commit
向使用者報告並等待確認：

> 功能已實作完成，skill 草稿已更新於 `.amagi/pending/skill-xxx.md`。
> 是否要 commit？

⛔ 使用者確認前，不得自行執行 git commit。

---

## 複雜任務：維護任務狀態檔

跨多個檔案、需要多步驟的任務，請更新 `.amagi/workflow-state.md`：
記錄目前任務、進行到哪一步、已完成項、卡住的原因。
這樣即使對話變長，也能從狀態檔接續，不會跳步或漏掉驗證。

任務完成後，依 `.amagi/after-task-review.md` 進行回顧。

---

<!-- AMAGI 會在這裡自動插入審核通過的記憶與規則 -->
"#, project_name = project_name)
}

fn build_initial_claude_md(project_name: &str) -> String {
    format!(r#"# {project_name} — Claude 工作規則

> 由 AMAGI Core 自動建立。本檔案記錄 Claude 在此專案應遵守的規則。

## 每次接到任務的標準流程（依序執行，不得跳過）

### 步驟一：查閱現有規範
查閱 `.claude/commands/` 目錄，確認是否有對應的技能規範：
- **有規範** → 按規範實作
- **無規範** → 按工程判斷實作

### 步驟二：建立 skill 草稿框架（實作之前）
在 `.amagi/pending/` 建立 `skill-<任務類型>.md`，先填入標題與預計步驟（可在實作後補充細節）：

```
---
title: 簡短描述這個技能
scope: project
---

## 描述
這個技能解決什麼問題、適用於什麼情境。

## 何時使用
列出會用到這個技能的情境或觸發關鍵字（例：新增 API、修 TypeScript 型別）。
這段讓未來的 AI 知道何時該套用此技能，請務必填寫。

## 步驟
1. （待實作後填入）

## 注意事項
- （待實作後填入）
```

### 步驟三：實作功能
完成程式碼修改，並將 skill 草稿的步驟與注意事項補充完整。

### 步驟四：詢問是否 commit
向老爺報告並等待確認：

> 功能已實作完成，skill 草稿已更新於 `.amagi/pending/skill-xxx.md`。
> 是否要 commit？

⛔ 老爺確認前，不得自行執行 git commit。

---

## 複雜任務：維護任務狀態檔

跨多個檔案、需要多步驟的任務，請更新 `.amagi/workflow-state.md`：
記錄目前任務、進行到哪一步、已完成項、卡住的原因。
這樣即使對話變長，也能從狀態檔接續，不會跳步或漏掉驗證。

任務完成後，依 `.amagi/after-task-review.md` 進行回顧。

---

<!-- AMAGI 會在這裡自動插入審核通過的規則 -->
"#, project_name = project_name)
}

pub fn get_project_info(project: &Project) -> ProjectInfo {
    let branch = std::process::Command::new("git")
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
    }
}
