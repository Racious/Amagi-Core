use std::path::Path;
use chrono::Utc;
use crate::AppError;
use crate::models::review::{ReviewItem, ReviewItemType, ReviewQueueData, ReviewStatus};
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
        // Blocked 項不可接受：一旦 Accepted 會進「待同步」卻永不寫檔／不出列（殭屍中間態），
        // 且形同放行敏感內容。無論單項或「全部接受」批次，一律靜默跳過。
        if ids.contains(&item.id) && item.item_type != ReviewItemType::Blocked {
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
        // Blocked 狀態機嚴格化：只允許 Pending →（確認丟棄）出列，不得轉 Ignored 灰名單長留；
        // 丟棄請走 discard_blocked_items。
        if ids.contains(&item.id) && item.item_type != ReviewItemType::Blocked {
            item.status = ReviewStatus::Ignored;
            item.reviewed_at = Some(Utc::now());
        }
    }
    json_store::write_json(&path, &data)
}

/// vault-first（[[adr-005-vault-first-sync]]）：項目成功寫入 vault 且衍生物刷新後，
/// 從佇列「出列」（實體移除），使 vault 成為唯一權威、佇列不再保留 `Synced` 帳本。
/// Phase 3 起 memory / skill / wiki 三型一致走此路徑，`mark_synced` 退役。
/// **型別防護（Codex #3）**：只移除「型別 + id」皆命中者，避免同 id 的異型別項被誤刪。
/// 寫入失敗的項目由呼叫端保留在 `Accepted`（不出列、可重試），避免中間態遺失。
pub fn remove_items_of_type(data_dir: &Path, ids: &[String], item_type: ReviewItemType) -> Result<(), AppError> {
    let path = queue_path(data_dir);
    let mut data: ReviewQueueData = json_store::read_json_or_default(&path);
    data.items.retain(|i| !(i.item_type == item_type && ids.contains(&i.id)));
    json_store::write_json(&path, &data)
}

/// vault-first 一次性遷移（Phase 3，[[adr-005-vault-first-sync]]）：清除佇列中**所有型別**殘留的
/// `Synced` 項。Phase 3 起 skill/wiki 亦入庫即出列、`Synced` 全面退役；殘留 Synced 項的內容皆已
/// 在 vault（skill→`_skills/`、wiki→知識頁、memory→`agent/memory/`），佇列帳本為冗餘。
/// 備份策略（設計審 R5）：`queue.premigration-p3.bak` 為**一次性快照**（create_new）——
/// AlreadyExists ＝ 沿用既有回滾點續行；Phase 3 後常態不再產生 Synced，「備份非本次清除前快照」
/// 僅限舊版/測試再寫入 Synced 的邊界情況，可接受。讀/寫備份失敗 → Err、**不清 queue**
/// （保留回滾證據）；**絕不刪任何 vault 檔**。回傳清除筆數。
pub fn migrate_drop_synced_items(data_dir: &Path) -> Result<usize, AppError> {
    let path = queue_path(data_dir);
    let mut data: ReviewQueueData = json_store::read_json_or_default(&path);
    let is_target = |i: &ReviewItem| i.status == ReviewStatus::Synced;
    if !data.items.iter().any(is_target) {
        return Ok(0);
    }
    let backup = path.with_extension("premigration-p3.bak");
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

// 註：`mark_synced` 與 `promote_scope_and_mark_synced` 已於 Phase 3 退役——
// 三型（memory/skill/wiki）入庫即出列、promote 改純 vault 檔案操作，佇列不再有 Synced 寫入路徑。
// `ReviewStatus::Synced` enum variant 保留供舊 queue.json 反序列化相容；實例由 migration 清除。

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
            // Blocked 項唯讀：不得改寫內容或型別（防止遮罩片段被編輯「洗白」後接受同步）。
            // UI 已隱藏編輯入口，此為後端最後防線。
            if item.item_type == ReviewItemType::Blocked {
                return Err(AppError::SafetyBlocked(
                    "封鎖項為唯讀，僅能「確認丟棄」；若為誤判請至原始檔處理後重新學習".into()));
            }
            *item = updated.clone();
            json_store::write_json(&path, &data)?;
            Ok(updated)
        }
        None => Err(AppError::ProjectNotFound(updated.id)),
    }
}

