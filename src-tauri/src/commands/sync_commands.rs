use tauri::State;
use crate::{AppError, AppState};
use crate::models::sync::{SyncResult, FileDiffPreview};
use crate::models::review::ReviewStatus;
use crate::core::{project_manager, review_queue, agent_exporter, skill_index};

#[tauri::command]
pub async fn sync_agent_files(
    project_id: String,
    state: State<'_, AppState>,
) -> Result<SyncResult, AppError> {
    let data_dir = state.data_dir.clone();
    let project = project_manager::get_project(&project_id, &data_dir)
        .ok_or_else(|| AppError::ProjectNotFound(project_id.clone()))?;

    let all_items = review_queue::list_items(&data_dir, Some(&project_id));
    let accepted: Vec<_> = all_items.into_iter()
        .filter(|i| i.status == ReviewStatus::Accepted)
        .collect();

    let mut result = agent_exporter::sync_agent_files(&project.path, &accepted)?;
    result.project_id = project_id.clone();

    // ── 同步完成後標記為 Synced ───────────────────────
    let synced_ids: Vec<String> = accepted.iter().map(|i| i.id.clone()).collect();
    review_queue::mark_synced(&data_dir, &synced_ids)?;

    // ── 歸檔已同步的 pending 技能檔 ──────────────────
    let history_dir = std::path::Path::new(&project.path).join(".amagi").join("history");
    for item in &accepted {
        if let Some(ref src) = item.source_pending_file {
            let src_path = std::path::Path::new(src);
            if src_path.exists() {
                if let Some(fname) = src_path.file_name() {
                    let dest = history_dir.join(fname);
                    let _ = std::fs::rename(src_path, &dest);
                }
            }
        }
    }

    // ── 重建技能索引並注入 CLAUDE.md / AGENTS.md ──────
    skill_index::refresh_skill_index(&project.path)?;

    Ok(result)
}

#[tauri::command]
pub async fn preview_sync_diff(
    project_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<FileDiffPreview>, AppError> {
    let data_dir = state.data_dir.clone();
    let project = project_manager::get_project(&project_id, &data_dir)
        .ok_or_else(|| AppError::ProjectNotFound(project_id.clone()))?;

    let all_items = review_queue::list_items(&data_dir, Some(&project_id));
    let accepted: Vec<_> = all_items.into_iter()
        .filter(|i| i.status == ReviewStatus::Accepted)
        .collect();

    Ok(agent_exporter::preview_sync_diff(&project.path, &accepted))
}

/// 重建技能索引：依 .amagi/skills/ 現有技能，重寫 CLAUDE.md / AGENTS.md 的索引區塊
#[tauri::command]
pub async fn rebuild_skill_index(
    project_id: String,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    let data_dir = state.data_dir.clone();
    let project = project_manager::get_project(&project_id, &data_dir)
        .ok_or_else(|| AppError::ProjectNotFound(project_id.clone()))?;
    skill_index::refresh_skill_index(&project.path)
}
