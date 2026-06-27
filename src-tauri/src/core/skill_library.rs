use std::path::Path;
use crate::AppError;

/// 技能庫中的一筆技能（原生目錄式 vault `_skills/<slug>/SKILL.md`）。
pub struct LibrarySkill {
    pub slug: String,
    pub name: String,
}

#[derive(Default)]
pub struct DistributeResult {
    pub skill_count: usize,
    pub repo_count: usize,
    pub written: Vec<String>,
}

/// 收集技能庫中的技能，回傳 (slug, content)。
///
/// 優先採原生目錄式 `_skills/<slug>/SKILL.md`（與 Claude/Codex 慣例及分發輸出一致）；
/// 為相容舊資料，亦接受扁平式 `_skills/<slug>.md`。
/// 合法技能 slug：非空、非 dot-prefixed（排除 `.system` 等保留/隱藏命名空間）、不含路徑分隔。
/// 防止分發誤寫 `~/.codex/skills/.system` 等保留目錄。
fn is_valid_skill_slug(slug: &str) -> bool {
    !slug.is_empty()
        && !slug.starts_with('.')
        && !slug.contains('/')
        && !slug.contains('\\')
}

fn collect_skills(dir: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return out,
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            // 原生目錄式：<slug>/SKILL.md
            let skill_md = p.join("SKILL.md");
            if let (Some(slug), Ok(content)) = (
                p.file_name().and_then(|s| s.to_str()).map(str::to_string),
                std::fs::read_to_string(&skill_md),
            ) {
                if is_valid_skill_slug(&slug) {
                    out.push((slug, content));
                }
            }
        } else if p.extension().and_then(|x| x.to_str()) == Some("md") {
            // 相容舊扁平式：<slug>.md
            if let (Some(slug), Ok(content)) = (
                p.file_stem().and_then(|s| s.to_str()).map(str::to_string),
                std::fs::read_to_string(&p),
            ) {
                if is_valid_skill_slug(&slug) {
                    out.push((slug, content));
                }
            }
        }
    }
    out
}

/// 列出技能庫（vault `_skills/`）中的技能。
pub fn list_library_skills(vault_root: &Path) -> Vec<LibrarySkill> {
    collect_skills(&vault_root.join("_skills"))
        .into_iter()
        .map(|(slug, content)| {
            let name = parse_name(&content).unwrap_or_else(|| slug.clone());
            LibrarySkill { slug, name }
        })
        .collect()
}

fn parse_name(content: &str) -> Option<String> {
    for line in content.lines().take(15) {
        if let Some(v) = line.trim().strip_prefix("name:") {
            return Some(v.trim().trim_matches('"').to_string());
        }
    }
    None
}

