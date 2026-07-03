use std::path::{Path, PathBuf};
use crate::AppError;
use crate::models::review::{ReviewItem, ReviewItemType, SyncScope, ReviewStatus, RiskLevel};
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
/// 寫入/讀取路徑安全閘的第一關：擋 `..`/絕對路徑/非 projects 形式逃逸出 vault（Codex r3 高）。
fn is_safe_project_vault_folder(vf: &str) -> bool {
    let p = Path::new(vf);
    !vf.is_empty()
        && p.is_relative()
        && p.components().all(|c| matches!(c, std::path::Component::Normal(_)))
        && matches!(p.components().next(),
            Some(std::path::Component::Normal(s)) if s == std::ffi::OsStr::new("projects"))
}

/// 檔名是否像 Amagi 產生的記憶檔（`<slug>-<1..8 ascii 英數>.md`）。
/// vault loader / 寫入只認此格式，忽略同目錄中手放的 `.md`（vault-first 後不再據此刪檔）。
fn looks_like_memory_file(name: &str) -> bool {
    match name.strip_suffix(".md").and_then(|stem| stem.rfind('-').map(|i| (stem, i))) {
        Some((stem, i)) if i > 0 => {
            let sfx = &stem[i + 1..];
            !sfx.is_empty() && sfx.len() <= 8 && sfx.chars().all(|c| c.is_ascii_alphanumeric())
        }
        _ => false,
    }
}

/// 安全解析專案記憶目錄 `<vault_root>/<vault_folder>/agent/memory`——作為**寫入/讀取安全閘**
/// （sync 寫入 / vault loader 讀取 / promote 皆走此 helper；vault-first 後不再有「以佇列刪 vault 孤兒檔」）。
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

