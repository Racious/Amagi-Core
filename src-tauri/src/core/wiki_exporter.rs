use std::path::Path;
use crate::AppError;
use crate::models::review::ReviewItem;
use crate::utils::fs_utils;

/// 寫入 vault 的結果。
pub struct WikiWriteResult {
    pub written: Vec<String>,
    pub skipped: Vec<String>,
}

/// 把已接受的 wiki 候選寫進 vault（adr-004 D3 三桶；2e-後續 改寫）。
///
/// 路徑規則：
/// - `sync_targets[0]` = 目標層（"general" / "shared" / "projects/<name>"）
/// - 專案層 → `<vault>/<layer>/<bucket>/<slug>.md`，bucket 由 `doc_router::bucket_for_type`
///   依 `type` 決定（knowledge / reports；`handoff` 防禦性落回 knowledge）
/// - general / shared → `<vault>/<layer>/<slug>.md`（扁平，不用 pages/ 子層）
///
/// 非破壞：目標檔已存在則略過，不覆寫既有手做內容（D7）。
pub fn write_wiki_pages(vault_root: &Path, items: &[ReviewItem]) -> Result<WikiWriteResult, AppError> {
    if !vault_root.is_dir() {
        return Err(AppError::InvalidPath(format!(
            "vault 路徑不存在：{}",
            vault_root.display()
        )));
    }

    let mut written = Vec::new();
    let mut skipped = Vec::new();

    for item in items {
        let layer = item
            .sync_targets
            .first()
            .cloned()
            .unwrap_or_else(|| "general".to_string());
        let category = if item.category.is_empty() {
            "concept".to_string()
        } else {
            item.category.clone()
        };
        let slug = {
            let s = fs_utils::slugify(&item.title);
            if s.is_empty() { "untitled".to_string() } else { s }
        };

        let dir = if layer.starts_with("projects/") {
            // 專案層：依 type 落三桶（複用 doc_router 桶映射，扁平，對齊 2e-前置 遷移後結構）。
            // 不再建舊 pages/<category>/——根治另一條「重新長出 pages/」的路徑（2e-後續）。
            let (bucket, _) = crate::core::doc_router::bucket_for_type(&category);
            // wiki 知識頁不會是 handoff；防禦性避免落到頂層 daily 語意的桶。
            let bucket = if bucket == "daily" { "knowledge" } else { bucket };
            vault_root.join(&layer).join(bucket)
        } else {
            // general / shared：扁平，不再用 pages/ 子層。
            vault_root.join(&layer)
        };
        std::fs::create_dir_all(&dir).map_err(|e| AppError::Io(e.to_string()))?;

        let file = dir.join(format!("{slug}.md"));
        if file.exists() {
            skipped.push(file.to_string_lossy().to_string());
            continue;
        }
        std::fs::write(&file, build_wiki_md(item, &category))
            .map_err(|e| AppError::Io(e.to_string()))?;
        written.push(file.to_string_lossy().to_string());
    }

    Ok(WikiWriteResult { written, skipped })
}

