use std::path::Path;
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

    let skills: Vec<&ReviewItem> = accepted.iter()
        .filter(|i| i.item_type == ReviewItemType::Skill)
        .collect();

    for skill in &skills {
        let slug = fs_utils::slugify(&skill.title);

        // codex_dir：Codex 原生技能；claude_skill_dir：Claude 原生技能（自動觸發）
        let (codex_dir, claude_skill_dir) = match skill.sync_scope {
            SyncScope::Global => {
                // 全域：~/.codex/skills/<slug>  和  ~/.claude/skills/<slug>
                let cd = fs_utils::global_codex_skills_dir()
                    .map(|d| d.join(&slug));
                let cs = fs_utils::global_claude_commands_dir()
                    .and_then(|c| c.parent().map(|p| p.join("skills").join(&slug)));
                (cd, cs)
            }
            SyncScope::Project => {
                // 專案層：<project>/.codex/skills/<slug> 和 <project>/.claude/skills/<slug>
                let cd = Some(Path::new(project_path).join(".codex").join("skills").join(&slug));
                let cs = Some(Path::new(project_path).join(".claude").join("skills").join(&slug));
                (cd, cs)
            }
        };

        let native = markdown::build_native_skill_md(skill);

        // ── .amagi/skills/ 主副本（永遠寫入，不論 scope）────
        let amagi_skills_dir = Path::new(project_path).join(".amagi").join("skills");
        std::fs::create_dir_all(&amagi_skills_dir).map_err(|e| AppError::Io(e.to_string()))?;
        let amagi_path = amagi_skills_dir.join(format!("{}.md", slug));
        markdown::write_with_backup(&amagi_path, &native)?;
        written.push(amagi_path.to_string_lossy().to_string());

        // ── Codex 原生技能：.codex/skills/<slug>/SKILL.md ──
        if let Some(dir) = codex_dir {
            std::fs::create_dir_all(&dir).map_err(|e| AppError::Io(e.to_string()))?;
            let path = dir.join("SKILL.md");
            markdown::write_with_backup(&path, &native)?;
            written.push(path.to_string_lossy().to_string());
        }

        // ── Claude 原生技能：.claude/skills/<slug>/SKILL.md（描述自動觸發）──
        if let Some(dir) = claude_skill_dir {
            std::fs::create_dir_all(&dir).map_err(|e| AppError::Io(e.to_string()))?;
            let path = dir.join("SKILL.md");
            markdown::write_with_backup(&path, &native)?;
            written.push(path.to_string_lossy().to_string());
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

    for skill in accepted.iter().filter(|i| i.item_type == ReviewItemType::Skill) {
        let slug = fs_utils::slugify(&skill.title);
        let claude_path = Path::new(project_path)
            .join(".claude").join("skills").join(&slug).join("SKILL.md");
        let new_content = markdown::build_native_skill_md(skill);
        let current = std::fs::read_to_string(&claude_path).ok();
        previews.push(FileDiffPreview {
            file_path: claude_path.to_string_lossy().to_string(),
            current_content: current,
            new_content,
            is_new_file: !claude_path.exists(),
        });
    }

    previews
}
