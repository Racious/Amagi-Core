use std::path::PathBuf;

use tauri::State;
use crate::{AppError, AppState};
use crate::models::review::{ReviewItem, ReviewApplyResult, ReviewItemType};
use crate::core::{agent_exporter, greylist, learn_engine, project_manager, review_queue, vault_manager};

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

/// 「確認丟棄」封鎖項：實體出列（僅 Blocked 型別，型別防護在 core 層）。
#[tauri::command]
pub async fn discard_blocked_items(
    ids: Vec<String>,
    state: State<'_, AppState>,
) -> Result<usize, AppError> {
    let data_dir = state.data_dir.clone();
    review_queue::discard_blocked_items(&data_dir, &ids)
}

#[tauri::command]
pub async fn update_review_item(
    item: ReviewItem,
    state: State<'_, AppState>,
) -> Result<ReviewItem, AppError> {
    let data_dir = state.data_dir.clone();
    review_queue::update_item(&data_dir, item)
}

// ── 封鎖項丟棄灰名單（adr-007）─────────────────────────────

/// 灰名單身分鍵 DTO：`(file_path, rule_label, value_digest)`，與 BlockedHit 對位。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GreylistKeyDto {
    pub file_path: Option<String>,
    pub rule_label: String,
    pub value_digest: String,
}

impl GreylistKeyDto {
    fn key(&self) -> greylist::GreylistKey {
        (self.file_path.clone(), self.rule_label.clone(), self.value_digest.clone())
    }
}

/// 解析本專案灰名單檔落點（adr-007 D3）：vault 未設定 → 明確 Err（寫入端不得靜默）；
/// 專案資料夾以 `Project.vault_folder` 權威解析、缺值 fallback 路徑推導，含 containment 驗證。
fn greylist_path_for(project_id: &str, data_dir: &std::path::Path) -> Result<PathBuf, AppError> {
    let root = vault_manager::get_vault_config(data_dir)
        .vault_path
        .ok_or_else(|| AppError::InvalidPath("尚未設定 vault 路徑，灰名單無法使用；請先到「設定」指定".into()))?;
    let project = project_manager::get_project(project_id, data_dir)
        .ok_or_else(|| AppError::ProjectNotFound(project_id.to_string()))?;
    let folder = project
        .vault_folder
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| agent_exporter::project_vault_folder(&project.path));
    greylist::resolve_greylist_path(&root, &folder)
}

/// 同卡並發靜音的操作鎖（impl-review 發現 2）：兩個靜音請求並發時，後者可能以
/// 舊卡快照計算殘餘、把已靜音值留回卡面。以 process-wide Mutex 序列化整段
/// 「讀卡→寫灰名單→動卡」；鎖內無 await、持鎖時間為毫秒級檔案操作。
static MUTE_OP_LOCK: once_cell::sync::Lazy<std::sync::Mutex<()>> =
    once_cell::sync::Lazy::new(|| std::sync::Mutex::new(()));

