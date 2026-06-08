use tauri::State;
use crate::{AppError, AppState};
use crate::models::diff::{ChangedFile, DiffBundle};
use crate::core::{project_manager, diff_export};

/// 列出指定專案底下所有異動檔（修改／新增／刪除／改名／未追蹤）
#[tauri::command]
pub async fn list_changed_files(
    project_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<ChangedFile>, AppError> {
    let data_dir = state.data_dir.clone();
    let project = project_manager::get_project(&project_id, &data_dir)
        .ok_or_else(|| AppError::ProjectNotFound(project_id.clone()))?;
    diff_export::list_changed_files(&project.path)
}

/// 對勾選的檔案產生 diff 文字（框1 異動／框2 新增刪除）
#[tauri::command]
pub async fn generate_diff_text(
    project_id: String,
    paths: Vec<String>,
    state: State<'_, AppState>,
) -> Result<DiffBundle, AppError> {
    let data_dir = state.data_dir.clone();
    let project = project_manager::get_project(&project_id, &data_dir)
        .ok_or_else(|| AppError::ProjectNotFound(project_id.clone()))?;
    diff_export::generate_diff_text(&project.path, &paths)
}
