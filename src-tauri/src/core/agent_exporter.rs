use std::path::{Path, PathBuf};
use crate::AppError;
use crate::models::review::{ReviewItem, ReviewItemType, SyncScope};
use crate::models::sync::{SyncResult, FileDiffPreview};
use crate::utils::{fs_utils, markdown};

/// 由專案路徑推導 vault 邏輯資料夾名（projects/<slug>），與 Project.vault_folder 預設一致。
pub fn project_vault_folder(project_path: &str) -> String {
    let name = Path::new(project_path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");
    format!("projects/{}", fs_utils::slugify(name))
}

/// 記憶檔名：slug(title) + 穩定 item id 片段（免佇列順序變動造成漂移/同名碰撞）。
fn memory_filename(item: &ReviewItem) -> String {
    let base = {
        let s = fs_utils::slugify(&item.title);
        if s.is_empty() { "memory".to_string() } else { s }
    };
    let short_id: String = item.id.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(8)
        .collect();
    let short_id = if short_id.is_empty() { "x".to_string() } else { short_id };
    format!("{}-{}.md", base, short_id)
}

/// 專案 vault_folder 是否安全（相對、各段皆 Normal、首段為 `projects`）。
/// 孤兒清理是刪檔表面，刪前以此擋 `..`/絕對路徑/非 projects 形式逃逸出 vault（Codex r3 高）。
fn is_safe_project_vault_folder(vf: &str) -> bool {
    let p = Path::new(vf);
    !vf.is_empty()
        && p.is_relative()
        && p.components().all(|c| matches!(c, std::path::Component::Normal(_)))
        && matches!(p.components().next(),
            Some(std::path::Component::Normal(s)) if s == std::ffi::OsStr::new("projects"))
}

/// 檔名是否像 Amagi 產生的記憶檔（`<slug>-<1..8 ascii 英數>.md`）。
/// 孤兒清理只刪此格式，避免誤刪同目錄中手放的 `.md`（Codex r3 高）。
fn looks_like_memory_file(name: &str) -> bool {
    match name.strip_suffix(".md").and_then(|stem| stem.rfind('-').map(|i| (stem, i))) {
        Some((stem, i)) if i > 0 => {
            let sfx = &stem[i + 1..];
            !sfx.is_empty() && sfx.len() <= 8 && sfx.chars().all(|c| c.is_ascii_alphanumeric())
        }
        _ => false,
    }
}

/// 安全解析專案記憶目錄 `<vault_root>/<vault_folder>/agent/memory`——作為「寫入＋刪除」的**唯一閘**
/// （Codex r4 高/中：sync 與 promote 兩條路徑都走此 helper，杜絕只封一支）。
/// 回傳 Some(dir) 僅當：① vault_folder 安全（相對/全 Normal/首段 projects）；
/// ② 該目錄「最深既存祖先」canonical 落在 canonical vault_root 之下（擋 symlink/junction 逃逸）。
/// 目錄可尚未存在（首次寫入）：對不存在路徑逐層上溯到既存祖先再驗。否則 None（呼叫端 skip/報錯）。
fn safe_project_memory_dir(vault_root: &Path, vault_folder: &str) -> Option<PathBuf> {
    if !is_safe_project_vault_folder(vault_folder) {
        return None;
    }
    let mem_dir = vault_root.join(vault_folder).join("agent").join("memory");
    // 從 mem_dir 上溯到「最深既存祖先」並 canonicalize（安全閘 fail-closed，Codex r5 中）：
    // 只把 NotFound 視為「尚不存在、繼續上溯」；權限/IO 等其他錯誤一律 None，不 fail-open。
    let mut anc: &Path = mem_dir.as_path();
    let canon_ancestor = loop {
        match anc.canonicalize() {
            Ok(c) => break c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => anc = anc.parent()?,
            Err(_) => return None,
        }
    };
    // vault_root 已存在 → 要求最深既存祖先落在 canonical vault_root 下（擋 vault 內既存 symlink/junction 逃逸）。
    // vault_root 為 NotFound（首次建立）→ 祖先已成功解析 + vault_folder 乾淨（無 ../絕對）→ 下方皆為待建乾淨段，安全。
    match vault_root.canonicalize() {
        Ok(croot) => if canon_ancestor.starts_with(&croot) { Some(mem_dir) } else { None },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Some(mem_dir),
        Err(_) => None,
    }
}

/// 由記憶內文取一句 hook（索引速查用）：優先 frontmatter 的 `description`，
/// 否則取 frontmatter 之後第一行非空、非標題的正文。跳過 YAML frontmatter，
/// 避免把 `---` 或 `name:` 當 hook（實機發現：原本抓「第一行非空」會誤抓 `---`）。截斷 40 字。
fn memory_hook(content: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;
    if lines.first().map(|l| l.trim()) == Some("---") {
        i = 1;
        while i < lines.len() {
            let t = lines[i].trim();
            if t == "---" { i += 1; break; }
            if let Some(rest) = t.strip_prefix("description:") {
                let d = rest.trim().trim_matches(['"', '\'']).trim();
                if !d.is_empty() { return truncate_hook(d); }
            }
            i += 1;
        }
    }
    while i < lines.len() {
        let t = lines[i].trim();
        if !t.is_empty() && !t.starts_with('#') && t != "---" {
            return truncate_hook(t.trim_start_matches(|c| c == '-' || c == '*' || c == ' '));
        }
        i += 1;
    }
    String::new()
}

fn truncate_hook(s: &str) -> String {
    if s.chars().count() > 40 {
        format!("{}…", s.chars().take(40).collect::<String>())
    } else {
        s.to_string()
    }
}

/// 由記憶項算出索引列：(檔名, 標題, 一句 hook)。sync / preview / promote 共用，避免漂移。
pub(crate) fn memory_index_entries(items: &[&ReviewItem]) -> Vec<(String, String, String)> {
    items.iter().map(|item| {
        (memory_filename(item), item.title.clone(), memory_hook(&item.content))
    }).collect()
}

/// 由技能清單算出各自 vault `_skills` 落點：slug 合法性守門（空/非法 → skill-<id>）、
/// 同批同名去重、相容舊扁平 `<slug>.md`。sync 與 preview 共用，確保落點一致。
fn skill_dest_paths(skills_root: &Path, skills: &[&ReviewItem]) -> Vec<PathBuf> {
    let mut seen = std::collections::HashSet::new();
    skills.iter().map(|skill| {
        let short_id: String = skill.id.chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .take(8).collect();
        let short_id = if short_id.is_empty() { "x".to_string() } else { short_id };
        let base = fs_utils::slugify(&skill.title);
        let base_slug = if crate::core::skill_library::is_valid_skill_slug(&base) {
            base
        } else {
            format!("skill-{}", short_id)
        };
        // 迴圈式唯一化：base → base-id → base-id-2 …，直到 seen 無此 slug，
        // 確保改名後仍唯一、不互相覆寫（Codex 3c 追審）。
        let mut slug = base_slug.clone();
        let mut n = 1;
        while seen.contains(&slug) {
            n += 1;
            slug = if n == 2 {
                format!("{}-{}", base_slug, short_id)
            } else {
                format!("{}-{}-{}", base_slug, short_id, n)
            };
        }
        seen.insert(slug.clone());
        let flat = skills_root.join(format!("{}.md", slug));
        if flat.is_file() {
            flat
        } else {
            skills_root.join(&slug).join("SKILL.md")
        }
    }).collect()
}

pub fn sync_agent_files(
    project_path: &str,
    vault_folder: Option<&str>,
    vault_root: Option<&Path>,
    accepted: &[ReviewItem],
    all_project_memory: &[ReviewItem],
) -> Result<SyncResult, AppError> {
    let mut written: Vec<String> = Vec::new();
    // 優先用顯式 Project.vault_folder（權威來源）；缺時才由路徑 basename 推導。
    let vault_folder = vault_folder
        .map(|s| s.to_string())
        .unwrap_or_else(|| project_vault_folder(project_path));

    // ── 專案層記憶 → vault `<vault_folder>/agent/memory/`；專案 AGENTS/CLAUDE 內聯索引 ──
    // 收 Accepted+Synced 全部專案記憶寫成「一事一檔」+ 重建 MEMORY.md 索引。
    // 專案 AGENTS/CLAUDE 內聯本專案記憶索引（非僅指標，實測薄指標不被跟讀）。
    let project_mem: Vec<&ReviewItem> = all_project_memory.iter()
        .filter(|i| i.item_type == ReviewItemType::Memory && i.sync_scope == SyncScope::Project)
        .collect();
    if let Some(vroot) = vault_root {
        let entries = memory_index_entries(&project_mem);

        // 安全閘：以共用 helper 解析記憶目錄（驗 vault_folder + canonical containment）。
        // 寫記憶檔/索引、孤兒清理、專案 AGENTS/CLAUDE 內聯，全部只在安全閘通過後才動（Codex r4/r7）。
        match safe_project_memory_dir(vroot, &vault_folder) {
            // hard gate（Codex r7 高）：有專案記憶要寫、但 vault_folder 不安全 → 直接 Err，
            // 阻止外層 mark_synced（不可「沒寫進 vault 卻標已同步」，違反 Phase 3a hard gate）。
            None if !project_mem.is_empty() => {
                return Err(AppError::InvalidPath(format!(
                    "不安全的 vault_folder「{vault_folder}」，拒絕寫入專案記憶（避免逃逸 vault）"
                )));
            }
            // 不安全且無專案記憶 → 無可寫/可清，整段跳過（不動 vault、不重寫專案 md）。
            None => {}
            Some(mem_dir) => {
                // 個別記憶檔：有記憶才寫
                if !entries.is_empty() {
                    std::fs::create_dir_all(&mem_dir).map_err(|e| AppError::Io(e.to_string()))?;
                    for (item, entry) in project_mem.iter().zip(&entries) {
                        let path = mem_dir.join(&entry.0);
                        // 衍生檔（可由 queue 重建）→ 無備份寫入，避免 agent/memory 累積 .bak 雜物。
                        std::fs::write(&path, markdown::build_memory_file(item)).map_err(|e| AppError::Io(e.to_string()))?;
                        written.push(path.to_string_lossy().to_string());
                    }
                }
                // MEMORY.md 索引 + 孤兒對帳清理：有記憶、或曾有現空（mem_dir 仍在 → 寫空索引反映「已無」）
                if !entries.is_empty() || mem_dir.exists() {
                    std::fs::create_dir_all(&mem_dir).map_err(|e| AppError::Io(e.to_string()))?;
                    let idx_path = mem_dir.join("MEMORY.md");
                    std::fs::write(&idx_path, markdown::build_memory_index(&entries))
                        .map_err(|e| AppError::Io(e.to_string()))?;
                    written.push(idx_path.to_string_lossy().to_string());

                    // 對帳清理：刪「不在權威集」的孤兒記憶檔，使「sync = vault 完全對齊 queue」、孤兒自癒。
                    // mem_dir 已確認安全；再以「命名格式 + 一般檔」雙保險（agent/memory 為受管目錄，
                    // 符合記憶命名格式之 .md 視為受管檔）；保留 MEMORY.md，不跟隨 symlink、不碰目錄。
                    let expected: std::collections::HashSet<&str> =
                        entries.iter().map(|e| e.0.as_str()).collect();
                    // TOCTOU 輕量強化（Codex 稽核中）：安全閘驗過 mem_dir 後到此刪除迴圈之間，
                    // 目錄仍可能被本機競態替換為 symlink/junction。刪前重新 canonicalize：
                    // ① mem_dir 本身仍須落在 vault 根下；② 每個待刪檔的 canonical 父目錄須等於
                    // canonical mem_dir。任一校驗失敗 → 回報 warning（非靜默略過），絕不在校驗外刪檔。
                    let canon_mem = std::fs::canonicalize(&mem_dir).ok();
                    let mem_within_vault = match (vroot.canonicalize().ok(), canon_mem.as_ref()) {
                        (Some(vr), Some(md)) => md.starts_with(&vr),
                        _ => false,
                    };
                    if !mem_within_vault {
                        written.push(format!(
                            "(略過孤兒清理：記憶目錄 canonical 校驗失敗，疑似 symlink/競態) {}",
                            mem_dir.to_string_lossy()
                        ));
                    } else if let Ok(rd) = std::fs::read_dir(&mem_dir) {
                        let canon_mem = canon_mem.unwrap(); // mem_within_vault 為真 → 必為 Some
                        for ent in rd.flatten() {
                            let fname = ent.file_name();
                            let fname = fname.to_string_lossy();
                            if fname == "MEMORY.md" || !looks_like_memory_file(&fname) {
                                continue;
                            }
                            if expected.contains(fname.as_ref()) {
                                continue;
                            }
                            let is_regular = std::fs::symlink_metadata(ent.path())
                                .map(|m| m.file_type().is_file())
                                .unwrap_or(false);
                            if !is_regular {
                                continue;
                            }
                            // 刪前重驗：該檔 canonical 父目錄須正好是 canonical mem_dir。
                            let parent_ok = std::fs::canonicalize(ent.path())
                                .ok()
                                .as_deref()
                                .and_then(Path::parent)
                                .map(|p| p == canon_mem.as_path())
                                .unwrap_or(false);
                            if !parent_ok {
                                written.push(format!(
                                    "(略過清理：路徑校驗失敗，疑似 symlink/競態) {}",
                                    ent.path().to_string_lossy()
                                ));
                                continue;
                            }
                            if std::fs::remove_file(ent.path()).is_ok() {
                                written.push(format!("(清理孤兒) {}", ent.path().to_string_lossy()));
                            }
                        }
                    }
                }

                // 專案 AGENTS/CLAUDE：與「有無記憶」解耦（空→「（尚無）」），只在安全閘通過後才寫，
                // 避免在 vault 記憶未實際寫入時還內聯指向不存在的記憶（Codex r7 高）。
                let agents_path = Path::new(project_path).join("AGENTS.md");
                let claude_path = Path::new(project_path).join("CLAUDE.md");
                if !entries.is_empty() || agents_path.exists() || claude_path.exists() {
                    let bullets = markdown::memory_bullets(&entries);
                    markdown::write_with_backup(&agents_path, &markdown::build_agents_md(&vault_folder, &bullets))?;
                    written.push(agents_path.to_string_lossy().to_string());
                    markdown::write_with_backup(&claude_path, &markdown::build_claude_md(Some(&vault_folder), &bullets))?;
                    written.push(claude_path.to_string_lossy().to_string());
                }
            }
        }
    }

    // ── 全域 scope 記憶：Phase 3a 暫不處理（Codex 高風險 #2）──
    // 舊行為以 build_*_claude_md 整檔覆寫含老爺人格與 AMAGI-VAULT 錨點的 ~/.claude/CLAUDE.md，
    // 風險過高，故 3a 停掉此路徑。延到 3b 改寫 vault general/agent/memory + 全域錨點指標。
    // command 層不會把全域記憶項標 Synced（留 Accepted 待 3b），故此處略過不寫、不遺失。

    // ── 技能 → vault `_skills/<slug>/SKILL.md`（單一來源；Phase 3c，老爺裁定 A：解耦）──
    // sync 只「進庫」，不再自動撒到 .amagi/.codex/.claude；分發改由 Skills 頁選擇性分發。
    let skills: Vec<&ReviewItem> = accepted.iter()
        .filter(|i| i.item_type == ReviewItemType::Skill)
        .collect();
    if let (Some(vroot), false) = (vault_root, skills.is_empty()) {
        let skills_root = vroot.join("_skills");
        let dests = skill_dest_paths(&skills_root, &skills);
        for (skill, dest) in skills.iter().zip(&dests) {
            markdown::write_with_backup(dest, &markdown::build_native_skill_md(skill))?;
            written.push(dest.to_string_lossy().to_string());
        }
    }

    Ok(SyncResult {
        project_id: String::new(),
        written_files: written,
        skipped_files: Vec::new(),
        blocked_conflicts: Vec::new(),
    })
}

pub fn preview_sync_diff(
    project_path: &str,
    vault_folder: Option<&str>,
    vault_root: Option<&Path>,
    accepted: &[ReviewItem],
    all_project_memory: &[ReviewItem],
) -> Vec<FileDiffPreview> {
    let mut previews = Vec::new();
    let vault_folder = vault_folder
        .map(|s| s.to_string())
        .unwrap_or_else(|| project_vault_folder(project_path));

    // 專案記憶 → vault 預覽。鏡像 sync 的條件（Codex r2 中 #2）：vault 已設即進入，
    // 即使 project_mem 空也預覽「空索引清理/指針重寫」，避免「預覽無變更、執行卻改檔」。
    let project_mem: Vec<&ReviewItem> = all_project_memory.iter()
        .filter(|i| i.item_type == ReviewItemType::Memory && i.sync_scope == SyncScope::Project)
        .collect();
    if let Some(vroot) = vault_root {
        let entries = memory_index_entries(&project_mem);
        // 與 sync 共用同一安全閘（Codex r7 中）：vault_folder/祖先不安全 → 不列 vault 記憶與專案 md diff，
        // 避免 preview 顯示一組 sync 實際不會寫（或會 Err）的路徑，維持 preview/sync 一致。
        if let Some(mem_dir) = safe_project_memory_dir(vroot, &vault_folder) {
            let bullets = markdown::memory_bullets(&entries);

            // AGENTS/CLAUDE：entries 非空、或檔已存在 → 會被重寫（空→「（尚無）」）
            let agents_path = Path::new(project_path).join("AGENTS.md");
            let claude_path = Path::new(project_path).join("CLAUDE.md");
            if !entries.is_empty() || agents_path.exists() || claude_path.exists() {
                previews.push(FileDiffPreview {
                    current_content: std::fs::read_to_string(&agents_path).ok(),
                    new_content: markdown::build_agents_md(&vault_folder, &bullets),
                    is_new_file: !agents_path.exists(),
                    file_path: agents_path.to_string_lossy().to_string(),
                });
                previews.push(FileDiffPreview {
                    current_content: std::fs::read_to_string(&claude_path).ok(),
                    new_content: markdown::build_claude_md(Some(&vault_folder), &bullets),
                    is_new_file: !claude_path.exists(),
                    file_path: claude_path.to_string_lossy().to_string(),
                });
            }
            // 個別記憶檔：僅非空才寫
            for (item, entry) in project_mem.iter().zip(&entries) {
                let path = mem_dir.join(&entry.0);
                previews.push(FileDiffPreview {
                    current_content: std::fs::read_to_string(&path).ok(),
                    new_content: markdown::build_memory_file(item),
                    is_new_file: !path.exists(),
                    file_path: path.to_string_lossy().to_string(),
                });
            }
            // MEMORY.md 索引：entries 非空、或 mem_dir 已存在（清空重寫）→ 預覽
            if !entries.is_empty() || mem_dir.exists() {
                let idx_path = mem_dir.join("MEMORY.md");
                previews.push(FileDiffPreview {
                    current_content: std::fs::read_to_string(&idx_path).ok(),
                    new_content: markdown::build_memory_index(&entries),
                    is_new_file: !idx_path.exists(),
                    file_path: idx_path.to_string_lossy().to_string(),
                });
            }
        }
    }

    // 技能 → vault `_skills`（Phase 3c·A）：preview 與 sync 共用 skill_dest_paths 算落點，確保一致。
    if let Some(vroot) = vault_root {
        let skills: Vec<&ReviewItem> = accepted.iter()
            .filter(|i| i.item_type == ReviewItemType::Skill)
            .collect();
        if !skills.is_empty() {
            let skills_root = vroot.join("_skills");
            let dests = skill_dest_paths(&skills_root, &skills);
            for (skill, dest) in skills.iter().zip(&dests) {
                previews.push(FileDiffPreview {
                    current_content: std::fs::read_to_string(dest).ok(),
                    new_content: markdown::build_native_skill_md(skill),
                    is_new_file: !dest.exists(),
                    file_path: dest.to_string_lossy().to_string(),
                });
            }
        }
    }

    previews
}

/// 全域 scope 記憶 → vault `general/agent/memory/`（Phase 3b-1，補 3a 缺口）。
/// 跨專案：`global_memory` 應為全專案的全集（Accepted+Synced 全域記憶），索引由此重建。
/// 與專案記憶共用 helper（memory_filename / build_memory_file / build_memory_index）。
/// 寫某個「跨專案 scope」記憶到 vault `<tier>/agent/memory/`（general=Global、shared=Shared）。
/// 跨專案全集 → 一事一檔 + 重建索引；複用專案記憶的 helper。
fn sync_tier_memory(vault_root: &Path, memory: &[ReviewItem], scope: SyncScope, tier: &str) -> Result<Vec<String>, AppError> {
    let mems: Vec<&ReviewItem> = memory.iter()
        .filter(|i| i.item_type == ReviewItemType::Memory && i.sync_scope == scope)
        .collect();
    if mems.is_empty() {
        return Ok(Vec::new());
    }
    let mem_dir = vault_root.join(tier).join("agent").join("memory");
    std::fs::create_dir_all(&mem_dir).map_err(|e| AppError::Io(e.to_string()))?;
    let mut written = Vec::new();
    let entries = memory_index_entries(&mems);
    for (item, entry) in mems.iter().zip(&entries) {
        let path = mem_dir.join(&entry.0);
        // 衍生檔（可由 queue 重建）→ 無備份寫入，避免 agent/memory 累積 .bak 雜物。
        std::fs::write(&path, markdown::build_memory_file(item)).map_err(|e| AppError::Io(e.to_string()))?;
        written.push(path.to_string_lossy().to_string());
    }
    let idx_path = mem_dir.join("MEMORY.md");
    std::fs::write(&idx_path, markdown::build_memory_index(&entries))
        .map_err(|e| AppError::Io(e.to_string()))?;
    written.push(idx_path.to_string_lossy().to_string());
    Ok(written)
}

fn preview_tier_memory(vault_root: &Path, memory: &[ReviewItem], scope: SyncScope, tier: &str) -> Vec<FileDiffPreview> {
    let mems: Vec<&ReviewItem> = memory.iter()
        .filter(|i| i.item_type == ReviewItemType::Memory && i.sync_scope == scope)
        .collect();
    if mems.is_empty() {
        return Vec::new();
    }
    let mem_dir = vault_root.join(tier).join("agent").join("memory");
    let entries = memory_index_entries(&mems);
    let mut previews = Vec::new();
    for (item, entry) in mems.iter().zip(&entries) {
        let path = mem_dir.join(&entry.0);
        previews.push(FileDiffPreview {
            current_content: std::fs::read_to_string(&path).ok(),
            new_content: markdown::build_memory_file(item),
            is_new_file: !path.exists(),
            file_path: path.to_string_lossy().to_string(),
        });
    }
    let idx_path = mem_dir.join("MEMORY.md");
    previews.push(FileDiffPreview {
        current_content: std::fs::read_to_string(&idx_path).ok(),
        new_content: markdown::build_memory_index(&entries),
        is_new_file: !idx_path.exists(),
        file_path: idx_path.to_string_lossy().to_string(),
    });
    previews
}

/// 全域記憶（Global scope）→ vault `general/agent/memory/`。
pub fn sync_global_memory(vault_root: &Path, memory: &[ReviewItem]) -> Result<Vec<String>, AppError> {
    sync_tier_memory(vault_root, memory, SyncScope::Global, "general")
}
/// 共用記憶（Shared scope，Phase 3b-2）→ vault `shared/agent/memory/`。
pub fn sync_shared_memory(vault_root: &Path, memory: &[ReviewItem]) -> Result<Vec<String>, AppError> {
    sync_tier_memory(vault_root, memory, SyncScope::Shared, "shared")
}
pub fn preview_global_memory(vault_root: &Path, memory: &[ReviewItem]) -> Vec<FileDiffPreview> {
    preview_tier_memory(vault_root, memory, SyncScope::Global, "general")
}
pub fn preview_shared_memory(vault_root: &Path, memory: &[ReviewItem]) -> Vec<FileDiffPreview> {
    preview_tier_memory(vault_root, memory, SyncScope::Shared, "shared")
}

/// 升級（Phase 3b-2）：把一筆專案記憶移到 shared/agent/memory。
/// 1) 刪舊 `projects/<vf>/agent/memory/<該筆>` + 以剩餘專案記憶重建該專案索引（清孤兒）；
/// 2) 把 Shared 全集寫進 `shared/agent/memory/`（呼叫端以該筆的 Shared 版本含於 all_shared；
///    queue 的 scope/status 由呼叫端在本函式 I/O 成功後才更新，失敗則維持原狀可重試）。
/// `promoted` 用於算舊檔名（檔名僅依 title+id，與 scope 無關）。回傳 shared 寫入清單。
pub fn promote_memory_to_shared(
    vault_root: &Path,
    vault_folder: &str,
    promoted: &ReviewItem,
    remaining_project_memory: &[ReviewItem],
    all_shared_memory: &[ReviewItem],
) -> Result<Vec<String>, AppError> {
    // 安全閘（Codex r4 高）：promote 的刪檔路徑與 sync 共用同一道防線，杜絕 vault_folder 污染逃逸。
    let proj_mem_dir = safe_project_memory_dir(vault_root, vault_folder)
        .ok_or_else(|| AppError::InvalidPath(format!("不安全的 vault_folder，拒絕升級刪檔：{vault_folder}")))?;
    // 1) 刪舊專案檔：限「檔名 == memory_filename(promoted) 的一般檔」（不跟隨 symlink、不誤刪）。
    let old_file = proj_mem_dir.join(memory_filename(promoted));
    let old_is_regular = std::fs::symlink_metadata(&old_file)
        .map(|m| m.file_type().is_file())
        .unwrap_or(false);
    if old_is_regular {
        std::fs::remove_file(&old_file).map_err(|e| AppError::Io(e.to_string()))?;
    }
    // 以剩餘專案記憶重建該專案索引（孤兒清除）
    let proj_mems: Vec<&ReviewItem> = remaining_project_memory.iter()
        .filter(|i| i.item_type == ReviewItemType::Memory && i.sync_scope == SyncScope::Project)
        .collect();
    let proj_idx = proj_mem_dir.join("MEMORY.md");
    if proj_mems.is_empty() {
        let _ = std::fs::remove_file(&proj_idx);
    } else {
        let entries = memory_index_entries(&proj_mems);
        std::fs::write(&proj_idx, markdown::build_memory_index(&entries))
            .map_err(|e| AppError::Io(e.to_string()))?;
    }
    // 2) 寫 Shared 全集 → shared/agent/memory
    sync_tier_memory(vault_root, all_shared_memory, SyncScope::Shared, "shared")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::review::{RiskLevel, ReviewStatus, SyncScope};
    use chrono::Utc;

    fn sk(id: &str, title: &str) -> ReviewItem {
        ReviewItem {
            id: id.into(), project_id: "p".into(),
            item_type: ReviewItemType::Skill, category: "skill".into(),
            title: title.into(), content: "x".into(),
            risk: RiskLevel::Low, status: ReviewStatus::Accepted,
            sync_targets: vec![], sync_scope: SyncScope::Project,
            source_pending_file: None, created_at: Utc::now(), reviewed_at: None,
        }
    }

    #[test]
    fn test_skill_dest_paths_dedups_after_rename() {
        // a="foo"、b="foo-bar"、c="foo"(id=bar)：c 撞 foo→改 foo-bar 又撞 b → 須再唯一化
        let root = std::path::Path::new("/no-such-vault/_skills");
        let (a, b, c) = (sk("aaa", "foo"), sk("xxx", "foo-bar"), sk("bar", "foo"));
        let items = vec![&a, &b, &c];
        let dests = skill_dest_paths(root, &items);
        let uniq: std::collections::HashSet<_> = dests.iter().collect();
        assert_eq!(uniq.len(), 3, "三筆落點須全唯一，不互相覆寫");
    }

    #[test]
    fn test_skill_dest_paths_empty_slug_fallback() {
        // 全符號標題 → slug 空 → fallback skill-<id>，落點仍為合法目錄式
        let root = std::path::Path::new("/no-such-vault/_skills");
        let s = sk("id123456", "###");
        let dests = skill_dest_paths(root, &[&s]);
        let p = dests[0].to_string_lossy().replace('\\', "/");
        assert!(p.contains("/_skills/skill-id123456/SKILL.md"), "空 slug 應 fallback skill-<id>，實得 {p}");
    }

    fn mem(id: &str, title: &str, scope: SyncScope) -> ReviewItem {
        ReviewItem {
            id: id.into(), project_id: "p".into(),
            item_type: ReviewItemType::Memory, category: "feedback".into(),
            title: title.into(), content: "內容一".into(),
            risk: RiskLevel::Low, status: ReviewStatus::Accepted,
            sync_targets: vec![], sync_scope: scope,
            source_pending_file: None, created_at: Utc::now(), reviewed_at: None,
        }
    }

    #[test]
    fn test_sync_cleans_orphan_project_memory_but_spares_others() {
        let root = std::env::temp_dir().join(format!("amagi-orphan-{}", uuid::Uuid::new_v4()));
        let vault = root.join("vault");
        let proj = root.join("proj");
        std::fs::create_dir_all(&proj).unwrap();
        let vf = "projects/p";
        let mem_dir = vault.join(vf).join("agent").join("memory");
        std::fs::create_dir_all(&mem_dir).unwrap();
        // 植入：孤兒記憶檔(記憶格式 .md)、非 .md 檔、非記憶格式的 .md
        std::fs::write(mem_dir.join("orphan-deadbeef.md"), "孤兒殘留").unwrap();
        std::fs::write(mem_dir.join("keep.txt"), "非 md，不可動").unwrap();
        std::fs::write(mem_dir.join("notes.md"), "手放筆記，非記憶格式，不可動").unwrap();

        let m = mem("real1", "真記憶", SyncScope::Project);
        sync_agent_files(proj.to_str().unwrap(), Some(vf), Some(&vault), &[], std::slice::from_ref(&m)).unwrap();

        // 真記憶檔在、MEMORY.md 在
        assert!(mem_dir.join(memory_filename(&m)).is_file(), "真記憶檔應寫入");
        assert!(mem_dir.join("MEMORY.md").is_file(), "索引應重建");
        // 記憶格式的孤兒被清
        assert!(!mem_dir.join("orphan-deadbeef.md").exists(), "記憶格式孤兒應被對帳清理");
        // 非 .md、非記憶格式 .md、MEMORY.md 不受影響
        assert!(mem_dir.join("keep.txt").exists(), "非 .md 檔不可被刪");
        assert!(mem_dir.join("notes.md").exists(), "非記憶格式 .md（手放）不可被刪");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_memory_hook_skips_frontmatter() {
        // 有 description → 取之（不抓 frontmatter 的 --- / name:）
        assert_eq!(
            memory_hook("---\nname: x\ndescription: 自動刷新驗證暗號為「白鶴亮翅」\n---\n正文"),
            "自動刷新驗證暗號為「白鶴亮翅」"
        );
        // 無 description → 取 frontmatter 後第一行正文（跳過標題）
        assert_eq!(memory_hook("---\nname: y\n---\n# 標題\n這是正文"), "這是正文");
        // 無 frontmatter → 第一行非空
        assert_eq!(memory_hook("直接正文\n第二行"), "直接正文");
        // 絕不回 ---
        assert_ne!(memory_hook("---\nname: z\n---\n內容"), "---");
        // 未閉合 frontmatter（無第二個 ---、無 description）→ 不回 ---（保守回空）
        assert_eq!(memory_hook("---\nname: noclose\nfoo: bar"), "");
        // CJK 超過 40 字 → 截斷加 …，且不切壞字元
        let long = "啊".repeat(50);
        let h = memory_hook(&long);
        assert_eq!(h.chars().count(), 41); // 40 + …
        assert!(h.ends_with('…'));
    }

    #[test]
    fn test_sync_unsafe_vault_folder_errs_not_silent() {
        // hard gate（Codex r7 高）：不安全 vault_folder + 有專案記憶 → 必須 Err，
        // 不可靜默跳過 vault 寫入卻讓外層 mark_synced。
        let root = std::env::temp_dir().join(format!("amagi-hardgate-{}", uuid::Uuid::new_v4()));
        let vault = root.join("vault");
        let proj = root.join("proj");
        std::fs::create_dir_all(&vault).unwrap();
        std::fs::create_dir_all(&proj).unwrap();
        let m = mem("hg1", "記憶", SyncScope::Project);
        let r = sync_agent_files(proj.to_str().unwrap(), Some("../escape"), Some(&vault), &[], std::slice::from_ref(&m));
        assert!(r.is_err(), "不安全 vault_folder + 有專案記憶 應回 Err");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_safe_dir_and_promote_reject_unsafe_vault_folder() {
        let root = std::env::temp_dir().join(format!("amagi-safedir-{}", uuid::Uuid::new_v4()));
        let vault = root.join("vault");
        std::fs::create_dir_all(&vault).unwrap();
        // 安全 → Some
        assert!(safe_project_memory_dir(&vault, "projects/p").is_some());
        // 首次建立：vault_root 尚不存在但 parent 存在 → 仍放行（祖先可解析、folder 乾淨）
        let fresh_root = root.join("fresh-vault");
        assert!(safe_project_memory_dir(&fresh_root, "projects/p").is_some(),
            "首次建立 vault 應放行");
        // 不安全 → None（擋逃逸）
        assert!(safe_project_memory_dir(&vault, "../escape").is_none());
        assert!(safe_project_memory_dir(&vault, "projects/../../etc").is_none());
        assert!(safe_project_memory_dir(&vault, "general").is_none());
        // promote 遇不安全 vault_folder → Err（拒絕刪檔，不逃逸）
        let m = mem("x", "t", SyncScope::Shared);
        let r = promote_memory_to_shared(&vault, "../escape", &m, &[], std::slice::from_ref(&m));
        assert!(r.is_err(), "不安全 vault_folder 應拒絕升級刪檔");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_orphan_cleanup_safety_helpers() {
        // vault_folder 驗證：擋逃逸
        assert!(is_safe_project_vault_folder("projects/foo"));
        assert!(!is_safe_project_vault_folder("../evil"));
        assert!(!is_safe_project_vault_folder("projects/../../etc"));
        assert!(!is_safe_project_vault_folder("general")); // 首段非 projects
        assert!(!is_safe_project_vault_folder(""));
        // 記憶檔命名格式
        assert!(looks_like_memory_file("title-abc12345.md"));
        assert!(looks_like_memory_file("a-b-c-deadbeef.md")); // slug 含 -，看最後段
        assert!(!looks_like_memory_file("notes.md"));         // 無 - 後綴
        assert!(!looks_like_memory_file("MEMORY.md"));
        assert!(!looks_like_memory_file("foo-toolongsuffix.md")); // 後綴 >8
        assert!(!looks_like_memory_file("foo-.md"));          // 空後綴
    }

    #[test]
    fn test_sync_global_memory_writes_general_only() {
        let root = std::env::temp_dir().join(format!("amagi-glob-{}", uuid::Uuid::new_v4()));
        let vault = root.join("vault");
        std::fs::create_dir_all(&vault).unwrap();

        // 全域 scope → general/agent/memory + 索引
        let g = mem("g1", "全域偏好", SyncScope::Global);
        let written = sync_global_memory(&vault, std::slice::from_ref(&g)).unwrap();
        assert!(written.iter().any(|f| {
            let p = f.replace('\\', "/");
            p.contains("/general/agent/memory/") && p.ends_with("MEMORY.md")
        }), "應寫 general 記憶索引");
        assert!(vault.join("general/agent/memory/MEMORY.md").is_file());
        assert!(written.iter().any(|f| {
            let p = f.replace('\\', "/");
            p.contains("/general/agent/memory/") && p.ends_with(".md") && !p.ends_with("MEMORY.md")
        }), "應寫個別全域記憶檔");

        // Project scope 不被 sync_global_memory 處理
        let p = mem("p1", "專案記憶", SyncScope::Project);
        assert!(sync_global_memory(&vault, std::slice::from_ref(&p)).unwrap().is_empty(),
            "Project scope 不該被 sync_global_memory 寫入");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_promote_memory_to_shared_moves_file() {
        let root = std::env::temp_dir().join(format!("amagi-promote-{}", uuid::Uuid::new_v4()));
        let vault = root.join("vault");
        let vf = "projects/foo";
        // 預先在專案層寫一筆記憶（模擬已同步）
        let proj_mem = mem("m1", "悔棋", SyncScope::Project);
        let proj_dir = vault.join(vf).join("agent").join("memory");
        std::fs::create_dir_all(&proj_dir).unwrap();
        let fname = memory_filename(&proj_mem);
        std::fs::write(proj_dir.join(&fname), markdown::build_memory_file(&proj_mem)).unwrap();
        std::fs::write(proj_dir.join("MEMORY.md"), "舊索引").unwrap();

        // 升級：該筆改 Shared、無剩餘專案記憶、all_shared 含該筆
        let mut shared = proj_mem.clone();
        shared.sync_scope = SyncScope::Shared;
        promote_memory_to_shared(&vault, vf, &proj_mem, &[], std::slice::from_ref(&shared)).unwrap();

        // 舊專案檔刪、無剩餘 → 專案索引移除
        assert!(!proj_dir.join(&fname).is_file(), "舊專案記憶檔應被刪");
        assert!(!proj_dir.join("MEMORY.md").is_file(), "無剩餘專案記憶 → 索引移除");
        // shared 落點建立 + 索引
        let shared_dir = vault.join("shared").join("agent").join("memory");
        assert!(shared_dir.join(&fname).is_file(), "應在 shared/agent/memory 建立記憶檔");
        assert!(shared_dir.join("MEMORY.md").is_file(), "應建立 shared 索引");

        let _ = std::fs::remove_dir_all(&root);
    }
}
