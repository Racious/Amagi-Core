use std::path::Path;
use tauri::State;
use crate::{AppError, AppState};
use crate::core::{project_manager, vault_manager::{self, VaultConfig, VaultSetResult}};
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
