use std::path::{Path, PathBuf};
use chrono::Utc;
use uuid::Uuid;
use crate::AppError;
use crate::models::bridge::*;

// ── 管道檔案路徑 ────────────────────────────────────────────

fn state_dir(project_path: &str) -> PathBuf {
    Path::new(project_path).join(".amagi").join("state")
}

fn next_step_path(project_path: &str) -> PathBuf {
    state_dir(project_path).join("next-step.md")
}

fn result_path(project_path: &str) -> PathBuf {
    state_dir(project_path).join("result.md")
}

fn run_state_path(project_path: &str) -> PathBuf {
    state_dir(project_path).join("bridge-run.json")
}

// ── 內建工作流程 ────────────────────────────────────────────

/// 內建的 feature-dev 流程：把「建立 skill 草稿」設計成強制的一步。
fn builtin_steps(workflow_id: &str) -> Vec<BridgeStep> {
    let raw: Vec<(&str, &str, &str)> = match workflow_id {
        "bug-fix" => vec![
            ("search", "搜尋與理解",
             "查閱 `.claude/commands/` 是否有對應規範；用關鍵字搜尋與這個 bug 相關的程式碼與檔案，理解問題發生的位置。這一步不要改任何程式碼。"),
            ("root-cause", "根因分析",
             "根據上一步的搜尋，分析這個 bug 的根本原因。說明問題出在哪、為什麼會發生。這一步不要改程式碼。"),
            ("fix-plan", "修正計畫",
             "提出最小可行的修正方案：要改哪些檔、怎麼改、有什麼風險。這一步不要改程式碼。"),
            ("implement", "實作修正",
             "依照計畫實作修正，只做計畫範圍內的修改。"),
            ("verify", "驗證",
             "執行 build / 測試，確認 bug 已修復且沒有新錯誤。若有錯誤，修正後再驗證。"),
            ("skill-draft", "建立 skill 草稿",
             "在 `.amagi/pending/` 建立 `skill-<任務類型>.md`，記錄這次除錯的可重複流程（描述、步驟、注意事項）。這是必做步驟。"),
            ("closeout", "收尾",
             "總結本次變更的所有檔案，提出 commit message 建議。詢問老爺是否 commit，未經同意不要 commit。"),
        ],
        // 預設 feature-dev
        _ => vec![
            ("search", "搜尋既有程式碼與規範",
             "查閱 `.claude/commands/` 是否有對應規範；用關鍵字搜尋專案中與這個任務相關的既有程式碼與檔案。這一步不要改任何程式碼。"),
            ("plan", "實作計畫",
             "根據任務與上一步的搜尋結果，提出最小可行的實作計畫：要改哪些檔、預計步驟、風險。這一步不要改程式碼。"),
            ("implement", "實作",
             "依照計畫實作功能，只做計畫範圍內的修改。"),
            ("verify", "驗證",
             "執行 build / 測試，確認沒有錯誤。若有錯誤，修正後再驗證。"),
            ("skill-draft", "建立 skill 草稿",
             "在 `.amagi/pending/` 建立 `skill-<任務類型>.md`，記錄這次任務的可重複流程。必須包含 `## 描述`、`## 何時使用`（列出觸發關鍵字/情境）、`## 步驟`、`## 注意事項`。其中「何時使用」很重要，它讓未來的 AI 知道何時該套用這個技能。這是必做步驟。"),
            ("closeout", "收尾",
             "總結本次變更的所有檔案，提出 commit message 建議。詢問老爺是否 commit，未經同意不要 commit。"),
        ],
    };

    raw.into_iter()
        .map(|(id, name, instruction)| BridgeStep {
            id: id.to_string(),
            name: name.to_string(),
            instruction: instruction.to_string(),
            status: BridgeStepStatus::Pending,
            result: None,
        })
        .collect()
}

fn workflow_display_name(workflow_id: &str) -> String {
    match workflow_id {
        "bug-fix" => "Bug 修復流程".to_string(),
        "feature-dev" => "功能開發流程".to_string(),
        other => other.replace('-', " "),
    }
}

// ── 流程操作 ────────────────────────────────────────────────

