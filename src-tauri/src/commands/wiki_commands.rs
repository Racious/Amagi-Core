use std::path::Path;
use chrono::Utc;
use uuid::Uuid;
use serde::Serialize;
use tauri::State;
use crate::{AppError, AppState};
use crate::models::review::{ReviewItem, ReviewItemType, ReviewStatus, RiskLevel, SyncScope};
use crate::core::{review_queue, vault_manager, wiki_exporter, clip_scanner, safety_filter};

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

/// 從檔案匯入知識：讀檔 → 保存原始來源到 vault sources/imported/ → 建草稿進佇列。
#[tauri::command]
pub async fn ingest_wiki_from_file(
    project_id: String,
    layer: String,
    page_type: String,
    file_path: String,
    state: State<'_, AppState>,
) -> Result<ReviewItem, AppError> {
    let content = std::fs::read_to_string(&file_path)
        .map_err(|e| AppError::Io(format!("讀取檔案失敗：{e}")))?;

    let safety = safety_filter::check(&content);
    if !safety.is_safe {
        let labels: Vec<String> = safety.hits.iter().map(|h| h.label.clone()).collect();
        return Err(AppError::SafetyBlocked(format!(
            "內容疑似含敏感資訊：{}",
            labels.join("、")
        )));
    }

    let p = Path::new(&file_path);
    let file_name = p
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("imported.md")
        .to_string();
    let title = p
        .file_stem()
        .and_then(|n| n.to_str())
        .unwrap_or("imported")
        .to_string();

    // 保存原始來源到 vault sources/imported/（非破壞：已存在則沿用）
    let cfg = vault_manager::get_vault_config(&state.data_dir);
    let mut source_ref = None;
    if let Some(ref vault_root) = cfg.vault_path {
        let src_dir = Path::new(vault_root).join("sources").join("imported");
        std::fs::create_dir_all(&src_dir).map_err(|e| AppError::Io(e.to_string()))?;
        let dest = src_dir.join(&file_name);
        if !dest.exists() {
            std::fs::copy(&file_path, &dest).map_err(|e| AppError::Io(e.to_string()))?;
        }
        source_ref = Some(format!("sources/imported/{file_name}"));
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
        source_pending_file: source_ref,
        created_at: Utc::now(),
        reviewed_at: None,
    };
    review_queue::add_items(&state.data_dir, vec![item.clone()])?;
    Ok(item)
}

/// 掃描 vault sources/clips/，為新剪藏產生 wiki 候選進佇列；回傳新增筆數。
#[tauri::command]
pub async fn scan_vault_clips(state: State<'_, AppState>) -> Result<usize, AppError> {
    let data_dir = state.data_dir.clone();
    let cfg = vault_manager::get_vault_config(&data_dir);
    let vault_root = cfg
        .vault_path
        .ok_or_else(|| AppError::InvalidPath("尚未設定 vault 路徑，請先到「設定」指定".into()))?;

    let existing: Vec<String> = review_queue::list_items(&data_dir, None)
        .into_iter()
        .filter(|i| i.item_type == ReviewItemType::Wiki)
        .filter_map(|i| i.source_pending_file)
        .collect();

    let candidates = clip_scanner::scan_clips(Path::new(&vault_root), &existing)?;
    let n = candidates.len();
    if n > 0 {
        review_queue::add_items(&data_dir, candidates)?;
    }
    Ok(n)
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

    // 只把「真的寫進去」的標記為 Synced；被略過（目標已存在）的退回 Pending，
    // 留在待審核區供老爺改標題重試，避免落入 accepted 的 UI 死角。
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
    if !res.skipped.is_empty() {
        let skipped_ids: Vec<String> = wiki_items
            .iter()
            .filter(|i| {
                let slug = crate::utils::fs_utils::slugify(&i.title);
                res.skipped.iter().any(|s| s.contains(&slug))
            })
            .map(|i| i.id.clone())
            .collect();
        review_queue::mark_pending(&data_dir, &skipped_ids)?;
    }

    Ok(WikiWriteResultDto {
        written: res.written,
        skipped: res.skipped,
    })
}
