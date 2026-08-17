use tauri::State;
use crate::{AppError, AppState};
use crate::models::sync::LearnResult;
use crate::core::{agent_exporter, git_scanner, greylist, learn_engine, pending_scanner, project_manager, review_queue, vault_manager};

#[tauri::command]
pub async fn learn_from_project(
    project_id: String,
    state: State<'_, AppState>,
) -> Result<LearnResult, AppError> {
    let data_dir = state.data_dir.clone();
    let project = project_manager::get_project(&project_id, &data_dir)
        .ok_or_else(|| AppError::ProjectNotFound(project_id.clone()))?;

    // ── 1. 從 git diff 產生候選（規則式）────────────────
    let scan = git_scanner::scan(&project_id, &project.path)?;
    // 灰名單（adr-007 D4）：vault 未設定、路徑不安全或檔案讀取失敗 → 一律空集合
    //（寧吵不漏——學習不因灰名單異常而失敗，只是已靜音項會重新出卡）。
    let suppressed = vault_manager::get_vault_config(&data_dir)
        .vault_path
        .and_then(|root| {
            let folder = project
                .vault_folder
                .clone()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| agent_exporter::project_vault_folder(&project.path));
            greylist::resolve_greylist_path(&root, &folder).ok()
        })
        .map(|p| greylist::read_keys_lenient(&p))
        .unwrap_or_default();
    let candidates = learn_engine::generate_candidates(
        &project_id,
        &scan.changed_files,
        &scan.diff_stat,
        &scan.diff_text,
        &suppressed,
    );

    // 規則式候選對佇列既有 Pending/Accepted 去重：重按「學習變更」冪等，
    // 不再重複入列相同記憶/Blocked/技能候選。
    let existing = review_queue::list_items(&data_dir, Some(&project_id));
    let mut candidates = learn_engine::dedup_against_queue(candidates, &existing);

    // ── 2. 從 .amagi/pending/ 撈取 Agent 寫入的技能草稿 ──
    let existing_sources: Vec<String> = existing.iter()
        .filter_map(|i| i.source_pending_file.clone())
        .collect();

    let pending = pending_scanner::scan_pending_skills(
        &project.path,
        &project_id,
        &existing_sources,
    )?;
    // ── 2b. 從 .amagi/pending/ 撈取 Agent 寫入的記憶草稿（P1）──
    // 記憶的自然產生者是 AI（任務收尾才知道哪個坑值得記），原本此通道只收技能、
    // 記憶無入口 → vault 記憶區長期為空。兩通道共用同一解析骨架與安全守門。
    let pending_mem = pending_scanner::scan_pending_memories(
        &project.path,
        &project_id,
        &existing_sources,
    )?;

    let pending_count = pending.items.len();
    let pending_memory_count = pending_mem.items.len();
    // N3：被安全過濾擋下的投遞檔不入列，但必須讓老爺看得到（原僅印 stderr）。
    let mut pending_skipped = pending.skipped;
    pending_skipped.extend(pending_mem.skipped);
    candidates.extend(pending.items);
    candidates.extend(pending_mem.items);

    // ── 統計 ──────────────────────────────────────────
    let blocked = candidates.iter().filter(|c| {
        matches!(c.item_type, crate::models::review::ReviewItemType::Blocked)
    }).count();

    let ids: Vec<String> = candidates.iter().map(|c| c.id.clone()).collect();
    let generated = candidates.len();

    review_queue::add_items(&data_dir, candidates)?;

    Ok(LearnResult {
        project_id,
        candidates_generated: generated,
        blocked_count: blocked,
        pending_skill_count: pending_count,
        pending_memory_count,
        pending_skipped,
        candidate_ids: ids,
    })
}
