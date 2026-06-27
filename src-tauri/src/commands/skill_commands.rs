use std::path::Path;
use serde::Serialize;
use tauri::State;
use crate::{AppError, AppState};
use crate::core::{skill_library, vault_manager, project_manager};
use crate::utils::fs_utils;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibrarySkillDto {
    pub slug: String,
    pub name: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DistributeResultDto {
    pub skill_count: usize,
    pub repo_count: usize,
    pub written_count: usize,
}

#[tauri::command]
pub async fn list_library_skills(state: State<'_, AppState>) -> Result<Vec<LibrarySkillDto>, AppError> {
    let cfg = vault_manager::get_vault_config(&state.data_dir);
    let vault_root = match cfg.vault_path {
        Some(v) => v,
        None => return Ok(vec![]),
    };
    Ok(skill_library::list_library_skills(Path::new(&vault_root))
        .into_iter()
        .map(|s| LibrarySkillDto { slug: s.slug, name: s.name })
        .collect())
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillSelectionDto {
    pub skill_slug: String,
    /// "global" 或某專案路徑
    pub target: String,
}

/// 選擇性分發：只把前端勾選的「技能 × 目標」配對寫出。取代粗暴的「全技能→全專案」。
#[tauri::command]
pub async fn distribute_skills_selective(
    selections: Vec<SkillSelectionDto>,
    state: State<'_, AppState>,
) -> Result<DistributeResultDto, AppError> {
    let data_dir = state.data_dir.clone();
    let cfg = vault_manager::get_vault_config(&data_dir);
    let vault_root = cfg
        .vault_path
        .ok_or_else(|| AppError::InvalidPath("尚未設定 vault 路徑，請先到「設定」指定".into()))?;
    let codex_dir = fs_utils::global_codex_skills_dir()
        .ok_or_else(|| AppError::Io("無法取得 ~/.codex/skills 路徑".into()))?;
    let claude_dir = fs_utils::global_claude_skills_dir()
        .ok_or_else(|| AppError::Io("無法取得 ~/.claude/skills 路徑".into()))?;

    // 白名單：只允許 "global" 與「已加入專案」的路徑，杜絕寫入未註冊目錄。
    let allowed: std::collections::HashSet<String> = project_manager::list_projects(&data_dir)
        .into_iter()
        .map(|p| p.path)
        .collect();
    let pairs: Vec<(String, String)> = selections
        .into_iter()
        .filter(|s| s.target == "global" || allowed.contains(&s.target))
        .map(|s| (s.skill_slug, s.target))
        .collect();
    let res = skill_library::distribute_selective(Path::new(&vault_root), &pairs, &codex_dir, &claude_dir)?;
    Ok(DistributeResultDto {
        skill_count: res.skill_count,
        repo_count: res.repo_count,
        written_count: res.written.len(),
    })
}
