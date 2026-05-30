use std::path::Path;
use std::process::Command;
use chrono::Utc;
use uuid::Uuid;
use crate::AppError;
use crate::models::workflow::*;

/// 掃描專案內的 .workflow/ 目錄，回傳工作流程清單
pub fn scan_project_workflows(project_id: &str, project_path: &str) -> ProjectWorkflows {
    let workflow_dir = Path::new(project_path).join(".workflow");
    let has_workflow_dir = workflow_dir.exists();
    let runner_path = workflow_dir.join("workflow-runner.js");
    let runner_exists = runner_path.exists();

    let mut workflows = Vec::new();

    if has_workflow_dir {
        // 嘗試讀取 workflow.yaml 或 workflows.yaml
        for name in &["workflow.yaml", "workflows.yaml", "workflow.yml"] {
            let yaml_path = workflow_dir.join(name);
            if yaml_path.exists() {
                if let Ok(content) = std::fs::read_to_string(&yaml_path) {
                    workflows.extend(parse_workflow_yaml(&content));
                }
                break;
            }
        }
    }

    // 取得專案名稱（從路徑最後一段）
    let project_name = Path::new(project_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    ProjectWorkflows {
        project_id: project_id.to_string(),
        project_name,
        project_path: project_path.to_string(),
        has_workflow_dir,
        runner_path: if runner_exists {
            Some(runner_path.to_string_lossy().to_string())
        } else {
            None
        },
        workflows,
    }
}

/// 解析 YAML 格式的工作流程定義（簡易版，不依賴 serde_yaml）
fn parse_workflow_yaml(content: &str) -> Vec<WorkflowDefinition> {
    // 簡易解析：尋找工作流程名稱區塊
    // 格式假設為 github-issue-fix: ... 等頂層 key
    let mut workflows = Vec::new();

    let mut current_name: Option<String> = None;
    let mut current_desc = String::new();
    let mut current_steps: Vec<WorkflowStep> = Vec::new();
    let mut current_inputs: Vec<WorkflowInput> = Vec::new();
    let mut in_steps = false;
    let mut in_inputs = false;
    let mut step_name = String::new();
    let mut step_desc = String::new();
    let mut step_requires_stop = false;

    for line in content.lines() {
        let trimmed = line.trim();

        // 頂層工作流程名稱（無縮排的 key:）
        if !line.starts_with(' ') && !line.starts_with('\t') && line.contains(':') && !trimmed.starts_with('#') {
            // 儲存上一個工作流程
            if let Some(name) = current_name.take() {
                if !step_name.is_empty() {
                    current_steps.push(make_step(&step_name, &step_desc, None, step_requires_stop));
                    step_name.clear(); step_desc.clear(); step_requires_stop = false;
                }
                if !name.is_empty() {
                    workflows.push(WorkflowDefinition {
                        id: name.clone(),
                        name: name.replace('-', " ").replace('_', " "),
                        description: current_desc.trim().to_string(),
                        steps: current_steps.clone(),
                        inputs: current_inputs.clone(),
                    });
                }
                current_steps.clear();
                current_inputs.clear();
                current_desc.clear();
            }
            in_steps = false;
            in_inputs = false;
            let key = trimmed.split(':').next().unwrap_or("").trim().to_string();
            if !key.is_empty() && !key.starts_with('#') {
                current_name = Some(key);
            }
        } else if trimmed.starts_with("steps:") {
            in_steps = true;
            in_inputs = false;
        } else if trimmed.starts_with("inputs:") {
            in_inputs = true;
            in_steps = false;
        } else if trimmed.starts_with("description:") && current_name.is_some() && !in_steps {
            let desc = trimmed.trim_start_matches("description:").trim().trim_matches('"').trim_matches('\'');
            if current_steps.is_empty() {
                current_desc = desc.to_string();
            }
        } else if in_steps && (trimmed.starts_with("- name:") || trimmed.starts_with("- id:")) {
            if !step_name.is_empty() {
                current_steps.push(make_step(&step_name, &step_desc, None, step_requires_stop));
                step_desc.clear(); step_requires_stop = false;
            }
            step_name = trimmed.split(':').nth(1).unwrap_or("").trim().trim_matches('"').to_string();
        } else if in_steps && trimmed.starts_with("description:") && !step_name.is_empty() {
            step_desc = trimmed.trim_start_matches("description:").trim().trim_matches('"').to_string();
        } else if in_steps && trimmed.contains("stop") {
            step_requires_stop = true;
        } else if in_inputs && trimmed.starts_with("- key:") {
            let key = trimmed.split(':').nth(1).unwrap_or("").trim().to_string();
            current_inputs.push(WorkflowInput {
                key: key.clone(),
                label: key.replace('_', " "),
                required: true,
                default_value: None,
            });
        }
    }

    // 收尾最後一個
    if let Some(name) = current_name {
        if !step_name.is_empty() {
            current_steps.push(make_step(&step_name, &step_desc, None, step_requires_stop));
        }
        if !name.is_empty() {
            workflows.push(WorkflowDefinition {
                id: name.clone(),
                name: name.replace('-', " ").replace('_', " "),
                description: current_desc.trim().to_string(),
                steps: current_steps,
                inputs: current_inputs,
            });
        }
    }

    // 若 YAML 解析失敗或無內容，回傳預設工作流程範本
    if workflows.is_empty() {
        workflows.push(default_github_issue_workflow());
    }

    workflows
}

fn make_step(name: &str, desc: &str, badge: Option<&str>, requires_stop: bool) -> WorkflowStep {
    WorkflowStep {
        id: crate::utils::fs_utils::slugify(name),
        name: name.to_string(),
        description: if desc.is_empty() { name.to_string() } else { desc.to_string() },
        badge: badge.map(String::from),
        requires_stop,
    }
}

/// 預設的 github-issue-fix 工作流程
fn default_github_issue_workflow() -> WorkflowDefinition {
    WorkflowDefinition {
        id: "github-issue-fix".to_string(),
        name: "GitHub Issue Fix".to_string(),
        description: "處理 GitHub Issue：蒐集資料、定位程式、修正並驗證。".to_string(),
        steps: vec![
            make_step("讀取問題內容", "讀取使用者回報、標籤、討論串與問題狀態。", Some("蒐集資料"), false),
            make_step("檢查目前專案狀態", "確認現有未提交變更，避免覆蓋他人修改。", Some("安全檢查"), false),
            make_step("整理白話目標", "將問題轉成「要達成什麼」與「怎樣算修好」。", Some("理解問題"), false),
            make_step("找相關程式碼", "用關鍵字搜尋相關檔案與函式。", Some("定位範圍"), false),
            make_step("確認過去踩過的坑", "查專案規則、既有修正經驗與影響範圍。", Some("人工判斷"), false),
            make_step("提出修法", "決定修改範圍與最小修正方案。", Some("修正計畫"), false),
            make_step("實際修改程式", "真正改檔。流程在此停住，等待人工確認。", Some("必須停下"), true),
            make_step("驗證與收尾", "確認建置正常、提交紀錄、關閉問題。", Some("完成確認"), false),
        ],
        inputs: vec![
            WorkflowInput { key: "issue_number".to_string(), label: "Issue 編號".to_string(), required: true, default_value: None },
            WorkflowInput { key: "repo".to_string(), label: "Repository（owner/repo）".to_string(), required: true, default_value: None },
        ],
    }
}

/// 呼叫 workflow-runner.js plan 指令
pub fn plan_workflow(
    runner_path: &str,
    workflow_id: &str,
    project_path: &str,
    inputs: &std::collections::HashMap<String, String>,
) -> Result<WorkflowRun, AppError> {
    let mut args = vec!["plan".to_string(), workflow_id.to_string()];
    for (k, v) in inputs {
        args.push("-i".to_string());
        args.push(format!("{}={}", k, v));
    }

    let output = Command::new("node")
        .arg(runner_path)
        .args(&args)
        .current_dir(project_path)
        .output()
        .map_err(|e| AppError::Io(format!("無法執行 workflow-runner.js：{}", e)))?;

    let log = vec![
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    ];

    Ok(WorkflowRun {
        id: Uuid::new_v4().to_string(),
        project_id: String::new(),
        workflow_id: workflow_id.to_string(),
        workflow_name: workflow_id.to_string(),
        inputs: inputs.clone(),
        status: if output.status.success() { WorkflowRunStatus::Done } else { WorkflowRunStatus::Failed },
        log,
        started_at: Utc::now(),
        finished_at: Some(Utc::now()),
    })
}

/// 產生給使用者複製的執行指令
pub fn generate_run_command(
    runner_path: &str,
    workflow_id: &str,
    inputs: &std::collections::HashMap<String, String>,
    mode: &str,
) -> String {
    let input_args: String = inputs
        .iter()
        .map(|(k, v)| format!("  -i {}={}", k, v))
        .collect::<Vec<_>>()
        .join(" `\n");

    let env_line = match mode {
        "record" => "$env:WORKFLOW_LOW_MODEL_MODE='record'\n".to_string(),
        "hermes" => "$env:WORKFLOW_LOW_MODEL_MODE='hermes'\n".to_string(),
        _ => String::new(),
    };

    format!(
        "{}node \"{}\" run {} `\n{}",
        env_line,
        runner_path,
        workflow_id,
        input_args
    )
}
