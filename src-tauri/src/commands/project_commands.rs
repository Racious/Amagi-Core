use tauri::State;
use crate::{AppError, AppState};
use crate::models::project::{ProjectInfo, InitResult};
use crate::core::{project_manager, vault_manager};

#[tauri::command]
pub async fn add_project(
    path: String,
    state: State<'_, AppState>,
) -> Result<ProjectInfo, AppError> {
    let data_dir = state.data_dir.clone();
    let project = project_manager::add_project(&path, &data_dir)?;
    Ok(project_manager::get_project_info(&project))
}

#[tauri::command]
pub async fn init_project(
    project_id: String,
    state: State<'_, AppState>,
) -> Result<InitResult, AppError> {
    let data_dir = state.data_dir.clone();
    let project = project_manager::get_project(&project_id, &data_dir)
        .ok_or_else(|| AppError::ProjectNotFound(project_id.clone()))?;
    // 防守深度：把本機 vault 根傳入，init 端拒絕在 vault 內落骨架（2026-07-03 事故）。
    let vault_root = vault_manager::get_vault_config(&data_dir).vault_path;
    project_manager::init_project(&project, vault_root.as_deref().map(std::path::Path::new))
}

#[tauri::command]
pub async fn list_projects(state: State<'_, AppState>) -> Result<Vec<ProjectInfo>, AppError> {
    let data_dir = state.data_dir.clone();
    let projects = project_manager::list_projects(&data_dir);
    Ok(projects.iter().map(project_manager::get_project_info).collect())
}

#[tauri::command]
pub async fn remove_project(
    project_id: String,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    let data_dir = state.data_dir.clone();
    project_manager::remove_project(&project_id, &data_dir)
}
