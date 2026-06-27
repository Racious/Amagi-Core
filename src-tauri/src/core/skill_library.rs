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

/// 把技能庫分發到指定 repo 的 `.claude/skills/<slug>/SKILL.md` 與 `.codex/skills/<slug>/SKILL.md`。
/// 這些是受管副本，分發即覆寫（單一來源 → 多處同步，adr-002 D6）。
pub fn distribute(vault_root: &Path, repo_paths: &[String]) -> Result<DistributeResult, AppError> {
    let dir = vault_root.join("_skills");
    let mut res = DistributeResult::default();
    if !dir.is_dir() {
        return Ok(res);
    }

    let skills = collect_skills(&dir);
    res.skill_count = skills.len();

    for repo in repo_paths {
        let repo_path = Path::new(repo);
        if !repo_path.is_dir() {
            continue;
        }
        res.repo_count += 1;
        for (slug, content) in &skills {
            for base in [".claude/skills", ".codex/skills"] {
                let target = repo_path.join(base).join(slug).join("SKILL.md");
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| AppError::Io(e.to_string()))?;
                }
                std::fs::write(&target, content).map_err(|e| AppError::Io(e.to_string()))?;
                res.written.push(target.to_string_lossy().to_string());
            }
        }
    }

    Ok(res)
}

/// 把技能庫分發到「全域」skills 目錄（~/.codex/skills、~/.claude/skills）。
/// 目標目錄以參數傳入，便於測試；指令層才解析真實全域路徑。
pub fn distribute_global(
    vault_root: &Path,
    codex_skills_dir: &Path,
    claude_skills_dir: &Path,
) -> Result<DistributeResult, AppError> {
    let dir = vault_root.join("_skills");
    let mut res = DistributeResult::default();
    if !dir.is_dir() {
        return Ok(res);
    }

    let skills = collect_skills(&dir);
    res.skill_count = skills.len();
    res.repo_count = 2; // 兩個全域目標：codex + claude

    for (slug, content) in &skills {
        for base in [codex_skills_dir, claude_skills_dir] {
            let target = base.join(slug).join("SKILL.md");
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|e| AppError::Io(e.to_string()))?;
            }
            std::fs::write(&target, content).map_err(|e| AppError::Io(e.to_string()))?;
            res.written.push(target.to_string_lossy().to_string());
        }
    }

    Ok(res)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_and_distribute() {
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

        let repo = root.join("repoA");
        std::fs::create_dir_all(&repo).unwrap();
        let res = distribute(&root, &[repo.to_string_lossy().to_string()]).unwrap();
        assert_eq!(res.skill_count, 1);
        assert_eq!(res.repo_count, 1);
        assert!(repo.join(".claude/skills/commit-flow/SKILL.md").exists());
        assert!(repo.join(".codex/skills/commit-flow/SKILL.md").exists());

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
    fn test_distribute_global() {
        let root = std::env::temp_dir().join(format!("amagi-glob-{}", uuid::Uuid::new_v4()));
        let lib = root.join("_skills");
        std::fs::create_dir_all(lib.join("codex-review")).unwrap();
        std::fs::write(
            lib.join("codex-review").join("SKILL.md"),
            "---\nname: \"審查\"\n---\n內容",
        )
        .unwrap();

        let codex = root.join("g_codex");
        let claude = root.join("g_claude");
        let res = distribute_global(&root, &codex, &claude).unwrap();
        assert_eq!(res.skill_count, 1);
        assert!(codex.join("codex-review/SKILL.md").exists());
        assert!(claude.join("codex-review/SKILL.md").exists());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_distribute_global_skips_reserved_slug() {
        let root = std::env::temp_dir().join(format!("amagi-dot-{}", uuid::Uuid::new_v4()));
        let lib = root.join("_skills");
        // dot-prefixed 保留命名空間，不該被分發
        std::fs::create_dir_all(lib.join(".system")).unwrap();
        std::fs::write(lib.join(".system").join("SKILL.md"), "---\nname: x\n---\n").unwrap();
        // 正常技能照分發
        std::fs::create_dir_all(lib.join("normal")).unwrap();
        std::fs::write(lib.join("normal").join("SKILL.md"), "---\nname: y\n---\n").unwrap();

        let codex = root.join("g_codex");
        let claude = root.join("g_claude");
        let res = distribute_global(&root, &codex, &claude).unwrap();
        assert_eq!(res.skill_count, 1, ".system 應被略過，只分發 normal");
        assert!(codex.join("normal/SKILL.md").exists());
        assert!(!codex.join(".system/SKILL.md").exists(), "絕不寫入 .system 保留命名空間");
        assert!(!claude.join(".system/SKILL.md").exists());

        let _ = std::fs::remove_dir_all(&root);
    }
}
