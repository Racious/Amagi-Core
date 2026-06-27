use std::path::{Path, PathBuf};
use serde::Serialize;
use tauri::State;
use crate::{AppError, AppState};
use crate::core::{skill_library, vault_manager, project_manager};
use crate::utils::fs_utils;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibrarySkillDto {
    pub slug: String,
    pub name: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DistributeResultDto {
    pub skill_count: usize,
    pub repo_count: usize,
    pub written_count: usize,
}

#[tauri::command]
pub async fn list_library_skills(state: State<'_, AppState>) -> Result<Vec<LibrarySkillDto>, AppError> {
    let cfg = vault_manager::get_vault_config(&state.data_dir);
    let vault_root = match cfg.vault_path {
        Some(v) => v,
        None => return Ok(vec![]),
    };
    Ok(skill_library::list_library_skills(Path::new(&vault_root))
        .into_iter()
        .map(|s| LibrarySkillDto { slug: s.slug, name: s.name })
        .collect())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdoptableSkillDto {
    pub slug: String,
    pub name: String,
    pub source: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdoptResultDto {
    pub adopted: Vec<String>,
    pub skipped: Vec<String>,
    pub missing: Vec<String>,
}

/// 收編來源根：指定專案 → 該專案 `.codex/.claude/skills`；否則 → 全域 `~/.codex,.claude/skills`。
/// 來源由後端依白名單解析，前端只給 project_id，不傳任意路徑（防把任意目錄複製進 vault）。
fn adopt_source_roots(
    project_id: Option<&str>,
    state: &State<'_, AppState>,
) -> Result<Vec<PathBuf>, AppError> {
    match project_id {
        Some(id) => {
            let project = project_manager::get_project(id, &state.data_dir)
                .ok_or_else(|| AppError::ProjectNotFound(id.to_string()))?;
            let base = Path::new(&project.path);
            Ok(vec![
                base.join(".codex").join("skills"),
                base.join(".claude").join("skills"),
            ])
        }
        None => {
            let codex = fs_utils::global_codex_skills_dir()
                .ok_or_else(|| AppError::Io("無法取得 ~/.codex/skills 路徑".into()))?;
            let claude = fs_utils::global_claude_skills_dir()
                .ok_or_else(|| AppError::Io("無法取得 ~/.claude/skills 路徑".into()))?;
            Ok(vec![codex, claude])
        }
    }
}

/// 掃描可收編進 vault 的技能候選（vault `_skills/` 尚無者）。project_id 為 None → 掃全域。
#[tauri::command]
pub async fn scan_adoptable_skills(
    project_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<AdoptableSkillDto>, AppError> {
    let cfg = vault_manager::get_vault_config(&state.data_dir);
    let vault_root = cfg
        .vault_path
        .ok_or_else(|| AppError::InvalidPath("尚未設定 vault 路徑，請先到「設定」指定".into()))?;
    let roots = adopt_source_roots(project_id.as_deref(), &state)?;
    Ok(skill_library::scan_adoptable(&roots, Path::new(&vault_root))
        .into_iter()
        .map(|s| AdoptableSkillDto { slug: s.slug, name: s.name, source: s.source })
        .collect())
}

/// 收編指定 slug 的技能進 vault `_skills/`（adr-004 D6 單一來源）。
/// 來源目錄由後端從白名單根 + slug 解析，不信任前端路徑；非破壞（vault 已有則略過）。
#[tauri::command]
pub async fn adopt_skills(
    slugs: Vec<String>,
    project_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<AdoptResultDto, AppError> {
    let cfg = vault_manager::get_vault_config(&state.data_dir);
    let vault_root = cfg
        .vault_path
        .ok_or_else(|| AppError::InvalidPath("尚未設定 vault 路徑，請先到「設定」指定".into()))?;
    let roots = adopt_source_roots(project_id.as_deref(), &state)?;

    // 對每個 slug，在白名單根中找第一個含 SKILL.md 且安全（非 symlink、canonical 在根下）的來源；
    // 後端解析、杜絕任意路徑與 symlink 繞出。找不到者記入 missing 供批次判讀。
    let mut items: Vec<(String, PathBuf)> = Vec::new();
    let mut missing: Vec<String> = Vec::new();
    for slug in slugs {
        let found = roots.iter().find_map(|r| {
            let d = r.join(&slug);
            if d.join("SKILL.md").is_file() && skill_library::is_safe_source_dir(r, &d) {
                Some(d)
            } else {
                None
            }
        });
        match found {
            Some(d) => items.push((slug, d)),
            None => missing.push(slug),
        }
    }
    let res = skill_library::adopt_skills(Path::new(&vault_root), &items)?;
    missing.extend(res.missing);
    Ok(AdoptResultDto { adopted: res.adopted, skipped: res.skipped, missing })
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillSelectionDto {
    pub skill_slug: String,
    /// "global" 或某專案路徑
    pub target: String,
}

/// 選擇性分發：只把前端勾選的「技能 × 目標」配對寫出。取代粗暴的「全技能→全專案」。
#[tauri::command]
pub async fn distribute_skills_selective(
    selections: Vec<SkillSelectionDto>,
    state: State<'_, AppState>,
) -> Result<DistributeResultDto, AppError> {
    let data_dir = state.data_dir.clone();
    let cfg = vault_manager::get_vault_config(&data_dir);
    let vault_root = cfg
        .vault_path
        .ok_or_else(|| AppError::InvalidPath("尚未設定 vault 路徑，請先到「設定」指定".into()))?;
    let codex_dir = fs_utils::global_codex_skills_dir()
        .ok_or_else(|| AppError::Io("無法取得 ~/.codex/skills 路徑".into()))?;
    let claude_dir = fs_utils::global_claude_skills_dir()
        .ok_or_else(|| AppError::Io("無法取得 ~/.claude/skills 路徑".into()))?;

    // 白名單：只允許 "global" 與「已加入專案」的路徑，杜絕寫入未註冊目錄。
    let allowed: std::collections::HashSet<String> = project_manager::list_projects(&data_dir)
        .into_iter()
        .map(|p| p.path)
        .collect();
    let pairs: Vec<(String, String)> = selections
        .into_iter()
        .filter(|s| s.target == "global" || allowed.contains(&s.target))
        .map(|s| (s.skill_slug, s.target))
        .collect();
    let res = skill_library::distribute_selective(Path::new(&vault_root), &pairs, &codex_dir, &claude_dir)?;
    Ok(DistributeResultDto {
        skill_count: res.skill_count,
        repo_count: res.repo_count,
        written_count: res.written.len(),
    })
}
