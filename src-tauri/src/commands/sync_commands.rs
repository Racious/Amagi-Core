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

    // 方案①跨機回填：sync 前把「vault 有、本機佇列無」的專案記憶補進佇列，
    // 使後續內聯與孤兒清理（皆以佇列為權威）涵蓋跨機 pull 來的記憶、不誤刪。
    if let Some(vroot) = vault_manager::get_vault_config(&data_dir).vault_path {
        let vf = project.vault_folder.clone()
            .unwrap_or_else(|| agent_exporter::project_vault_folder(&project.path));
        // 全佇列：id 碰撞守門需涵蓋跨專案/型別；去重 filter 內已限本專案，傳全佇列不影響去重語意。
        let existing = review_queue::list_items(&data_dir, None);
        let backfill = agent_exporter::reconcile_project_memory_from_vault(
            std::path::Path::new(&vroot), &vf, &project_id, &existing);
        if !backfill.is_empty() {
            review_queue::add_items(&data_dir, backfill)?;
        }
    }

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
    if vault_root.is_some() {
        if let Err(e) = vault_manager::refresh_global_anchor(&data_dir) {
            result.skipped_files.push(format!(
                "⚠ 全域錨點刷新失敗（{e}）：記憶已寫入 vault，但 ~/.claude/CLAUDE.md／~/.codex/AGENTS.md 未更新，新對話可能讀到舊索引；請到「設定」重設一次 vault 路徑。"
            ));
        }
    }

    // ── 同步完成後標記為 Synced ───────────────────────
    // 跨專案記憶已有 vault 落點 → 全部 accepted + 實際寫入的跨專案 Accepted 記憶皆標 Synced
    let mut synced_ids: Vec<String> = accepted.iter().map(|i| i.id.clone()).collect();
    if vault_root.is_some() {
        for g in &all_cross_memory {
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
    // 預覽也反映跨機回填（不寫佇列，僅併入計算，使預覽與實際 sync 一致）。
    let mut all_project_memory = all_project_memory;
    if let Some(vroot) = vault_root_path {
        let vf = project.vault_folder.clone()
            .unwrap_or_else(|| agent_exporter::project_vault_folder(&project.path));
        // 全佇列供 id 碰撞守門（跨專案）；去重 filter 內已限本專案。
        let queue_all = review_queue::list_items(&data_dir, None);
        all_project_memory.extend(
            agent_exporter::reconcile_project_memory_from_vault(vroot, &vf, &project_id, &queue_all));
    }
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

    // queue-first：先把登記簿定案（scope→Shared 且 Synced，原子單次寫入，無中間態）。
    // queue 是真相來源，sync 本就「讓 vault 對齊 queue」，故先定案、再讓 vault 對齊；
    // 即使後續 vault 對齊失敗，queue 已說共用 → 按「同步」或下次 sync 自癒、且不重複。
    review_queue::promote_scope_and_mark_synced(&data_dir, &item_id)?;

    // 讓 vault 對齊 queue（冪等：移舊專案檔[存在才刪] + 重建索引 + 寫 shared 全集）。
    // 失敗時 queue 已是共用 → 回明確訊息引導按「同步」校正（sync 會對齊、不重複）。
    if let Err(e) = agent_exporter::promote_memory_to_shared(
        std::path::Path::new(&vault_root),
        &vault_folder,
        &item,
        &remaining_project,
        &all_shared,
    ) {
        return Err(AppError::Io(format!(
            "已標記為共用，但檔案搬移未完成（{e}）。請按「同步」校正——系統會照登記簿把檔案補到位、不會重複。"
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
