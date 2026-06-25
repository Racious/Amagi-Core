use std::path::Path;
use chrono::Utc;
use crate::AppError;
use crate::models::review::ReviewItem;
use crate::utils::fs_utils;

/// 寫入 vault 的結果。
pub struct WikiWriteResult {
    pub written: Vec<String>,
    pub skipped: Vec<String>,
}

/// 把已接受的 wiki 候選寫進 vault（adr-002 D8/D9）。
///
/// 路徑規則：
/// - `sync_targets[0]` = 目標層（"general" / "shared" / "projects/<name>"）
/// - 專案層 → `<vault>/<layer>/pages/<category>/<slug>.md`
/// - general / shared → `<vault>/<layer>/pages/<slug>.md`
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
            // 對齊既有骨架資料夾命名：spec → specs
            let folder = if category == "spec" { "specs" } else { category.as_str() };
            vault_root.join(&layer).join("pages").join(folder)
        } else {
            vault_root.join(&layer).join("pages")
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
    let date = Utc::now().format("%Y-%m-%d");
    // 若萃取自原始來源，frontmatter 回指出處（adr-002 D9）
    let source_line = match &item.source_pending_file {
        Some(s) if !s.is_empty() => format!("source: {s}\n"),
        _ => String::new(),
    };
    format!(
        "---\nid: {id}\ntitle: {title}\ntype: {ty}\nstatus: active\nconfidence: medium\nsalience: 5\ntags: []\nlast_updated: {date}\n{source}protected: false\n---\n\n# {title}\n\n{content}\n",
        id = id,
        title = item.title,
        ty = category,
        date = date,
        source = source_line,
        content = item.content
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
    fn test_write_to_project_layer_path() {
        let dir = std::env::temp_dir().join(format!("amagi-wiki-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let it = wiki_item("Bridge Design", "adr", "projects/amagi-core", "x");
        let res = write_wiki_pages(&dir, std::slice::from_ref(&it)).unwrap();
        assert_eq!(res.written.len(), 1);
        let expected = dir.join("projects/amagi-core/pages/adr/bridge-design.md");
        assert!(expected.exists());
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
        assert!(dir.join("general/pages/設計模式.md").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
