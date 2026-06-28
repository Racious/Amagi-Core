use tauri::State;
use crate::{AppError, AppState};
use crate::models::sync::{SyncResult, FileDiffPreview, ItemConflict};
use crate::models::review::{ReviewStatus, ReviewItem, ReviewItemType, SyncScope};
use crate::core::{project_manager, review_queue, agent_exporter, conflict_filter, vault_manager};

/// 掃描待同步項目，回傳偵測到衝突的項目
fn scan_item_conflicts(items: &[ReviewItem]) -> Vec<ItemConflict> {
    let mut out = Vec::new();
    for item in items {
        let r = conflict_filter::check(&item.content);
        if r.has_conflict {
            out.push(ItemConflict {
                item_id: item.id.clone(),
                item_title: item.title.clone(),
                reasons: r.conflicts.iter()
                    .map(|c| format!("{}（命中：{}）", c.reason, c.matched))
                    .collect(),
            });
        }
    }
    out
}

#[tauri::command]
pub async fn sync_agent_files(
    project_id: String,
    force: bool,
    state: State<'_, AppState>,
) -> Result<SyncResult, AppError> {
    let data_dir = state.data_dir.clone();
    let project = project_manager::get_project(&project_id, &data_dir)
        .ok_or_else(|| AppError::ProjectNotFound(project_id.clone()))?;

    let all_items = review_queue::list_items(&data_dir, Some(&project_id));
    let accepted: Vec<ReviewItem> = all_items.iter()
        .filter(|i| i.status == ReviewStatus::Accepted)
        .cloned()
        .collect();
    // Phase 3a：專案層記憶以 Accepted+Synced 全集寫進 vault（含既有，非破壞）
    let all_project_memory: Vec<ReviewItem> = all_items.iter()
        .filter(|i| i.item_type == ReviewItemType::Memory
            && i.sync_scope == SyncScope::Project
            && matches!(i.status, ReviewStatus::Accepted | ReviewStatus::Synced))
        .cloned()
        .collect();
    // Phase 3b-1：全域 scope 記憶為「跨專案全集」→ vault general/agent/memory；索引由全集重建
    let all_global_memory: Vec<ReviewItem> = review_queue::list_items(&data_dir, None)
        .into_iter()
        .filter(|i| i.item_type == ReviewItemType::Memory
            && i.sync_scope == SyncScope::Global
            && matches!(i.status, ReviewStatus::Accepted | ReviewStatus::Synced))
        .collect();

    // ── 衝突卡控：除非 force 放行，否則偵測到衝突就擋下（不寫任何檔）──
    if !force {
        let conflicts = scan_item_conflicts(&accepted);
        if !conflicts.is_empty() {
            return Ok(SyncResult {
                project_id: project_id.clone(),
                written_files: Vec::new(),
                skipped_files: Vec::new(),
                blocked_conflicts: conflicts,
            });
        }
    }

    let vault_root = vault_manager::get_vault_config(&data_dir).vault_path;
    // hard gate（Phase 3a/3c）：有專案記憶或技能但 vault 未設 → 拒絕，
    // 避免記憶/技能無落點卻仍被標 Synced（資料遺失）。
    let has_skills = accepted.iter().any(|i| i.item_type == ReviewItemType::Skill);
    // 全域記憶為跨專案全集 → gate 以全集判斷（不只本專案），與實際會寫入的範圍一致（Codex 3b-1 追審）
    let has_global = all_global_memory.iter().any(|i| i.status == ReviewStatus::Accepted);
    if (!all_project_memory.is_empty() || has_skills || has_global) && vault_root.is_none() {
        return Err(AppError::InvalidPath(
            "尚未設定 vault 路徑：記憶/技能需寫入 vault，請先到「設定」指定 vault 資料夾".into()));
    }
    let mut result = agent_exporter::sync_agent_files(
        &project.path,
        project.vault_folder.as_deref(),
        vault_root.as_deref().map(std::path::Path::new),
        &accepted,
        &all_project_memory,
    )?;
    result.project_id = project_id.clone();

    // Phase 3b-1：全域記憶 → vault general/agent/memory（跨專案全集；補 3a 缺口）
    if let Some(vroot) = vault_root.as_deref().map(std::path::Path::new) {
        let global_written = agent_exporter::sync_global_memory(vroot, &all_global_memory)?;
        result.written_files.extend(global_written);
    }

    // ── 同步完成後標記為 Synced ───────────────────────
    // 3b-1 後全域記憶已有 vault 落點 → 全部 accepted（含本專案全域記憶）皆標 Synced
    let mut synced_ids: Vec<String> = accepted.iter().map(|i| i.id.clone()).collect();
    // 跨專案寫入 vault 的 Accepted 全域記憶也一併標 Synced，否則別專案的會卡 Accepted、每次 sync 重寫（Codex 3b-1 #1）
    if vault_root.is_some() {
        for g in &all_global_memory {
            if g.status == ReviewStatus::Accepted && !synced_ids.contains(&g.id) {
                synced_ids.push(g.id.clone());
            }
        }
    }
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
    let accepted: Vec<ReviewItem> = all_items.iter()
        .filter(|i| i.status == ReviewStatus::Accepted)
        .cloned()
        .collect();
    let all_project_memory: Vec<ReviewItem> = all_items.iter()
        .filter(|i| i.item_type == ReviewItemType::Memory
            && i.sync_scope == SyncScope::Project
            && matches!(i.status, ReviewStatus::Accepted | ReviewStatus::Synced))
        .cloned()
        .collect();
    let all_global_memory: Vec<ReviewItem> = review_queue::list_items(&data_dir, None)
        .into_iter()
        .filter(|i| i.item_type == ReviewItemType::Memory
            && i.sync_scope == SyncScope::Global
            && matches!(i.status, ReviewStatus::Accepted | ReviewStatus::Synced))
        .collect();
    let vault_root = vault_manager::get_vault_config(&data_dir).vault_path;
    // 與 sync 一致的 hard gate：vault 未設 + 有專案記憶/技能/全域記憶 → preview 也報錯。
    let has_skills = accepted.iter().any(|i| i.item_type == ReviewItemType::Skill);
    let has_global = all_global_memory.iter().any(|i| i.status == ReviewStatus::Accepted);
    if (!all_project_memory.is_empty() || has_skills || has_global) && vault_root.is_none() {
        return Err(AppError::InvalidPath(
            "尚未設定 vault 路徑：記憶/技能需寫入 vault，請先到「設定」指定 vault 資料夾".into()));
    }

    let vault_root_path = vault_root.as_deref().map(std::path::Path::new);
    let mut previews = agent_exporter::preview_sync_diff(
        &project.path,
        project.vault_folder.as_deref(),
        vault_root_path,
        &accepted,
        &all_project_memory,
    );
    // Phase 3b-1：附上全域記憶（general/agent/memory）的 preview
    if let Some(vroot) = vault_root_path {
        previews.extend(agent_exporter::preview_global_memory(vroot, &all_global_memory));
    }
    Ok(previews)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::review::*;
    use chrono::Utc;

    fn item(title: &str, content: &str) -> ReviewItem {
        ReviewItem {
            id: title.into(),
            project_id: "p".into(),
            item_type: ReviewItemType::Memory,
            category: "feedback".into(),
            title: title.into(),
            content: content.into(),
            risk: RiskLevel::Low,
            status: ReviewStatus::Accepted,
            sync_targets: vec![],
            sync_scope: SyncScope::Project,
            source_pending_file: None,
            created_at: Utc::now(),
            reviewed_at: None,
        }
    }

    #[test]
    fn test_gate_flags_conflicting_item() {
        let items = vec![
            item("乾淨記憶", "用 --author 指定作者，不動 config"),
            item("有問題記憶", "git config --local user.name \"あまぎ\""),
        ];
        let conflicts = scan_item_conflicts(&items);
        // 只有第二筆該被標
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].item_title, "有問題記憶");
        assert!(!conflicts[0].reasons.is_empty());
    }

    #[test]
    fn test_gate_passes_clean_items() {
        let items = vec![item("乾淨", "在 gameStore 新增 undo()，撤回上一步")];
        assert!(scan_item_conflicts(&items).is_empty());
    }
}
