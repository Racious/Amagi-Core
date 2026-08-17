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

/// 為歸檔目標產生不碰撞的檔名：`<name>.md` 未占用即直接用；已占用則附時間戳
/// `<stem>-<stamp>.md`，仍碰撞再加序號。迴圈式唯一化風格沿用
/// `agent_exporter::skill_dest_paths`；`stamp` 由呼叫方傳入以便測試不依賴時鐘。
pub(crate) fn unique_archive_dest(history_dir: &std::path::Path, fname: &str, stamp: &str) -> std::path::PathBuf {
    let first = history_dir.join(fname);
    if !first.exists() {
        return first;
    }
    let p = std::path::Path::new(fname);
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("pending");
    let ext = p.extension().and_then(|s| s.to_str()).unwrap_or("md");
    let mut n = 1;
    loop {
        n += 1;
        let cand = if n == 2 {
            history_dir.join(format!("{stem}-{stamp}.{ext}"))
        } else {
            history_dir.join(format!("{stem}-{stamp}-{n}.{ext}"))
        };
        if !cand.exists() {
            return cand;
        }
    }
}

/// 歸檔已同步的 pending 來源檔（技能／記憶共用），回傳需呈現給老爺的警告。
///
/// N1（2026-08-17，見 vault `2026-08-17-memory-ui-completion-review-r2.md`）：原實作為
/// `let _ = std::fs::rename(..)`——Windows 上目標已存在時 `fs::rename` **回 Err 而非覆蓋**，
/// 錯誤被吞掉後歸檔靜默失敗、pending 檔留在原地；該項同步後已從佇列出列
/// （`review_queue::remove_items_of_type` 不留 `Synced` 帳本），`learn_commands` 依
/// `source_pending_file` 的去重隨之失效 → **下輪學習重複入列同一筆**。
/// 故此處：先確保 history 目錄存在、撞名產唯一檔名、失敗一律回報（不再靜默）。
pub(crate) fn archive_pending_sources(
    history_dir: &std::path::Path,
    items: &[&ReviewItem],
    stamp: &str,
) -> Vec<String> {
    let mut warnings = Vec::new();
    // 僅處理來源檔仍在場者：已被手動移除＝無事可歸檔，非失敗。
    let srcs: Vec<&String> = items.iter()
        .filter_map(|i| i.source_pending_file.as_ref())
        .filter(|src| std::path::Path::new(src.as_str()).exists())
        .collect();
    if srcs.is_empty() {
        return warnings;
    }
    if let Err(e) = std::fs::create_dir_all(history_dir) {
        warnings.push(format!(
            "⚠ 無法建立歸檔目錄 .amagi/history（{e}）：{} 個 pending 來源檔未歸檔。內容已寫入 vault，但來源檔仍留在 .amagi/pending/，下次學習會重複列出同一批候選——請手動移走這些檔，或修正目錄權限後再同步一次。",
            srcs.len()));
        return warnings;
    }
    for src in srcs {
        let src_path = std::path::Path::new(src.as_str());
        let fname = match src_path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n,
            None => continue,
        };
        let dest = unique_archive_dest(history_dir, fname, stamp);
        if let Err(e) = std::fs::rename(src_path, &dest) {
            warnings.push(format!(
                "⚠ pending 來源檔歸檔失敗：{fname}（{e}）。內容已寫入 vault，但該檔仍留在 .amagi/pending/，下次學習會重複列出同一候選——請手動刪除或移到 .amagi/history/。"));
        }
    }
    warnings
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

    // ── 同步完成後：記憶與技能一律「出列」（Phase 3，[[adr-005-vault-first-sync]]）───────
    // vault-first：項目成功寫入 vault 後從佇列**移除**（出列），不再標 Synced 長留——
    // vault 為唯一權威，杜絕「vault 端刪除被佇列全集復活」的幽靈與佇列帳本膨脹。
    // 專案記憶：其衍生物（專案 AGENTS/CLAUDE）已於 agent_exporter 寫入成功（否則 `?` 提早返回）→ 照常出列。
    // 跨層記憶（Global/Shared）：其衍生物＝全域錨點，僅在 anchor_ok 時出列，否則留 Accepted 可重試（Codex #1）。
    // 技能：vault `_skills` 寫入成功（同上 `?` 保證）→ 出列；分發已由 Skills 頁直讀 vault，無衍生物待刷。
    // 明確只出列 Skill 型別：Blocked 項不寫檔，維持原狀留佇列（不再被舊語意誤標 Synced）。
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
        review_queue::remove_items_of_type(&data_dir, &memory_done, ReviewItemType::Memory)?;
    }
    let skill_done: Vec<String> = accepted.iter()
        .filter(|i| i.item_type == ReviewItemType::Skill)
        .map(|i| i.id.clone())
        .collect();
    if !skill_done.is_empty() {
        review_queue::remove_items_of_type(&data_dir, &skill_done, ReviewItemType::Skill)?;
    }

    // ── 歸檔已同步的 pending 來源檔（技能／記憶共用）──────────────────
    let history_dir = std::path::Path::new(&project.path).join(".amagi").join("history");
    let archive_targets: Vec<&ReviewItem> = accepted.iter()
        .filter(|i| i.source_pending_file.is_some())
        .collect();
    let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string();
    result.skipped_files.extend(
        archive_pending_sources(&history_dir, &archive_targets, &stamp));

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

