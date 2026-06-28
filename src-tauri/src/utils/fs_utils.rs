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
