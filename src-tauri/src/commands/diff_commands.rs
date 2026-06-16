use std::path::Path;
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

/// 用檔案總管開啟專案目錄；給 rel_path 則定位並選中該檔（對非 ASCII 檔名亦適用）
#[tauri::command]
pub async fn reveal_in_explorer(
    project_id: String,
    rel_path: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    let data_dir = state.data_dir.clone();
    let project = project_manager::get_project(&project_id, &data_dir)
        .ok_or_else(|| AppError::ProjectNotFound(project_id.clone()))?;

    let target = match rel_path.as_deref() {
        Some(rel) => {
            // 沿用差異匯出的相對路徑安全驗證（防跳脫／絕對路徑／旗標注入）
            crate::core::git_scanner::validate_rel_path(rel)?;
            Path::new(&project.path).join(rel)
        }
        None => Path::new(&project.path).to_path_buf(),
    };
    if !target.exists() {
        return Err(AppError::InvalidPath(format!("路徑不存在：{}", target.display())));
    }

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        // explorer 只認反斜線；git 相對路徑用正斜線，混合分隔符會讓 /select 找不到檔
        let path_str = target.to_string_lossy().replace('/', "\\");
        // 用一般 Command：不加 CREATE_NO_WINDOW（explorer 是 GUI，本就不閃主控台，
        // 該旗標反而會干擾它開窗）。
        let mut cmd = std::process::Command::new("explorer");
        // raw_arg 自行控制引號：/select, 之後的路徑要「單獨」加引號，否則含空格路徑
        // 會被 Rust 整段包成一組引號，explorer 解析失敗、定位不到檔。
        if rel_path.is_some() {
            cmd.raw_arg(format!("/select,\"{}\"", path_str));
        } else {
            cmd.raw_arg(format!("\"{}\"", path_str));
        }
        // explorer 即使成功也常回傳非 0 退出碼，故只 spawn 不檢查狀態
        cmd.spawn().map_err(|e| AppError::Io(e.to_string()))?;
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = target; // 非 Windows 平台暫不支援
    }
    Ok(())
}
