use std::path::{Path, PathBuf};
use crate::AppError;

/// 技能庫中的一筆技能（原生目錄式 vault `_skills/<slug>/SKILL.md`）。
pub struct LibrarySkill {
    pub slug: String,
    pub name: String,
}

/// 可收編進 vault 的候選技能（散落各處、vault `_skills/` 尚無者）。
pub struct AdoptableSkill {
    pub slug: String,
    pub name: String,
    /// 來源技能目錄（顯示用；收編時由指令層從白名單根重新解析，不信任此字串）。
    pub source: String,
}

#[derive(Default)]
pub struct AdoptResult {
    /// 已收編進 vault 的 slug。
    pub adopted: Vec<String>,
    /// 略過的 slug（vault 已有同名 → 非破壞不覆寫）。
    pub skipped: Vec<String>,
    /// 無法收編的 slug（找不到來源 / 缺 SKILL.md / 非法 slug / 來源為 symlink），供批次判讀。
    pub missing: Vec<String>,
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

/// 掃描來源目錄群，列出「可收編進 vault」的技能候選（adr-004 D6 收編單一來源）。
/// 候選條件：目錄式 `<slug>/SKILL.md`、slug 合法、vault `_skills/` 尚未收錄。
/// 多來源中同 slug 只列一次（去重，取先遇到者）。
pub fn scan_adoptable(sources: &[PathBuf], vault_root: &Path) -> Vec<AdoptableSkill> {
    let existing: std::collections::HashSet<String> = list_library_skills(vault_root)
        .into_iter()
        .map(|s| s.slug)
        .collect();
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for src in sources {
        let entries = match std::fs::read_dir(src) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for e in entries.flatten() {
            let p = e.path();
            if !p.is_dir() {
                continue;
            }
            let slug = match p.file_name().and_then(|s| s.to_str()) {
                Some(s) => s.to_string(),
                None => continue,
            };
            if !is_valid_skill_slug(&slug) || existing.contains(&slug) || seen.contains(&slug) {
                continue;
            }
            // 來源須安全：本身非 symlink/junction 且 canonical 仍在來源根下，
            // 杜絕 `~/.codex/skills/foo -> 外部目錄` 繞出白名單（Codex 複審 D-中）。
            if !is_safe_source_dir(src, &p) {
                continue;
            }
            // 必須含 SKILL.md 才算技能目錄
            let content = match std::fs::read_to_string(p.join("SKILL.md")) {
                Ok(c) => c,
                Err(_) => continue,
            };
            let name = parse_name(&content).unwrap_or_else(|| slug.clone());
            seen.insert(slug.clone());
            out.push(AdoptableSkill {
                slug,
                name,
                source: p.to_string_lossy().to_string(),
            });
        }
    }
    out
}

/// 收編：把指定 `(slug, 來源技能目錄)` 整個目錄樹複製進 vault `_skills/<slug>/`。
/// 非破壞：vault 已有同 slug（含舊扁平 `<slug>.md`）→ 略過，不覆寫既有正本。
/// 原子：先複製到 `_skills` 下的 `.adopt-tmp-*` 暫存目錄，成功後 rename，失敗清暫存——
/// 避免半成品 `<slug>/` 殘留而被下次誤判略過（Codex 複審 D-中）。
pub fn adopt_skills(vault_root: &Path, items: &[(String, PathBuf)]) -> Result<AdoptResult, AppError> {
    let skills_root = vault_root.join("_skills");
    // existing 涵蓋目錄式與舊扁平式（collect_skills），防在 `<slug>.md` 旁再建 `<slug>/` 並存正本（D-低）。
    let existing: std::collections::HashSet<String> = list_library_skills(vault_root)
        .into_iter()
        .map(|s| s.slug)
        .collect();
    let mut res = AdoptResult::default();
    for (slug, source) in items {
        // 非法 slug / 缺 SKILL.md / 來源本身為 symlink → 無法收編（縱深，指令層通常已過濾）
        let src_is_symlink = std::fs::symlink_metadata(source)
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(true);
        if !is_valid_skill_slug(slug) || !source.join("SKILL.md").is_file() || src_is_symlink {
            res.missing.push(slug.clone());
            continue;
        }
        let dest = skills_root.join(slug);
        if existing.contains(slug) || dest.exists() {
            res.skipped.push(slug.clone());
            continue;
        }
        // canonical 來源根：供遞迴複製擋 junction/reparse 子目錄穿越（Codex r2 D-中）。
        let canon_root = match source.canonicalize() {
            Ok(c) => c,
            Err(_) => {
                res.missing.push(slug.clone());
                continue;
            }
        };
        // 原子收編：temp → rename。temp 以 `.` 開頭，is_valid_skill_slug 排除，不會被誤列為技能。
        let tmp = skills_root.join(format!(".adopt-tmp-{}-{}", slug, uuid::Uuid::new_v4()));
        if let Err(e) = copy_dir_recursive(source, &tmp, &canon_root) {
            let _ = std::fs::remove_dir_all(&tmp);
            return Err(e);
        }
        if let Err(e) = std::fs::rename(&tmp, &dest) {
            let _ = std::fs::remove_dir_all(&tmp);
            return Err(AppError::Io(e.to_string()));
        }
        res.adopted.push(slug.clone());
    }
    Ok(res)
}

/// 來源技能目錄是否安全：本身非 symlink/junction，且 canonical 仍落在來源根 canonical 之下。
/// 防 symlink/junction 把白名單外的外部目錄收編進 vault（Codex 複審 D-中）。
pub fn is_safe_source_dir(root: &Path, dir: &Path) -> bool {
    let is_symlink = std::fs::symlink_metadata(dir)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(true);
    if is_symlink {
        return false;
    }
    match (root.canonicalize(), dir.canonicalize()) {
        (Ok(cr), Ok(cd)) => cd.starts_with(&cr),
        _ => false,
    }
}

/// 遞迴複製目錄樹（含附屬檔，如 scripts/references）。跳過 symlink；子目錄 canonical
/// 須仍在 `canon_root`（來源根 canonical）之下才遞迴，擋 symlink 與 Windows junction/reparse 穿越。
fn copy_dir_recursive(from: &Path, to: &Path, canon_root: &Path) -> Result<(), AppError> {
    std::fs::create_dir_all(to).map_err(|e| AppError::Io(e.to_string()))?;
    for e in std::fs::read_dir(from).map_err(|e| AppError::Io(e.to_string()))?.flatten() {
        let p = e.path();
        // 跳過 symlink（本機技能目錄不應含；避免循環/穿越）
        if std::fs::symlink_metadata(&p)
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(true)
        {
            continue;
        }
        let dest = to.join(e.file_name());
        if p.is_dir() {
            // 子目錄 canonical 須仍在來源根下，擋 junction/reparse 指向外部（Codex r2 D-中）
            let inside = p
                .canonicalize()
                .map(|cp| cp.starts_with(canon_root))
                .unwrap_or(false);
            if !inside {
                continue;
            }
            copy_dir_recursive(&p, &dest, canon_root)?;
        } else {
            std::fs::copy(&p, &dest).map_err(|e| AppError::Io(e.to_string()))?;
        }
    }
    Ok(())
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
    fn test_scan_adoptable_filters_existing_invalid_and_dedups() {
        let root = std::env::temp_dir().join(format!("amagi-adopt-{}", uuid::Uuid::new_v4()));
        // vault 已有 codex-review
        let lib = root.join("_skills");
        std::fs::create_dir_all(lib.join("codex-review")).unwrap();
        std::fs::write(lib.join("codex-review").join("SKILL.md"), "---\nname: cr\n---\n").unwrap();
        // 來源 A（codex 全域）：hatch-pet（新）、codex-review（vault 已有）、.system（保留）、no-md（無 SKILL.md）
        let src_a = root.join("codex_skills");
        std::fs::create_dir_all(src_a.join("hatch-pet")).unwrap();
        std::fs::write(src_a.join("hatch-pet").join("SKILL.md"), "---\nname: \"養寵物\"\n---\n").unwrap();
        std::fs::create_dir_all(src_a.join("codex-review")).unwrap();
        std::fs::write(src_a.join("codex-review").join("SKILL.md"), "---\nname: cr\n---\n").unwrap();
        std::fs::create_dir_all(src_a.join(".system")).unwrap();
        std::fs::write(src_a.join(".system").join("SKILL.md"), "---\nname: s\n---\n").unwrap();
        std::fs::create_dir_all(src_a.join("no-md")).unwrap();
        // 來源 B（claude 全域）：hatch-pet（與 A 重複，應去重）
        let src_b = root.join("claude_skills");
        std::fs::create_dir_all(src_b.join("hatch-pet")).unwrap();
        std::fs::write(src_b.join("hatch-pet").join("SKILL.md"), "---\nname: \"養寵物\"\n---\n").unwrap();

        let cands = scan_adoptable(&[src_a.clone(), src_b.clone()], &root);
        // 只有 hatch-pet 一筆：codex-review 已在 vault、.system 保留、no-md 無 SKILL.md、跨來源去重
        assert_eq!(cands.len(), 1);
        assert_eq!(cands[0].slug, "hatch-pet");
        assert_eq!(cands[0].name, "養寵物");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_adopt_copies_tree_and_is_non_destructive() {
        let root = std::env::temp_dir().join(format!("amagi-adopt2-{}", uuid::Uuid::new_v4()));
        // 來源技能含附屬檔（scripts/run.sh）
        let src = root.join("src").join("hatch-pet");
        std::fs::create_dir_all(src.join("scripts")).unwrap();
        std::fs::write(src.join("SKILL.md"), "---\nname: pet\n---\n步驟").unwrap();
        std::fs::write(src.join("scripts").join("run.sh"), "echo hi").unwrap();

        let vault = root.join("vault");
        std::fs::create_dir_all(&vault).unwrap();

        let items = vec![("hatch-pet".to_string(), src.clone())];
        let res = adopt_skills(&vault, &items).unwrap();
        assert_eq!(res.adopted, vec!["hatch-pet".to_string()]);
        assert!(res.skipped.is_empty());
        // 整個目錄樹（含附屬檔）都被複製
        assert!(vault.join("_skills/hatch-pet/SKILL.md").is_file());
        assert!(vault.join("_skills/hatch-pet/scripts/run.sh").is_file());

        // 再收編一次 → 非破壞略過、不覆寫
        std::fs::write(vault.join("_skills/hatch-pet/SKILL.md"), "已被改動").unwrap();
        let res2 = adopt_skills(&vault, &items).unwrap();
        assert_eq!(res2.skipped, vec!["hatch-pet".to_string()]);
        assert!(res2.adopted.is_empty());
        let kept = std::fs::read_to_string(vault.join("_skills/hatch-pet/SKILL.md")).unwrap();
        assert_eq!(kept, "已被改動", "vault 既有正本不被覆寫");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_adopt_rejects_invalid_slug_and_missing_skill_md() {
        let root = std::env::temp_dir().join(format!("amagi-adopt3-{}", uuid::Uuid::new_v4()));
        let vault = root.join("vault");
        std::fs::create_dir_all(&vault).unwrap();
        // 無 SKILL.md 的來源
        let bad = root.join("bad");
        std::fs::create_dir_all(&bad).unwrap();
        let items = vec![
            (".system".to_string(), bad.clone()),     // 非法 slug
            ("nomd".to_string(), bad.clone()),         // 無 SKILL.md
        ];
        let res = adopt_skills(&vault, &items).unwrap();
        assert!(res.adopted.is_empty());
        // 兩者都記入 missing（可判讀），不靜默丟棄
        assert!(res.missing.contains(&".system".to_string()));
        assert!(res.missing.contains(&"nomd".to_string()));
        assert!(!vault.join("_skills/.system").exists());
        assert!(!vault.join("_skills/nomd").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_adopt_flat_existing_prevents_dir_duplicate() {
        // vault 已有舊扁平式 _skills/foo.md → 收編 foo 應略過，不再建目錄式 _skills/foo/（D-低）
        let root = std::env::temp_dir().join(format!("amagi-adopt4-{}", uuid::Uuid::new_v4()));
        let vault = root.join("vault");
        std::fs::create_dir_all(vault.join("_skills")).unwrap();
        std::fs::write(vault.join("_skills/foo.md"), "---\nname: foo\n---\n舊扁平").unwrap();
        let src = root.join("src/foo");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("SKILL.md"), "---\nname: foo\n---\n新").unwrap();

        let res = adopt_skills(&vault, &[("foo".to_string(), src)]).unwrap();
        assert_eq!(res.skipped, vec!["foo".to_string()]);
        assert!(res.adopted.is_empty());
        assert!(!vault.join("_skills/foo").exists(), "不可在扁平 foo.md 旁建目錄式 foo/");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_is_safe_source_dir_containment() {
        let root = std::env::temp_dir().join(format!("amagi-safe-{}", uuid::Uuid::new_v4()));
        let inside = root.join("skills").join("alpha");
        std::fs::create_dir_all(&inside).unwrap();
        let outside = root.join("elsewhere");
        std::fs::create_dir_all(&outside).unwrap();

        // 根下的一般目錄 → 安全
        assert!(is_safe_source_dir(&root.join("skills"), &inside));
        // 根外目錄 → 不安全（canonical 不在根下）
        assert!(!is_safe_source_dir(&root.join("skills"), &outside));
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
