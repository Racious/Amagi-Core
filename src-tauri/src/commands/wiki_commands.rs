use std::path::Path;
use chrono::Utc;
use uuid::Uuid;
use serde::Serialize;
use tauri::State;
use crate::{AppError, AppState};
use crate::models::review::{ReviewItem, ReviewItemType, ReviewStatus, RiskLevel, SyncScope};
use crate::core::{review_queue, vault_manager, wiki_exporter, safety_filter};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WikiWriteResultDto {
    pub written: Vec<String>,
    pub skipped: Vec<String>,
}

/// 匯入一筆知識頁草稿，進入審核佇列（status: Pending）。
#[tauri::command]
pub async fn ingest_wiki_page(
    project_id: String,
    layer: String,
    page_type: String,
    title: String,
    content: String,
    state: State<'_, AppState>,
) -> Result<ReviewItem, AppError> {
    let safety = safety_filter::check(&content);
    if !safety.is_safe {
        let labels: Vec<String> = safety.hits.iter().map(|h| h.label.clone()).collect();
        return Err(AppError::SafetyBlocked(format!(
            "內容疑似含敏感資訊：{}",
            labels.join("、")
        )));
    }
    if title.trim().is_empty() {
        return Err(AppError::InvalidPath("標題不可為空".into()));
    }

    let item = ReviewItem {
        id: Uuid::new_v4().to_string(),
        project_id,
        item_type: ReviewItemType::Wiki,
        category: page_type,
        title,
        content,
        risk: RiskLevel::Low,
        status: ReviewStatus::Pending,
        sync_targets: vec![layer],
        sync_scope: SyncScope::Project,
        source_pending_file: None,
        created_at: Utc::now(),
        reviewed_at: None,
    };
    review_queue::add_items(&state.data_dir, vec![item.clone()])?;
    Ok(item)
}

/// 接受指定的 wiki 候選並寫入 vault；成功者標記為 Synced。
#[tauri::command]
pub async fn write_wiki_pages(
    ids: Vec<String>,
    state: State<'_, AppState>,
) -> Result<WikiWriteResultDto, AppError> {
    let data_dir = state.data_dir.clone();
    let cfg = vault_manager::get_vault_config(&data_dir);
    let vault_root = cfg
        .vault_path
        .ok_or_else(|| AppError::InvalidPath("尚未設定 vault 路徑，請先到「設定」指定".into()))?;

    let accepted = review_queue::accept_items(&data_dir, &ids)?;
    let wiki_items: Vec<ReviewItem> = accepted
        .into_iter()
        .filter(|i| i.item_type == ReviewItemType::Wiki)
        .collect();

    let res = wiki_exporter::write_wiki_pages(Path::new(&vault_root), &wiki_items)?;

    // 只把「真的寫進去」的標記為 Synced；被略過（已存在）的維持 Accepted 供老爺處理
    if !res.written.is_empty() {
        let written_ids: Vec<String> = wiki_items
            .iter()
            .filter(|i| {
                let slug = crate::utils::fs_utils::slugify(&i.title);
                res.written.iter().any(|w| w.contains(&slug))
            })
            .map(|i| i.id.clone())
            .collect();
        review_queue::mark_synced(&data_dir, &written_ids)?;
    }

    Ok(WikiWriteResultDto {
        written: res.written,
        skipped: res.skipped,
    })
}
