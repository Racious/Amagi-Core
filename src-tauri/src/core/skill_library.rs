use std::path::{Path, PathBuf};
use crate::AppError;

/// 技能庫中的一筆技能（原生目錄式 vault `_skills/<slug>/SKILL.md`）。
pub struct LibrarySkill {
    pub slug: String,
    pub name: String,
    /// 完整 SKILL.md 內容（供詳情跳窗）。
    pub content: String,
    /// 是否已分發到全域（~/.codex/skills 或 ~/.claude/skills 任一存在）。
    pub distributed_global: bool,
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
    /// 被選為目標、但磁碟目錄已不存在/非目錄的專案路徑（去重）。
    /// 類比 `AdoptResult::missing`：不靜默跳過，回報讓使用者知情（如 projects.json
    /// 殘留但磁碟目錄已刪的「幽靈專案」）。`global` 永不入此清單。
    pub invalid_targets: Vec<String>,
}

#[derive(Default)]
pub struct UndistributeResult {
    /// 已移除的目標目錄路徑（每個 base/slug 一筆）。
    pub removed: Vec<String>,
    /// 實際有移除到東西的技能數（去重）。
    pub skill_count: usize,
    /// 實際有移除到東西的目標數（去重；global 算一個）。
    pub target_count: usize,
    /// 被選為目標、但磁碟目錄已不存在/非目錄的專案路徑（去重）。
    pub invalid_targets: Vec<String>,
}

/// 收集技能庫中的技能，回傳 (slug, content)。
///
/// 優先採原生目錄式 `_skills/<slug>/SKILL.md`（與 Claude/Codex 慣例及分發輸出一致）；
/// 為相容舊資料，亦接受扁平式 `_skills/<slug>.md`。
/// 合法技能 slug：非空、非 dot-prefixed（排除 `.system` 等保留/隱藏命名空間）、不含路徑分隔。
/// 防止分發誤寫 `~/.codex/skills/.system` 等保留目錄。
pub(crate) fn is_valid_skill_slug(slug: &str) -> bool {
    !slug.is_empty()
        && !slug.starts_with('.')
        && !slug.contains('/')
        && !slug.contains('\\')
}

/// 已知官方/內建技能 slug（Codex/Claude 隨附）。收編它們會造成「過時副本＋雙重來源」，
/// 故掃描候選時排除（adr-004 收編準則）。採黑名單：現實中唯一誤報源就是這些內建技能
/// （Claude 官方技能走 plugin、不落 skills/ 目錄，掃不到）；新官方技能出現時補此清單。
/// 註：`.system/imagegen` 等 dot-prefixed 已由 `is_valid_skill_slug` 擋掉，此處保險再列非 dot 形式。
const KNOWN_OFFICIAL_SKILLS: &[&str] = &["hatch-pet", "imagegen"];

