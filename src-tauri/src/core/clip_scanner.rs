use std::path::Path;
use chrono::Utc;
use uuid::Uuid;
use crate::AppError;
use crate::models::review::{ReviewItem, ReviewItemType, ReviewStatus, RiskLevel, SyncScope};
use crate::core::safety_filter;

/// 掃描 vault `sources/clips/*.md`，為尚未匯入的剪藏產生 wiki 候選（adr-002 D9）。
///
/// `already_imported`：既有佇列中 wiki 項目的 source 路徑集合，用於去重，避免重複產生候選。
pub fn scan_clips(vault_root: &Path, already_imported: &[String]) -> Result<Vec<ReviewItem>, AppError> {
    let clips_dir = vault_root.join("sources").join("clips");
    if !clips_dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut out = Vec::new();
    let entries = std::fs::read_dir(&clips_dir).map_err(|e| AppError::Io(e.to_string()))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let file_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        let source_ref = format!("sources/clips/{file_name}");
        if already_imported.iter().any(|s| s == &source_ref) {
            continue; // 已匯入，跳過
        }
        let raw = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        // 含敏感資訊的剪藏不自動產候選
        if !safety_filter::check(&raw).is_safe {
            continue;
        }

        let (fm_title, body) = split_frontmatter(&raw);
        let title = fm_title.unwrap_or_else(|| {
            path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("clip")
                .to_string()
        });
        let category = classify(&title, &body);

        out.push(ReviewItem {
            id: Uuid::new_v4().to_string(),
            project_id: String::new(),
            item_type: ReviewItemType::Wiki,
            category,
            title,
            content: body,
            risk: RiskLevel::Low,
            status: ReviewStatus::Pending,
            sync_targets: vec!["general".to_string()],
            sync_scope: SyncScope::Project,
            source_pending_file: Some(source_ref),
            blocked_hits: vec![],
            created_at: Utc::now(),
            reviewed_at: None,
        });
    }

    Ok(out)
}

/// 取 frontmatter 的 title 與正文（去掉開頭 `---` 區塊）。
fn split_frontmatter(raw: &str) -> (Option<String>, String) {
    let trimmed = raw.trim_start();
    if let Some(rest) = trimmed.strip_prefix("---") {
        if let Some(end) = rest.find("\n---") {
            let fm = &rest[..end];
            let body = rest[end + 4..]
                .trim_start_matches(['\n', '\r'])
                .to_string();
            let title = fm.lines().find_map(|l| {
                l.trim()
                    .strip_prefix("title:")
                    .map(|v| v.trim().trim_matches('"').to_string())
            });
            return (title, body);
        }
    }
    (None, raw.to_string())
}

/// 規則式分類：依關鍵字判頁面型別（老爺可在審核時改派）。
fn classify(title: &str, body: &str) -> String {
    let hay = format!("{} {}", title.to_lowercase(), body.to_lowercase());
    let has = |kws: &[&str]| kws.iter().any(|k| hay.contains(k));
    if has(&["bug", "錯誤", "exception", "stack trace", "修復", "解法", "troubleshoot"]) {
        "troubleshooting".to_string()
    } else if has(&["adr", "決策", "decision", "trade-off", "權衡"]) {
        "adr".to_string()
    } else if has(&["api", "規格", "spec", "endpoint", "schema"]) {
        "spec".to_string()
    } else {
        "concept".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_frontmatter() {
        let raw = "---\ntitle: \"我的剪藏\"\nurl: http://x\n---\n\n正文內容";
        let (title, body) = split_frontmatter(raw);
        assert_eq!(title.as_deref(), Some("我的剪藏"));
        assert_eq!(body, "正文內容");
    }

    #[test]
    fn test_split_no_frontmatter() {
        let (title, body) = split_frontmatter("純正文");
        assert!(title.is_none());
        assert_eq!(body, "純正文");
    }

    #[test]
    fn test_classify() {
        assert_eq!(classify("修復登入 bug", "stack trace ..."), "troubleshooting");
        assert_eq!(classify("採用 ADR", "決策權衡"), "adr");
        assert_eq!(classify("API 規格", "endpoint schema"), "spec");
        assert_eq!(classify("一般筆記", "隨手記"), "concept");
    }

    #[test]
    fn test_scan_dedup_and_missing_dir() {
        // 不存在 clips 目錄 → 空
        let dir = std::env::temp_dir().join(format!("amagi-clip-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(scan_clips(&dir, &[]).unwrap().is_empty());

        // 放一個剪藏 → 產 1 候選；去重後 → 0
        let clips = dir.join("sources").join("clips");
        std::fs::create_dir_all(&clips).unwrap();
        std::fs::write(clips.join("a.md"), "---\ntitle: A\n---\n本文").unwrap();
        let got = scan_clips(&dir, &[]).unwrap();
        assert_eq!(got.len(), 1);
        let dedup = scan_clips(&dir, &["sources/clips/a.md".to_string()]).unwrap();
        assert_eq!(dedup.len(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
