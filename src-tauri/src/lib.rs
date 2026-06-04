use std::path::PathBuf;
use tauri::Manager;

mod commands;
mod core;
mod models;
mod utils;

#[cfg(test)]
mod e2e_test;

use commands::project_commands::*;
use commands::scan_commands::*;
use commands::learn_commands::*;
use commands::review_commands::*;
use commands::sync_commands::*;
use commands::workflow_commands::*;
use commands::bridge_commands::*;

#[derive(Debug, serde::Serialize, thiserror::Error)]
#[serde(tag = "kind", content = "message")]
pub enum AppError {
    #[error("IO 錯誤：{0}")]
    Io(String),
    #[error("Git 錯誤：{0}")]
    Git(String),
    #[error("找不到專案：{0}")]
    ProjectNotFound(String),
    #[error("無效路徑：{0}")]
    InvalidPath(String),
    #[error("安全過濾封鎖：{0}")]
    SafetyBlocked(String),
    #[error("序列化錯誤：{0}")]
    Serialization(String),
    #[error("不允許的指令：{0}")]
    CommandNotAllowed(String),
}

pub struct AppState {
    pub data_dir: PathBuf,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let data_dir = utils::fs_utils::app_data_dir()
        .expect("無法初始化 AppData 目錄");

    std::fs::create_dir_all(&data_dir).expect("無法建立 AppData 目錄");

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState { data_dir })
        .setup(|app| {
            core::tray::setup_tray(app.handle())?;

            let window = app.get_webview_window("main").unwrap();
            let app_handle = app.handle().clone();
            window.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    if let Some(w) = app_handle.get_webview_window("main") {
                        let _ = w.hide();
                    }
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            add_project,
            init_project,
            list_projects,
            remove_project,
            scan_project,
            learn_from_project,
            list_review_items,
            accept_review_items,
            ignore_review_items,
            update_review_item,
            sync_agent_files,
            preview_sync_diff,
            scan_project_workflows,
            list_all_workflows,
            generate_workflow_command,
            plan_workflow,
            start_bridge_run,
            advance_bridge_run,
            get_bridge_run,
            cancel_bridge_run,
        ])
        .run(tauri::generate_context!())
        .expect("AMAGI Core 啟動失敗");
}
