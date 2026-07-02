use chrono::Utc;
use uuid::Uuid;
use crate::models::review::{ReviewItem, ReviewItemType, RiskLevel, ReviewStatus, SyncScope};
use crate::core::safety_filter;

pub fn generate_candidates(
    project_id: &str,
    changed_files: &[String],
    diff_stat: &str,
    diff_text: &str,
) -> Vec<ReviewItem> {
    let mut candidates = Vec::new();

    // 偵測疑似機密：不再「一次擋全部」，而是加一筆「待確認」封鎖項
    //（帶規則名與遮罩片段供判斷），其餘正常候選照常產生。
    let safety = safety_filter::check(diff_text);
    if !safety.is_safe {
        candidates.push(blocked_item(project_id, &safety.hits));
    }

    let has_readme = changed_files.iter().any(|f| {
        let lower = f.to_lowercase();
        lower == "readme.md" || lower.ends_with("/readme.md")
    });
    let readme_lines = count_added_lines(diff_text, "README.md");
    if has_readme && readme_lines > 10 {
        candidates.push(make_memory(
            project_id,
            "project_rule",
            "README 文件定位規則",
            "此專案的 README 有大幅度更新，建議保存文件定位說明或 README 寫作規範。",
            RiskLevel::Low,
            vec!["AGENTS.md".into(), "CLAUDE.md".into()],
        ));
    }

    let has_package = changed_files.iter().any(|f| {
        matches!(
            f.to_lowercase().as_str(),
            "package.json" | "cargo.toml" | "pom.xml" | "build.gradle" | "pyproject.toml"
        )
    });
    if has_package {
        candidates.push(make_memory(
            project_id,
            "tech_stack",
            "技術棧或建構指令更新",
            "專案依賴設定檔有變更，建議更新技術棧說明或常用建構指令記憶。",
            RiskLevel::Low,
            vec!["AGENTS.md".into()],
        ));
    }

    let has_workflow = changed_files.iter().any(|f| {
        f.contains(".github/workflows") || f.contains(".gitlab-ci")
    });
    if has_workflow {
        candidates.push(make_memory(
            project_id,
            "ci_cd_workflow",
            "CI/CD 流程更新",
            "CI/CD 工作流程設定有變更，建議保存 release 或部署流程記憶。",
            RiskLevel::Medium,
            vec!["AGENTS.md".into()],
        ));
    }

    let has_tauri_conf = changed_files.iter().any(|f| {
        f.to_lowercase().contains("tauri.conf")
    });
    if has_tauri_conf {
        candidates.push(make_memory(
            project_id,
            "tauri_config",
            "Tauri 設定更新",
            "tauri.conf.json 有變更，建議保存 Tauri 設定注意事項或 release 前檢查流程。",
            RiskLevel::Medium,
            vec!["AGENTS.md".into()],
        ));
        candidates.push(make_skill(
            project_id,
            "tauri-release-checklist",
            "建立 Tauri release 前檢查流程技能，確保每次發版前完成必要步驟。",
        ));
    }

    let has_agents_md = changed_files.iter().any(|f| {
        let lower = f.to_lowercase();
        lower == "agents.md" || lower == "claude.md"
    });
    if has_agents_md {
        candidates.push(make_memory(
            project_id,
            "agent_rule",
            "Agent 規則更新",
            "AGENTS.md 或 CLAUDE.md 有變更，建議同步更新全域或專案 Agent 規則。",
            RiskLevel::Medium,
            vec!["AGENTS.md".into(), "CLAUDE.md".into()],
        ));
    }

    let _ = diff_stat;
    candidates
}

