use std::path::Path;
use chrono::Utc;
use crate::AppError;
use crate::models::review::{ReviewItem, ReviewItemType, ReviewQueueData, ReviewStatus, SyncScope};
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

/// vault-first（[[adr-005-vault-first-sync]]）：記憶成功寫入 vault 且衍生物刷新後，
/// 從佇列「出列」（實體移除），使 vault 成為唯一權威、佇列不再保留 `Synced` 帳本。
/// 取代 `mark_synced` 長留——杜絕「vault 端刪除被佇列全集復活」的幽靈。
/// **型別防護（Codex #3）**：只移除 `item_type == Memory` 且 id 命中者，避免同 id 的技能/wiki 被誤刪。
/// 寫入失敗的項目由呼叫端保留在 `Accepted`（不出列、可重試），避免中間態遺失。
pub fn remove_memory_items(data_dir: &Path, ids: &[String]) -> Result<(), AppError> {
    let path = queue_path(data_dir);
    let mut data: ReviewQueueData = json_store::read_json_or_default(&path);
    data.items.retain(|i| !(i.item_type == ReviewItemType::Memory && ids.contains(&i.id)));
    json_store::write_json(&path, &data)
}

/// vault-first 一次性遷移（[[adr-005-vault-first-sync]]）：清除佇列中殘留的 `Synced` **記憶**項。
/// vault-first 後 Synced 退役、記憶成功入庫即出列；殘留的 Synced 記憶會被舊語意於下次同步復活
/// （老爺踩到的幽靈種子即此）。保守、可回滾：僅在確有目標時才動作，動作前備份 `queue.json`
/// 為 `queue.premigration.bak`（首次不覆蓋既有備份）。保留 Pending/Accepted/Ignored 與技能項
/// （技能維持現狀，Phase 1 暫存債）；**絕不刪任何 vault 檔**。回傳清除筆數。
pub fn migrate_drop_synced_memory(data_dir: &Path) -> Result<usize, AppError> {
    let path = queue_path(data_dir);
    let mut data: ReviewQueueData = json_store::read_json_or_default(&path);
    let is_target = |i: &ReviewItem|
        i.item_type == ReviewItemType::Memory && i.status == ReviewStatus::Synced;
    if !data.items.iter().any(is_target) {
        return Ok(0);
    }
    // 備份為「可回滾」的硬前提（Codex #2 / R2）：首次遷移須先成功備份 queue.json，
    // 讀或寫備份失敗 → 直接回 Err、**不清 queue**（保留回滾證據）。
    // 原子建立（create_new）避免 exists()→write() 競態覆蓋既有備份：
    // AlreadyExists → 既有備份存在，不覆蓋、續行；其他錯誤 → Err、不清 queue。
    let backup = path.with_extension("premigration.bak");
    let raw = std::fs::read_to_string(&path).map_err(|e| AppError::Io(e.to_string()))?;
    match std::fs::OpenOptions::new().write(true).create_new(true).open(&backup) {
        Ok(mut f) => {
            use std::io::Write;
            f.write_all(raw.as_bytes()).map_err(|e| AppError::Io(e.to_string()))?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => { /* 既有備份，不覆蓋、續行 */ }
        Err(e) => return Err(AppError::Io(e.to_string())),
    }
    let before = data.items.len();
    data.items.retain(|i| !is_target(i));
    let removed = before - data.items.len();
    json_store::write_json(&path, &data)?;
    Ok(removed)
}

/// 原子升級：單次讀寫，同時把指定項 scope→Shared 且 status→Synced。
/// 避免「set_scope 成功、mark_synced 失敗」的 Shared+Accepted 中間態（Codex r3 低）。
pub fn promote_scope_and_mark_synced(data_dir: &Path, id: &str) -> Result<(), AppError> {
    let path = queue_path(data_dir);
    let mut data: ReviewQueueData = json_store::read_json_or_default(&path);
    for item in &mut data.items {
        if item.id == id {
            item.sync_scope = SyncScope::Shared;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::review::RiskLevel;

    fn item(id: &str, t: ReviewItemType, s: ReviewStatus) -> ReviewItem {
        ReviewItem {
            id: id.into(), project_id: "p".into(), item_type: t,
            category: "feedback".into(), title: id.into(), content: "x".into(),
            risk: RiskLevel::Low, status: s, sync_targets: vec![],
            sync_scope: SyncScope::Project, source_pending_file: None,
            created_at: Utc::now(), reviewed_at: None,
        }
    }

    #[test]
    fn test_remove_memory_items_type_guarded() {
        let dir = std::env::temp_dir().join(format!("amagi-rq-rm-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        // 同 id 的記憶與技能：出列只移除記憶項，技能不受影響（型別防護，Codex #3）
        add_items(&dir, vec![
            item("dup", ReviewItemType::Memory, ReviewStatus::Accepted),
            item("dup", ReviewItemType::Skill, ReviewStatus::Synced),
            item("m2", ReviewItemType::Memory, ReviewStatus::Accepted),
        ]).unwrap();
        remove_memory_items(&dir, &["dup".to_string()]).unwrap();
        let items = list_items(&dir, None);
        assert!(!items.iter().any(|i| i.id == "dup" && i.item_type == ReviewItemType::Memory), "dup 記憶應出列");
        assert!(items.iter().any(|i| i.id == "dup" && i.item_type == ReviewItemType::Skill), "同 id 技能不得被誤刪");
        assert!(items.iter().any(|i| i.id == "m2"), "未命中項保留");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_migrate_drops_synced_memory_keeps_others_and_backs_up() {
        let dir = std::env::temp_dir().join(format!("amagi-rq-mig-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        add_items(&dir, vec![
            item("m-synced", ReviewItemType::Memory, ReviewStatus::Synced),
            item("m-accepted", ReviewItemType::Memory, ReviewStatus::Accepted),
            item("m-pending", ReviewItemType::Memory, ReviewStatus::Pending),
            item("s-synced", ReviewItemType::Skill, ReviewStatus::Synced),
        ]).unwrap();
        let removed = migrate_drop_synced_memory(&dir).unwrap();
        assert_eq!(removed, 1, "只清 Synced 記憶");
        let ids: Vec<String> = list_items(&dir, None).into_iter().map(|i| i.id).collect();
        assert!(!ids.contains(&"m-synced".to_string()), "Synced 記憶應被清");
        assert!(ids.contains(&"m-accepted".to_string()), "Accepted 記憶保留（中間態、可重試）");
        assert!(ids.contains(&"m-pending".to_string()), "Pending 記憶保留");
        assert!(ids.contains(&"s-synced".to_string()), "技能（Synced）維持現狀保留（Phase 1 暫存債）");
        assert!(dir.join("review-queue").join("queue.premigration.bak").exists(), "動作前應備份");
        // 冪等：再跑一次為 no-op
        assert_eq!(migrate_drop_synced_memory(&dir).unwrap(), 0, "冪等：無殘留即 0");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