/// 「誤判，不再提醒」（hit 級，adr-007 D1）：把勾選的命中寫入灰名單後，
/// 全選 → 整卡出列；部分 → 卡就地更新為殘餘命中。
/// 原子順序：**先寫灰名單成功、後動卡**——寫失敗回 Err、卡不動（fail-closed）。
/// 舊卡（無 blocked_hits）或勾選鍵不屬於該卡 → Err（不解析 content 文案、不越權）。
#[tauri::command]
pub async fn discard_blocked_as_false_positive(
    project_id: String,
    item_id: String,
    selected: Vec<GreylistKeyDto>,
    state: State<'_, AppState>,
) -> Result<usize, AppError> {
    let data_dir = state.data_dir.clone();
    if selected.is_empty() {
        return Err(AppError::InvalidPath("未勾選任何命中值".into()));
    }
    let _op = MUTE_OP_LOCK
        .lock()
        .map_err(|_| AppError::Io("靜音操作鎖失效".into()))?;
    let items = review_queue::list_items(&data_dir, Some(&project_id));
    let card = items
        .iter()
        .find(|i| i.item_type == ReviewItemType::Blocked && i.id == item_id)
        .ok_or_else(|| AppError::InvalidPath(format!("找不到封鎖卡：{item_id}")))?;
    if card.blocked_hits.is_empty() {
        return Err(AppError::InvalidPath(
            "此為舊版封鎖卡（無結構化命中），無法靜音；請改用「確認丟棄」，重新學習產生的新卡即可靜音".into(),
        ));
    }
    // 越權防護：勾選鍵必須全數屬於該卡
    let card_keys: greylist::GreylistKeySet = card.blocked_hits.iter().map(greylist::hit_key).collect();
    let selected_keys: Vec<greylist::GreylistKey> = selected.iter().map(|k| k.key()).collect();
    if let Some(bad) = selected_keys.iter().find(|k| !card_keys.contains(*k)) {
        return Err(AppError::InvalidPath(format!("勾選值不屬於此卡：{}", bad.2)));
    }
    let selected_set: greylist::GreylistKeySet = selected_keys.into_iter().collect();

    // 先寫灰名單（Mutex 序列化；失敗 → Err、卡不動）
    let path = greylist_path_for(&project_id, &data_dir)?;
    let now = chrono::Utc::now();
    let entries: Vec<greylist::GreylistEntry> = card
        .blocked_hits
        .iter()
        .filter(|h| selected_set.contains(&greylist::hit_key(h)))
        .map(|h| greylist::GreylistEntry {
            file_path: h.file_path.clone(),
            rule_label: h.rule_label.clone(),
            value_digest: h.value_digest.clone(),
            masked: h.masked.clone(),
            scope: greylist::GREYLIST_SCOPE_EXACT.to_string(),
            source_item_id: card.id.clone(),
            source_created_at: card.created_at,
            created_at: now,
        })
        .collect();
    let added = greylist::append_entries(&path, entries)?;

    // 後動卡：以**最新**卡狀態計算殘餘（灰名單已寫入，卡若已被他方出列則無事可做），
    // 全選出列、部分就地更新殘餘。
    let latest = review_queue::list_items(&data_dir, Some(&project_id))
        .into_iter()
        .find(|i| i.item_type == ReviewItemType::Blocked && i.id == item_id);
    if let Some(latest) = latest {
        let residual: Vec<_> = latest
            .blocked_hits
            .iter()
            .filter(|h| !selected_set.contains(&greylist::hit_key(h)))
            .cloned()
            .collect();
        if residual.is_empty() {
            review_queue::discard_blocked_items(&data_dir, &[item_id])?;
        } else {
            let content = learn_engine::render_blocked_content(&residual);
            review_queue::update_blocked_hits(&data_dir, &item_id, residual, content)?;
        }
    }
    Ok(added)
}

/// 灰名單檢視：直讀 vault JSON。檔不存在＝空清單；存在但損壞 → 明確 Err
///（UI 據此顯示「灰名單讀取失敗，靜音效果已暫停」，不得靜默回空）。
#[tauri::command]
pub async fn list_blocked_greylist(
    project_id: String,
    state: State<'_, AppState>,
) -> Result<greylist::GreylistData, AppError> {
    let data_dir = state.data_dir.clone();
    let path = greylist_path_for(&project_id, &data_dir)?;
    greylist::read_data(&path)
}

/// 解除靜音：按 key 移除條目；下次學習該值照常出卡。回傳實際移除筆數。
#[tauri::command]
pub async fn remove_greylist_entries(
    project_id: String,
    keys: Vec<GreylistKeyDto>,
    state: State<'_, AppState>,
) -> Result<usize, AppError> {
    let data_dir = state.data_dir.clone();
    let path = greylist_path_for(&project_id, &data_dir)?;
    let keys: Vec<greylist::GreylistKey> = keys.iter().map(|k| k.key()).collect();
    greylist::remove_entries(&path, &keys)
}
