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
use commands::doc_router_commands::*;
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

    // vault-first 一次性遷移（adr-005，Phase 3 擴至全型別）：清佇列殘留的 Synced 項
    // （memory/skill/wiki 皆已入庫即出列，殘留者內容皆在 vault）。可回滾（queue.premigration-p3.bak）。
    // 啟動時執行、冪等（無殘留即 no-op）。
    if let Ok(n) = core::review_queue::migrate_drop_synced_items(&data_dir) {
        if n > 0 {
            eprintln!("[AMAGI] vault-first 遷移：已清除 {n} 筆殘留 Synced 佇列項（已備份 queue.premigration-p3.bak）");
        }
    }

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

            // 明確指定主視窗（工作列 ICON_BIG）圖示為高解析 256px。
            // 否則 Tauri 預設視窗圖示會挑 bundle.icon 清單裡的小 PNG（32x32），
            // 工作列以小圖放大顯示 → 模糊；改餵 256px 讓 Windows 往下縮，清晰。
            // 桌面捷徑/exe 用多尺寸 .ico 不受影響（本來就清晰）。
            if let Ok(icon) = tauri::image::Image::from_bytes(include_bytes!("../icons/128x128@2x.png")) {
                let _ = window.set_icon(icon);
            }

            // 標題列/Alt-Tab 小圖（ICON_SMALL, ~16px）另餵 32px 小尺寸專用圖：
            // Tauri set_icon 會把 ICON_BIG/SMALL 設成同一張 256，標題列硬縮到 16 會有白點雜訊。
            // 此處用原生 WM_SETICON 單獨覆寫 ICON_SMALL（與系統匣同款 32px，縮放乾淨），
            // ICON_BIG（256，工作列）維持不變。建立失敗則沿用上面的 set_icon，不致退步。
            #[cfg(windows)]
            {
                use windows_sys::Win32::UI::WindowsAndMessaging::{
                    CreateIconFromResourceEx, SendMessageW, ICON_SMALL, LR_DEFAULTCOLOR, WM_SETICON,
                };
                if let Ok(hwnd) = window.hwnd() {
                    let png: &[u8] = include_bytes!("../icons/32x32.png");
                    // ficon=TRUE(1)、dwver=0x00030000(3.0)、cx/cy=0 取原生 32px
                    let hicon = unsafe {
                        CreateIconFromResourceEx(png.as_ptr(), png.len() as u32, 1, 0x0003_0000, 0, 0, LR_DEFAULTCOLOR)
                    };
                    if !hicon.is_null() {
                        unsafe {
                            SendMessageW(hwnd.0 as _, WM_SETICON, ICON_SMALL as usize, hicon as isize);
                        }
                    }
                }
            }

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
            discard_blocked_items,
            discard_blocked_as_false_positive,
            list_blocked_greylist,
            remove_greylist_entries,
            update_review_item,
            sync_agent_files,
            preview_sync_diff,
            promote_memory,
            list_vault_memories,
            scan_project_workflows,
            list_all_workflows,
            generate_workflow_command,
            plan_workflow,
            start_bridge_run,
            advance_bridge_run,
            get_bridge_run,
            cancel_bridge_run,
            set_vault_path,
            deploy_global_doctrine,
            get_vault_config,
            get_vault_status,
            init_project_vault,
            vault_git_status,
            vault_git_pull,
            vault_git_sync,
            ingest_wiki_page,
            ingest_wiki_from_file,
            scan_vault_clips,
            write_wiki_pages,
            preview_document_route,
            route_document,
            list_library_skills,
            distribute_skills_selective,
            undistribute_skills,
            scan_adoptable_skills,
            adopt_skills,
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
