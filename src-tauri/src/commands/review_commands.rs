use tauri::State;
use crate::{AppError, AppState};
use crate::models::review::{ReviewItem, ReviewApplyResult};
use crate::core::review_queue;

#[tauri::command]
pub async fn list_review_items(
    project_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<ReviewItem>, AppError> {
    let data_dir = state.data_dir.clone();
    Ok(review_queue::list_items(&data_dir, project_id.as_deref()))
}

#[tauri::command]
pub async fn accept_review_items(
    ids: Vec<String>,
    state: State<'_, AppState>,
) -> Result<ReviewApplyResult, AppError> {
    let data_dir = state.data_dir.clone();
    let accepted = review_queue::accept_items(&data_dir, &ids)?;
    Ok(ReviewApplyResult {
        accepted_ids: accepted.iter().map(|i| i.id.clone()).collect(),
        written_files: vec![],
    })
}

#[tauri::command]
pub async fn ignore_review_items(
    ids: Vec<String>,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    let data_dir = state.data_dir.clone();
    review_queue::ignore_items(&data_dir, &ids)
}

#[tauri::command]
pub async fn update_review_item(
    item: ReviewItem,
    state: State<'_, AppState>,
) -> Result<ReviewItem, AppError> {
    let data_dir = state.data_dir.clone();
    review_queue::update_item(&data_dir, item)
}