/// general/shared 跨層記憶目錄安全閘（Codex #4）：與 `safe_project_memory_dir` 同強度。
/// 限 tier ∈ {general, shared}，且該目錄「最深既存祖先」canonical 後仍落在 canonical vault_root 下
/// （擋 tier 目錄被 symlink/junction 指向 vault 外而讀/寫外部檔）。目錄可尚未存在（首次寫入）。
fn safe_tier_memory_dir(vault_root: &Path, tier: &str) -> Option<PathBuf> {
    if tier != "general" && tier != "shared" {
        return None;
    }
    let mem_dir = vault_root.join(tier).join("agent").join("memory");
    let mut anc: &Path = mem_dir.as_path();
    let canon_ancestor = loop {
        match anc.canonicalize() {
            Ok(c) => break c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => anc = anc.parent()?,
            Err(_) => return None,
        }
    };
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

/// 解析 vault 記憶檔（格式見 markdown::build_memory_file）→ (id, title, category, body, created)。
/// `id` 為 frontmatter 身分鍵（R2 最小版）；legacy 檔無 `id:` → None（呼叫端 fallback 檔名 idfrag）。
fn parse_memory_file(raw: &str) -> (Option<String>, String, String, String, Option<chrono::DateTime<chrono::Utc>>) {
    use chrono::TimeZone;
    let lines: Vec<&str> = raw.lines().collect();
    if lines.first().map(|l| l.trim()) != Some("---") {
        return (None, String::new(), String::new(), raw.trim().to_string(), None);
    }
    let (mut id, mut title, mut category, mut created) = (None, String::new(), String::new(), None);
    let mut i = 1;
    while i < lines.len() {
        let t = lines[i].trim();
        if t == "---" { i += 1; break; }
        if let Some(r) = t.strip_prefix("id:") {
            let v = r.trim().trim_matches('"').trim().to_string();
            if !v.is_empty() { id = Some(v); }
        } else if let Some(r) = t.strip_prefix("title:") {
            title = r.trim().trim_matches('"').trim().to_string();
        } else if let Some(r) = t.strip_prefix("category:") {
            category = r.trim().to_string();
        } else if let Some(r) = t.strip_prefix("created:") {
            if let Ok(d) = chrono::NaiveDate::parse_from_str(r.trim(), "%Y-%m-%d") {
                if let Some(ndt) = d.and_hms_opt(0, 0, 0) {
                    // 與 build_memory_file 寫出時的 Local 日期對稱：用 Local 午夜轉 UTC，
                    // 避免「UTC 午夜→Local 顯示倒退一天」在負時區逐次 sync 漂移（Codex 中）。
                    created = chrono::Local.from_local_datetime(&ndt).single()
                        .map(|dt| dt.with_timezone(&chrono::Utc));
                }
            }
        }
        i += 1;
    }
    let body = lines.get(i..).map(|s| s.join("\n")).unwrap_or_default().trim().to_string();
    (id, title, category, body, created)
}

/// 讀取一個記憶目錄中「所有合法受管記憶檔」→ ReviewItem（vault-first 權威來源核心）。
/// 純函式：只讀該目錄、不寫/不刪。格式守門與 reconcile 一致（frontmatter title/category/created
/// 齊備 + slug 與檔名一致），忽略 MEMORY.md/非受管檔名/非一般檔（symlink），**絕不刪除**。
/// scope/project_id 留預設，呼叫端視需要覆寫（索引重建僅用到 id/title/content/created）。
fn read_memory_dir(mem_dir: &Path) -> Vec<ReviewItem> {
    let canon_mem = match std::fs::canonicalize(mem_dir) { Ok(c) => c, Err(_) => return Vec::new() };
    let rd = match std::fs::read_dir(mem_dir) { Ok(r) => r, Err(_) => return Vec::new() };
    let mut out = Vec::new();
    for ent in rd.flatten() {
        let fname = ent.file_name().to_string_lossy().to_string();
        if fname == "MEMORY.md" || !looks_like_memory_file(&fname) { continue; }
        // 只認一般檔，不跟 symlink。
        let is_regular = std::fs::symlink_metadata(ent.path())
            .map(|m| m.file_type().is_file()).unwrap_or(false);
        if !is_regular { continue; }
        // 讀檔前驗 canonical 父目錄＝mem_dir，防判斷後被換成 symlink 逃逸（TOCTOU）。
        let parent_ok = std::fs::canonicalize(ent.path()).ok()
            .as_deref().and_then(Path::parent)
            .map(|p| p == canon_mem.as_path()).unwrap_or(false);
        if !parent_ok { continue; }
        let raw = match std::fs::read_to_string(ent.path()) { Ok(s) => s, Err(_) => continue };
        let (fm_id, title, category, content, created) = parse_memory_file(&raw);
        // 格式守門：只認具備合法 frontmatter 的記憶檔，避免把手放殘留/惡意 .md 洗白。
        if title.is_empty() || category.is_empty() || created.is_none() { continue; }
        let stem = fname.strip_suffix(".md").unwrap_or(&fname);
        let (slug_part, idfrag) = match stem.rfind('-') {
            Some(i) => (&stem[..i], stem[i + 1..].to_string()),
            None => continue,
        };
        if fs_utils::slugify(&title) != slug_part { continue; }
        // 身分鍵（R2 最小版 id frontmatter，adr-005/spec §5）：優先 frontmatter `id`（完整、穩定），
        // legacy 檔無 id → fallback 檔名 idfrag。下方一致性守門以 memory_filename 對回原檔
        // （id 前 8 位英數 == 檔名 idfrag），frontmatter id 被竄改成對不上檔名者一律忽略。
        let item = ReviewItem {
            id: fm_id.unwrap_or(idfrag),
            project_id: String::new(),
            item_type: ReviewItemType::Memory,
            category,
            title,
            content,
            risk: RiskLevel::Low,
            status: ReviewStatus::Synced,
            sync_targets: Vec::new(),
            sync_scope: SyncScope::Project,
            source_pending_file: None,
            created_at: created.unwrap(), // is_none 已於格式守門擋掉
            reviewed_at: None,
        };
        // 一致性守門：算出的檔名須等於既有檔名（再防洗白 + 確保 memory_filename 對回原檔）。
        if memory_filename(&item) == fname {
            out.push(item);
        }
    }
    out
}

/// vault-first：專案記憶權威集（安全閘解析目錄後讀取）。sync 以此重建索引/內聯，取代「佇列全集」。
pub fn load_project_memory_from_vault(vault_root: &Path, vault_folder: &str) -> Vec<ReviewItem> {
    match safe_project_memory_dir(vault_root, vault_folder) {
        Some(d) if d.is_dir() => read_memory_dir(&d),
        _ => Vec::new(),
    }
}

/// vault-first：某跨專案層（general/shared）記憶權威集。走 safe_tier_memory_dir 圍堵閘（Codex #4）。
fn load_tier_memory_from_vault(vault_root: &Path, tier: &str, scope: SyncScope) -> Vec<ReviewItem> {
    let dir = match safe_tier_memory_dir(vault_root, tier) {
        Some(d) if d.is_dir() => d,
        _ => return Vec::new(),
    };
    let mut items = read_memory_dir(&dir);
    for it in &mut items { it.sync_scope = scope.clone(); }
    items
}

/// vault-first：共用層（shared）記憶權威集（Phase 3 公開：list_vault_memories / promote 收斂判定用）。
pub fn load_shared_memory_from_vault(vault_root: &Path) -> Vec<ReviewItem> {
    load_tier_memory_from_vault(vault_root, "shared", SyncScope::Shared)
}
/// vault-first：全域層（general）記憶權威集（Phase 3 公開：list_vault_memories 用）。
pub fn load_global_memory_from_vault(vault_root: &Path) -> Vec<ReviewItem> {
    load_tier_memory_from_vault(vault_root, "general", SyncScope::Global)
}

// 註：跨機回填 `reconcile_project_memory_from_vault` 已於 vault-first 反轉（[[adr-005-vault-first-sync]]）
// 退役——sync/preview 改以 `load_project_memory_from_vault` 直接讀 vault 為權威，且已移除
// 「以佇列集合刪 vault 孤兒檔」的清理，故無跨機誤刪風險，回填補丁不再需要。

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

/// 共用防守閘（2026-07-03 事故）：project_path 等於 vault 根、或位於其下 → Err。
/// 凡「以 project_path 為根寫檔」的路徑（sync 內聯重寫、promote 衍生物刷新等）
/// 皆須在**任何寫入/搬檔前**過此閘，fail-closed。
pub fn ensure_project_outside_vault(vault_root: &Path, project_path: &str) -> Result<(), AppError> {
    if fs_utils::is_same_or_under(vault_root, Path::new(project_path)) {
        return Err(AppError::InvalidPath(format!(
            "專案路徑「{project_path}」位於 Amagi-Vault 知識庫內，拒絕寫入——\
             vault 是知識庫、非專案，請自專案清單移除該項目。"
        )));
    }
    Ok(())
}

pub fn sync_agent_files(
    project_path: &str,
    vault_folder: Option<&str>,
    vault_root: Option<&Path>,
    accepted: &[ReviewItem],
    all_project_memory: &[ReviewItem],
) -> Result<SyncResult, AppError> {
    // 防守深度（2026-07-03 事故）：project_path 落在 vault 內（例：vault 被註冊成專案的
    // 存量資料）→ 拒寫，否則下方會把 vault 根 CLAUDE.md/AGENTS.md 覆寫成專案指針。
    // 第一道閘在 add_project；此處擋「閘前已註冊」與其他呼叫路徑。
    if let Some(vroot) = vault_root {
        ensure_project_outside_vault(vroot, project_path)?;
    }
    let mut written: Vec<String> = Vec::new();
    // 優先用顯式 Project.vault_folder（權威來源）；缺時才由路徑 basename 推導。
    let vault_folder = vault_folder
        .map(|s| s.to_string())
        .unwrap_or_else(|| project_vault_folder(project_path));

    // ── 專案層記憶 → vault `<vault_folder>/agent/memory/`；專案 AGENTS/CLAUDE 內聯索引 ──
    // vault-first：只寫入本輪新核可（Accepted）記憶進 vault；索引/內聯改以 vault 現有檔為權威重建。
    // 專案 AGENTS/CLAUDE 內聯本專案記憶索引（非僅指標，實測薄指標不被跟讀）。
    let project_mem: Vec<&ReviewItem> = all_project_memory.iter()
        .filter(|i| i.item_type == ReviewItemType::Memory && i.sync_scope == SyncScope::Project)
        .collect();
    if let Some(vroot) = vault_root {
        // 安全閘：以共用 helper 解析記憶目錄（驗 vault_folder + canonical containment）。
        // 寫記憶檔/索引、孤兒清理、專案 AGENTS/CLAUDE 內聯，全部只在安全閘通過後才動（Codex r4/r7）。
        match safe_project_memory_dir(vroot, &vault_folder) {
            // hard gate（Codex r7 高）：有專案記憶要寫、但 vault_folder 不安全 → 直接 Err，
            // 阻止外層出列（不可「沒寫進 vault 卻視為已入庫」，違反 Phase 3a hard gate）。
            None if !project_mem.is_empty() => {
                return Err(AppError::InvalidPath(format!(
                    "不安全的 vault_folder「{vault_folder}」，拒絕寫入專案記憶（避免逃逸 vault）"
                )));
            }
            // 不安全且無專案記憶 → 無可寫/可清，整段跳過（不動 vault、不重寫專案 md）。
            None => {}
            Some(mem_dir) => {
                // 1) 寫入新核可（Accepted）的專案記憶進 vault（新記憶入庫）。Synced 已在 vault，
                //    不重寫（避免復活已被刪的檔）；migration 後佇列本就無 Synced（[[adr-005-vault-first-sync]]）。
                let new_mem: Vec<&ReviewItem> = project_mem.iter().copied()
                    .filter(|i| i.status == ReviewStatus::Accepted).collect();
                if !new_mem.is_empty() {
                    std::fs::create_dir_all(&mem_dir).map_err(|e| AppError::Io(e.to_string()))?;
                    for item in &new_mem {
                        let path = mem_dir.join(memory_filename(item));
                        // 衍生檔 → 無備份寫入，避免 agent/memory 累積 .bak 雜物。
                        std::fs::write(&path, markdown::build_memory_file(item)).map_err(|e| AppError::Io(e.to_string()))?;
                        written.push(path.to_string_lossy().to_string());
                    }
                }
                // 2) vault-first：以 vault 現有檔為權威重建索引/內聯（取代「佇列全集」）。
                let vault_items = load_project_memory_from_vault(vroot, &vault_folder);
                let vault_refs: Vec<&ReviewItem> = vault_items.iter().collect();
                let entries = memory_index_entries(&vault_refs);
                // MEMORY.md 索引：有記憶、或 mem_dir 已存在（清空重寫反映「已無」）。
                // vault-first：不再做「以佇列集合刪 vault 孤兒檔」的對帳清理——vault 即真相，
                // 非受管檔一律忽略、絕不自動刪（[[adr-005-vault-first-sync]]）。
                if !entries.is_empty() || mem_dir.exists() {
                    std::fs::create_dir_all(&mem_dir).map_err(|e| AppError::Io(e.to_string()))?;
                    let idx_path = mem_dir.join("MEMORY.md");
                    std::fs::write(&idx_path, markdown::build_memory_index(&entries))
                        .map_err(|e| AppError::Io(e.to_string()))?;
                    written.push(idx_path.to_string_lossy().to_string());
                }

                // 專案 AGENTS/CLAUDE：與「有無記憶」解耦（空→「（尚無）」），以 vault 權威集內聯。
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
        // 與 sync 共用同一安全閘（Codex r7 中）：vault_folder/祖先不安全 → 不列 vault 記憶與專案 md diff，
        // 避免 preview 顯示一組 sync 實際不會寫（或會 Err）的路徑，維持 preview/sync 一致。
        if let Some(mem_dir) = safe_project_memory_dir(vroot, &vault_folder) {
            // vault-first：預期 vault 集合 = vault 現有 ∪ 本輪新核可（同檔名以新項為準），
            // 與 sync（寫新項後以 vault 重建）產出的索引/內聯一致。
            let new_mem: Vec<&ReviewItem> = project_mem.iter().copied()
                .filter(|i| i.status == ReviewStatus::Accepted).collect();
            let new_names: std::collections::HashSet<String> =
                new_mem.iter().map(|i| memory_filename(i)).collect();
            let vault_items = load_project_memory_from_vault(vroot, &vault_folder);
            let mut expected: Vec<&ReviewItem> = vault_items.iter()
                .filter(|it| !new_names.contains(&memory_filename(it))).collect();
            expected.extend(new_mem.iter().copied());
            let entries = memory_index_entries(&expected);
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
            // 個別記憶檔：本輪新核可項（會實際寫入 vault）
            for item in &new_mem {
                let path = mem_dir.join(memory_filename(item));
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
    let mut written = Vec::new();
    // 1) 寫入新核可（Accepted）的跨層記憶進 vault。Synced 已在 vault，不重寫（避免復活已刪的檔）。
    let new_mems: Vec<&ReviewItem> = memory.iter()
        .filter(|i| i.item_type == ReviewItemType::Memory
            && i.sync_scope == scope
            && i.status == ReviewStatus::Accepted)
        .collect();
    // 安全閘（Codex #4）：tier 目錄不安全時，有待寫記憶 → Err（不靜默跳過寫入卻讓外層出列）；無則略過。
    let mem_dir = match safe_tier_memory_dir(vault_root, tier) {
        Some(d) => d,
        None => {
            if new_mems.is_empty() { return Ok(Vec::new()); }
            return Err(AppError::InvalidPath(format!("不安全的 {tier} 記憶目錄，拒絕寫入跨層記憶")));
        }
    };
    if !new_mems.is_empty() {
        std::fs::create_dir_all(&mem_dir).map_err(|e| AppError::Io(e.to_string()))?;
        for item in &new_mems {
            let path = mem_dir.join(memory_filename(item));
            std::fs::write(&path, markdown::build_memory_file(item)).map_err(|e| AppError::Io(e.to_string()))?;
            written.push(path.to_string_lossy().to_string());
        }
    }
    // 2) vault-first：以 vault 現有檔為權威重建索引（取代「佇列全集」）。
    //    空集合但 mem_dir 已存在 → 寫空索引反映「已無」（三層語意與 project 一致）。
    let vault_items = load_tier_memory_from_vault(vault_root, tier, scope);
    let refs: Vec<&ReviewItem> = vault_items.iter().collect();
    let entries = memory_index_entries(&refs);
    if !entries.is_empty() || mem_dir.exists() {
        std::fs::create_dir_all(&mem_dir).map_err(|e| AppError::Io(e.to_string()))?;
        let idx_path = mem_dir.join("MEMORY.md");
        std::fs::write(&idx_path, markdown::build_memory_index(&entries))
            .map_err(|e| AppError::Io(e.to_string()))?;
        written.push(idx_path.to_string_lossy().to_string());
    }
    Ok(written)
}

fn preview_tier_memory(vault_root: &Path, memory: &[ReviewItem], scope: SyncScope, tier: &str) -> Vec<FileDiffPreview> {
    // 安全閘（Codex #4）：與 sync_tier_memory 共用；不安全 tier 目錄 → 不列 diff（維持 preview/sync 一致）。
    let mem_dir = match safe_tier_memory_dir(vault_root, tier) {
        Some(d) => d,
        None => return Vec::new(),
    };
    // vault-first：預期集合 = vault 現有 ∪ 本輪新核可（同檔名以新項為準），與 sync_tier_memory 一致。
    let new_mems: Vec<&ReviewItem> = memory.iter()
        .filter(|i| i.item_type == ReviewItemType::Memory
            && i.sync_scope == scope
            && i.status == ReviewStatus::Accepted)
        .collect();
    let new_names: std::collections::HashSet<String> =
        new_mems.iter().map(|i| memory_filename(i)).collect();
    let vault_items = load_tier_memory_from_vault(vault_root, tier, scope);
    let mut expected: Vec<&ReviewItem> = vault_items.iter()
        .filter(|it| !new_names.contains(&memory_filename(it))).collect();
    expected.extend(new_mems.iter().copied());
    let entries = memory_index_entries(&expected);
    let mut previews = Vec::new();
    // 個別記憶檔：本輪新核可項（會實際寫入 vault）
    for item in &new_mems {
        let path = mem_dir.join(memory_filename(item));
        previews.push(FileDiffPreview {
            current_content: std::fs::read_to_string(&path).ok(),
            new_content: markdown::build_memory_file(item),
            is_new_file: !path.exists(),
            file_path: path.to_string_lossy().to_string(),
        });
    }
    // MEMORY.md 索引：有記憶、或 mem_dir 已存在（清空重寫）→ 預覽
    if !entries.is_empty() || mem_dir.exists() {
        let idx_path = mem_dir.join("MEMORY.md");
        previews.push(FileDiffPreview {
            current_content: std::fs::read_to_string(&idx_path).ok(),
            new_content: markdown::build_memory_index(&entries),
            is_new_file: !idx_path.exists(),
            file_path: idx_path.to_string_lossy().to_string(),
        });
    }
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

/// promote 結果：`moved`＝本次實際搬了檔（false＝續跑收斂，僅補刷索引）。
pub struct PromoteOutcome {
    pub moved: bool,
}

/// promote 的 shared 同 id **嚴格**預檢（R2 高）：不可用寬鬆 loader——`read_memory_dir` 對讀取失敗
/// 靜默跳過，身分集合不完整時 promote 會誤判 0 筆而寫出第二份身分（fail-open）。
/// 規則：只掃「受管檔名且檔名 id 片段 == 目標 id 前 8 位英數」的項目（守門規則下，片段不符者
/// 不可能持有同 id；非受管檔不屬身分集合，由檔名層級檢查兜住）——命中片段者必須是一般檔、
/// 可讀、且能解析出身分 id，否則 Err（fail-closed）。回傳可確認的 (檔名, 完整 id) 清單。
fn strict_scan_shared_same_idfrag(shared_dir: &Path, idfrag: &str) -> Result<Vec<(String, String)>, AppError> {
    let mut out = Vec::new();
    let rd = match std::fs::read_dir(shared_dir) {
        Ok(r) => r,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
        Err(e) => return Err(AppError::Io(format!("shared 記憶目錄無法列舉（{e}），中止升級"))),
    };
    for ent in rd.flatten() {
        let fname = ent.file_name().to_string_lossy().to_string();
        if fname == "MEMORY.md" || !looks_like_memory_file(&fname) { continue; }
        let stem = fname.strip_suffix(".md").unwrap_or(&fname);
        let file_frag = match stem.rfind('-') {
            Some(i) => stem[i + 1..].to_string(),
            None => continue,
        };
        if file_frag != idfrag { continue; }
        // 片段命中 → 必須能正面確認身分，否則 fail-closed
        let is_regular = std::fs::symlink_metadata(ent.path())
            .map(|m| m.file_type().is_file()).unwrap_or(false);
        if !is_regular {
            return Err(AppError::InvalidPath(format!(
                "shared/agent/memory/{fname} 非一般檔（symlink/目錄），身分無法確認，中止升級")));
        }
        let raw = std::fs::read_to_string(ent.path()).map_err(|e| AppError::Io(format!(
            "shared/agent/memory/{fname} 讀取失敗（{e}）：同 id 身分無法確認，中止升級（未寫任何檔）")))?;
        let (fm_id, title, category, _content, created) = parse_memory_file(&raw);
        if title.is_empty() || category.is_empty() || created.is_none() {
            return Err(AppError::InvalidPath(format!(
                "shared/agent/memory/{fname} 缺必要 frontmatter，身分無法確認，中止升級；請先修復或移除該檔")));
        }
        out.push((fname, fm_id.unwrap_or(file_frag)));
    }
    Ok(out)
}

/// 目標 id 的檔名片段（與 `memory_filename` 同規則：前 8 位 ASCII 英數）。
fn id_frag(id: &str) -> String {
    let s: String = id.chars().filter(|c| c.is_ascii_alphanumeric()).take(8).collect();
    if s.is_empty() { "x".to_string() } else { s }
}

/// 以 vault 現有檔重建某記憶目錄的 MEMORY.md 索引（目錄不存在則跳過；空集合寫空索引，
/// 與 sync 的三層語意一致「反映已無」）。promote 兩側索引重建共用。
fn rebuild_memory_index_in(mem_dir: &Path, items: &[ReviewItem]) -> Result<(), AppError> {
    if !mem_dir.is_dir() { return Ok(()); }
    let refs: Vec<&ReviewItem> = items.iter().collect();
    let entries = memory_index_entries(&refs);
    std::fs::write(mem_dir.join("MEMORY.md"), markdown::build_memory_index(&entries))
        .map_err(|e| AppError::Io(e.to_string()))
}

/// 升級（Phase 3 vault-first，[[adr-005-vault-first-sync]]）：把一筆專案記憶移到 shared/agent/memory。
/// **純 vault 檔案操作、零 queue 參與**——以 `memory_id`（frontmatter id 或 legacy 檔名 idfrag）
/// 在專案層權威集定位。順序與失敗語意（設計審 R1/R3）：
/// 1) **先寫 shared**（專用寫檔，不走 sync_tier_memory 的「只寫 Accepted」狀態過濾路徑）；
///    目標已存在且內容不同 → Err 不覆寫（非破壞）；同內容 → 視為已寫入（冪等重試）。
/// 2) **刪專案檔前讀回驗證** shared 檔確實落地且內容一致——杜絕「沒寫成就刪源」的資料遺失窗口。
/// 3) 兩側索引由 vault 現有檔重建。
/// **可續跑收斂**：專案層無此 id 但 shared 有 → 先前搬移已完成（中斷重試），只補刷索引。
pub fn promote_memory_to_shared(
    vault_root: &Path,
    vault_folder: &str,
    memory_id: &str,
) -> Result<PromoteOutcome, AppError> {
    // 安全閘（Codex r4 高）：promote 的刪檔路徑與 sync 共用同一道防線，杜絕 vault_folder 污染逃逸。
    let proj_mem_dir = safe_project_memory_dir(vault_root, vault_folder)
        .ok_or_else(|| AppError::InvalidPath(format!("不安全的 vault_folder，拒絕升級：{vault_folder}")))?;
    let proj_items = load_project_memory_from_vault(vault_root, vault_folder);
    let matches: Vec<&ReviewItem> = proj_items.iter().filter(|i| i.id == memory_id).collect();
    if matches.len() > 1 {
        return Err(AppError::InvalidPath(format!(
            "記憶 id「{memory_id}」在專案層命中 {} 筆（id 片段碰撞），請先改名消歧再升級", matches.len())));
    }

    let moved = match matches.first() {
        Some(&item) => {
            let shared_dir = safe_tier_memory_dir(vault_root, "shared")
                .ok_or_else(|| AppError::InvalidPath("不安全的 shared 記憶目錄，拒絕升級".into()))?;
            std::fs::create_dir_all(&shared_dir).map_err(|e| AppError::Io(e.to_string()))?;
            let fname = memory_filename(item);
            let dest = shared_dir.join(&fname);
            let expected = markdown::build_memory_file(item);
            // id 唯一性（外審 #2＋R2 高）：以**嚴格預檢**掃 shared 同片段檔——不可讀/無法確認身分
            // 即 Err（fail-closed），杜絕「loader 靜默跳過不可讀檔 → 誤判 0 筆 → 寫出第二份身分」。
            // 同 id 恰 1 筆且檔名一致 → 交由下方檔名層級檢查判冪等；檔名不同或多筆 → Err 人工消歧。
            let same_id: Vec<(String, String)> =
                strict_scan_shared_same_idfrag(&shared_dir, &id_frag(&item.id))?
                    .into_iter().filter(|(_, fid)| fid == &item.id).collect();
            match same_id.len() {
                0 => {}
                1 if same_id[0].0 == fname => {}
                1 => return Err(AppError::InvalidPath(format!(
                    "shared 已有同 id「{}」但不同檔名（{}），拒絕再寫一份；請先消歧既有檔再升級",
                    item.id, same_id[0].0))),
                n => return Err(AppError::InvalidPath(format!(
                    "shared 已有同 id「{}」共 {n} 筆，請先人工消歧再升級", item.id))),
            }
            match std::fs::read_to_string(&dest) {
                Ok(existing) if existing == expected => { /* 冪等重試：shared 已有同內容 */ }
                Ok(_) => return Err(AppError::InvalidPath(format!(
                    "shared/agent/memory/{fname} 已存在且內容不同，拒絕覆寫（非破壞）；請先處理既有檔再升級"))),
                // 只有「確定不存在」才建新檔（外審 #1）：其他讀取錯誤（權限/非 UTF-8/暫時 IO）
                // 可能代表既有檔在場但讀不了 → 一律 Err、不覆寫、不刪源（非破壞 fail-closed）。
                Err(e) if e.kind() == std::io::ErrorKind::NotFound =>
                    std::fs::write(&dest, &expected).map_err(|e| AppError::Io(e.to_string()))?,
                Err(e) => return Err(AppError::Io(format!(
                    "shared/agent/memory/{fname} 讀取失敗（{e}）：為避免覆寫既有資料，中止升級（未刪任何檔）"))),
            }
            // 讀回驗證後才刪源（R1）
            let readback = std::fs::read_to_string(&dest)
                .map_err(|e| AppError::Io(format!("shared 寫入驗證失敗（{e}），中止刪除專案檔")))?;
            if readback != expected {
                return Err(AppError::Io(format!(
                    "shared/agent/memory/{fname} 內容驗證不符，中止刪除專案檔")));
            }
            // 刪舊專案檔：限一般檔、不跟隨 symlink（沿用現行防護）
            let old_file = proj_mem_dir.join(&fname);
            let old_is_regular = std::fs::symlink_metadata(&old_file)
                .map(|m| m.file_type().is_file()).unwrap_or(false);
            if old_is_regular {
                std::fs::remove_file(&old_file).map_err(|e| AppError::Io(e.to_string()))?;
            }
            true
        }
        None => {
            // 專案層無此 id：shared **嚴格預檢**（R2 高，與寫入路徑同標準）——
            // 恰 1 筆 → 續跑收斂（僅補刷索引）；多筆（外審 #2）→ Err 人工消歧；皆無 → 找不到。
            let shared_dir = safe_tier_memory_dir(vault_root, "shared")
                .ok_or_else(|| AppError::InvalidPath("不安全的 shared 記憶目錄，拒絕收斂".into()))?;
            let n = strict_scan_shared_same_idfrag(&shared_dir, &id_frag(memory_id))?
                .into_iter().filter(|(_, fid)| fid == memory_id).count();
            match n {
                0 => return Err(AppError::InvalidPath(format!(
                    "找不到記憶 id「{memory_id}」（專案層與 shared 皆無）"))),
                1 => {}
                n => return Err(AppError::InvalidPath(format!(
                    "shared 已有同 id「{memory_id}」共 {n} 筆，請先人工消歧再收斂"))),
            }
            false
        }
    };

    // 兩側索引由 vault 現有檔重建
    rebuild_memory_index_in(&proj_mem_dir,
        &load_project_memory_from_vault(vault_root, vault_folder))?;
    if let Some(shared_dir) = safe_tier_memory_dir(vault_root, "shared") {
        rebuild_memory_index_in(&shared_dir, &load_shared_memory_from_vault(vault_root))?;
    }
    Ok(PromoteOutcome { moved })
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

    #[test]
    fn test_skill_sync_writes_vault_and_no_revive_after_delete() {
        // Phase 3 回歸：技能寫入 vault `_skills`（唯一權威）；vault 端刪除後，
        // 再 sync（佇列無該項——入庫已出列）不得復活。
        let root = std::env::temp_dir().join(format!("amagi-skill-norevive-{}", uuid::Uuid::new_v4()));
        let vault = root.join("vault");
        let proj = root.join("proj");
        std::fs::create_dir_all(&proj).unwrap();
        std::fs::create_dir_all(&vault).unwrap();
        let s = sk("sk1", "my-skill");
        sync_agent_files(proj.to_str().unwrap(), Some("projects/p"), Some(&vault),
            std::slice::from_ref(&s), &[]).unwrap();
        let dest = vault.join("_skills").join("my-skill").join("SKILL.md");
        assert!(dest.is_file(), "技能應寫入 vault _skills");
        // vault 端刪除 → 再 sync（無新核可）→ 不復活
        std::fs::remove_dir_all(vault.join("_skills").join("my-skill")).unwrap();
        sync_agent_files(proj.to_str().unwrap(), Some("projects/p"), Some(&vault), &[], &[]).unwrap();
        assert!(!dest.exists(), "vault 刪除的技能不得被 sync 復活");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_ensure_project_outside_vault_guard() {
        // promote_memory 等「以 project_path 為根寫檔」路徑共用的防守閘（Codex 高 #1 回歸）。
        // command 層需 State 無法直測，此處直測其呼叫的底層閘；
        // 「搬檔前先過閘」由 promote_memory 內呼叫順序保證（閘在 promote_memory_to_shared 之前）。
        let root = std::env::temp_dir().join(format!("amagi-guard-{}", uuid::Uuid::new_v4()));
        let vault = root.join("vault");
        let sub = vault.join("projects").join("x");
        std::fs::create_dir_all(&sub).unwrap();

        assert!(ensure_project_outside_vault(&vault, vault.to_str().unwrap()).is_err(), "vault 根應拒");
        assert!(ensure_project_outside_vault(&vault, sub.to_str().unwrap()).is_err(), "vault 子路徑應拒");
        #[cfg(windows)]
        assert!(ensure_project_outside_vault(&vault, &vault.to_string_lossy().to_uppercase()).is_err(),
            "大小寫變體應拒");

        let proj = root.join("proj");
        std::fs::create_dir_all(&proj).unwrap();
        assert!(ensure_project_outside_vault(&vault, proj.to_str().unwrap()).is_ok(), "vault 外專案應過");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_sync_rejects_project_path_inside_vault() {
        // 防守深度（2026-07-03 事故）：project_path 等於/位於 vault 根 → 拒寫，
        // 不得把 vault 根 CLAUDE.md/AGENTS.md 覆寫成專案指針。
        let root = std::env::temp_dir().join(format!("amagi-sync-invault-{}", uuid::Uuid::new_v4()));
        let vault = root.join("vault");
        std::fs::create_dir_all(&vault).unwrap();
        std::fs::write(vault.join("CLAUDE.md"), "# Wiki 規範源頭").unwrap();

        // vault 根本身
        let r = sync_agent_files(vault.to_str().unwrap(), Some("projects/vault"), Some(&vault), &[], &[]);
        assert!(r.is_err(), "vault 根作為 project_path 應被拒");
        // vault 內子路徑
        let sub = vault.join("projects").join("x");
        std::fs::create_dir_all(&sub).unwrap();
        assert!(sync_agent_files(sub.to_str().unwrap(), Some("projects/x"), Some(&vault), &[], &[]).is_err(),
            "vault 子路徑作為 project_path 應被拒");
        // 大小寫變體
        #[cfg(windows)]
        assert!(sync_agent_files(&vault.to_string_lossy().to_uppercase(), Some("projects/vault"), Some(&vault), &[], &[]).is_err(),
            "vault 路徑大小寫變體應被拒");
        // vault 根 CLAUDE.md 未被動過
        assert_eq!(std::fs::read_to_string(vault.join("CLAUDE.md")).unwrap(), "# Wiki 規範源頭",
            "vault 根 CLAUDE.md 不得被覆寫");

        // 正常專案（vault 外）→ 通過
        let proj = root.join("proj");
        std::fs::create_dir_all(&proj).unwrap();
        assert!(sync_agent_files(proj.to_str().unwrap(), Some("projects/p"), Some(&vault), &[], &[]).is_ok(),
            "vault 外的正常專案應可同步");

        let _ = std::fs::remove_dir_all(&root);
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
    fn test_sync_vault_first_ignores_nonmanaged_files() {
        // vault-first：sync 不再「以佇列集合刪 vault 孤兒檔」。非受管/無合法 frontmatter 的檔
        // 一律忽略（不載入索引、也絕不刪除）；真記憶正常寫入並索引。
        let root = std::env::temp_dir().join(format!("amagi-vf-ignore-{}", uuid::Uuid::new_v4()));
        let vault = root.join("vault");
        let proj = root.join("proj");
        std::fs::create_dir_all(&proj).unwrap();
        let vf = "projects/p";
        let mem_dir = vault.join(vf).join("agent").join("memory");
        std::fs::create_dir_all(&mem_dir).unwrap();
        // 像記憶檔名但無 frontmatter、非 .md、非記憶格式 .md
        std::fs::write(mem_dir.join("orphan-deadbeef.md"), "純文字殘留，無 frontmatter").unwrap();
        std::fs::write(mem_dir.join("keep.txt"), "非 md").unwrap();
        std::fs::write(mem_dir.join("notes.md"), "手放筆記，非記憶格式").unwrap();

        let m = mem("real1", "真記憶", SyncScope::Project);
        sync_agent_files(proj.to_str().unwrap(), Some(vf), Some(&vault), &[], std::slice::from_ref(&m)).unwrap();

        // 真記憶寫入 + 索引重建（含真記憶）
        assert!(mem_dir.join(memory_filename(&m)).is_file(), "真記憶檔應寫入");
        let idx = std::fs::read_to_string(mem_dir.join("MEMORY.md")).unwrap();
        assert!(idx.contains("真記憶"), "索引應含真記憶");
        // vault-first：非受管檔一律保留、絕不刪除
        assert!(mem_dir.join("orphan-deadbeef.md").exists(), "無 frontmatter 檔應被忽略、不刪");
        assert!(mem_dir.join("keep.txt").exists(), "非 .md 檔不可被刪");
        assert!(mem_dir.join("notes.md").exists(), "非記憶格式 .md 不可被刪");
        // 非受管檔不得混入索引
        assert!(!idx.contains("純文字殘留"), "非受管檔不得進索引");

        let _ = std::fs::remove_dir_all(&root);
    }

    // ── vault-first loader 與反轉回歸 ──────────────────────────
    #[test]
    fn test_load_project_memory_from_vault_skips_invalid() {
        let root = std::env::temp_dir().join(format!("amagi-load-{}", uuid::Uuid::new_v4()));
        let vault = root.join("vault");
        let vf = "projects/p";
        let mem_dir = vault.join(vf).join("agent").join("memory");
        std::fs::create_dir_all(&mem_dir).unwrap();
        // 合法記憶檔
        let m = mem("deadbeef", "合法記憶", SyncScope::Project);
        std::fs::write(mem_dir.join(memory_filename(&m)), markdown::build_memory_file(&m)).unwrap();
        // 無 frontmatter、缺 created、非記憶格式檔名 → 皆不得載入
        std::fs::write(mem_dir.join("orphan-deadbee1.md"), "純文字，無 frontmatter").unwrap();
        std::fs::write(mem_dir.join("partial-cafebabe.md"), "---\ntitle: x\ncategory: y\n---\n內容").unwrap();
        std::fs::write(mem_dir.join("notes.md"), "手放筆記，非記憶格式").unwrap();

        let loaded = load_project_memory_from_vault(&vault, vf);
        assert_eq!(loaded.len(), 1, "只應載入合法記憶檔");
        assert_eq!(loaded[0].title, "合法記憶");
        assert_eq!(loaded[0].content, "內容一");
        assert_eq!(memory_filename(&loaded[0]), memory_filename(&m), "檔名須對回原檔");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_sync_vault_first_no_revive_after_vault_delete() {
        // 反轉核心回歸：vault 端刪除記憶檔後，sync（佇列無該項）不得復活它，索引/CLAUDE 不含它。
        let root = std::env::temp_dir().join(format!("amagi-vf-norevive-{}", uuid::Uuid::new_v4()));
        let vault = root.join("vault");
        let proj = root.join("proj");
        let vf = "projects/p";
        let mem_dir = vault.join(vf).join("agent").join("memory");
        std::fs::create_dir_all(&mem_dir).unwrap();
        std::fs::create_dir_all(&proj).unwrap();

        // 情境一：vault 有一筆記憶、佇列空 → sync 以 vault 為權威內聯它。
        let m = mem("deadbeef", "既有記憶", SyncScope::Project);
        std::fs::write(mem_dir.join(memory_filename(&m)), markdown::build_memory_file(&m)).unwrap();
        sync_agent_files(proj.to_str().unwrap(), Some(vf), Some(&vault), &[], &[]).unwrap();
        let claude1 = std::fs::read_to_string(proj.join("CLAUDE.md")).unwrap();
        assert!(claude1.contains("既有記憶"), "vault-first：sync 應以 vault 為權威內聯既有記憶");

        // 情境二：刪 vault 檔 → 再 sync（佇列仍空）→ 不得復活、索引/CLAUDE 不含它。
        std::fs::remove_file(mem_dir.join(memory_filename(&m))).unwrap();
        sync_agent_files(proj.to_str().unwrap(), Some(vf), Some(&vault), &[], &[]).unwrap();
        assert!(!mem_dir.join(memory_filename(&m)).exists(), "vault 檔刪除後不得被 sync 復活");
        let idx = std::fs::read_to_string(mem_dir.join("MEMORY.md")).unwrap();
        assert!(!idx.contains("既有記憶"), "索引不得含已刪記憶");
        let claude2 = std::fs::read_to_string(proj.join("CLAUDE.md")).unwrap();
        assert!(!claude2.contains("既有記憶"), "CLAUDE 內聯不得含已刪記憶");
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
        // 不可靜默跳過 vault 寫入卻讓外層出列（沒入庫不得視為已同步）。
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
        let r = promote_memory_to_shared(&vault, "../escape", "x");
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

        // Project scope 不被 sync_global_memory 當作 general 記憶寫入（general 仍只有 g1）
        let p = mem("p1", "專案記憶", SyncScope::Project);
        sync_global_memory(&vault, std::slice::from_ref(&p)).unwrap();
        let p_as_global = mem("p1", "專案記憶", SyncScope::Global); // 僅用來算 p1 的檔名（與 scope 無關）
        assert!(!vault.join("general/agent/memory").join(memory_filename(&p_as_global)).exists(),
            "Project scope 記憶不該被寫入 general");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_promote_memory_to_shared_moves_file_vault_first() {
        // Phase 3 核心回歸：佇列零參與（本測試完全不碰 queue），僅憑 vault 檔案即可升級。
        let root = std::env::temp_dir().join(format!("amagi-promote-{}", uuid::Uuid::new_v4()));
        let vault = root.join("vault");
        let vf = "projects/foo";
        // 預先在專案層寫兩筆記憶（模擬已同步；m2 留下驗證索引重建）
        let m1 = mem("m1", "悔棋", SyncScope::Project);
        let m2 = mem("m2", "技術棧", SyncScope::Project);
        let proj_dir = vault.join(vf).join("agent").join("memory");
        std::fs::create_dir_all(&proj_dir).unwrap();
        let fname = memory_filename(&m1);
        std::fs::write(proj_dir.join(&fname), markdown::build_memory_file(&m1)).unwrap();
        std::fs::write(proj_dir.join(memory_filename(&m2)), markdown::build_memory_file(&m2)).unwrap();
        std::fs::write(proj_dir.join("MEMORY.md"), "舊索引").unwrap();

        let out = promote_memory_to_shared(&vault, vf, "m1").unwrap();
        assert!(out.moved, "應實際搬移");

        // 舊專案檔刪、剩餘集重建索引（含 m2、不含 m1）
        assert!(!proj_dir.join(&fname).is_file(), "舊專案記憶檔應被刪");
        let proj_idx = std::fs::read_to_string(proj_dir.join("MEMORY.md")).unwrap();
        assert!(proj_idx.contains("技術棧"), "專案索引應含剩餘記憶");
        assert!(!proj_idx.contains("悔棋"), "專案索引不得含已升級記憶");
        // shared 落點建立 + 索引
        let shared_dir = vault.join("shared").join("agent").join("memory");
        assert!(shared_dir.join(&fname).is_file(), "應在 shared/agent/memory 建立記憶檔");
        let shared_idx = std::fs::read_to_string(shared_dir.join("MEMORY.md")).unwrap();
        assert!(shared_idx.contains("悔棋"), "shared 索引應含升級後記憶");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_promote_not_found_and_shared_conflict() {
        let root = std::env::temp_dir().join(format!("amagi-promote2-{}", uuid::Uuid::new_v4()));
        let vault = root.join("vault");
        let vf = "projects/foo";
        let proj_dir = vault.join(vf).join("agent").join("memory");
        std::fs::create_dir_all(&proj_dir).unwrap();
        // 找不到（專案層與 shared 皆無）→ Err
        assert!(promote_memory_to_shared(&vault, vf, "ghost").is_err(), "皆無此 id 應 Err");

        // shared 已存在同檔名但**不同內容** → Err 不覆寫（非破壞），且不刪專案檔
        let m1 = mem("m1", "悔棋", SyncScope::Project);
        let fname = memory_filename(&m1);
        std::fs::write(proj_dir.join(&fname), markdown::build_memory_file(&m1)).unwrap();
        let shared_dir = vault.join("shared").join("agent").join("memory");
        std::fs::create_dir_all(&shared_dir).unwrap();
        std::fs::write(shared_dir.join(&fname), "別筆既有內容").unwrap();
        assert!(promote_memory_to_shared(&vault, vf, "m1").is_err(), "shared 撞名不同內容應 Err");
        assert!(proj_dir.join(&fname).is_file(), "衝突時專案檔不得被刪（無資料遺失窗口）");
        assert_eq!(std::fs::read_to_string(shared_dir.join(&fname)).unwrap(), "別筆既有內容",
            "shared 既有檔不得被覆寫");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_promote_unreadable_dest_errs_no_overwrite() {
        // 外審 #1：shared 同名檔存在但讀取失敗（非 UTF-8）→ Err、不覆寫、不刪專案源檔。
        let root = std::env::temp_dir().join(format!("amagi-promote4-{}", uuid::Uuid::new_v4()));
        let vault = root.join("vault");
        let vf = "projects/foo";
        let m1 = mem("m1", "悔棋", SyncScope::Project);
        let fname = memory_filename(&m1);
        let proj_dir = vault.join(vf).join("agent").join("memory");
        let shared_dir = vault.join("shared").join("agent").join("memory");
        std::fs::create_dir_all(&proj_dir).unwrap();
        std::fs::create_dir_all(&shared_dir).unwrap();
        std::fs::write(proj_dir.join(&fname), markdown::build_memory_file(&m1)).unwrap();
        // 無效 UTF-8 位元組 → read_to_string 回 InvalidData（非 NotFound）
        let garbage: &[u8] = &[0xff, 0xfe, 0xfd, 0x80];
        std::fs::write(shared_dir.join(&fname), garbage).unwrap();

        assert!(promote_memory_to_shared(&vault, vf, "m1").is_err(), "目標檔不可讀應 Err（fail-closed）");
        assert!(proj_dir.join(&fname).is_file(), "專案源檔不得被刪");
        assert_eq!(std::fs::read(shared_dir.join(&fname)).unwrap(), garbage, "既有檔不得被覆寫");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_promote_same_id_different_filename_errs() {
        // 外審 #2：shared 已有同 id 但不同檔名 → 拒絕寫第二份身分；收斂分支同 id 多筆 → Err。
        let root = std::env::temp_dir().join(format!("amagi-promote5-{}", uuid::Uuid::new_v4()));
        let vault = root.join("vault");
        let vf = "projects/foo";
        let m1 = mem("m1", "悔棋", SyncScope::Project);
        let proj_dir = vault.join(vf).join("agent").join("memory");
        let shared_dir = vault.join("shared").join("agent").join("memory");
        std::fs::create_dir_all(&proj_dir).unwrap();
        std::fs::create_dir_all(&shared_dir).unwrap();
        std::fs::write(proj_dir.join(memory_filename(&m1)), markdown::build_memory_file(&m1)).unwrap();
        // shared 既有：同 id「m1」、不同標題 → 不同檔名（受管格式、可被 loader 載入）
        let other = mem("m1", "other-title", SyncScope::Shared);
        std::fs::write(shared_dir.join(memory_filename(&other)), markdown::build_memory_file(&other)).unwrap();

        assert!(promote_memory_to_shared(&vault, vf, "m1").is_err(), "同 id 不同檔名應 Err 消歧");
        assert!(proj_dir.join(memory_filename(&m1)).is_file(), "源檔不得被刪");
        assert!(!shared_dir.join(memory_filename(&m1)).exists(), "不得寫出第二份同 id 記憶");

        // 收斂分支：專案層無此 id、shared 有兩筆同 id → Err 人工消歧
        std::fs::remove_file(proj_dir.join(memory_filename(&m1))).unwrap();
        std::fs::write(shared_dir.join(memory_filename(&m1)), markdown::build_memory_file(&m1)).unwrap();
        assert!(promote_memory_to_shared(&vault, vf, "m1").is_err(), "shared 同 id 多筆收斂應 Err");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_promote_strict_preflight_unreadable_managed_file() {
        // R2 高：shared 有「受管檔名＋同 id 片段」但不可讀（無效 UTF-8）→ 身分集合無法確認，
        // promote 必須 Err（fail-closed）、不寫新檔、不刪源；與目標無關的壞檔不得誤傷。
        let root = std::env::temp_dir().join(format!("amagi-promote6-{}", uuid::Uuid::new_v4()));
        let vault = root.join("vault");
        let vf = "projects/foo";
        let m1 = mem("m1", "悔棋", SyncScope::Project);
        let fname = memory_filename(&m1);
        let proj_dir = vault.join(vf).join("agent").join("memory");
        let shared_dir = vault.join("shared").join("agent").join("memory");
        std::fs::create_dir_all(&proj_dir).unwrap();
        std::fs::create_dir_all(&shared_dir).unwrap();
        std::fs::write(proj_dir.join(&fname), markdown::build_memory_file(&m1)).unwrap();
        // 同 id 片段（-m1）、不同檔名、不可讀 → 嚴格預檢必須擋下
        let garbage: &[u8] = &[0xff, 0xfe, 0x80, 0x81];
        std::fs::write(shared_dir.join("other-m1.md"), garbage).unwrap();

        assert!(promote_memory_to_shared(&vault, vf, "m1").is_err(),
            "同片段不可讀受管檔在場 → 應 Err（fail-closed），不得誤判 0 筆");
        assert!(proj_dir.join(&fname).is_file(), "源檔不得被刪");
        assert!(!shared_dir.join(&fname).exists(), "不得寫出新檔");

        // 換成「無關片段」的壞檔 → 不影響身分判定，promote 應成功
        std::fs::remove_file(shared_dir.join("other-m1.md")).unwrap();
        std::fs::write(shared_dir.join("unrelated-zz9.md"), garbage).unwrap();
        promote_memory_to_shared(&vault, vf, "m1").unwrap();
        assert!(shared_dir.join(&fname).is_file(), "無關壞檔不得阻擋升級");
        assert!(!proj_dir.join(&fname).exists(), "升級完成應刪源");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_promote_resumes_after_interruption() {
        // R3 續跑收斂：①shared 已有同內容、專案檔還在（刪除前中斷）→ 重跑完成刪除；
        // ②專案檔已刪、shared 有（索引重建前中斷）→ 重跑僅補刷索引（moved=false）。
        let root = std::env::temp_dir().join(format!("amagi-promote3-{}", uuid::Uuid::new_v4()));
        let vault = root.join("vault");
        let vf = "projects/foo";
        let m1 = mem("m1", "悔棋", SyncScope::Project);
        let fname = memory_filename(&m1);
        let proj_dir = vault.join(vf).join("agent").join("memory");
        let shared_dir = vault.join("shared").join("agent").join("memory");
        std::fs::create_dir_all(&proj_dir).unwrap();
        std::fs::create_dir_all(&shared_dir).unwrap();

        // ① 兩側各一份（同內容）
        std::fs::write(proj_dir.join(&fname), markdown::build_memory_file(&m1)).unwrap();
        std::fs::write(shared_dir.join(&fname), markdown::build_memory_file(&m1)).unwrap();
        let out = promote_memory_to_shared(&vault, vf, "m1").unwrap();
        assert!(out.moved, "同內容重試應完成搬移（刪源）");
        assert!(!proj_dir.join(&fname).is_file(), "重試後專案檔應被刪");
        assert!(shared_dir.join(&fname).is_file(), "shared 檔保留");

        // ② 只剩 shared → 收斂路徑
        let out2 = promote_memory_to_shared(&vault, vf, "m1").unwrap();
        assert!(!out2.moved, "已搬移完成 → 僅收斂索引");
        assert!(shared_dir.join("MEMORY.md").is_file(), "shared 索引應存在");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_loader_prefers_frontmatter_id() {
        // R2 最小版：frontmatter 有 id → 以完整 id 為身分鍵；一致性守門仍過（idfrag == 檔名片段）。
        let root = std::env::temp_dir().join(format!("amagi-fmid-{}", uuid::Uuid::new_v4()));
        let vault = root.join("vault");
        let vf = "projects/p";
        let mem_dir = vault.join(vf).join("agent").join("memory");
        std::fs::create_dir_all(&mem_dir).unwrap();
        // 完整 uuid id：檔名只取前 8 位英數
        let full_id = "a1b2c3d4-e5f6-7890-abcd-ef0123456789";
        let m = mem(full_id, "完整鍵記憶", SyncScope::Project);
        std::fs::write(mem_dir.join(memory_filename(&m)), markdown::build_memory_file(&m)).unwrap();
        // legacy 檔（無 id frontmatter）→ fallback 檔名 idfrag
        std::fs::write(mem_dir.join("legacy-cafebabe.md"),
            "---\ntitle: \"legacy\"\ncategory: feedback\ncreated: 2026-07-01\n---\n舊檔").unwrap();

        let loaded = load_project_memory_from_vault(&vault, vf);
        assert_eq!(loaded.len(), 2);
        assert!(loaded.iter().any(|i| i.id == full_id), "frontmatter id 應為完整身分鍵");
        assert!(loaded.iter().any(|i| i.id == "cafebabe"), "legacy 檔 fallback 檔名 idfrag");
        // 竄改 id 對不上檔名 → 一致性守門忽略該檔
        std::fs::write(mem_dir.join("evil-deadbeef.md"),
            "---\nid: zzzzzzzz-0000\ntitle: \"evil\"\ncategory: feedback\ncreated: 2026-07-01\n---\nx").unwrap();
        let loaded2 = load_project_memory_from_vault(&vault, vf);
        assert!(!loaded2.iter().any(|i| i.title == "evil"), "id 與檔名不符的檔應被守門忽略");
        let _ = std::fs::remove_dir_all(&root);
    }
}
