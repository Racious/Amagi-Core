use std::path::Path;
use serde::Serialize;
use tauri::State;
use crate::{AppError, AppState};
use crate::core::{skill_library, vault_manager, project_manager};

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

#[tauri::command]
pub async fn distribute_skill_library(state: State<'_, AppState>) -> Result<DistributeResultDto, AppError> {
    let data_dir = state.data_dir.clone();
    let cfg = vault_manager::get_vault_config(&data_dir);
    let vault_root = cfg
        .vault_path
        .ok_or_else(|| AppError::InvalidPath("尚未設定 vault 路徑，請先到「設定」指定".into()))?;

    let repos: Vec<String> = project_manager::list_projects(&data_dir)
        .into_iter()
        .map(|p| p.path)
        .collect();

    let res = skill_library::distribute(Path::new(&vault_root), &repos)?;
    Ok(DistributeResultDto {
        skill_count: res.skill_count,
        repo_count: res.repo_count,
        written_count: res.written.len(),
    })
}
