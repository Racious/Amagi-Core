use std::path::Path;
use crate::AppError;

const START_MARKER: &str = "<!-- AMAGI:SKILL-INDEX:START -->";
const END_MARKER: &str = "<!-- AMAGI:SKILL-INDEX:END -->";

/// 單一技能的索引條目
struct SkillEntry {
    title: String,
    slug: String,
    when_to_use: String,
}

/// 掃描 .amagi/skills/*.md，組出技能索引並注入 CLAUDE.md 與 AGENTS.md。
/// 在同步技能之後呼叫。
pub fn refresh_skill_index(project_path: &str) -> Result<(), AppError> {
    let entries = scan_skills(project_path);
    let block = build_index_block(&entries);

    for fname in &["CLAUDE.md", "AGENTS.md"] {
        let path = Path::new(project_path).join(fname);
        if path.exists() {
            let original = std::fs::read_to_string(&path)
                .map_err(|e| AppError::Io(e.to_string()))?;
            let updated = inject_block(&original, &block);
            if updated != original {
                std::fs::write(&path, updated)
                    .map_err(|e| AppError::Io(e.to_string()))?;
            }
        }
    }
    Ok(())
}

/// 掃描 .amagi/skills/ 下的技能 markdown（略過 .bak）
fn scan_skills(project_path: &str) -> Vec<SkillEntry> {
    let dir = Path::new(project_path).join(".amagi").join("skills");
    let mut entries = Vec::new();

    let read = match std::fs::read_dir(&dir) {
        Ok(r) => r,
        Err(_) => return entries,
    };

    for item in read.flatten() {
        let path = item.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        if let Ok(content) = std::fs::read_to_string(&path) {
            let slug = path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            entries.push(parse_entry(&content, slug));
        }
    }

    entries.sort_by(|a, b| a.title.cmp(&b.title));
    entries
}

/// 從技能 markdown 抽出 標題 與 何時使用
fn parse_entry(content: &str, slug: String) -> SkillEntry {
    let title = extract_frontmatter_field(content, "description")
        .or_else(|| extract_heading(content))
        .unwrap_or_else(|| slug.clone());

    let when_to_use = extract_section(content, "何時使用")
        .or_else(|| extract_section(content, "描述"))
        .unwrap_or_else(|| "（未說明使用時機）".to_string());

    SkillEntry { title, slug, when_to_use }
}

/// 取 frontmatter 的某欄位（例如 description）
fn extract_frontmatter_field(content: &str, field: &str) -> Option<String> {
    let mut in_fm = false;
    for line in content.lines() {
        let t = line.trim();
        if t == "---" {
            if in_fm { break; }
            in_fm = true;
            continue;
        }
        if in_fm {
            if let Some(rest) = t.strip_prefix(&format!("{}:", field)) {
                let v = rest.trim();
                if !v.is_empty() {
                    return Some(v.to_string());
                }
            }
        }
    }
    None
}

/// 取第一個 `# ` 標題
fn extract_heading(content: &str) -> Option<String> {
    for line in content.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("# ") {
            return Some(rest.trim().to_string());
        }
    }
    None
}

/// 取某個 `## 段落` 的第一行非空內容
fn extract_section(content: &str, section: &str) -> Option<String> {
    let header = format!("## {}", section);
    let mut in_section = false;
    for line in content.lines() {
        let t = line.trim();
        if t.starts_with("## ") {
            in_section = t == header;
            continue;
        }
        if in_section && !t.is_empty() {
            // 去掉清單符號
            let cleaned = t.trim_start_matches(['-', '*', ' ']).trim();
            if !cleaned.is_empty() {
                return Some(cleaned.to_string());
            }
        }
    }
    None
}

/// 組出索引區塊（含 marker）
fn build_index_block(entries: &[SkillEntry]) -> String {
    let mut body = String::new();
    body.push_str(START_MARKER);
    body.push('\n');
    body.push_str("## 可用技能索引（AMAGI 自動維護）\n\n");

    if entries.is_empty() {
        body.push_str("（目前沒有已同步的技能）\n");
    } else {
        body.push_str("接到任務時，先看這份清單是否有對應技能。有的話照該技能執行。\n\n");
        for e in entries {
            body.push_str(&format!(
                "- **{}**（`/{}`）— 何時使用：{}\n",
                e.title, e.slug, e.when_to_use
            ));
        }
    }

    body.push_str(END_MARKER);
    body
}

/// 把索引區塊注入文字：若已有 marker 則替換，否則附加在結尾
fn inject_block(original: &str, block: &str) -> String {
    if let (Some(start), Some(end)) = (original.find(START_MARKER), original.find(END_MARKER)) {
        if start < end {
            let end_full = end + END_MARKER.len();
            let mut result = String::new();
            result.push_str(&original[..start]);
            result.push_str(block);
            result.push_str(&original[end_full..]);
            return result;
        }
    }
    // 沒有 marker → 附加在結尾
    let mut result = original.trim_end().to_string();
    result.push_str("\n\n");
    result.push_str(block);
    result.push('\n');
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_section() {
        let md = "## 描述\n這是描述內容\n\n## 何時使用\n當你要新增函數時\n";
        assert_eq!(extract_section(md, "何時使用").as_deref(), Some("當你要新增函數時"));
        assert_eq!(extract_section(md, "描述").as_deref(), Some("這是描述內容"));
    }

    #[test]
    fn test_extract_frontmatter_field() {
        let md = "---\nname: foo\ndescription: 我的技能\n---\n# 標題\n";
        assert_eq!(extract_frontmatter_field(md, "description").as_deref(), Some("我的技能"));
    }

    #[test]
    fn test_inject_replaces_existing_block() {
        let original = format!("# Title\n\n{}\nOLD\n{}\n\n尾巴", START_MARKER, END_MARKER);
        let block = format!("{}\nNEW\n{}", START_MARKER, END_MARKER);
        let result = inject_block(&original, &block);
        assert!(result.contains("NEW"));
        assert!(!result.contains("OLD"));
        assert!(result.contains("尾巴"));
    }

    #[test]
    fn test_inject_appends_when_no_marker() {
        let original = "# Title\n\n內容";
        let block = format!("{}\nINDEX\n{}", START_MARKER, END_MARKER);
        let result = inject_block(original, &block);
        assert!(result.contains("內容"));
        assert!(result.contains("INDEX"));
    }
}