/// promote 回傳：`moved`＝實際搬檔（false＝續跑收斂）；`warnings`＝best-effort 衍生物的失敗提示。
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PromoteResultDto {
    pub moved: bool,
    pub warnings: Vec<String>,
}

/// 升級（Phase 3 vault-first，[[adr-005-vault-first-sync]]）：把一筆專案層記憶提升為跨專案共用。
/// **純 vault 檔案操作、零 queue 參與**——由 `(project_id, memory_id)` 在 vault 專案層權威集定位，
/// 先寫 shared（讀回驗證）再刪專案檔，兩側索引由 vault 重建（agent_exporter 內）。
/// 衍生物語意（設計審 R3）：專案 AGENTS/CLAUDE 內聯重寫失敗 → Err 可重試（promote 可續跑收斂，
/// 重試不重複搬檔）；全域錨點刷新維持 best-effort，失敗以 warning 回報前端。
#[tauri::command]
pub async fn promote_memory(
    project_id: String,
    memory_id: String,
    state: State<'_, AppState>,
) -> Result<PromoteResultDto, AppError> {
    let data_dir = state.data_dir.clone();
    let project = project_manager::get_project(&project_id, &data_dir)
        .ok_or_else(|| AppError::ProjectNotFound(project_id.clone()))?;
    let vault_root = vault_manager::get_vault_config(&data_dir).vault_path
        .ok_or_else(|| AppError::InvalidPath("尚未設定 vault 路徑，無法升級記憶".into()))?;
    let vault_folder = project.vault_folder.clone()
        .unwrap_or_else(|| agent_exporter::project_vault_folder(&project.path));
    let vroot = std::path::Path::new(&vault_root);

    // 防守深度（2026-07-03 事故，Codex 高）：存量「project.path 落在 vault 內」的專案，
    // 下方會以 project.path 為根重寫 AGENTS/CLAUDE 內聯——必須在**搬檔前** fail-closed，
    // 否則記憶已升級、指針拒寫，留下半完成狀態。
    agent_exporter::ensure_project_outside_vault(vroot, &project.path)?;

    let outcome = agent_exporter::promote_memory_to_shared(vroot, &vault_folder, &memory_id)?;

    // 重寫來源專案 AGENTS/CLAUDE 內聯索引（以 vault 剩餘權威集重建；空→「（尚無）」）。
    // 失敗 → Err（非靜默）：升級已入 shared，殘留舊內聯會誤導 AI；重試 promote 走收斂路徑補刷。
    let remaining = agent_exporter::load_project_memory_from_vault(vroot, &vault_folder);
    let remaining_refs: Vec<&ReviewItem> = remaining.iter().collect();
    let entries = agent_exporter::memory_index_entries(&remaining_refs);
    let bullets = crate::utils::markdown::memory_bullets(&entries);
    let agents_path = std::path::Path::new(&project.path).join("AGENTS.md");
    if agents_path.exists() {
        crate::utils::markdown::write_with_backup(
            &agents_path,
            &crate::utils::markdown::build_agents_md(&vault_folder, &bullets),
        ).map_err(|e| AppError::Io(format!(
            "升級已完成，但專案 AGENTS.md 內聯重寫失敗（{e}）；請再次執行「升級為共用」重試（冪等、只補刷衍生物）")))?;
    }
    let claude_path = std::path::Path::new(&project.path).join("CLAUDE.md");
    if claude_path.exists() {
        crate::utils::markdown::write_with_backup(
            &claude_path,
            &crate::utils::markdown::build_claude_md(Some(&vault_folder), &bullets),
        ).map_err(|e| AppError::Io(format!(
            "升級已完成，但專案 CLAUDE.md 內聯重寫失敗（{e}）；請再次執行「升級為共用」重試（冪等、只補刷衍生物）")))?;
    }
    // shared 已變動 → 刷新全域錨點內聯索引（best-effort，失敗以 warning 回報，重試 promote 或重設 vault 可收斂）。
    let mut warnings = Vec::new();
    if let Err(e) = vault_manager::refresh_global_anchor(&data_dir) {
        warnings.push(format!(
            "全域錨點刷新失敗（{e}）：升級已完成，但 ~/.claude/CLAUDE.md／~/.codex/AGENTS.md 的記憶索引未更新；請再升級重試或到「設定」重設 vault 路徑。"));
    }
    Ok(PromoteResultDto { moved: outcome.moved, warnings })
}

