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

    // vault-first（[[adr-005-vault-first-sync]]）：不再做「vault→佇列回填」。
    // 內聯/索引改由 agent_exporter 直接讀 vault 為權威（load_*_from_vault），
    // 且已移除「以佇列集合刪 vault 孤兒檔」的清理 → 無跨機誤刪風險，故回填 reconcile 退役。
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
    // Phase 3b：跨專案 scope 記憶全集（Global→general、Shared→shared）→ 各自 vault 桶；索引由全集重建
    let all_cross_memory: Vec<ReviewItem> = review_queue::list_items(&data_dir, None)
        .into_iter()
        .filter(|i| i.item_type == ReviewItemType::Memory
            && matches!(i.sync_scope, SyncScope::Global | SyncScope::Shared)
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
    // 跨專案記憶（Global/Shared）為全集 → gate 以全集判斷，與實際會寫入的範圍一致
    let has_cross = all_cross_memory.iter().any(|i| i.status == ReviewStatus::Accepted);
    if (!all_project_memory.is_empty() || has_skills || has_cross) && vault_root.is_none() {
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

    // Phase 3b：全域/共用記憶 → vault general/shared agent/memory（跨專案全集）
    if let Some(vroot) = vault_root.as_deref().map(std::path::Path::new) {
        result.written_files.extend(agent_exporter::sync_global_memory(vroot, &all_cross_memory)?);
        result.written_files.extend(agent_exporter::sync_shared_memory(vroot, &all_cross_memory)?);
    }

    // 內聯索引自動刷新：把更新後的 general/shared 記憶索引重寫進全域錨點，
    // 使新對話開場即讀到最新（不必手動重設 vault）。失敗不回滾已完成的同步，
    // 但須讓使用者「看得到」錨點未刷新（Codex 中 #2）——否則記憶標 Synced 卻讀不到。
    // 記錄全域錨點是否刷新成功（Codex #1）：跨層記憶（general/shared）的衍生物＝全域錨點，
    // 刷新失敗則其記憶不出列、留 Accepted 可重試，符合狀態機「寫 vault + 衍生物刷新成功後才出列」。
    let mut anchor_ok = true;
    if vault_root.is_some() {
        if let Err(e) = vault_manager::refresh_global_anchor(&data_dir) {
            anchor_ok = false;
            result.skipped_files.push(format!(
                "⚠ 全域錨點刷新失敗（{e}）：記憶已寫入 vault，但 ~/.claude/CLAUDE.md／~/.codex/AGENTS.md 未更新，新對話可能讀到舊索引；跨層記憶保留於佇列待重試，請到「設定」重設一次 vault 路徑或再同步一次。"
            ));
        }
    }

    // ── 同步完成後：記憶「出列」、技能維持 Synced ───────────────────────
    // vault-first（[[adr-005-vault-first-sync]]）：記憶成功寫入 vault 後從佇列**移除**（出列），
    // 不再標 Synced 長留——vault 為唯一權威，杜絕「vault 端刪除被佇列全集復活」的幽靈。
    // 專案記憶：其衍生物（專案 AGENTS/CLAUDE）已於 agent_exporter 寫入成功（否則 `?` 提早返回）→ 照常出列。
    // 跨層記憶（Global/Shared）：其衍生物＝全域錨點，僅在 anchor_ok 時出列，否則留 Accepted 可重試（Codex #1）。
    // 技能/wiki 本輪維持現狀（Phase 1 暫存債，spec §7）→ 仍標 Synced。
    let mut memory_done: Vec<String> = accepted.iter()
        .filter(|i| i.item_type == ReviewItemType::Memory && i.sync_scope == SyncScope::Project)
        .map(|i| i.id.clone())
        .collect();
    if vault_root.is_some() && anchor_ok {
        for g in &all_cross_memory {
            if g.status == ReviewStatus::Accepted && !memory_done.contains(&g.id) {
                memory_done.push(g.id.clone());
            }
        }
    }
    if !memory_done.is_empty() {
        review_queue::remove_memory_items(&data_dir, &memory_done)?;
    }
    let skill_done: Vec<String> = accepted.iter()
        .filter(|i| i.item_type != ReviewItemType::Memory)
        .map(|i| i.id.clone())
        .collect();
    if !skill_done.is_empty() {
        review_queue::mark_synced(&data_dir, &skill_done)?;
    }

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
    let all_cross_memory: Vec<ReviewItem> = review_queue::list_items(&data_dir, None)
        .into_iter()
        .filter(|i| i.item_type == ReviewItemType::Memory
            && matches!(i.sync_scope, SyncScope::Global | SyncScope::Shared)
            && matches!(i.status, ReviewStatus::Accepted | ReviewStatus::Synced))
        .collect();
    let vault_root = vault_manager::get_vault_config(&data_dir).vault_path;
    // 與 sync 一致的 hard gate：vault 未設 + 有專案記憶/技能/跨專案記憶 → preview 也報錯。
    let has_skills = accepted.iter().any(|i| i.item_type == ReviewItemType::Skill);
    let has_cross = all_cross_memory.iter().any(|i| i.status == ReviewStatus::Accepted);
    if (!all_project_memory.is_empty() || has_skills || has_cross) && vault_root.is_none() {
        return Err(AppError::InvalidPath(
            "尚未設定 vault 路徑：記憶/技能需寫入 vault，請先到「設定」指定 vault 資料夾".into()));
    }

    let vault_root_path = vault_root.as_deref().map(std::path::Path::new);
    // vault-first（[[adr-005-vault-first-sync]]）：preview 與 sync 同源，直接以 vault 現有檔為權威
    // （agent_exporter 內部 load_*_from_vault），不再回填佇列。
    let mut previews = agent_exporter::preview_sync_diff(
        &project.path,
        project.vault_folder.as_deref(),
        vault_root_path,
        &accepted,
        &all_project_memory,
    );
    // Phase 3b：附上全域/共用記憶（general/shared agent/memory）的 preview
    if let Some(vroot) = vault_root_path {
        previews.extend(agent_exporter::preview_global_memory(vroot, &all_cross_memory));
        previews.extend(agent_exporter::preview_shared_memory(vroot, &all_cross_memory));
    }
    Ok(previews)
}

/// Phase 3b-2 升級：把一筆「已同步的專案層記憶」提升為跨專案共用（scope→Shared、移到 shared/agent/memory）。
#[tauri::command]
pub async fn promote_memory(item_id: String, state: State<'_, AppState>) -> Result<(), AppError> {
    let data_dir = state.data_dir.clone();
    let item = review_queue::list_items(&data_dir, None)
        .into_iter()
        .find(|i| i.id == item_id)
        .ok_or_else(|| AppError::InvalidPath(format!("找不到記憶項：{item_id}")))?;
    if item.item_type != ReviewItemType::Memory
        || item.sync_scope != SyncScope::Project
        || !matches!(item.status, ReviewStatus::Accepted | ReviewStatus::Synced)
    {
        return Err(AppError::InvalidPath("只能升級「已同步的專案層記憶」到共用".into()));
    }
    let project = project_manager::get_project(&item.project_id, &data_dir)
        .ok_or_else(|| AppError::ProjectNotFound(item.project_id.clone()))?;
    let vault_root = vault_manager::get_vault_config(&data_dir).vault_path
        .ok_or_else(|| AppError::InvalidPath("尚未設定 vault 路徑，無法升級記憶".into()))?;
    let vault_folder = project.vault_folder.clone()
        .unwrap_or_else(|| agent_exporter::project_vault_folder(&project.path));

    // 先在記憶體算「升級後」預期集合，不先動 queue——避免 I/O 失敗留半升級狀態（Codex 3b-2 #2）
    let all = review_queue::list_items(&data_dir, None);
    // 本專案剩餘 Project 記憶（排除這筆）
    let remaining_project: Vec<ReviewItem> = all.iter()
        .filter(|i| i.project_id == item.project_id
            && i.id != item_id
            && i.item_type == ReviewItemType::Memory
            && i.sync_scope == SyncScope::Project
            && matches!(i.status, ReviewStatus::Accepted | ReviewStatus::Synced))
        .cloned()
        .collect();
    // 既有 Shared 記憶 + 這筆（預設為 Shared）
    let mut all_shared: Vec<ReviewItem> = all.iter()
        .filter(|i| i.item_type == ReviewItemType::Memory
            && i.sync_scope == SyncScope::Shared
            && matches!(i.status, ReviewStatus::Accepted | ReviewStatus::Synced))
        .cloned()
        .collect();
    let mut promoted_shared = item.clone();
    promoted_shared.sync_scope = SyncScope::Shared;
    all_shared.push(promoted_shared);

    // ⚠️ Phase 3 legacy（queue-first）：promote 尚未納入 vault-first 反轉（spec §9 待改），
    // 仍走「先定案 queue（scope→Shared 且 Synced，原子單次寫入）、再讓 vault 對齊」。
    // 此為 promote 專屬 legacy 路徑，**不代表一般 sync 的權威**（一般 sync 已 vault-first）。
    review_queue::promote_scope_and_mark_synced(&data_dir, &item_id)?;

    // 讓 vault 對齊此 promote 定案（冪等：移舊專案檔[存在才刪] + 重建索引 + 寫 shared 全集）。
    // 失敗時回明確訊息引導重試 promote（此 legacy 路徑冪等、不重複）。
    if let Err(e) = agent_exporter::promote_memory_to_shared(
        std::path::Path::new(&vault_root),
        &vault_folder,
        &item,
        &remaining_project,
        &all_shared,
    ) {
        return Err(AppError::Io(format!(
            "已標記為共用，但檔案搬移未完成（{e}）。請再次執行「升級為共用」重試——此步驟冪等、不會重複（promote 為 Phase 3 legacy 流程）。"
        )));
    }

    // ── 次要反映（best-effort，失敗不回滾已完成的升級）──
    // 重寫來源專案 AGENTS/CLAUDE 的內聯索引：升級後 vault 已少這一筆，必讀檔須同步，
    // 否則殘留舊內聯條目誤導 AI（Codex 中 #3）。以「剩餘專案記憶」重建（空→「（尚無）」）。
    let remaining_refs: Vec<&ReviewItem> = remaining_project.iter().collect();
    let entries = agent_exporter::memory_index_entries(&remaining_refs);
    let bullets = crate::utils::markdown::memory_bullets(&entries);
    let agents_path = std::path::Path::new(&project.path).join("AGENTS.md");
    if agents_path.exists() {
        let _ = crate::utils::markdown::write_with_backup(
            &agents_path,
            &crate::utils::markdown::build_agents_md(&vault_folder, &bullets),
        );
    }
    let claude_path = std::path::Path::new(&project.path).join("CLAUDE.md");
    if claude_path.exists() {
        let _ = crate::utils::markdown::write_with_backup(
            &claude_path,
            &crate::utils::markdown::build_claude_md(Some(&vault_folder), &bullets),
        );
    }
    // shared 已變動 → 刷新全域錨點內聯索引（best-effort）。
    let _ = vault_manager::refresh_global_anchor(&data_dir);
    Ok(())
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