/// 選擇性分發：只把指定的「技能 → 目標」配對寫出。
/// `selections` 每筆為 `(skill_slug, target)`，`target` 為 `"global"` 或某專案路徑。
/// global → 寫入傳入的 `codex_global` / `claude_global`；專案 → 寫入 `<repo>/.codex,.claude/skills`。
pub fn distribute_selective(
    vault_root: &Path,
    selections: &[(String, String)],
    codex_global: &Path,
    claude_global: &Path,
) -> Result<DistributeResult, AppError> {
    let skills: std::collections::HashMap<String, String> =
        collect_skills(&vault_root.join("_skills")).into_iter().collect();

    let mut res = DistributeResult::default();
    let mut seen_skills = std::collections::HashSet::new();
    let mut seen_targets = std::collections::HashSet::new();

    for (slug, target) in selections {
        // 來源技能須存在且合法（collect_skills 已過 is_valid_skill_slug）
        let content = match skills.get(slug) {
            Some(c) => c,
            None => continue,
        };
        let (codex_base, claude_base): (std::path::PathBuf, std::path::PathBuf) = if target == "global" {
            (codex_global.to_path_buf(), claude_global.to_path_buf())
        } else {
            let repo = Path::new(target);
            if !repo.is_dir() {
                continue;
            }
            (repo.join(".codex").join("skills"), repo.join(".claude").join("skills"))
        };
        for base in [codex_base, claude_base] {
            let dest = base.join(slug).join("SKILL.md");
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent).map_err(|e| AppError::Io(e.to_string()))?;
            }
            std::fs::write(&dest, content).map_err(|e| AppError::Io(e.to_string()))?;
            res.written.push(dest.to_string_lossy().to_string());
        }
        seen_skills.insert(slug.clone());
        seen_targets.insert(target.clone());
    }

    res.skill_count = seen_skills.len();
    res.repo_count = seen_targets.len(); // 此處語意為「目標數」（全域算一個）
    Ok(res)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_library_skills() {
        let root = std::env::temp_dir().join(format!("amagi-skilllib-{}", uuid::Uuid::new_v4()));
        let lib = root.join("_skills");
        // 原生目錄式：_skills/commit-flow/SKILL.md
        std::fs::create_dir_all(lib.join("commit-flow")).unwrap();
        std::fs::write(
            lib.join("commit-flow").join("SKILL.md"),
            "---\nname: \"提交流程\"\ndescription: \"x\"\n---\n步驟",
        )
        .unwrap();

        let listed = list_library_skills(&root);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "提交流程");
        assert_eq!(listed[0].slug, "commit-flow");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_flat_format_still_supported() {
        // 相容舊扁平式：_skills/<slug>.md
        let root = std::env::temp_dir().join(format!("amagi-skilllib-{}", uuid::Uuid::new_v4()));
        let lib = root.join("_skills");
        std::fs::create_dir_all(&lib).unwrap();
        std::fs::write(
            lib.join("legacy.md"),
            "---\nname: \"舊技能\"\n---\n內容",
        )
        .unwrap();

        let listed = list_library_skills(&root);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "舊技能");
        assert_eq!(listed[0].slug, "legacy");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_distribute_skips_reserved_slug() {
        let root = std::env::temp_dir().join(format!("amagi-dot-{}", uuid::Uuid::new_v4()));
        let lib = root.join("_skills");
        // dot-prefixed 保留命名空間，collect_skills 應過濾，永不分發
        std::fs::create_dir_all(lib.join(".system")).unwrap();
        std::fs::write(lib.join(".system").join("SKILL.md"), "---\nname: x\n---\n").unwrap();
        std::fs::create_dir_all(lib.join("normal")).unwrap();
        std::fs::write(lib.join("normal").join("SKILL.md"), "---\nname: y\n---\n").unwrap();

        let codex = root.join("g_codex");
        let claude = root.join("g_claude");
        // 即使刻意選 .system 為目標，也因 collect_skills 過濾而不存在 → 跳過
        let sel = vec![
            (".system".to_string(), "global".to_string()),
            ("normal".to_string(), "global".to_string()),
        ];
        let res = distribute_selective(&root, &sel, &codex, &claude).unwrap();
        assert_eq!(res.skill_count, 1, ".system 應被略過，只分發 normal");
        assert!(codex.join("normal/SKILL.md").exists());
        assert!(!codex.join(".system/SKILL.md").exists(), "絕不寫入 .system 保留命名空間");
        assert!(!claude.join(".system/SKILL.md").exists());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_distribute_selective() {
        let root = std::env::temp_dir().join(format!("amagi-sel-{}", uuid::Uuid::new_v4()));
        let lib = root.join("_skills");
        std::fs::create_dir_all(lib.join("alpha")).unwrap();
        std::fs::write(lib.join("alpha").join("SKILL.md"), "---\nname: a\n---\n").unwrap();
        std::fs::create_dir_all(lib.join("beta")).unwrap();
        std::fs::write(lib.join("beta").join("SKILL.md"), "---\nname: b\n---\n").unwrap();

        let codex_global = root.join("g_codex");
        let claude_global = root.join("g_claude");
        let repo = root.join("repoA");
        std::fs::create_dir_all(&repo).unwrap();

        // alpha → global；beta → repoA（各只去各自目標）
        let sel = vec![
            ("alpha".to_string(), "global".to_string()),
            ("beta".to_string(), repo.to_string_lossy().to_string()),
        ];
        let res = distribute_selective(&root, &sel, &codex_global, &claude_global).unwrap();
        assert_eq!(res.skill_count, 2);
        assert_eq!(res.repo_count, 2); // global + repoA

        // alpha 只進 global、不進 repoA
        assert!(codex_global.join("alpha/SKILL.md").exists());
        assert!(claude_global.join("alpha/SKILL.md").exists());
        assert!(!repo.join(".codex/skills/alpha/SKILL.md").exists());
        // beta 只進 repoA、不進 global
        assert!(repo.join(".codex/skills/beta/SKILL.md").exists());
        assert!(repo.join(".claude/skills/beta/SKILL.md").exists());
        assert!(!codex_global.join("beta/SKILL.md").exists());

        let _ = std::fs::remove_dir_all(&root);
    }
}