/// 組正式頁面 Markdown（含 frontmatter）。
pub fn build_wiki_md(item: &ReviewItem, category: &str) -> String {
    let id = format!("wiki-{}", &uuid::Uuid::new_v4().to_string()[..8]);
    // 顯示用日期取本地時區：避免台北凌晨以 UTC 計算差一天（F3）。
    let date = chrono::Local::now().format("%Y-%m-%d");
    // 若萃取自原始來源，frontmatter 回指出處（adr-002 D9）
    let source_line = match &item.source_pending_file {
        Some(s) if !s.is_empty() => format!("source: {s}\n"),
        _ => String::new(),
    };
    // 若內容首個非空行已是 H1（常見於檔案匯入的原文），不再前置 title 標題，避免重複 H1。
    let has_own_h1 = item
        .content
        .lines()
        .find(|l| !l.trim().is_empty())
        .map(|l| l.trim_start().starts_with("# "))
        .unwrap_or(false);
    let body = if has_own_h1 {
        item.content.clone()
    } else {
        format!("# {}\n\n{}", item.title, item.content)
    };
    format!(
        "---\nid: {id}\ntitle: {title}\ntype: {ty}\nstatus: active\nconfidence: medium\nsalience: 5\ntags: []\nlast_updated: {date}\n{source}protected: false\n---\n\n{body}\n",
        id = id,
        title = item.title,
        ty = category,
        date = date,
        source = source_line,
        body = body
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::review::*;
    use chrono::Utc;

    fn wiki_item(title: &str, category: &str, layer: &str, content: &str) -> ReviewItem {
        ReviewItem {
            id: "w".into(),
            project_id: "p".into(),
            item_type: ReviewItemType::Wiki,
            category: category.into(),
            title: title.into(),
            content: content.into(),
            risk: RiskLevel::Low,
            status: ReviewStatus::Accepted,
            sync_targets: vec![layer.into()],
            sync_scope: SyncScope::Project,
            source_pending_file: None,
            created_at: Utc::now(),
            reviewed_at: None,
        }
    }

    #[test]
    fn test_build_wiki_md_has_frontmatter() {
        let it = wiki_item("Bridge 設計", "adr", "projects/amagi-core", "決策內容");
        let md = build_wiki_md(&it, "adr");
        assert!(md.starts_with("---\n"));
        assert!(md.contains("type: adr"));
        assert!(md.contains("title: Bridge 設計"));
        assert!(md.contains("# Bridge 設計"));
        assert!(md.contains("決策內容"));
    }

    #[test]
    fn test_build_wiki_md_no_duplicate_h1_when_content_has_h1() {
        let it = wiki_item("檔名標題", "adr", "general", "# 原文標題\n\n內文");
        let md = build_wiki_md(&it, "adr");
        // 內容自帶 H1 → 不再前置 # title
        assert!(md.contains("# 原文標題"));
        assert!(!md.contains("# 檔名標題"));
        // 仍只有一個 H1
        assert_eq!(md.matches("\n# ").count(), 1);
    }

    #[test]
    fn test_write_to_project_layer_path() {
        let dir = std::env::temp_dir().join(format!("amagi-wiki-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let it = wiki_item("Bridge Design", "adr", "projects/amagi-core", "x");
        let res = write_wiki_pages(&dir, std::slice::from_ref(&it)).unwrap();
        assert_eq!(res.written.len(), 1);
        // 2e-後續：adr → knowledge 桶（扁平），不再 pages/adr/
        let expected = dir.join("projects/amagi-core/knowledge/bridge-design.md");
        assert!(expected.exists());
        assert!(!dir.join("projects/amagi-core/pages").exists(), "不再建舊 pages/");
        // 再寫一次應略過（非破壞）
        let res2 = write_wiki_pages(&dir, std::slice::from_ref(&it)).unwrap();
        assert_eq!(res2.written.len(), 0);
        assert_eq!(res2.skipped.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_write_to_general_layer_path() {
        let dir = std::env::temp_dir().join(format!("amagi-wiki-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let it = wiki_item("設計模式", "concept", "general", "x");
        write_wiki_pages(&dir, std::slice::from_ref(&it)).unwrap();
        // 2e-後續：general/shared 扁平，不再 pages/ 子層
        assert!(dir.join("general/設計模式.md").exists());
        assert!(!dir.join("general/pages").exists(), "general 不再用 pages/ 子層");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_write_review_type_to_reports_bucket() {
        // 2e-後續：test-report/review 類 → reports 桶（複用 doc_router 桶映射）
        let dir = std::env::temp_dir().join(format!("amagi-wiki-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let it = wiki_item("某次審查", "review", "projects/foo", "x");
        write_wiki_pages(&dir, std::slice::from_ref(&it)).unwrap();
        assert!(dir.join("projects/foo/reports/某次審查.md").exists(), "review → reports 桶");
        assert!(!dir.join("projects/foo/knowledge/某次審查.md").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