/// 開始一個新的 File Bridge 流程
pub fn start_run(
    project_id: &str,
    project_path: &str,
    workflow_id: &str,
    task: &str,
) -> Result<BridgeRun, AppError> {
    let mut steps = builtin_steps(workflow_id);
    if steps.is_empty() {
        return Err(AppError::Io("工作流程沒有任何步驟".to_string()));
    }
    steps[0].status = BridgeStepStatus::Active;

    let now = Utc::now();
    let run = BridgeRun {
        id: Uuid::new_v4().to_string(),
        project_id: project_id.to_string(),
        project_path: project_path.to_string(),
        workflow_id: workflow_id.to_string(),
        workflow_name: workflow_display_name(workflow_id),
        task: task.to_string(),
        steps,
        current_step: 0,
        status: BridgeRunStatus::AwaitingResult,
        created_at: now,
        updated_at: now,
    };

    // 建立 state 目錄
    std::fs::create_dir_all(state_dir(project_path))
        .map_err(|e| AppError::Io(e.to_string()))?;

    // 清空舊的 result.md（避免讀到上一個流程的殘留）
    let _ = std::fs::remove_file(result_path(project_path));

    write_next_step_file(&run)?;
    save_run(&run)?;
    Ok(run)
}

/// 讀取 result.md，記錄到當前步驟，推進到下一步
pub fn advance_run(project_path: &str) -> Result<BridgeRun, AppError> {
    let mut run = load_run(project_path)?
        .ok_or_else(|| AppError::Io("找不到進行中的流程".to_string()))?;

    if run.status != BridgeRunStatus::AwaitingResult {
        return Ok(run);
    }

    // 讀取 AI 寫回的結果
    let result_file = result_path(project_path);
    let result = std::fs::read_to_string(&result_file)
        .map_err(|_| AppError::Io("找不到 result.md，AI 尚未寫回結果".to_string()))?;
    if result.trim().is_empty() {
        return Err(AppError::Io("result.md 是空的，AI 尚未寫回結果".to_string()));
    }

    // 記錄到當前步驟並標記完成
    let idx = run.current_step;
    run.steps[idx].result = Some(result.trim().to_string());
    run.steps[idx].status = BridgeStepStatus::Done;

    // 清掉 result.md，準備下一步
    let _ = std::fs::remove_file(&result_file);

    if idx + 1 < run.steps.len() {
        // 推進
        run.current_step = idx + 1;
        run.steps[idx + 1].status = BridgeStepStatus::Active;
        run.updated_at = Utc::now();
        write_next_step_file(&run)?;
    } else {
        // 全部完成
        run.status = BridgeRunStatus::Done;
        run.updated_at = Utc::now();
        let _ = std::fs::remove_file(next_step_path(project_path));
    }

    save_run(&run)?;
    Ok(run)
}

/// 取得目前進行中的流程（若無則回傳 None）
pub fn get_run(project_path: &str) -> Result<Option<BridgeRun>, AppError> {
    load_run(project_path)
}

/// 中止目前流程
pub fn cancel_run(project_path: &str) -> Result<(), AppError> {
    if let Some(mut run) = load_run(project_path)? {
        run.status = BridgeRunStatus::Cancelled;
        run.updated_at = Utc::now();
        save_run(&run)?;
    }
    let _ = std::fs::remove_file(next_step_path(project_path));
    Ok(())
}

// ── 內部：讀寫狀態與管道檔 ──────────────────────────────────

fn save_run(run: &BridgeRun) -> Result<(), AppError> {
    let path = run_state_path(&run.project_path);
    std::fs::create_dir_all(state_dir(&run.project_path))
        .map_err(|e| AppError::Io(e.to_string()))?;
    let json = serde_json::to_string_pretty(run)
        .map_err(|e| AppError::Io(e.to_string()))?;
    std::fs::write(&path, json).map_err(|e| AppError::Io(e.to_string()))?;
    Ok(())
}

fn load_run(project_path: &str) -> Result<Option<BridgeRun>, AppError> {
    let path = run_state_path(project_path);
    if !path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&path)
        .map_err(|e| AppError::Io(e.to_string()))?;
    let run: BridgeRun = serde_json::from_str(&content)
        .map_err(|e| AppError::Io(e.to_string()))?;
    Ok(Some(run))
}