/// 「確認丟棄」封鎖項（實體出列）：僅移除 `Blocked` 型別且 id 命中者。
/// Blocked 為純通知卡（safety_filter 不擋其他候選），丟棄無資料損失；
/// 型別防護確保任意 id 也動不了 memory/skill/wiki。回傳實際移除筆數。
pub fn discard_blocked_items(data_dir: &Path, ids: &[String]) -> Result<usize, AppError> {
    let path = queue_path(data_dir);
    let mut data: ReviewQueueData = json_store::read_json_or_default(&path);
    let before = data.items.len();
    data.items.retain(|i| !(i.item_type == ReviewItemType::Blocked && ids.contains(&i.id)));
    let removed = before - data.items.len();
    if removed > 0 {
        json_store::write_json(&path, &data)?;
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::review::{RiskLevel, SyncScope};

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
    fn test_remove_items_of_type_guarded() {
        let dir = std::env::temp_dir().join(format!("amagi-rq-rm-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        // 同 id 的記憶與技能：出列只移除指定型別，異型別不受影響（型別防護，Codex #3）
        add_items(&dir, vec![
            item("dup", ReviewItemType::Memory, ReviewStatus::Accepted),
            item("dup", ReviewItemType::Skill, ReviewStatus::Accepted),
            item("m2", ReviewItemType::Memory, ReviewStatus::Accepted),
            item("w1", ReviewItemType::Wiki, ReviewStatus::Accepted),
        ]).unwrap();
        remove_items_of_type(&dir, &["dup".to_string()], ReviewItemType::Memory).unwrap();
        let items = list_items(&dir, None);
        assert!(!items.iter().any(|i| i.id == "dup" && i.item_type == ReviewItemType::Memory), "dup 記憶應出列");
        assert!(items.iter().any(|i| i.id == "dup" && i.item_type == ReviewItemType::Skill), "同 id 技能不得被誤刪");
        assert!(items.iter().any(|i| i.id == "m2"), "未命中項保留");
        // 三型一致：技能/wiki 亦可出列
        remove_items_of_type(&dir, &["dup".to_string()], ReviewItemType::Skill).unwrap();
        remove_items_of_type(&dir, &["w1".to_string()], ReviewItemType::Wiki).unwrap();
        let items = list_items(&dir, None);
        assert!(!items.iter().any(|i| i.id == "dup"), "技能亦應可出列");
        assert!(!items.iter().any(|i| i.id == "w1"), "wiki 亦應可出列");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_migrate_drops_all_synced_keeps_others_and_backs_up() {
        let dir = std::env::temp_dir().join(format!("amagi-rq-mig-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        add_items(&dir, vec![
            item("m-synced", ReviewItemType::Memory, ReviewStatus::Synced),
            item("m-accepted", ReviewItemType::Memory, ReviewStatus::Accepted),
            item("m-pending", ReviewItemType::Memory, ReviewStatus::Pending),
            item("m-ignored", ReviewItemType::Memory, ReviewStatus::Ignored),
            item("s-synced", ReviewItemType::Skill, ReviewStatus::Synced),
            item("w-synced", ReviewItemType::Wiki, ReviewStatus::Synced),
        ]).unwrap();
        let removed = migrate_drop_synced_items(&dir).unwrap();
        assert_eq!(removed, 3, "Phase 3：清全型別 Synced（memory/skill/wiki）");
        let ids: Vec<String> = list_items(&dir, None).into_iter().map(|i| i.id).collect();
        assert!(!ids.contains(&"m-synced".to_string()), "Synced 記憶應被清");
        assert!(!ids.contains(&"s-synced".to_string()), "Synced 技能應被清（Phase 3 暫存債清償）");
        assert!(!ids.contains(&"w-synced".to_string()), "Synced wiki 應被清");
        assert!(ids.contains(&"m-accepted".to_string()), "Accepted 保留（中間態、可重試）");
        assert!(ids.contains(&"m-pending".to_string()), "Pending 保留");
        assert!(ids.contains(&"m-ignored".to_string()), "Ignored 保留（spec §8 未定案 2，維持長留）");
        assert!(dir.join("review-queue").join("queue.premigration-p3.bak").exists(), "動作前應備份 p3 快照");
        // 冪等：再跑一次為 no-op
        assert_eq!(migrate_drop_synced_items(&dir).unwrap(), 0, "冪等：無殘留即 0");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_accept_skips_blocked_items() {
        let dir = std::env::temp_dir().join(format!("amagi-rq-acc-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        add_items(&dir, vec![
            item("b1", ReviewItemType::Blocked, ReviewStatus::Pending),
            item("m1", ReviewItemType::Memory, ReviewStatus::Pending),
        ]).unwrap();
        // 「全部接受」情境：ids 混入封鎖項 → 只有記憶被接受，封鎖項維持 Pending
        let accepted = accept_items(&dir, &["b1".to_string(), "m1".to_string()]).unwrap();
        assert_eq!(accepted.len(), 1, "封鎖項不得被接受");
        assert_eq!(accepted[0].id, "m1");
        let items = list_items(&dir, None);
        let b = items.iter().find(|i| i.id == "b1").unwrap();
        assert_eq!(b.status, ReviewStatus::Pending, "封鎖項應維持 Pending，不得成為待同步殭屍");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_ignore_skips_blocked_items() {
        let dir = std::env::temp_dir().join(format!("amagi-rq-ign-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        add_items(&dir, vec![
            item("b1", ReviewItemType::Blocked, ReviewStatus::Pending),
            item("m1", ReviewItemType::Memory, ReviewStatus::Pending),
        ]).unwrap();
        // Blocked 只能 Pending →（確認丟棄）出列：ignore 不得將其轉 Ignored（Codex #2）
        ignore_items(&dir, &["b1".to_string(), "m1".to_string()]).unwrap();
        let items = list_items(&dir, None);
        let b = items.iter().find(|i| i.id == "b1").unwrap();
        assert_eq!(b.status, ReviewStatus::Pending, "封鎖項不得被 ignore 轉狀態");
        let m = items.iter().find(|i| i.id == "m1").unwrap();
        assert_eq!(m.status, ReviewStatus::Ignored, "一般項照常忽略");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_update_rejects_blocked_items() {
        let dir = std::env::temp_dir().join(format!("amagi-rq-upd-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        add_items(&dir, vec![item("b1", ReviewItemType::Blocked, ReviewStatus::Pending)]).unwrap();
        // 試圖把封鎖項改型別＋改內容（洗白）→ 必須被拒，且佇列內容不變
        let mut laundered = item("b1", ReviewItemType::Memory, ReviewStatus::Pending);
        laundered.content = "看起來無害的內容".into();
        assert!(update_item(&dir, laundered).is_err(), "封鎖項唯讀，改寫必須被拒");
        let items = list_items(&dir, None);
        assert_eq!(items[0].item_type, ReviewItemType::Blocked, "型別不得被改");
        assert_eq!(items[0].content, "x", "內容不得被改");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_discard_blocked_only_removes_blocked_type() {
        let dir = std::env::temp_dir().join(format!("amagi-rq-dis-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        add_items(&dir, vec![
            item("b1", ReviewItemType::Blocked, ReviewStatus::Pending),
            item("m1", ReviewItemType::Memory, ReviewStatus::Pending),
        ]).unwrap();
        // 丟棄時混入非封鎖 id → 型別防護，只出列封鎖項
        let removed = discard_blocked_items(&dir, &["b1".to_string(), "m1".to_string()]).unwrap();
        assert_eq!(removed, 1, "僅封鎖項被出列");
        let items = list_items(&dir, None);
        assert!(!items.iter().any(|i| i.id == "b1"), "封鎖項應實體出列");
        assert!(items.iter().any(|i| i.id == "m1"), "非封鎖項不受影響");
        // 冪等：再丟一次為 0
        assert_eq!(discard_blocked_items(&dir, &["b1".to_string()]).unwrap(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_discard_blocked_removes_any_status() {
        let dir = std::env::temp_dir().join(format!("amagi-rq-dis2-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        // 舊版殘留（Codex R2）：G1 歷史殭屍（Accepted）與舊 UI 忽略（Ignored）的封鎖項，
        // 丟棄不限狀態，皆須可出列
        add_items(&dir, vec![
            item("b-acc", ReviewItemType::Blocked, ReviewStatus::Accepted),
            item("b-ign", ReviewItemType::Blocked, ReviewStatus::Ignored),
        ]).unwrap();
        let removed = discard_blocked_items(&dir, &["b-acc".to_string(), "b-ign".to_string()]).unwrap();
        assert_eq!(removed, 2, "Accepted/Ignored 封鎖殘留皆應可丟棄出列");
        assert!(list_items(&dir, None).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_migrate_backup_already_exists_still_clears() {
        // R5 邊界：p3 備份已存在（AlreadyExists）→ 沿用既有回滾點續行、仍完成清除，
        // 且不覆蓋既有備份內容（一次性快照語意）。
        let dir = std::env::temp_dir().join(format!("amagi-rq-mig2-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("review-queue")).unwrap();
        let backup = dir.join("review-queue").join("queue.premigration-p3.bak");
        std::fs::write(&backup, "既有快照").unwrap();
        add_items(&dir, vec![item("s-synced", ReviewItemType::Skill, ReviewStatus::Synced)]).unwrap();
        assert_eq!(migrate_drop_synced_items(&dir).unwrap(), 1, "備份已存在仍應完成清除");
        assert_eq!(std::fs::read_to_string(&backup).unwrap(), "既有快照", "既有備份不得被覆蓋");
        assert!(list_items(&dir, None).is_empty(), "Synced 項應被清");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
