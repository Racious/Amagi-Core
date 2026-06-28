use std::path::Path;
use chrono::Utc;
use crate::AppError;
use crate::models::review::{ReviewItem, ReviewQueueData, ReviewStatus, SyncScope};
use crate::utils::json_store;

fn queue_path(data_dir: &Path) -> std::path::PathBuf {
    data_dir.join("review-queue").join("queue.json")
}

pub fn add_items(data_dir: &Path, items: Vec<ReviewItem>) -> Result<(), AppError> {
    let path = queue_path(data_dir);
    let mut data: ReviewQueueData = json_store::read_json_or_default(&path);
    data.items.extend(items);
    json_store::write_json(&path, &data)
}

pub fn list_items(data_dir: &Path, project_id: Option<&str>) -> Vec<ReviewItem> {
    let path = queue_path(data_dir);
    let data: ReviewQueueData = json_store::read_json_or_default(&path);
    match project_id {
        Some(pid) => data.items.into_iter().filter(|i| i.project_id == pid).collect(),
        None => data.items,
    }
}

pub fn accept_items(data_dir: &Path, ids: &[String]) -> Result<Vec<ReviewItem>, AppError> {
    let path = queue_path(data_dir);
    let mut data: ReviewQueueData = json_store::read_json_or_default(&path);
    let mut accepted = Vec::new();
    for item in &mut data.items {
        if ids.contains(&item.id) {
            item.status = ReviewStatus::Accepted;
            item.reviewed_at = Some(Utc::now());
            accepted.push(item.clone());
        }
    }
    json_store::write_json(&path, &data)?;
    Ok(accepted)
}

pub fn ignore_items(data_dir: &Path, ids: &[String]) -> Result<(), AppError> {
    let path = queue_path(data_dir);
    let mut data: ReviewQueueData = json_store::read_json_or_default(&path);
    for item in &mut data.items {
        if ids.contains(&item.id) {
            item.status = ReviewStatus::Ignored;
            item.reviewed_at = Some(Utc::now());
        }
    }
    json_store::write_json(&path, &data)
}

/// 變更某筆 item 的 sync_scope（Phase 3b-2 升級用：Project → Shared）。
pub fn set_scope(data_dir: &Path, id: &str, scope: SyncScope) -> Result<(), AppError> {
    let path = queue_path(data_dir);
    let mut data: ReviewQueueData = json_store::read_json_or_default(&path);
    for item in &mut data.items {
        if item.id == id {
            item.sync_scope = scope.clone();
        }
    }
    json_store::write_json(&path, &data)
}

pub fn mark_synced(data_dir: &Path, ids: &[String]) -> Result<(), AppError> {
    let path = queue_path(data_dir);
    let mut data: ReviewQueueData = json_store::read_json_or_default(&path);
    for item in &mut data.items {
        if ids.contains(&item.id) {
            item.status = ReviewStatus::Synced;
        }
    }
    json_store::write_json(&path, &data)
}

/// 把指定項目退回 Pending（例如寫入時因目標已存在而略過，須留在待審核供老爺改標題重試）。
pub fn mark_pending(data_dir: &Path, ids: &[String]) -> Result<(), AppError> {
    let path = queue_path(data_dir);
    let mut data: ReviewQueueData = json_store::read_json_or_default(&path);
    for item in &mut data.items {
        if ids.contains(&item.id) {
            item.status = ReviewStatus::Pending;
            item.reviewed_at = None;
        }
    }
    json_store::write_json(&path, &data)
}

pub fn update_item(data_dir: &Path, updated: ReviewItem) -> Result<ReviewItem, AppError> {
    let path = queue_path(data_dir);
    let mut data: ReviewQueueData = json_store::read_json_or_default(&path);
    let found = data.items.iter_mut().find(|i| i.id == updated.id);
    match found {
        Some(item) => {
            *item = updated.clone();
            json_store::write_json(&path, &data)?;
            Ok(updated)
        }
        None => Err(AppError::ProjectNotFound(updated.id)),
    }
}
