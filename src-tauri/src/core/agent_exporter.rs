use std::path::Path;
use crate::AppError;
use crate::models::review::{ReviewItem, ReviewItemType, SyncScope};
use crate::models::sync::{SyncResult, FileDiffPreview};
use crate::utils::{fs_utils, markdown};

pub fn sync_agent_files(project_path: &str, accepted: &[ReviewItem]) -> Result<SyncResult, AppError> {
    let mut written: Vec<String> = Vec::new();

    let memories: Vec<&ReviewItem> = accepted.iter()
        .filter(|i| i.item_type == ReviewItemType::Memory)
        .collect();

    if !memories.is_empty() {
        // ── 專案層 AGENTS.md ──────────────────────────────
        let project_memories: Vec<ReviewItem> = memories.iter()
            .filter(|i| i.sync_scope == SyncScope::Project)
            .map(|i| (*i).clone())
            .collect();
        if !project_memories.is_empty() {
            let agents_path = Path::new(project_path).join("AGENTS.md");
            let content = markdown::build_agents_md(&project_memories);
            markdown::write_with_backup(&agents_path, &content)?;
            written.push(agents_path.to_string_lossy().to_string());
        }

        // ── 全域層 ~/.claude/CLAUDE.md ───────────────────
        let global_memories: Vec<ReviewItem> = memories.iter()
            .filter(|i| i.sync_scope == SyncScope::Global)
            .map(|i| (*i).clone())
            .collect();
        if !global_memories.is_empty() {
            if let Some(global_claude) = fs_utils::global_claude_md_path() {
                if let Some(parent) = global_claude.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| AppError::Io(e.to_string()))?;
                }
                let content = markdown::build_claude_md(&global_memories);
                markdown::write_with_backup(&global_claude, &content)?;
                written.push(global_claude.to_string_lossy().to_string());
            }
        }

        // ── 專案層 CLAUDE.md（agent_rule 類型）─────────────
        let agent_rules: Vec<ReviewItem> = memories.iter()
            .filter(|i| i.category == "agent_rule" && i.sync_scope == SyncScope::Project)
            .map(|i| (*i).clone())
            .collect();
        if !agent_rules.is_empty() {
            let claude_path = Path::new(project_path).join("CLAUDE.md");
            let content = markdown::build_claude_md(&agent_rules);
            markdown::write_with_backup(&claude_path, &content)?;
            written.push(claude_path.to_string_lossy().to_string());
        }
    }

    let skills: Vec<&ReviewItem> = accepted.iter()
        .filter(|i| i.item_type == ReviewItemType::Skill)
        .collect();

    for skill in &skills {
        let slug = fs_utils::slugify(&skill.title);

        let (codex_dir, claude_cmd_dir) = match skill.sync_scope {
            SyncScope::Global => {
                // 全域：~/.codex/skills/<slug>  和  ~/.claude/commands/
                let cd = fs_utils::global_codex_skills_dir()
                    .map(|d| d.join(&slug));
                let cc = fs_utils::global_claude_commands_dir();
                (cd, cc)
            }
            SyncScope::Project => {
                // 專案層：<project>/.codex/skills/<slug>
                let cd = Some(Path::new(project_path).join(".codex").join("skills").join(&slug));
                let cc = Some(Path::new(project_path).join(".claude").join("commands"));
                (cd, cc)
            }
        };

        // ── .amagi/skills/ 主副本（永遠寫入，不論 scope）────
        let amagi_skills_dir = Path::new(project_path).join(".amagi").join("skills");
        std::fs::create_dir_all(&amagi_skills_dir).map_err(|e| AppError::Io(e.to_string()))?;
        let amagi_path = amagi_skills_dir.join(format!("{}.md", slug));
        markdown::write_with_backup(&amagi_path, &markdown::build_skill_md(skill))?;
        written.push(amagi_path.to_string_lossy().to_string());

        // ── AI 工具目錄（依 scope 決定位置）────────────────
        if let Some(dir) = codex_dir {
            std::fs::create_dir_all(&dir).map_err(|e| AppError::Io(e.to_string()))?;
            let path = dir.join("SKILL.md");
            markdown::write_with_backup(&path, &markdown::build_skill_md(skill))?;
            written.push(path.to_string_lossy().to_string());
        }

        if let Some(dir) = claude_cmd_dir {
            std::fs::create_dir_all(&dir).map_err(|e| AppError::Io(e.to_string()))?;
            let path = dir.join(format!("{}.md", slug));
            markdown::write_with_backup(&path, &markdown::build_claude_command_md(skill))?;
            written.push(path.to_string_lossy().to_string());
        }
    }

    Ok(SyncResult {
        project_id: String::new(),
        written_files: written,
        skipped_files: Vec::new(),
    })
}

pub fn preview_sync_diff(project_path: &str, accepted: &[ReviewItem]) -> Vec<FileDiffPreview> {
    let mut previews = Vec::new();

    let memories: Vec<ReviewItem> = accepted.iter()
        .filter(|i| i.item_type == ReviewItemType::Memory)
        .cloned()
        .collect();

    if !memories.is_empty() {
        let agents_path = Path::new(project_path).join("AGENTS.md");
        let new_content = markdown::build_agents_md(&memories);
        let current = std::fs::read_to_string(&agents_path).ok();
        previews.push(FileDiffPreview {
            file_path: agents_path.to_string_lossy().to_string(),
            current_content: current,
            new_content,
            is_new_file: !agents_path.exists(),
        });
    }

    for skill in accepted.iter().filter(|i| i.item_type == ReviewItemType::Skill) {
        let slug = fs_utils::slugify(&skill.title);
        let codex_path = Path::new(project_path)
            .join(".codex").join("skills").join(&slug).join("SKILL.md");
        let new_content = markdown::build_skill_md(skill);
        let current = std::fs::read_to_string(&codex_path).ok();
        previews.push(FileDiffPreview {
            file_path: codex_path.to_string_lossy().to_string(),
            current_content: current,
            new_content,
            is_new_file: !codex_path.exists(),
        });
    }

    previews
}