/// 刪除記憶的預覽（P3 二段確認用）：待刪檔案身分 ＋ git 可復原性判斷。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryDeletionPreview {
    pub file_name: String,
    pub title: String,
    /// 該檔在 vault 內的相對路徑（給老爺確認落點）
    pub relative_path: String,
    /// 是否為全域層（general）——前端據此加強警示（Q3：blast radius 最大）
    pub is_global: bool,
    /// git 復原性人話說明（非 git repo／有未提交變更／已提交各不同）
    pub git_note: String,
    /// 保守判斷：僅在「vault 是 git repo 且該檔無未提交變更」時為 true
    pub recoverable_by_git: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteMemoryResultDto {
    pub deleted_file: String,
    /// 衍生物刷新的警告；非空代表 vault 已刪但索引/內聯未完全更新（UI 不可顯示為單純成功）
    pub warnings: Vec<String>,
}

/// scope 字串 → SyncScope。只接受三個合法值，不接受任意輸入。
fn parse_scope(scope: &str) -> Result<SyncScope, AppError> {
    match scope {
        "project" => Ok(SyncScope::Project),
        "shared" => Ok(SyncScope::Shared),
        "global" => Ok(SyncScope::Global),
        other => Err(AppError::InvalidPath(format!("未知的記憶範圍「{other}」，拒絕執行"))),
    }
}

/// 判斷待刪檔的 git 復原性。**保守**：任何無法確認的狀況都不宣稱可復原。
///
/// 以 `vault_git::file_commit_state` 直接對該路徑發問，**不解析 `git status` 字串**——
/// core.quotepath 會把中文檔名轉義成八進位，字串比對會永遠不命中而誤判為「已提交」
/// （2026-08-17 實機驗證抓到的 bug；記憶檔名幾乎必然含中文，屬常態命中）。
fn git_recovery_note(vault_root: &std::path::Path, rel_path: &str) -> (bool, String) {
    use crate::core::vault_git::{self, FileCommitState};
    if !vault_git::is_git_work_tree(vault_root) {
        return (false, "⚠ vault 不在 git 版控下：刪除後無法從 git 復原。".into());
    }
    match vault_git::file_commit_state(vault_root, rel_path) {
        Ok(FileCommitState::Untracked) => (false,
            "⚠ 此檔尚未提交進 git（未追蹤）：刪除後無法從 git 復原，且不會留下任何備份。".into()),
        Ok(FileCommitState::Modified) => (false,
            "⚠ 此檔有未提交的變更：git 最多只能復原到上一次提交的版本。".into()),
        Ok(FileCommitState::Committed) => (true,
            "此檔已提交進 git，必要時可從 git 歷史復原（但仍請確認後再刪）。".into()),
        Err(e) => (false, format!(
            "⚠ 無法確認 vault git 狀態（{e}）：無法判斷能否從 git 復原，請謹慎。")),
    }
}

