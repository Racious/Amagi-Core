use tauri::State;
use crate::{AppError, AppState};
use crate::models::bridge::BridgeRun;
use crate::core::{project_manager, bridge_engine, vault_manager};

/// 開始一個新的 File Bridge 流程
#[tauri::command]
pub async fn start_bridge_run(
    project_id: String,
    workflow_id: String,
    task: String,
    state: State<'_, AppState>,
) -> Result<BridgeRun, AppError> {
    let data_dir = state.data_dir.clone();
    let project = project_manager::get_project(&project_id, &data_dir)
        .ok_or_else(|| AppError::ProjectNotFound(project_id.clone()))?;
    // 防守深度（2026-07-03 事故）：存量「path 在 vault 內」的專案，bridge 會在
    // vault 內建 .amagi/state → 寫入型 command 一律先過閘（get 為只讀，不擋）。
    vault_manager::ensure_project_path_outside_vault(&project.path, &data_dir)?;
    bridge_engine::start_run(&project_id, &project.path, &workflow_id, &task)
}

/// 讀取 result.md 並推進到下一步
#[tauri::command]
pub async fn advance_bridge_run(
    project_id: String,
    state: State<'_, AppState>,
) -> Result<BridgeRun, AppError> {
    let data_dir = state.data_dir.clone();
    let project = project_manager::get_project(&project_id, &data_dir)
        .ok_or_else(|| AppError::ProjectNotFound(project_id.clone()))?;
    vault_manager::ensure_project_path_outside_vault(&project.path, &data_dir)?;
    bridge_engine::advance_run(&project.path)
}

/// 取得目前進行中的流程（無則回傳 null）
#[tauri::command]
pub async fn get_bridge_run(
    project_id: String,
    state: State<'_, AppState>,
) -> Result<Option<BridgeRun>, AppError> {
    let data_dir = state.data_dir.clone();
    let project = project_manager::get_project(&project_id, &data_dir)
        .ok_or_else(|| AppError::ProjectNotFound(project_id.clone()))?;
    bridge_engine::get_run(&project.path)
}

/// 中止目前流程
#[tauri::command]
pub async fn cancel_bridge_run(
    project_id: String,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    let data_dir = state.data_dir.clone();
    let project = project_manager::get_project(&project_id, &data_dir)
        .ok_or_else(|| AppError::ProjectNotFound(project_id.clone()))?;
    // cancel 會刪除 project.path 下的狀態檔，同屬寫入型 → 過閘。
    vault_manager::ensure_project_path_outside_vault(&project.path, &data_dir)?;
    bridge_engine::cancel_run(&project.path)
}
