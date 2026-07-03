use std::path::Path;
use crate::AppError;

pub fn is_git_repo(path: &str) -> bool {
    Path::new(path).join(".git").is_dir()
}

pub fn app_data_dir() -> Result<std::path::PathBuf, AppError> {
    dirs::data_dir()
        .map(|d| d.join("AMAGI Core"))
        .ok_or_else(|| AppError::Io("無法取得 AppData 目錄".into()))
}

/// ~/.codex/skills/
pub fn global_codex_skills_dir() -> Option<std::path::PathBuf> {
    dirs::home_dir().map(|h| h.join(".codex").join("skills"))
}

/// ~/.claude/skills/
pub fn global_claude_skills_dir() -> Option<std::path::PathBuf> {
    dirs::home_dir().map(|h| h.join(".claude").join("skills"))
}

/// ~/.claude/CLAUDE.md（全域記憶）
pub fn global_claude_md_path() -> Option<std::path::PathBuf> {
    dirs::home_dir().map(|h| h.join(".claude").join("CLAUDE.md"))
}

/// ~/.codex/AGENTS.md（Codex 全域指令）
pub fn global_codex_agents_md_path() -> Option<std::path::PathBuf> {
    dirs::home_dir().map(|h| h.join(".codex").join("AGENTS.md"))
}

/// `path` canonical 後是否等於 `root` 或位於其下。
/// canonical 比對吸收大小寫、斜線方向、`\\?\` 前綴與 symlink/junction 差異；
/// 任一側 canonicalize 失敗（例：路徑不存在）→ 退回字面正規化比對
/// （統一分隔符＋去尾斜線＋不分大小寫），仍擋大小寫與斜線變體。
/// 用途為「拒絕寫入 vault」的安全閘：寧可多擋、不可漏放（fail-closed）。
pub fn is_same_or_under(root: &Path, path: &Path) -> bool {
    if let (Ok(r), Ok(p)) = (root.canonicalize(), path.canonicalize()) {
        return p == r || p.starts_with(&r);
    }
    let norm = |p: &Path| {
        p.to_string_lossy()
            .replace('\\', "/")
            .trim_end_matches('/')
            .to_lowercase()
    };
    let r = norm(root);
    let p = norm(path);
    !r.is_empty() && (p == r || p.starts_with(&format!("{r}/")))
}

pub fn slugify(title: &str) -> String {
    title
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn tmp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("amagi-fsu-{tag}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn test_is_same_or_under_existing_paths() {
        let root = tmp_dir("root");
        let sub = root.join("projects").join("x");
        std::fs::create_dir_all(&sub).unwrap();
        let sibling = tmp_dir("sib");

        assert!(is_same_or_under(&root, &root), "等於根本身");
        assert!(is_same_or_under(&root, &sub), "子路徑");
        assert!(!is_same_or_under(&root, &sibling), "無關路徑");
        assert!(!is_same_or_under(&sub, &root), "反向（根不在子下）");

        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&sibling);
    }

    #[cfg(windows)]
    #[test]
    fn test_is_same_or_under_case_and_slash_variants() {
        let root = tmp_dir("case");
        let sub = root.join("Inner");
        std::fs::create_dir_all(&sub).unwrap();

        // 大小寫變體（Windows canonicalize 解回實際大小寫）
        let upper = PathBuf::from(root.to_string_lossy().to_uppercase());
        assert!(is_same_or_under(&root, &upper), "大小寫變體應視為同路徑");
        // 斜線方向變體
        let fwd = PathBuf::from(sub.to_string_lossy().replace('\\', "/"));
        assert!(is_same_or_under(&root, &fwd), "正斜線變體應視為子路徑");
        // 尾斜線變體
        let trailing = PathBuf::from(format!("{}\\", root.to_string_lossy()));
        assert!(is_same_or_under(&root, &trailing), "尾斜線變體應視為同路徑");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_is_same_or_under_lexical_fallback_nonexistent() {
        // 兩側皆不存在 → 走字面正規化：仍須擋大小寫/斜線變體與前綴誤判
        let root = Path::new("D:\\no-such-amagi-vault");
        assert!(is_same_or_under(root, Path::new("D:\\No-Such-Amagi-Vault")), "字面大小寫變體");
        assert!(is_same_or_under(root, Path::new("D:/no-such-amagi-vault/projects/x")), "字面子路徑（正斜線）");
        assert!(!is_same_or_under(root, Path::new("D:\\no-such-amagi-vault2")), "同前綴的兄弟目錄不得誤判");
    }
}
