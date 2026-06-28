use std::path::{Path, PathBuf};
use crate::AppError;
use crate::models::review::{ReviewItem, ReviewItemType, SyncScope};
use crate::models::sync::{SyncResult, FileDiffPreview};
use crate::utils::{fs_utils, markdown};

/// 由專案路徑推導 vault 邏輯資料夾名（projects/<slug>），與 Project.vault_folder 預設一致。
pub fn project_vault_folder(project_path: &str) -> String {
    let name = Path::new(project_path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");
    format!("projects/{}", fs_utils::slugify(name))
}

/// 記憶檔名：slug(title) + 穩定 item id 片段（免佇列順序變動造成漂移/同名碰撞）。
fn memory_filename(item: &ReviewItem) -> String {
    let base = {
        let s = fs_utils::slugify(&item.title);
        if s.is_empty() { "memory".to_string() } else { s }
    };
    let short_id: String = item.id.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(8)
        .collect();
    let short_id = if short_id.is_empty() { "x".to_string() } else { short_id };
    format!("{}-{}.md", base, short_id)
}

/// 由記憶項算出索引列：(檔名, 標題, 一句 hook)。sync 與 preview 共用，避免漂移。
fn memory_index_entries(items: &[&ReviewItem]) -> Vec<(String, String, String)> {
    items.iter().map(|item| {
        let fname = memory_filename(item);
        let hook = item.content.lines()
            .map(str::trim)
            .find(|l| !l.is_empty())
            .map(|l| if l.chars().count() > 40 {
                format!("{}…", l.chars().take(40).collect::<String>())
            } else {
                l.to_string()
            })
            .unwrap_or_default();
        (fname, item.title.clone(), hook)
    }).collect()
}

/// 由技能清單算出各自 vault `_skills` 落點：slug 合法性守門（空/非法 → skill-<id>）、
/// 同批同名去重、相容舊扁平 `<slug>.md`。sync 與 preview 共用，確保落點一致。
fn skill_dest_paths(skills_root: &Path, skills: &[&ReviewItem]) -> Vec<PathBuf> {
    let mut seen = std::collections::HashSet::new();
    skills.iter().map(|skill| {
        let short_id: String = skill.id.chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .take(8).collect();
        let short_id = if short_id.is_empty() { "x".to_string() } else { short_id };
        let base = fs_utils::slugify(&skill.title);
        let base_slug = if crate::core::skill_library::is_valid_skill_slug(&base) {
            base
        } else {
            format!("skill-{}", short_id)
        };
        // 迴圈式唯一化：base → base-id → base-id-2 …，直到 seen 無此 slug，
        // 確保改名後仍唯一、不互相覆寫（Codex 3c 追審）。
        let mut slug = base_slug.clone();
        let mut n = 1;
        while seen.contains(&slug) {
            n += 1;
            slug = if n == 2 {
                format!("{}-{}", base_slug, short_id)
            } else {
                format!("{}-{}-{}", base_slug, short_id, n)
            };
        }
        seen.insert(slug.clone());
        let flat = skills_root.join(format!("{}.md", slug));
        if flat.is_file() {
            flat
        } else {
            skills_root.join(&slug).join("SKILL.md")
        }
    }).collect()
}

pub fn sync_agent_files(
    project_path: &str,
    vault_folder: Option<&str>,
    vault_root: Option<&Path>,
    accepted: &[ReviewItem],
    all_project_memory: &[ReviewItem],
) -> Result<SyncResult, AppError> {
    let mut written: Vec<String> = Vec::new();
    // 優先用顯式 Project.vault_folder（權威來源）；缺時才由路徑 basename 推導。
    let vault_folder = vault_folder
        .map(|s| s.to_string())
        .unwrap_or_else(|| project_vault_folder(project_path));

    // ── 專案層記憶 → vault `<vault_folder>/agent/memory/`（Phase 3a：A 純指標）──
    // 收 Accepted+Synced 全部專案記憶寫成「一事一檔」+ 重建 MEMORY.md 索引；
    // 專案 AGENTS.md / CLAUDE.md 改為純指標（記憶內容保全於 vault，非破壞）。
    let project_mem: Vec<&ReviewItem> = all_project_memory.iter()
        .filter(|i| i.item_type == ReviewItemType::Memory && i.sync_scope == SyncScope::Project)
        .collect();
    if let (Some(vroot), false) = (vault_root, project_mem.is_empty()) {
        let mem_dir = vroot.join(&vault_folder).join("agent").join("memory");
        std::fs::create_dir_all(&mem_dir).map_err(|e| AppError::Io(e.to_string()))?;

        let entries = memory_index_entries(&project_mem);
        for (item, entry) in project_mem.iter().zip(&entries) {
            let path = mem_dir.join(&entry.0);
            markdown::write_with_backup(&path, &markdown::build_memory_file(item))?;
            written.push(path.to_string_lossy().to_string());
        }
        // 重建索引（entries 已涵蓋全部 Accepted+Synced 專案記憶 → 即完整索引）
        let idx_path = mem_dir.join("MEMORY.md");
        std::fs::write(&idx_path, markdown::build_memory_index(&entries))
            .map_err(|e| AppError::Io(e.to_string()))?;
        written.push(idx_path.to_string_lossy().to_string());

        // 專案 AGENTS.md / CLAUDE.md → 純指標
        let agents_path = Path::new(project_path).join("AGENTS.md");
        markdown::write_with_backup(&agents_path, &markdown::build_agents_md(&vault_folder))?;
        written.push(agents_path.to_string_lossy().to_string());
        let claude_path = Path::new(project_path).join("CLAUDE.md");
        markdown::write_with_backup(&claude_path, &markdown::build_claude_md(Some(&vault_folder)))?;
        written.push(claude_path.to_string_lossy().to_string());
    }

    // ── 全域 scope 記憶：Phase 3a 暫不處理（Codex 高風險 #2）──
    // 舊行為以 build_*_claude_md 整檔覆寫含老爺人格與 AMAGI-VAULT 錨點的 ~/.claude/CLAUDE.md，
    // 風險過高，故 3a 停掉此路徑。延到 3b 改寫 vault general/agent/memory + 全域錨點指標。
    // command 層不會把全域記憶項標 Synced（留 Accepted 待 3b），故此處略過不寫、不遺失。

    // ── 技能 → vault `_skills/<slug>/SKILL.md`（單一來源；Phase 3c，老爺裁定 A：解耦）──
    // sync 只「進庫」，不再自動撒到 .amagi/.codex/.claude；分發改由 Skills 頁選擇性分發。
    let skills: Vec<&ReviewItem> = accepted.iter()
        .filter(|i| i.item_type == ReviewItemType::Skill)
        .collect();
    if let (Some(vroot), false) = (vault_root, skills.is_empty()) {
        let skills_root = vroot.join("_skills");
        let dests = skill_dest_paths(&skills_root, &skills);
        for (skill, dest) in skills.iter().zip(&dests) {
            markdown::write_with_backup(dest, &markdown::build_native_skill_md(skill))?;
            written.push(dest.to_string_lossy().to_string());
        }
    }

    Ok(SyncResult {
        project_id: String::new(),
        written_files: written,
        skipped_files: Vec::new(),
        blocked_conflicts: Vec::new(),
    })
}

pub fn preview_sync_diff(
    project_path: &str,
    vault_folder: Option<&str>,
    vault_root: Option<&Path>,
    accepted: &[ReviewItem],
    all_project_memory: &[ReviewItem],
) -> Vec<FileDiffPreview> {
    let mut previews = Vec::new();
    let vault_folder = vault_folder
        .map(|s| s.to_string())
        .unwrap_or_else(|| project_vault_folder(project_path));

    // 專案記憶 → vault：預覽 AGENTS.md 純指標 + 每筆 vault 記憶檔 + MEMORY.md 索引（Codex #4）。
    let project_mem: Vec<&ReviewItem> = all_project_memory.iter()
        .filter(|i| i.item_type == ReviewItemType::Memory && i.sync_scope == SyncScope::Project)
        .collect();
    if let (Some(vroot), false) = (vault_root, project_mem.is_empty()) {
        let mem_dir = vroot.join(&vault_folder).join("agent").join("memory");
        let agents_path = Path::new(project_path).join("AGENTS.md");
        previews.push(FileDiffPreview {
            current_content: std::fs::read_to_string(&agents_path).ok(),
            new_content: markdown::build_agents_md(&vault_folder),
            is_new_file: !agents_path.exists(),
            file_path: agents_path.to_string_lossy().to_string(),
        });
        // 與 sync 一致：CLAUDE.md 也改純指標（Codex 追審 #A）
        let claude_path = Path::new(project_path).join("CLAUDE.md");
        previews.push(FileDiffPreview {
            current_content: std::fs::read_to_string(&claude_path).ok(),
            new_content: markdown::build_claude_md(Some(&vault_folder)),
            is_new_file: !claude_path.exists(),
            file_path: claude_path.to_string_lossy().to_string(),
        });
        let entries = memory_index_entries(&project_mem);
        for (item, entry) in project_mem.iter().zip(&entries) {
            let path = mem_dir.join(&entry.0);
            previews.push(FileDiffPreview {
                current_content: std::fs::read_to_string(&path).ok(),
                new_content: markdown::build_memory_file(item),
                is_new_file: !path.exists(),
                file_path: path.to_string_lossy().to_string(),
            });
        }
        let idx_path = mem_dir.join("MEMORY.md");
        previews.push(FileDiffPreview {
            current_content: std::fs::read_to_string(&idx_path).ok(),
            new_content: markdown::build_memory_index(&entries),
            is_new_file: !idx_path.exists(),
            file_path: idx_path.to_string_lossy().to_string(),
        });
    }

    // 技能 → vault `_skills`（Phase 3c·A）：preview 與 sync 共用 skill_dest_paths 算落點，確保一致。
    if let Some(vroot) = vault_root {
        let skills: Vec<&ReviewItem> = accepted.iter()
            .filter(|i| i.item_type == ReviewItemType::Skill)
            .collect();
        if !skills.is_empty() {
            let skills_root = vroot.join("_skills");
            let dests = skill_dest_paths(&skills_root, &skills);
            for (skill, dest) in skills.iter().zip(&dests) {
                previews.push(FileDiffPreview {
                    current_content: std::fs::read_to_string(dest).ok(),
                    new_content: markdown::build_native_skill_md(skill),
                    is_new_file: !dest.exists(),
                    file_path: dest.to_string_lossy().to_string(),
                });
            }
        }
    }

    previews
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::review::{RiskLevel, ReviewStatus, SyncScope};
    use chrono::Utc;

    fn sk(id: &str, title: &str) -> ReviewItem {
        ReviewItem {
            id: id.into(), project_id: "p".into(),
            item_type: ReviewItemType::Skill, category: "skill".into(),
            title: title.into(), content: "x".into(),
            risk: RiskLevel::Low, status: ReviewStatus::Accepted,
            sync_targets: vec![], sync_scope: SyncScope::Project,
            source_pending_file: None, created_at: Utc::now(), reviewed_at: None,
        }
    }

    #[test]
    fn test_skill_dest_paths_dedups_after_rename() {
        // a="foo"、b="foo-bar"、c="foo"(id=bar)：c 撞 foo→改 foo-bar 又撞 b → 須再唯一化
        let root = std::path::Path::new("/no-such-vault/_skills");
        let (a, b, c) = (sk("aaa", "foo"), sk("xxx", "foo-bar"), sk("bar", "foo"));
        let items = vec![&a, &b, &c];
        let dests = skill_dest_paths(root, &items);
        let uniq: std::collections::HashSet<_> = dests.iter().collect();
        assert_eq!(uniq.len(), 3, "三筆落點須全唯一，不互相覆寫");
    }

    #[test]
    fn test_skill_dest_paths_empty_slug_fallback() {
        // 全符號標題 → slug 空 → fallback skill-<id>，落點仍為合法目錄式
        let root = std::path::Path::new("/no-such-vault/_skills");
        let s = sk("id123456", "###");
        let dests = skill_dest_paths(root, &[&s]);
        let p = dests[0].to_string_lossy().replace('\\', "/");
        assert!(p.contains("/_skills/skill-id123456/SKILL.md"), "空 slug 應 fallback skill-<id>，實得 {p}");
    }
}