/// 把當前步驟的 prompt 寫到 next-step.md（這是給 AI 讀的）
fn write_next_step_file(run: &BridgeRun) -> Result<(), AppError> {
    let idx = run.current_step;
    let step = &run.steps[idx];
    let total = run.steps.len();

    let content = format!(
        r#"# AMAGI 工作流程：{workflow}
# 步驟 {n}/{total}：{step_name}

## 本次任務
{task}

## 這一步要做的事
{instruction}

## 完成後（重要，請務必照做）
1. 把這一步的結果寫進 `.amagi/state/result.md`
   （說明你做了什麼、發現什麼、產出或修改了哪些檔案）
2. 回報老爺：「步驟 {n}（{step_name}）完成」
3. **停下來等待**，不要自己接著做下一步。老爺會在 AMAGI 推進。
"#,
        workflow = run.workflow_name,
        n = idx + 1,
        total = total,
        step_name = step.name,
        task = run.task,
        instruction = step.instruction,
    );

    std::fs::write(next_step_path(&run.project_path), content)
        .map_err(|e| AppError::Io(e.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 建立一個隔離的暫存專案目錄
    fn temp_project() -> String {
        let dir = std::env::temp_dir().join(format!("amagi-bridge-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.to_string_lossy().to_string()
    }

    fn cleanup(path: &str) {
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn test_start_creates_first_step() {
        let project = temp_project();
        let run = start_run("pid", &project, "feature-dev", "新增測試功能").unwrap();

        assert_eq!(run.status, BridgeRunStatus::AwaitingResult);
        assert_eq!(run.current_step, 0);
        assert_eq!(run.steps[0].status, BridgeStepStatus::Active);
        assert_eq!(run.steps[1].status, BridgeStepStatus::Pending);

        // next-step.md 應該存在且包含任務與第一步
        let next = std::fs::read_to_string(next_step_path(&project)).unwrap();
        assert!(next.contains("新增測試功能"));
        assert!(next.contains("步驟 1/"));

        cleanup(&project);
    }

    #[test]
    fn test_advance_requires_result_file() {
        let project = temp_project();
        start_run("pid", &project, "feature-dev", "任務").unwrap();

        // 沒有 result.md 時推進應失敗
        let err = advance_run(&project);
        assert!(err.is_err());

        cleanup(&project);
    }

    #[test]
    fn test_full_flow_to_done() {
        let project = temp_project();
        let run = start_run("pid", &project, "feature-dev", "任務").unwrap();
        let total = run.steps.len();

        // 逐步走完
        for i in 0..total {
            // 模擬 AI 寫回 result.md
            std::fs::write(result_path(&project), format!("步驟 {} 的結果", i + 1)).unwrap();
            let run = advance_run(&project).unwrap();

            if i + 1 < total {
                assert_eq!(run.status, BridgeRunStatus::AwaitingResult);
                assert_eq!(run.current_step, i + 1);
                assert_eq!(run.steps[i].status, BridgeStepStatus::Done);
                assert!(run.steps[i].result.is_some());
                // result.md 應該已被清空（下一步重新寫）
                assert!(!result_path(&project).exists());
                // next-step.md 應指向下一步
                let next = std::fs::read_to_string(next_step_path(&project)).unwrap();
                assert!(next.contains(&format!("步驟 {}/", i + 2)));
            } else {
                // 最後一步完成
                assert_eq!(run.status, BridgeRunStatus::Done);
                assert!(!next_step_path(&project).exists());
            }
        }

        cleanup(&project);
    }

    #[test]
    fn test_feature_dev_has_skill_draft_step() {
        let project = temp_project();
        let run = start_run("pid", &project, "feature-dev", "任務").unwrap();
        assert!(run.steps.iter().any(|s| s.id == "skill-draft"));
        cleanup(&project);
    }

    #[test]
    fn test_cancel_run() {
        let project = temp_project();
        start_run("pid", &project, "feature-dev", "任務").unwrap();
        cancel_run(&project).unwrap();

        let run = get_run(&project).unwrap().unwrap();
        assert_eq!(run.status, BridgeRunStatus::Cancelled);
        assert!(!next_step_path(&project).exists());

        cleanup(&project);
    }
}