fn blocked_item(project_id: &str, hits: &[safety_filter::SafetyHit]) -> ReviewItem {
    let mut lines = vec![
        "AMAGI 偵測到這次變更中有下列疑似機密，已擋下自動保存。".to_string(),
        "請確認是否為誤判：".to_string(),
        String::new(),
    ];
    for h in hits {
        lines.push(format!("• {}：{}", h.label, h.masked));
    }
    lines.push(String::new());
    lines.push("處置建議：".to_string());
    lines.push("- 若是誤判（例如 commit SHA、雜湊值），點「確認丟棄」即可，不影響其他候選。".to_string());
    lines.push("- 若確為真實機密，請先從原始檔與 git 紀錄中移除，再點「確認丟棄」；切勿同步進 AGENTS.md／CLAUDE.md。".to_string());

    ReviewItem {
        id: Uuid::new_v4().to_string(),
        project_id: project_id.to_string(),
        item_type: ReviewItemType::Blocked,
        category: "sensitive".to_string(),
        title: "疑似敏感內容（待確認）".to_string(),
        content: lines.join("\n"),
        risk: RiskLevel::High,
        // 改為 Pending：留在審核佇列供老爺檢視判斷，而非自動忽略
        status: ReviewStatus::Pending,
        sync_targets: vec![],
        sync_scope: SyncScope::Project,
        source_pending_file: None,
        created_at: Utc::now(),
        reviewed_at: None,
    }
}

fn make_memory(
    project_id: &str,
    category: &str,
    title: &str,
    content: &str,
    risk: RiskLevel,
    sync_targets: Vec<String>,
) -> ReviewItem {
    ReviewItem {
        id: Uuid::new_v4().to_string(),
        project_id: project_id.to_string(),
        item_type: ReviewItemType::Memory,
        category: category.to_string(),
        title: title.to_string(),
        content: content.to_string(),
        risk,
        status: ReviewStatus::Pending,
        sync_targets,
        sync_scope: SyncScope::Project,
        source_pending_file: None,
        created_at: Utc::now(),
        reviewed_at: None,
    }
}

fn make_skill(project_id: &str, title: &str, content: &str) -> ReviewItem {
    ReviewItem {
        id: Uuid::new_v4().to_string(),
        project_id: project_id.to_string(),
        item_type: ReviewItemType::Skill,
        category: "skill".to_string(),
        title: title.to_string(),
        content: content.to_string(),
        risk: RiskLevel::Medium,
        status: ReviewStatus::Pending,
        sync_targets: vec![
            ".codex/skills".into(),
            ".claude/commands".into(),
        ],
        sync_scope: SyncScope::Project,
        source_pending_file: None,
        created_at: Utc::now(),
        reviewed_at: None,
    }
}

fn count_added_lines(diff_text: &str, filename: &str) -> usize {
    let lower_filename = filename.to_lowercase();
    let mut in_file = false;
    let mut count = 0usize;
    for line in diff_text.lines() {
        if line.starts_with("diff --git") {
            in_file = line.to_lowercase().contains(&lower_filename);
        }
        if in_file && line.starts_with('+') && !line.starts_with("+++") {
            count += 1;
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_readme_rule_fires() {
        let files = vec!["README.md".to_string()];
        // 產生 15 個獨立的 added lines（每行以 + 開頭）
        let added_lines: String = (0..15).map(|i| format!("+新增內容第{}行\n", i)).collect();
        let big_diff = format!("diff --git a/README.md b/README.md\n{}", added_lines);
        let candidates = generate_candidates("proj1", &files, "", &big_diff);
        assert!(candidates.iter().any(|c| c.category == "project_rule"));
    }

    #[test]
    fn test_sensitive_adds_reviewable_blocked_item() {
        let files = vec!["README.md".to_string()];
        let diff = "api_key=sk-secret123abc";
        let candidates = generate_candidates("proj1", &files, "", diff);
        let blocked = candidates
            .iter()
            .find(|c| c.item_type == ReviewItemType::Blocked)
            .expect("應有一筆封鎖項");
        // 改為待確認（Pending），留在佇列供檢視，而非自動忽略
        assert_eq!(blocked.status, ReviewStatus::Pending);
        // 內容應帶規則名，讓使用者看得出觸發原因
        assert!(blocked.content.contains("API key"));
    }

    #[test]
    fn test_workflow_rule_fires() {
        let files = vec![".github/workflows/release.yml".to_string()];
        let candidates = generate_candidates("proj1", &files, "", "normal diff content");
        assert!(candidates.iter().any(|c| c.category == "ci_cd_workflow"));
    }
}
