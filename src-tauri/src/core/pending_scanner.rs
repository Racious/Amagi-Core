use std::path::Path;
use chrono::Utc;
use uuid::Uuid;
use crate::AppError;
use crate::models::review::{ReviewItem, ReviewItemType, RiskLevel, ReviewStatus, SyncScope};
use crate::core::safety_filter;

/// 掃描 <project>/.amagi/pending/skill-*.md，
/// 解析 Agent 寫入的技能草稿，回傳候選 ReviewItem。
pub fn scan_pending_skills(
    project_path: &str,
    project_id: &str,
    existing_sources: &[String],  // 已在佇列中的來源檔路徑，避免重複加入
) -> Result<Vec<ReviewItem>, AppError> {
    let pending_dir = Path::new(project_path).join(".amagi").join("pending");

    if !pending_dir.exists() {
        return Ok(vec![]);
    }

    let entries = std::fs::read_dir(&pending_dir)
        .map_err(|e| AppError::Io(e.to_string()))?;

    let mut items = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();

        // 只處理 skill-*.md，忽略 README / AGENT_INSTRUCTIONS 等說明檔
        let name = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");
        if !name.starts_with("skill-") || !name.ends_with(".md") {
            continue;
        }

        let path_str = path.to_string_lossy().to_string();

        // 若已在佇列中，跳過避免重複
        if existing_sources.contains(&path_str) {
            continue;
        }

        let raw = std::fs::read_to_string(&path)
            .map_err(|e| AppError::Io(e.to_string()))?;

        // 安全過濾：若含敏感資料，拒絕加入
        let safety = safety_filter::check(&raw);
        if !safety.is_safe {
            eprintln!("[AMAGI] pending skill '{}' 含敏感資料，已略過", name);
            continue;
        }

        let (frontmatter, body) = split_frontmatter(&raw);
        let title = extract_field(&frontmatter, "title")
            .unwrap_or_else(|| name.trim_end_matches(".md").to_string());
        let scope_str = extract_field(&frontmatter, "scope").unwrap_or_default();
        // 三層標籤驅動：project / shared / global；未知值保守 fallback Project（Codex 3b-2 #1）
        let sync_scope = match scope_str.to_lowercase().as_str() {
            "global" => SyncScope::Global,
            "shared" => SyncScope::Shared,
            _ => SyncScope::Project,
        };

        let item = ReviewItem {
            id: Uuid::new_v4().to_string(),
            project_id: project_id.to_string(),
            item_type: ReviewItemType::Skill,
            category: "skill".to_string(),
            title,
            content: body.trim().to_string(),
            risk: RiskLevel::Low,
            status: ReviewStatus::Pending,
            sync_targets: vec![
                ".codex/skills".into(),
                ".claude/commands".into(),
            ],
            sync_scope,
            source_pending_file: Some(path_str),
            created_at: Utc::now(),
            reviewed_at: None,
        };

        items.push(item);
    }

    Ok(items)
}

// ── 內部工具函數 ───────────────────────────────────────

/// 分割 YAML frontmatter（--- 之間）與正文
fn split_frontmatter(content: &str) -> (String, String) {
    let lines: Vec<&str> = content.lines().collect();
    if lines.first().map(|l| l.trim()) != Some("---") {
        return (String::new(), content.to_string());
    }

    let end = lines[1..].iter().position(|l| l.trim() == "---");
    match end {
        Some(i) => {
            let fm = lines[1..=i].join("\n");
            let body = lines[i + 2..].join("\n");
            (fm, body)
        }
        None => (String::new(), content.to_string()),
    }
}

/// 從 frontmatter 字串取出 key: value
fn extract_field(fm: &str, key: &str) -> Option<String> {
    for line in fm.lines() {
        if let Some(rest) = line.strip_prefix(&format!("{}:", key)) {
            return Some(rest.trim().to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_frontmatter() {
        let content = "---\ntitle: 測試技能\nscope: project\n---\n## 步驟\n1. 做某事\n";
        let (fm, body) = split_frontmatter(content);
        assert!(fm.contains("title: 測試技能"));
        assert!(body.contains("## 步驟"));
    }

    #[test]
    fn test_extract_field() {
        let fm = "title: 我的技能\nscope: global\n";
        assert_eq!(extract_field(fm, "title"), Some("我的技能".to_string()));
        assert_eq!(extract_field(fm, "scope"), Some("global".to_string()));
        assert_eq!(extract_field(fm, "missing"), None);
    }

    #[test]
    fn test_no_frontmatter() {
        let content = "## 步驟\n1. 沒有 frontmatter";
        let (fm, body) = split_frontmatter(content);
        assert!(fm.is_empty());
        assert!(body.contains("## 步驟"));
    }

    #[test]
    fn test_scan_parses_three_scopes() {
        // scope: project / shared / global / 未標 → 對應 SyncScope，未知 fallback Project（Codex 3b-2 #1）
        let root = std::env::temp_dir().join(format!("amagi-pending-{}", Uuid::new_v4()));
        let pending = root.join(".amagi").join("pending");
        std::fs::create_dir_all(&pending).unwrap();
        std::fs::write(pending.join("skill-s.md"), "---\ntitle: 共用技能\nscope: shared\n---\n## 描述\n做某事").unwrap();
        std::fs::write(pending.join("skill-g.md"), "---\ntitle: 全域技能\nscope: global\n---\n## 描述\n做某事").unwrap();
        std::fs::write(pending.join("skill-u.md"), "---\ntitle: 未標技能\n---\n## 描述\n做某事").unwrap();

        let items = scan_pending_skills(root.to_str().unwrap(), "p1", &[]).unwrap();
        let scope_of = |t: &str| items.iter().find(|i| i.title == t).unwrap().sync_scope.clone();
        assert_eq!(scope_of("共用技能"), SyncScope::Shared);
        assert_eq!(scope_of("全域技能"), SyncScope::Global);
        assert_eq!(scope_of("未標技能"), SyncScope::Project);

        let _ = std::fs::remove_dir_all(&root);
    }
}
