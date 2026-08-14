use std::path::Path;
use tauri::State;
use crate::{AppError, AppState};
use crate::core::{output_style_library, project_manager, vault_git, vault_manager::{self, VaultConfig, VaultSetResult, VaultStatus, DeployResult}};
use crate::models::project::InitResult;

#[tauri::command]
pub async fn set_vault_path(
    path: String,
    state: State<'_, AppState>,
) -> Result<VaultSetResult, AppError> {
    vault_manager::set_vault_path(&path, &state.data_dir)
}

/// 步驟5「同步全域」：把 vault `general/_meta/global-agent-config.md` 整檔部署到
/// 本機 ~/.claude/CLAUDE.md 與 ~/.codex/AGENTS.md（fail-closed + 備份 + 原子寫入）。
#[tauri::command]
pub async fn deploy_global_doctrine(state: State<'_, AppState>) -> Result<DeployResult, AppError> {
    vault_manager::deploy_global_doctrine(&state.data_dir)
}

/// Output style 分發：vault `_output-styles/*.md` → `~/.claude/output-styles/`（覆蓋、冪等），
/// 並 ensure `~/.claude/settings.json` 的 `outputStyle` 預設（缺補「天城」、有值不動、壞檔跳過）。
#[tauri::command]
pub async fn distribute_output_styles(
    state: State<'_, AppState>,
) -> Result<output_style_library::OutputStyleDistributeResult, AppError> {
    let vault_path = vault_manager::get_vault_config(&state.data_dir).vault_path
        .ok_or_else(|| AppError::InvalidPath("尚未設定 vault 路徑，無法分發 output styles".into()))?;
    let styles_dest = crate::utils::fs_utils::global_claude_output_styles_dir()
        .ok_or_else(|| AppError::Io("無法取得 ~/.claude/output-styles 路徑".into()))?;
    let settings = crate::utils::fs_utils::global_claude_settings_json_path()
        .ok_or_else(|| AppError::Io("無法取得 ~/.claude/settings.json 路徑".into()))?;
    output_style_library::distribute_output_styles(Path::new(&vault_path), &styles_dest, &settings)
}

#[tauri::command]
pub async fn get_vault_config(state: State<'_, AppState>) -> Result<VaultConfig, AppError> {
    Ok(vault_manager::get_vault_config(&state.data_dir))
}

/// 首次啟動引導（2c）用：回報 vault 是否已設定、是否已掛 git。
#[tauri::command]
pub async fn get_vault_status(state: State<'_, AppState>) -> Result<VaultStatus, AppError> {
    Ok(vault_manager::get_vault_status(&state.data_dir))
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
