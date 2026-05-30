use tauri::State;
use crate::{AppError, AppState};
use crate::models::sync::ScanResult;
use crate::core::{project_manager, git_scanner};

#[tauri::command]
pub async fn scan_project(
    project_id: String,
    state: State<'_, AppState>,
) -> Result<ScanResult, AppError> {
    let data_dir = state.data_dir.clone();
    let project = project_manager::get_project(&project_id, &data_dir)
        .ok_or_else(|| AppError::ProjectNotFound(project_id.clone()))?;
    git_scanner::scan(&project_id, &project.path)
}
