use tauri::State;
use crate::{AppError, AppState};
use crate::models::sync::LearnResult;
use crate::core::{project_manager, git_scanner, learn_engine, pending_scanner, review_queue};

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
    let candidates = learn_engine::generate_candidates(
        &project_id,
        &scan.changed_files,
        &scan.diff_stat,
        &scan.diff_text,
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

    let pending_count = pending.len();
    candidates.extend(pending);

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
        candidate_ids: ids,
    })
}
