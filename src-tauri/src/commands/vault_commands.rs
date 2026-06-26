use std::path::Path;
use tauri::State;
use crate::{AppError, AppState};
use crate::core::{project_manager, vault_git, vault_manager::{self, VaultConfig, VaultSetResult}};
use crate::models::project::InitResult;

#[tauri::command]
pub async fn set_vault_path(
    path: String,
    state: State<'_, AppState>,
) -> Result<VaultSetResult, AppError> {
    vault_manager::set_vault_path(&path, &state.data_dir)
}

#[tauri::command]
pub async fn get_vault_config(state: State<'_, AppState>) -> Result<VaultConfig, AppError> {
    Ok(vault_manager::get_vault_config(&state.data_dir))
}

#[tauri::command]
pub async fn init_project_vault(
    project_id: String,
    state: State<'_, AppState>,
) -> Result<InitResult, AppError> {
    let data_dir = state.data_dir.clone();
    let project = project_manager::get_project(&project_id, &data_dir)
        .ok_or_else(|| AppError::ProjectNotFound(project_id.clone()))?;
    let cfg = vault_manager::get_vault_config(&data_dir);
    let vault_root = cfg
        .vault_path
        .ok_or_else(|| AppError::InvalidPath("尚未設定 vault 路徑，請先到「設定」指定".into()))?;
    project_manager::init_project_vault(&project, Path::new(&vault_root))
}

fn vault_root(state: &State<'_, AppState>) -> Result<String, AppError> {
    vault_manager::get_vault_config(&state.data_dir)
        .vault_path
        .ok_or_else(|| AppError::InvalidPath("尚未設定 vault 路徑，請先到「設定」指定".into()))
}

#[tauri::command]
pub async fn vault_git_status(state: State<'_, AppState>) -> Result<String, AppError> {
    vault_git::status_short(Path::new(&vault_root(&state)?))
}

#[tauri::command]
pub async fn vault_git_pull(state: State<'_, AppState>) -> Result<String, AppError> {
    vault_git::pull(Path::new(&vault_root(&state)?))
}

#[tauri::command]
pub async fn vault_git_sync(
    message: Option<String>,
    state: State<'_, AppState>,
) -> Result<String, AppError> {
    let msg = message
        .filter(|m| !m.trim().is_empty())
        .unwrap_or_else(|| "wiki: Amagi Core 自動同步".to_string());
    vault_git::sync(Path::new(&vault_root(&state)?), &msg)
}
