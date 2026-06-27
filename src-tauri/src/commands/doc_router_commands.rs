use std::path::Path;
use serde::Serialize;
use tauri::State;
use crate::{AppError, AppState};
use crate::core::{agent_exporter, doc_router, project_manager, safety_filter, vault_manager};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteDecisionDto {
    pub doc_type: String,
    pub bucket: String,
    pub dir_relative: String,
    pub destination: String,
    pub is_fallback: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteResultDto {
    pub doc_type: String,
    pub bucket: String,
    pub destination: String,
    pub written: bool,
    pub skipped: bool,
    pub is_fallback: bool,
}

/// 解析 vault 根；未設定則回明確錯誤。
fn vault_root(state: &State<'_, AppState>) -> Result<String, AppError> {
    vault_manager::get_vault_config(&state.data_dir)
        .vault_path
        .ok_or_else(|| AppError::InvalidPath("尚未設定 vault 路徑，請先到「設定」指定".into()))
}

/// 由 project_id 解析其 vault 邏輯資料夾（如 `projects/amagi-core`）。
/// project_id 為 None → 回 None（handoff 等頂層落點不需專案）。
fn resolve_project_folder(
    project_id: Option<&str>,
    state: &State<'_, AppState>,
) -> Result<Option<String>, AppError> {
    match project_id {
        None => Ok(None),
        Some(id) => {
            let project = project_manager::get_project(id, &state.data_dir)
                .ok_or_else(|| AppError::ProjectNotFound(id.to_string()))?;
            // 沿用既有 fallback：缺 vault_folder（舊 state）時由專案路徑推導 projects/<slug>，
            // 與 sync/preview/init 一致，避免舊專案無法路由（Codex 審查 D-中）。
            let folder = project
                .vault_folder
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| agent_exporter::project_vault_folder(&project.path));
            Ok(Some(folder))
        }
    }
}

/// 乾跑預覽：回報文件將依 `type` 落到哪個桶與最終路徑，不寫入。
#[tauri::command]
pub async fn preview_document_route(
    project_id: Option<String>,
    content: String,
    filename: Option<String>,
    state: State<'_, AppState>,
) -> Result<RouteDecisionDto, AppError> {
    let project_folder = resolve_project_folder(project_id.as_deref(), &state)?;
    let (decision, destination) =
        doc_router::preview_route(project_folder.as_deref(), &content, filename.as_deref())?;
    Ok(RouteDecisionDto {
        doc_type: decision.doc_type,
        bucket: decision.bucket,
        dir_relative: decision.dir_relative,
        destination,
        is_fallback: decision.is_fallback,
    })
}

/// 文件路由器（adr-004 D7-②/D8 硬性兜底）：把 AI 產出依 frontmatter `type`
/// 寫入 vault 對應桶。寫入前過安全過濾、非破壞（目標已存在則略過）。
#[tauri::command]
pub async fn route_document(
    project_id: Option<String>,
    content: String,
    filename: Option<String>,
    state: State<'_, AppState>,
) -> Result<RouteResultDto, AppError> {
    let safety = safety_filter::check(&content);
    if !safety.is_safe {
        let labels: Vec<String> = safety.hits.iter().map(|h| h.label.clone()).collect();
        return Err(AppError::SafetyBlocked(format!(
            "內容疑似含敏感資訊：{}",
            labels.join("、")
        )));
    }

    let root = vault_root(&state)?;
    let project_folder = resolve_project_folder(project_id.as_deref(), &state)?;

    let res = doc_router::route_document(
        Path::new(&root),
        project_folder.as_deref(),
        &content,
        filename.as_deref(),
    )?;

    Ok(RouteResultDto {
        doc_type: res.decision.doc_type,
        bucket: res.decision.bucket,
        destination: res.destination,
        written: res.written,
        skipped: res.skipped,
        is_fallback: res.decision.is_fallback,
    })
}
