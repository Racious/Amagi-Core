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
use commands::diff_commands::*;
use commands::learn_commands::*;
use commands::review_commands::*;
use commands::sync_commands::*;
use commands::workflow_commands::*;
use commands::bridge_commands::*;
use commands::vault_commands::*;
use commands::wiki_commands::*;
use commands::skill_commands::*;

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

/// 清理 %TEMP% 中本 app 殘留的更新安裝包（NSIS：「AMAGI Core-<版本>-installer.exe」）。
/// App 能啟動即代表上次更新已完成，這些殘留皆可安全刪除；刪不掉也忽略，下次啟動再試。
fn cleanup_update_residue() {
    cleanup_residue_in(&std::env::temp_dir());
}

/// 刪除指定目錄中符合更新安裝包命名的檔案（核心邏輯，便於測試）。
fn cleanup_residue_in(dir: &std::path::Path) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("AMAGI Core-") && name.ends_with("-installer.exe") {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let data_dir = utils::fs_utils::app_data_dir()
        .expect("無法初始化 AppData 目錄");

    std::fs::create_dir_all(&data_dir).expect("無法建立 AppData 目錄");

    tauri::Builder::default()
        // 單一實例：必須最先註冊。再次啟動時不另開行程，
        // 而是把既有（可能縮在系統匣的）視窗叫回前景。
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                if w.is_minimized().unwrap_or(false) {
                    let _ = w.unminimize();
                }
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState { data_dir })
        .setup(|app| {
            // 背景清理上次更新殘留的安裝包（不阻塞啟動）
            std::thread::spawn(cleanup_update_residue);

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
            list_changed_files,
            generate_diff_text,
            reveal_in_explorer,
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
            set_vault_path,
            get_vault_config,
            init_project_vault,
            ingest_wiki_page,
            ingest_wiki_from_file,
            scan_vault_clips,
            write_wiki_pages,
            list_library_skills,
            distribute_skill_library,
        ])
        .run(tauri::generate_context!())
        .expect("AMAGI Core 啟動失敗");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cleanup_residue_in() {
        let dir = std::env::temp_dir().join(format!("amagi-cleanup-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("AMAGI Core-0.1.5-installer.exe"), b"x").unwrap();
        std::fs::write(dir.join("AMAGI Core-0.1.6-installer.exe"), b"x").unwrap();
        std::fs::write(dir.join("other-installer.exe"), b"x").unwrap(); // 不該刪
        std::fs::write(dir.join("AMAGI Core-notes.txt"), b"x").unwrap(); // 不該刪

        cleanup_residue_in(&dir);

        assert!(!dir.join("AMAGI Core-0.1.5-installer.exe").exists());
        assert!(!dir.join("AMAGI Core-0.1.6-installer.exe").exists());
        assert!(dir.join("other-installer.exe").exists());
        assert!(dir.join("AMAGI Core-notes.txt").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