/// 以 scope＋id 定位待刪記憶並回傳預覽（不刪任何檔）。
/// 走與 delete 完全相同的安全閘，故「同 id 多筆」等 fail-closed 情況在此就會報錯。
#[tauri::command]
pub async fn preview_memory_deletion(
    scope: String,
    project_id: Option<String>,
    memory_id: String,
    state: State<'_, AppState>,
) -> Result<MemoryDeletionPreview, AppError> {
    let data_dir = state.data_dir.clone();
    let sc = parse_scope(&scope)?;
    let vault_root = vault_manager::get_vault_config(&data_dir).vault_path
        .ok_or_else(|| AppError::InvalidPath("尚未設定 vault 路徑，無法刪除記憶".into()))?;
    let vroot = std::path::Path::new(&vault_root);
    let vault_folder = resolve_vault_folder(&sc, project_id.as_deref(), &data_dir)?;

    let (mem_dir, fname, item) =
        agent_exporter::locate_memory_for_delete(vroot, &sc, vault_folder.as_deref(), &memory_id)?;
    let rel = mem_dir.strip_prefix(vroot).unwrap_or(&mem_dir)
        .join(&fname).to_string_lossy().to_string();
    let (recoverable, note) = git_recovery_note(vroot, &rel);
    Ok(MemoryDeletionPreview {
        file_name: fname,
        title: item.title,
        relative_path: rel.replace('\\', "/"),
        is_global: matches!(sc, SyncScope::Global),
        git_note: note,
        recoverable_by_git: recoverable,
    })
}

/// Project scope 需由 project_id 推出 vault_folder；shared/global 不需要。
fn resolve_vault_folder(
    scope: &SyncScope,
    project_id: Option<&str>,
    data_dir: &std::path::Path,
) -> Result<Option<String>, AppError> {
    if !matches!(scope, SyncScope::Project) {
        return Ok(None);
    }
    let pid = project_id.ok_or_else(|| {
        AppError::InvalidPath("刪除專案層記憶需指定專案，拒絕執行".into())
    })?;
    let project = project_manager::get_project(pid, data_dir)
        .ok_or_else(|| AppError::ProjectNotFound(pid.to_string()))?;
    Ok(Some(project.vault_folder.clone()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| agent_exporter::project_vault_folder(&project.path))))
}

