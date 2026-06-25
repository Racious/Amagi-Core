use std::path::Path;
use crate::AppError;

/// 技能庫中的一筆技能（vault `_skills/<slug>.md`）。
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

/// 列出技能庫（vault `_skills/`）中的技能。
pub fn list_library_skills(vault_root: &Path) -> Vec<LibrarySkill> {
    let dir = vault_root.join("_skills");
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) != Some("md") {
                continue;
            }
            let slug = p
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("skill")
                .to_string();
            let name = std::fs::read_to_string(&p)
                .ok()
                .and_then(|c| parse_name(&c))
                .unwrap_or_else(|| slug.clone());
            out.push(LibrarySkill { slug, name });
        }
    }
    out
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

    let skills: Vec<(String, String)> = std::fs::read_dir(&dir)
        .map_err(|e| AppError::Io(e.to_string()))?
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) != Some("md") {
                return None;
            }
            let slug = p.file_stem().and_then(|s| s.to_str())?.to_string();
            let content = std::fs::read_to_string(&p).ok()?;
            Some((slug, content))
        })
        .collect();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_and_distribute() {
        let root = std::env::temp_dir().join(format!("amagi-skilllib-{}", uuid::Uuid::new_v4()));
        let lib = root.join("_skills");
        std::fs::create_dir_all(&lib).unwrap();
        std::fs::write(
            lib.join("commit-flow.md"),
            "---\nname: \"提交流程\"\ndescription: \"x\"\n---\n步驟",
        )
        .unwrap();

        let listed = list_library_skills(&root);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "提交流程");

        let repo = root.join("repoA");
        std::fs::create_dir_all(&repo).unwrap();
        let res = distribute(&root, &[repo.to_string_lossy().to_string()]).unwrap();
        assert_eq!(res.skill_count, 1);
        assert_eq!(res.repo_count, 1);
        assert!(repo.join(".claude/skills/commit-flow/SKILL.md").exists());
        assert!(repo.join(".codex/skills/commit-flow/SKILL.md").exists());

        let _ = std::fs::remove_dir_all(&root);
    }
}