/// slug 是否為已知官方/內建技能（不應收編進 vault）。
/// 大小寫不敏感比對：Windows 檔名大小寫不敏感，防 `Hatch-Pet` 之類別名繞過（Codex F3-低）。
fn is_known_official(slug: &str) -> bool {
    KNOWN_OFFICIAL_SKILLS
        .iter()
        .any(|s| s.eq_ignore_ascii_case(slug))
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
    let codex = crate::utils::fs_utils::global_codex_skills_dir();
    let claude = crate::utils::fs_utils::global_claude_skills_dir();
    let in_global = |slug: &str| {
        let hit = |d: &Option<PathBuf>| d.as_ref()
            .map(|p| p.join(slug).join("SKILL.md").is_file())
            .unwrap_or(false);
        hit(&codex) || hit(&claude)
    };
    collect_skills(&vault_root.join("_skills"))
        .into_iter()
        .map(|(slug, content)| {
            let name = parse_name(&content).unwrap_or_else(|| slug.clone());
            let distributed_global = in_global(&slug);
            LibrarySkill { slug, name, content, distributed_global }
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
    let mut seen_invalid = std::collections::HashSet::new();

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
                // 目標目錄不存在/非目錄 → 不靜默跳過，記入回報（每目標只記一次）。
                if seen_invalid.insert(target.clone()) {
                    res.invalid_targets.push(target.clone());
                }
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

/// 某技能是否已分發到指定專案目錄（`<repo>/.codex/skills/<slug>/SKILL.md`
/// 或 `.claude` 任一存在）。供前端呈現「此技能目前分發到哪些專案」。
pub fn skill_in_project(repo: &Path, slug: &str) -> bool {
    if !is_valid_skill_slug(slug) {
        return false;
    }
    repo.join(".codex").join("skills").join(slug).join("SKILL.md").is_file()
        || repo.join(".claude").join("skills").join(slug).join("SKILL.md").is_file()
}

/// 選擇性移除分發：把指定的「技能 → 目標」配對從目標的 skills 目錄移除。
/// 與 `distribute_selective` 對稱（取消分發）。`target` 為 `"global"` 或某專案路徑。
///
/// 安全（此為唯一的刪除表面，需嚴守）：
/// - `slug` 須通過 `is_valid_skill_slug`（非空、非 dot-prefixed、不含路徑分隔），
///   杜絕 `..`、絕對路徑、保留命名空間穿越。
/// - 只移除 `<base>/<slug>` 這**一層**目錄；`base` 由呼叫端白名單（global 或已註冊專案）。
/// - 目標若為 symlink/junction → 只移除連結本身、不遞迴其指向（避免刪到連結外部目標）。
pub fn undistribute_selective(
    selections: &[(String, String)],
    codex_global: &Path,
    claude_global: &Path,
) -> Result<UndistributeResult, AppError> {
    let mut res = UndistributeResult::default();
    let mut seen_skills = std::collections::HashSet::new();
    let mut seen_targets = std::collections::HashSet::new();
    let mut seen_invalid = std::collections::HashSet::new();

    for (slug, target) in selections {
        // slug 防護：非法（含 .. / 路徑分隔 / dot-prefixed）一律不碰。
        if !is_valid_skill_slug(slug) {
            continue;
        }
        let (codex_base, claude_base): (PathBuf, PathBuf) = if target == "global" {
            (codex_global.to_path_buf(), claude_global.to_path_buf())
        } else {
            let repo = Path::new(target);
            if !repo.is_dir() {
                if seen_invalid.insert(target.clone()) {
                    res.invalid_targets.push(target.clone());
                }
                continue;
            }
            (repo.join(".codex").join("skills"), repo.join(".claude").join("skills"))
        };
        let mut removed_any = false;
        for base in [codex_base, claude_base] {
            let dir = base.join(slug);
            // 防衛性核對：最終層級必為 slug、parent 必為 base（slug 已驗證無分隔符，理應恆成立）。
            if dir.file_name().and_then(|s| s.to_str()) != Some(slug.as_str())
                || dir.parent() != Some(base.as_path())
            {
                continue;
            }
            match std::fs::symlink_metadata(&dir) {
                Ok(meta) => {
                    if meta.file_type().is_symlink() {
                        // 只拆連結本身，不遞迴其指向目標（dir-symlink/junction 走 remove_dir、
                        // file-symlink 走 remove_file）。須檢查結果：成功才計入；NotFound 視為
                        // 冪等略過；其餘錯誤如實回報，避免「顯示已移除、磁碟仍在」的失真（Codex 低）。
                        match std::fs::remove_dir(&dir).or_else(|_| std::fs::remove_file(&dir)) {
                            Ok(()) => {
                                res.removed.push(dir.to_string_lossy().to_string());
                                removed_any = true;
                            }
                            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                            Err(e) => return Err(AppError::Io(e.to_string())),
                        }
                    } else if meta.is_dir() {
                        std::fs::remove_dir_all(&dir).map_err(|e| AppError::Io(e.to_string()))?;
                        res.removed.push(dir.to_string_lossy().to_string());
                        removed_any = true;
                    }
                }
                Err(_) => {} // 不存在 → 本就無分發，略過（冪等）
            }
        }
        if removed_any {
            seen_skills.insert(slug.clone());
            seen_targets.insert(target.clone());
        }
    }

    res.skill_count = seen_skills.len();
    res.target_count = seen_targets.len();
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
            if !is_valid_skill_slug(&slug)
                || is_known_official(&slug)
                || existing.contains(&slug)
                || seen.contains(&slug)
            {
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
        // 非法 slug / 官方黑名單 / 缺 SKILL.md / 來源本身為 symlink → 無法收編。
        // 官方黑名單同步擋在核心（縱深）：scan 已過濾候選，但 adopt_skills 是公開後端表面，
        // 直接以官方 slug 呼叫（Tauri command / 批次）不應繞過收編準則（Codex F2-中）。
        let src_is_symlink = std::fs::symlink_metadata(source)
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(true);
        if !is_valid_skill_slug(slug)
            || is_known_official(slug)
            || !source.join("SKILL.md").is_file()
            || src_is_symlink
        {
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
    fn test_scan_adoptable_filters_existing_invalid_official_and_dedups() {
        let root = std::env::temp_dir().join(format!("amagi-adopt-{}", uuid::Uuid::new_v4()));
        // vault 已有 codex-review
        let lib = root.join("_skills");
        std::fs::create_dir_all(lib.join("codex-review")).unwrap();
        std::fs::write(lib.join("codex-review").join("SKILL.md"), "---\nname: cr\n---\n").unwrap();
        // 來源 A（codex 全域）：deploy-helper（自製新）、hatch-pet（官方→黑名單擋）、
        //   codex-review（vault 已有）、.system（保留）、no-md（無 SKILL.md）
        let src_a = root.join("codex_skills");
        std::fs::create_dir_all(src_a.join("deploy-helper")).unwrap();
        std::fs::write(src_a.join("deploy-helper").join("SKILL.md"), "---\nname: \"部署助手\"\n---\n").unwrap();
        std::fs::create_dir_all(src_a.join("hatch-pet")).unwrap();
        std::fs::write(src_a.join("hatch-pet").join("SKILL.md"), "---\nname: \"養寵物\"\n---\n").unwrap();
        std::fs::create_dir_all(src_a.join("codex-review")).unwrap();
        std::fs::write(src_a.join("codex-review").join("SKILL.md"), "---\nname: cr\n---\n").unwrap();
        std::fs::create_dir_all(src_a.join(".system")).unwrap();
        std::fs::write(src_a.join(".system").join("SKILL.md"), "---\nname: s\n---\n").unwrap();
        std::fs::create_dir_all(src_a.join("no-md")).unwrap();
        // 來源 B（claude 全域）：deploy-helper（與 A 重複，應去重）
        let src_b = root.join("claude_skills");
        std::fs::create_dir_all(src_b.join("deploy-helper")).unwrap();
        std::fs::write(src_b.join("deploy-helper").join("SKILL.md"), "---\nname: \"部署助手\"\n---\n").unwrap();

        let cands = scan_adoptable(&[src_a.clone(), src_b.clone()], &root);
        // 只有 deploy-helper 一筆：codex-review 已在 vault、hatch-pet 官方黑名單、
        // .system 保留、no-md 無 SKILL.md、跨來源去重
        assert_eq!(cands.len(), 1);
        assert_eq!(cands[0].slug, "deploy-helper");
        assert_eq!(cands[0].name, "部署助手");
        // hatch-pet 不得被列為候選（官方黑名單，避免過時副本＋雙重來源）
        assert!(!cands.iter().any(|c| c.slug == "hatch-pet"));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_adopt_copies_tree_and_is_non_destructive() {
        let root = std::env::temp_dir().join(format!("amagi-adopt2-{}", uuid::Uuid::new_v4()));
        // 來源技能含附屬檔（scripts/run.sh）。slug 用自製名（非官方黑名單），
        // 才驗得到正常收編路徑；官方 slug 的擋除由 test_adopt_rejects_official_blocklisted_slug 覆蓋。
        let src = root.join("src").join("deploy-helper");
        std::fs::create_dir_all(src.join("scripts")).unwrap();
        std::fs::write(src.join("SKILL.md"), "---\nname: pet\n---\n步驟").unwrap();
        std::fs::write(src.join("scripts").join("run.sh"), "echo hi").unwrap();

        let vault = root.join("vault");
        std::fs::create_dir_all(&vault).unwrap();

        let items = vec![("deploy-helper".to_string(), src.clone())];
        let res = adopt_skills(&vault, &items).unwrap();
        assert_eq!(res.adopted, vec!["deploy-helper".to_string()]);
        assert!(res.skipped.is_empty());
        // 整個目錄樹（含附屬檔）都被複製
        assert!(vault.join("_skills/deploy-helper/SKILL.md").is_file());
        assert!(vault.join("_skills/deploy-helper/scripts/run.sh").is_file());

        // 再收編一次 → 非破壞略過、不覆寫
        std::fs::write(vault.join("_skills/deploy-helper/SKILL.md"), "已被改動").unwrap();
        let res2 = adopt_skills(&vault, &items).unwrap();
        assert_eq!(res2.skipped, vec!["deploy-helper".to_string()]);
        assert!(res2.adopted.is_empty());
        let kept = std::fs::read_to_string(vault.join("_skills/deploy-helper/SKILL.md")).unwrap();
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
    fn test_adopt_rejects_official_blocklisted_slug() {
        // 縱深：即使來源含合法 SKILL.md，官方黑名單 slug 也不得被 adopt_skills 收編。
        // scan 已擋候選，但 adopt_skills 是公開後端表面，直接呼叫不應繞過收編準則（Codex 低）。
        let root = std::env::temp_dir().join(format!("amagi-adopt5-{}", uuid::Uuid::new_v4()));
        let vault = root.join("vault");
        std::fs::create_dir_all(&vault).unwrap();
        // 各官方來源都備合法 SKILL.md，確保「被擋」是因黑名單、而非缺檔
        let mk = |dir: &str| {
            let s = root.join("src").join(dir);
            std::fs::create_dir_all(&s).unwrap();
            std::fs::write(s.join("SKILL.md"), "---\nname: x\n---\n步驟").unwrap();
            s
        };
        let items = vec![
            ("hatch-pet".to_string(), mk("hatch-pet")),    // 清單第一項
            ("Hatch-Pet".to_string(), mk("hatch-pet-alias")), // 大小寫別名（Windows 不敏感）
            ("imagegen".to_string(), mk("imagegen")),      // 清單第二項
        ];
        let res = adopt_skills(&vault, &items).unwrap();
        // 全被官方黑名單擋：記入 missing（可判讀），不收編、不略過
        assert!(res.adopted.is_empty(), "官方 slug 不得被收編");
        assert!(res.skipped.is_empty());
        for slug in ["hatch-pet", "Hatch-Pet", "imagegen"] {
            assert!(res.missing.contains(&slug.to_string()), "{slug} 應記入 missing");
        }
        // vault 不得建立任何官方技能目錄
        assert!(!vault.join("_skills/hatch-pet").exists());
        assert!(!vault.join("_skills/Hatch-Pet").exists());
        assert!(!vault.join("_skills/imagegen").exists());
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

    #[test]
    fn test_distribute_reports_invalid_targets() {
        let root = std::env::temp_dir().join(format!("amagi-inv-{}", uuid::Uuid::new_v4()));
        let lib = root.join("_skills");
        std::fs::create_dir_all(lib.join("alpha")).unwrap();
        std::fs::write(lib.join("alpha").join("SKILL.md"), "---\nname: a\n---\n").unwrap();

        let codex_global = root.join("g_codex");
        let claude_global = root.join("g_claude");
        // 正常專案存在
        let live = root.join("live-repo");
        std::fs::create_dir_all(&live).unwrap();
        // 幽靈專案：projects.json 有記錄但磁碟目錄不存在
        let ghost = root.join("ghost-repo");
        let ghost_target = ghost.to_string_lossy().to_string();

        // alpha → live、ghost、global；ghost 目錄不存在 → 應入 invalid_targets
        // 同一 ghost 選兩次（不同技能）仍只記一筆（去重）
        let sel = vec![
            ("alpha".to_string(), live.to_string_lossy().to_string()),
            ("alpha".to_string(), ghost_target.clone()),
            ("alpha".to_string(), "global".to_string()),
        ];
        let res = distribute_selective(&root, &sel, &codex_global, &claude_global).unwrap();

        // 幽靈目標被回報、且不靜默
        assert_eq!(res.invalid_targets, vec![ghost_target.clone()]);
        // global 與正常專案行為不變：照常寫入、計入 repo_count，不混入 invalid
        assert_eq!(res.repo_count, 2, "live + global 兩個有效目標");
        assert!(codex_global.join("alpha/SKILL.md").exists());
        assert!(live.join(".codex/skills/alpha/SKILL.md").exists());
        assert!(live.join(".claude/skills/alpha/SKILL.md").exists());
        // 幽靈目標完全沒被寫出（目錄不存在，亦未被建立）
        assert!(!ghost.exists());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_skill_in_project() {
        let root = std::env::temp_dir().join(format!("amagi-inproj-{}", uuid::Uuid::new_v4()));
        let repo = root.join("repoA");
        // codex 有 alpha、claude 有 beta、無 gamma
        std::fs::create_dir_all(repo.join(".codex/skills/alpha")).unwrap();
        std::fs::write(repo.join(".codex/skills/alpha/SKILL.md"), "x").unwrap();
        std::fs::create_dir_all(repo.join(".claude/skills/beta")).unwrap();
        std::fs::write(repo.join(".claude/skills/beta/SKILL.md"), "x").unwrap();

        assert!(skill_in_project(&repo, "alpha"), "codex 有 → true");
        assert!(skill_in_project(&repo, "beta"), "claude 有 → true");
        assert!(!skill_in_project(&repo, "gamma"), "都沒有 → false");
        // 非法 slug 一律 false（不碰路徑）
        assert!(!skill_in_project(&repo, "../escape"));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_undistribute_removes_only_targeted_slug() {
        let root = std::env::temp_dir().join(format!("amagi-undist-{}", uuid::Uuid::new_v4()));
        let codex_g = root.join("g_codex");
        let claude_g = root.join("g_claude");
        let repo = root.join("repoA");

        // 先佈好：global 有 alpha、beta；repoA 有 alpha
        for base in [&codex_g, &claude_g] {
            std::fs::create_dir_all(base.join("alpha")).unwrap();
            std::fs::write(base.join("alpha/SKILL.md"), "a").unwrap();
            std::fs::create_dir_all(base.join("beta")).unwrap();
            std::fs::write(base.join("beta/SKILL.md"), "b").unwrap();
        }
        for sub in [".codex/skills", ".claude/skills"] {
            std::fs::create_dir_all(repo.join(sub).join("alpha")).unwrap();
            std::fs::write(repo.join(sub).join("alpha/SKILL.md"), "a").unwrap();
        }

        // 移除 global 的 alpha + repoA 的 alpha；beta 不動
        let sel = vec![
            ("alpha".to_string(), "global".to_string()),
            ("alpha".to_string(), repo.to_string_lossy().to_string()),
        ];
        let res = undistribute_selective(&sel, &codex_g, &claude_g).unwrap();
        assert_eq!(res.skill_count, 1);
        assert_eq!(res.target_count, 2, "global + repoA");

        // alpha 全沒了
        assert!(!codex_g.join("alpha").exists());
        assert!(!claude_g.join("alpha").exists());
        assert!(!repo.join(".codex/skills/alpha").exists());
        assert!(!repo.join(".claude/skills/alpha").exists());
        // beta（未指定）原封不動
        assert!(codex_g.join("beta/SKILL.md").exists());
        assert!(claude_g.join("beta/SKILL.md").exists());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_undistribute_is_idempotent_and_guards_slug() {
        let root = std::env::temp_dir().join(format!("amagi-undist2-{}", uuid::Uuid::new_v4()));
        let codex_g = root.join("g_codex");
        let claude_g = root.join("g_claude");
        std::fs::create_dir_all(&codex_g).unwrap();
        std::fs::create_dir_all(&claude_g).unwrap();
        // 旁置一個「不該被碰」的目錄，驗證非法 slug 不會穿越刪除
        std::fs::create_dir_all(codex_g.join("keep")).unwrap();
        std::fs::write(codex_g.join("keep/SKILL.md"), "k").unwrap();

        // 不存在的 slug → 冪等、不報錯、零移除
        let res = undistribute_selective(
            &[("ghost".to_string(), "global".to_string())],
            &codex_g,
            &claude_g,
        )
        .unwrap();
        assert_eq!(res.skill_count, 0);
        assert!(res.removed.is_empty());

        // 非法 slug（路徑穿越）→ 被 is_valid_skill_slug 擋，keep 不受影響
        let res2 = undistribute_selective(
            &[("../keep".to_string(), "global".to_string()), (".system".to_string(), "global".to_string())],
            &codex_g,
            &claude_g,
        )
        .unwrap();
        assert!(res2.removed.is_empty(), "非法 slug 不得移除任何東西");
        assert!(codex_g.join("keep/SKILL.md").exists(), "旁置目錄不被穿越刪除");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_undistribute_symlink_removes_link_only() {
        // 取消分發遇到 symlink/junction 時，只拆連結本身、不遞迴刪其指向 target。
        // symlink 建立需權限（Windows 未開開發者模式會失敗）→ 建不成則略過，
        // 不讓測試在無權限環境誤判失敗。
        let root = std::env::temp_dir().join(format!("amagi-undist-sym-{}", uuid::Uuid::new_v4()));
        let codex_g = root.join("g_codex");
        let claude_g = root.join("g_claude");
        std::fs::create_dir_all(&codex_g).unwrap();
        std::fs::create_dir_all(&claude_g).unwrap();
        // 連結指向的外部 target，內含不該被刪的檔
        let target = root.join("external");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("keep.txt"), "k").unwrap();

        let link = codex_g.join("alpha");
        #[cfg(windows)]
        let made = std::os::windows::fs::symlink_dir(&target, &link).is_ok();
        #[cfg(unix)]
        let made = std::os::unix::fs::symlink(&target, &link).is_ok();
        if !made {
            eprintln!("跳過 test_undistribute_symlink_removes_link_only：無權限建 symlink");
            let _ = std::fs::remove_dir_all(&root);
            return;
        }

        let res = undistribute_selective(
            &[("alpha".to_string(), "global".to_string())],
            &codex_g,
            &claude_g,
        )
        .unwrap();

        // 連結已被移除（symlink_metadata 找不到）
        assert!(std::fs::symlink_metadata(&link).is_err(), "symlink 連結本身應被移除");
        // 但指向的 target 內容原封不動（沒被遞迴刪）
        assert!(target.join("keep.txt").exists(), "不得遞迴刪除連結指向的外部 target");
        assert!(res.removed.iter().any(|p| p.contains("alpha")), "成功移除才計入 removed");

        let _ = std::fs::remove_dir_all(&root);
    }
}