/// 刪除一筆 vault 記憶（P3）。**vault 檔即真相**：刪源檔＝該記憶消失，不做 `.trash/` 軟刪除
/// （會成為第二真相並重開「幽靈復活」風險）。前端須先呼叫 `preview_memory_deletion`
/// 呈現二段確認（含 git 復原性與全域影響）後才呼叫本 command。
///
/// 衍生物語意（沿用 promote 的分工）：
/// - vault 源檔＋該層 `MEMORY.md`：由 `agent_exporter::delete_memory_file` 處理，失敗即 Err。
/// - 專案 AGENTS/CLAUDE 內聯：刪除已完成後才重寫，失敗以 **warning** 回報（不 Err——
///   記憶已刪無法回滾，Err 會讓前端誤判「沒刪成功」而重試；warning 可引導重刷）。
/// - 全域錨點（shared/general）：同上 best-effort warning。
#[tauri::command]
pub async fn delete_memory(
    scope: String,
    project_id: Option<String>,
    memory_id: String,
    state: State<'_, AppState>,
) -> Result<DeleteMemoryResultDto, AppError> {
    let data_dir = state.data_dir.clone();
    let sc = parse_scope(&scope)?;
    let vault_root = vault_manager::get_vault_config(&data_dir).vault_path
        .ok_or_else(|| AppError::InvalidPath("尚未設定 vault 路徑，無法刪除記憶".into()))?;
    let vroot = std::path::Path::new(&vault_root);
    let vault_folder = resolve_vault_folder(&sc, project_id.as_deref(), &data_dir)?;

    // 專案層：內聯會以 project.path 為根重寫 → 沿用 promote 的前置 fail-closed，
    // 避免「記憶已刪、指針拒寫」的半完成狀態（2026-07-03 事故同型）。
    let project = match (&sc, project_id.as_deref()) {
        (SyncScope::Project, Some(pid)) => {
            let p = project_manager::get_project(pid, &data_dir)
                .ok_or_else(|| AppError::ProjectNotFound(pid.to_string()))?;
            agent_exporter::ensure_project_outside_vault(vroot, &p.path)?;
            Some(p)
        }
        _ => None,
    };

    let outcome = agent_exporter::delete_memory_file(
        vroot, &sc, vault_folder.as_deref(), &memory_id)?;

    // ── 衍生物刷新（刪除已完成，一律 best-effort + warning）──────────────
    let mut warnings: Vec<String> = Vec::new();
    if let (Some(p), Some(vf)) = (project.as_ref(), vault_folder.as_deref()) {
        let remaining = agent_exporter::load_project_memory_from_vault(vroot, vf);
        let refs: Vec<&ReviewItem> = remaining.iter().collect();
        let entries = agent_exporter::memory_index_entries(&refs);
        let bullets = crate::utils::markdown::memory_bullets(&entries);
        for (fname, body) in [
            ("AGENTS.md", crate::utils::markdown::build_agents_md(vf, &bullets)),
            ("CLAUDE.md", crate::utils::markdown::build_claude_md(Some(vf), &bullets)),
        ] {
            let path = std::path::Path::new(&p.path).join(fname);
            if path.exists() {
                if let Err(e) = crate::utils::markdown::write_with_backup(&path, &body) {
                    warnings.push(format!(
                        "記憶已刪除，但專案 {fname} 內聯重寫失敗（{e}）：該檔仍列著已刪的記憶，請再同步一次以補刷。"));
                }
            }
        }
    } else if let Err(e) = vault_manager::refresh_global_anchor(&data_dir) {
        // shared/general 的衍生物＝全域錨點
        warnings.push(format!(
            "記憶已刪除，但全域錨點刷新失敗（{e}）：~/.claude/CLAUDE.md／~/.codex/AGENTS.md 仍列著已刪的記憶；請到「設定」重設一次 vault 路徑或再同步一次。"));
    }
    // 跨機提醒（C2 共識：本輪不做全量 maintenance command，但須告知）
    if !matches!(sc, SyncScope::Project) {
        warnings.push(
            "提醒：其他機器需 git pull 後再執行一次「同步全域 doctrine」，其本機 AI 讀取索引才會更新。".into());
    }
    Ok(DeleteMemoryResultDto { deleted_file: outcome.deleted_file, warnings })
}

/// 記憶庫頁資料源（Phase 3 vault-first）：直接掃 vault 三層記憶（唯一權威），
/// 取代「佇列篩 Synced」（Phase 1 出列後佇列常態無 Synced，舊資料源恆空）。
/// vault 未設 → 空集合（與 list_library_skills 同慣例）。status 一律回 Synced（僅供前端顯示相容）。
#[tauri::command]
pub async fn list_vault_memories(state: State<'_, AppState>) -> Result<Vec<ReviewItem>, AppError> {
    let data_dir = state.data_dir.clone();
    let vault_root = match vault_manager::get_vault_config(&data_dir).vault_path {
        Some(v) => v,
        None => return Ok(Vec::new()),
    };
    let vroot = std::path::Path::new(&vault_root);
    let mut out: Vec<ReviewItem> = Vec::new();
    for p in project_manager::list_projects(&data_dir) {
        let vf = p.vault_folder.clone()
            .unwrap_or_else(|| agent_exporter::project_vault_folder(&p.path));
        let mut items = agent_exporter::load_project_memory_from_vault(vroot, &vf);
        for it in &mut items { it.project_id = p.id.clone(); }
        out.extend(items);
    }
    out.extend(agent_exporter::load_shared_memory_from_vault(vroot));
    out.extend(agent_exporter::load_global_memory_from_vault(vroot));
    Ok(out)
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
            blocked_hits: vec![],
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

    // ── N1 pending 歸檔（見 archive_pending_sources 註解）──────────────

    fn tmp_root(tag: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("amagi-{tag}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    /// 帶 pending 來源檔的項目：實際建檔，回傳 (item, 來源路徑)
    fn item_with_pending(pending_dir: &std::path::Path, fname: &str) -> ReviewItem {
        let src = pending_dir.join(fname);
        std::fs::write(&src, "內容").unwrap();
        let mut it = item(fname, "x");
        it.source_pending_file = Some(src.to_string_lossy().to_string());
        it
    }

    #[test]
    fn test_archive_creates_history_dir_and_moves_source() {
        let root = tmp_root("archive-basic");
        let pending = root.join(".amagi").join("pending");
        std::fs::create_dir_all(&pending).unwrap();
        // history 目錄刻意不預建 → 須由 archive 自行建立（原實作缺此步，rename 會失敗）
        let history = root.join(".amagi").join("history");
        let it = item_with_pending(&pending, "memory-a.md");

        let warns = archive_pending_sources(&history, &[&it], "20260817-101500");

        assert!(warns.is_empty(), "正常歸檔不應有警告，實得 {warns:?}");
        assert!(history.join("memory-a.md").is_file(), "來源檔應已移入 history");
        assert!(!pending.join("memory-a.md").exists(), "pending 不應殘留已歸檔來源檔");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_archive_same_name_keeps_both_copies() {
        // N1 核心迴歸：history 已有同名檔時，舊實作在 Windows 回 Err 並被吞掉
        // → pending 殘留 → 下輪重複入列。修正後須兩份都留、pending 清空。
        let root = tmp_root("archive-collide");
        let pending = root.join(".amagi").join("pending");
        let history = root.join(".amagi").join("history");
        std::fs::create_dir_all(&pending).unwrap();
        std::fs::create_dir_all(&history).unwrap();
        std::fs::write(history.join("memory-dup.md"), "先前歸檔的內容").unwrap();
        let it = item_with_pending(&pending, "memory-dup.md");

        let warns = archive_pending_sources(&history, &[&it], "20260817-101500");

        assert!(warns.is_empty(), "撞名應自動唯一化而非報錯，實得 {warns:?}");
        assert!(!pending.join("memory-dup.md").exists(), "pending 不應殘留（否則下輪重複入列）");
        assert_eq!(
            std::fs::read_to_string(history.join("memory-dup.md")).unwrap(),
            "先前歸檔的內容",
            "既有歷史檔不得被覆蓋");
        assert!(history.join("memory-dup-20260817-101500.md").is_file(),
            "新檔應以時間戳唯一化落檔");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_archive_missing_source_is_not_a_failure() {
        // 來源檔已被手動移除 → 無事可歸檔，不應產生警告
        let root = tmp_root("archive-missing");
        let history = root.join(".amagi").join("history");
        let mut it = item("memory-gone.md", "x");
        it.source_pending_file = Some(root.join("nope").join("memory-gone.md")
            .to_string_lossy().to_string());

        let warns = archive_pending_sources(&history, &[&it], "20260817-101500");

        assert!(warns.is_empty(), "來源不在場不算失敗，實得 {warns:?}");
        assert!(!history.exists(), "無事可做時不應建立 history 目錄");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_unique_archive_dest_second_collision_adds_index() {
        // 同秒內第三份同名檔：base → base-stamp → base-stamp-3
        let root = tmp_root("archive-thrice");
        std::fs::write(root.join("m.md"), "1").unwrap();
        std::fs::write(root.join("m-20260817-101500.md"), "2").unwrap();

        let dest = unique_archive_dest(&root, "m.md", "20260817-101500");

        assert!(dest.ends_with("m-20260817-101500-3.md"), "實得 {dest:?}");
        let _ = std::fs::remove_dir_all(&root);
    }
}
