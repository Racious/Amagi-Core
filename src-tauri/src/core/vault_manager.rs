use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use crate::AppError;
use crate::core::safety_filter;
use crate::utils::{fs_utils, json_store, markdown};

/// 受管區塊標記。寫入全局 ~/.claude/CLAUDE.md 時，僅替換這兩個標記之間的內容，
/// 標記之外（老爺的人格設定等）一字不動。
const BEGIN_MARKER: &str = "<!-- AMAGI-VAULT:BEGIN (Amagi Core 管理，勿手改) -->";
const END_MARKER: &str = "<!-- AMAGI-VAULT:END -->";

/// 本機 vault 設定，存於 AppData/vault.json（各機獨立，不進任何 repo）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultConfig {
    pub vault_path: Option<String>,
    pub pointer_written: bool,
}

/// 設定 vault 路徑的結果，回報給前端。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultSetResult {
    pub vault_path: String,
    /// 該資料夾是否看起來已是 vault（含 CLAUDE.md 與 index.md）。
    pub looks_like_vault: bool,
    pub claude_md_path: String,
    pub backup_made: bool,
    /// "appended"（首次附加）或 "replaced"（替換既有受管區塊）。
    pub pointer_action: String,
    /// vault 內是否存在全域 doctrine 源檔（`general/_meta/global-agent-config.md`）。
    /// 為 true 時前端可於設路徑後跳確認、順手部署全域 doctrine（步驟5 自動化，A 案）。
    pub has_doctrine_source: bool,
}

fn config_path(data_dir: &Path) -> PathBuf {
    data_dir.join("vault.json")
}

pub fn get_vault_config(data_dir: &Path) -> VaultConfig {
    json_store::read_json_or_default(&config_path(data_dir))
}

/// 讀本機 vault 設定並驗 project_path 不在 vault 內；未設 vault → 放行。
/// 「以 project.path 為根寫檔」的 command（bridge、init 等）入口共用此閘
/// （2026-07-03 事故防守深度；核心比對在 agent_exporter::ensure_project_outside_vault）。
pub fn ensure_project_path_outside_vault(project_path: &str, data_dir: &Path) -> Result<(), AppError> {
    if let Some(vp) = get_vault_config(data_dir).vault_path {
        crate::core::agent_exporter::ensure_project_outside_vault(Path::new(&vp), project_path)?;
    }
    Ok(())
}

/// vault 設定狀態，供首次啟動引導（2c）判斷是否需引導、是否已掛 git（保命）。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultStatus {
    /// 是否已設定 vault 路徑（未設 → 首次啟動引導）。
    pub configured: bool,
    pub vault_path: Option<String>,
    /// vault 資料夾是否已是 git repo（未掛 → 強烈建議掛 git，adr-004 D1 保命）。
    pub is_git_repo: bool,
}

pub fn get_vault_status(data_dir: &Path) -> VaultStatus {
    let cfg = get_vault_config(data_dir);
    let is_git_repo = cfg
        .vault_path
        .as_deref()
        .map(fs_utils::is_git_repo)
        .unwrap_or(false);
    VaultStatus {
        configured: cfg.vault_path.is_some(),
        vault_path: cfg.vault_path,
        is_git_repo,
    }
}

/// vault 內是否存在全域 doctrine 源檔（`general/_meta/global-agent-config.md`）。
/// 抽為純函式以可單元測試——不觸發 `set_vault_path` 的全域檔寫入，避免測試污染家目錄。
fn detect_doctrine_source(vault_path: &Path) -> bool {
    vault_path
        .join("general").join("_meta").join("global-agent-config.md")
        .is_file()
}

/// 設定本機 vault 路徑：
/// 1. 驗證資料夾存在
/// 2. 組受管區塊並過安全過濾
/// 3. 寫入全局 ~/.claude/CLAUDE.md 與 ~/.codex/AGENTS.md（僅替換受管區塊，先備份 .bak）
/// 4. 持久化本機設定
pub fn set_vault_path(path: &str, data_dir: &Path) -> Result<VaultSetResult, AppError> {
    let p = Path::new(path);
    if !p.is_dir() {
        return Err(AppError::InvalidPath(format!("資料夾不存在或不是目錄：{path}")));
    }

    // D7：偵測是否為既有 vault（僅作提示，不阻擋）
    let looks_like_vault = p.join("CLAUDE.md").is_file() && p.join("index.md").is_file();

    // 組受管區塊，寫入前過安全過濾（純路徑通常不會命中，仍依規範把關）
    let block = build_pointer_block(path);
    let safety = safety_filter::check(&block);
    if !safety.is_safe {
        let mut labels: Vec<String> = safety.hits.iter().map(|h| h.label.clone()).collect();
        labels.dedup(); // D0(a) find_iter 後同規則多值 → label 去重（同規則命中連續，dedup 足夠）
        return Err(AppError::SafetyBlocked(format!(
            "vault 路徑內容疑似含敏感資訊：{}",
            labels.join("、")
        )));
    }

    let claude_md = fs_utils::global_claude_md_path()
        .ok_or_else(|| AppError::Io("無法取得 ~/.claude/CLAUDE.md 路徑".into()))?;

    let (claude_backup, pointer_action) = write_managed_block(&claude_md, &block)?;

    // 同步寫入 Codex 全局錨點 ~/.codex/AGENTS.md（同一受管區塊、同安全機制：只動標記間、.bak、冪等）
    let codex_backup = match fs_utils::global_codex_agents_md_path() {
        Some(codex_agents) => write_managed_block(&codex_agents, &block)?.0,
        None => false,
    };
    let backup_made = claude_backup || codex_backup;

    let cfg = VaultConfig {
        vault_path: Some(path.to_string()),
        pointer_written: true,
    };
    json_store::write_json(&config_path(data_dir), &cfg)?;

    // adr-008：vault 掛 git 時冪等補設同步紀律 config（pull.rebase / rebase.autoStash，僅 --local）。
    // best-effort：config 設不上不該擋 vault 設定（pull/sync 入口另有防禦性補設）。
    // 判定用 rev-parse（vault_git::is_git_work_tree）而非 .git 目錄——linked worktree 也涵蓋。
    if crate::core::vault_git::is_git_work_tree(p) {
        if let Err(e) = crate::core::vault_git::ensure_repo_config(p) {
            eprintln!("[AMAGI] vault git config 補設失敗（不影響 vault 設定）：{e}");
        }
    }

    // 偵測全域 doctrine 源檔是否就位（供前端決定是否提議自動部署，步驟5 A 案）。
    let has_doctrine_source = detect_doctrine_source(p);

    Ok(VaultSetResult {
        vault_path: path.to_string(),
        looks_like_vault,
        claude_md_path: claude_md.to_string_lossy().to_string(),
        backup_made,
        pointer_action,
        has_doctrine_source,
    })
}

/// 讀某層記憶索引（`<tier>/agent/memory/MEMORY.md`）的條目行（以 `-` 開頭者），
/// 供內聯進錨點。回傳 None＝該層尚無索引或無條目。
/// 只取條目行（去掉索引檔自身的標題／引言），內聯版面乾淨。
fn read_tier_memory_entries(vault_path: &str, tier: &str) -> Option<String> {
    let p = Path::new(vault_path)
        .join(tier).join("agent").join("memory").join("MEMORY.md");
    let content = std::fs::read_to_string(&p).ok()?;
    // 防衛縱深：內聯進全域錨點前中和 HTML comment delimiters，
    // 杜絕 MEMORY.md（可能被手改/舊資料）含 `<!-- AMAGI-VAULT:END -->` 假標記破壞 splice 邊界。
    let entries: Vec<String> = content
        .lines()
        .map(|l| l.trim_end())
        .filter(|l| l.trim_start().starts_with('-'))
        .map(|l| l.replace("<!--", "<! --").replace("-->", "-- >"))
        .collect();
    if entries.is_empty() { None } else { Some(entries.join("\n")) }
}

/// 全域錨點受管區塊：**內聯** general／shared 記憶索引（非僅指標）。
/// 實測顯示薄指標不會被主動跟讀；內聯到「必讀的 CLAUDE.md／AGENTS.md」最可靠。
/// 索引隨 set_vault_path（及日後 sync 刷新）以當下 vault 內容重建。
fn build_pointer_block(vault_path: &str) -> String {
    let general = read_tier_memory_entries(vault_path, "general");
    let shared = read_tier_memory_entries(vault_path, "shared");
    let mut s = String::new();
    s.push_str(BEGIN_MARKER);
    s.push_str("\n# Amagi-Vault 知識庫\n");
    s.push_str(&format!("路徑：{vault_path}\n"));
    s.push_str("對話開始時讀取該路徑 index.md 與最近 3 份 daily/；涉專案時先讀該專案 handoff.md（當前狀態活頁，最新進度），規則依該路徑 CLAUDE.md。\n\n");
    s.push_str("## 記憶速查（以下索引已內聯，開場即視為已知；需細節再讀對應 `<層>/agent/memory/<檔>`）\n\n");
    s.push_str("### 全域記憶（general，每次對話都適用）\n");
    match &general {
        Some(e) => { s.push_str(e); s.push('\n'); }
        None => s.push_str("（尚無）\n"),
    }
    s.push_str("\n### 共用記憶（shared，跨專案）\n");
    match &shared {
        Some(e) => { s.push_str(e); s.push('\n'); }
        None => s.push_str("（尚無）\n"),
    }
    s.push_str("\n> 當前專案的記憶索引另見該專案的 CLAUDE.md／AGENTS.md。\n");
    s.push_str(END_MARKER);
    s
}

/// 以當下 vault 記憶內容，重寫全域錨點受管區塊（~/.claude/CLAUDE.md、~/.codex/AGENTS.md）。
/// 供 sync 後呼叫，使內聯的 general／shared 索引自動跟上最新，不必手動重設 vault。
/// 未設 vault → 無動作（Ok）。
pub fn refresh_global_anchor(data_dir: &Path) -> Result<(), AppError> {
    let vault_path = match get_vault_config(data_dir).vault_path {
        Some(v) => v,
        None => return Ok(()),
    };
    let block = build_pointer_block(&vault_path);
    // 縱深：block 內聯了 general/shared 的 MEMORY.md 條目（可能來自舊資料或手改），
    // 內聯前再過一次 safety_filter——杜絕既有 MEMORY.md 的裸 token 被擴散進全域錨點
    // （~/.claude、~/.codex）。命中則 fail-soft：不刷新錨點、回 Err 供 sync 轉成 warning，
    // 不阻斷記憶已落 vault（Codex 稽核低）。
    let safety = safety_filter::check(&block);
    if !safety.is_safe {
        let mut labels: Vec<String> = safety.hits.iter().map(|h| h.label.clone()).collect();
        labels.dedup(); // D0(a) find_iter 後同規則多值 → label 去重（同規則命中連續，dedup 足夠）
        return Err(AppError::SafetyBlocked(format!(
            "全域錨點刷新偵測到疑似敏感內容（{}），已略過刷新以免擴散到 ~/.claude／~/.codex；請檢查 general／shared 的 MEMORY.md。",
            labels.join("、")
        )));
    }
    if let Some(claude_md) = fs_utils::global_claude_md_path() {
        write_managed_block(&claude_md, &block)?;
    }
    if let Some(codex_agents) = fs_utils::global_codex_agents_md_path() {
        write_managed_block(&codex_agents, &block)?;
    }
    Ok(())
}

/// 將受管區塊寫入目標檔，先備份 .bak。
/// 回傳 (是否做了備份, "appended"|"replaced")。
fn write_managed_block(path: &Path, block: &str) -> Result<(bool, String), AppError> {
    let existing = std::fs::read_to_string(path).unwrap_or_default();
    let (new_content, action) = splice_managed_block(&existing, block);
    let backup_made = path.exists();
    markdown::write_with_backup(path, &new_content)?;
    Ok((backup_made, action.to_string()))
}

/// 純函式：把受管區塊嵌進既有內容。
/// - 已有 BEGIN/END 標記 → 只替換其間（冪等，不堆疊）
/// - 否則 → 附加於檔尾，原內容保持不變
fn splice_managed_block(existing: &str, block: &str) -> (String, &'static str) {
    match (existing.find(BEGIN_MARKER), existing.find(END_MARKER)) {
        (Some(bi), Some(ei)) if ei > bi => {
            let end_full = ei + END_MARKER.len();
            let mut s = String::with_capacity(existing.len() + block.len());
            s.push_str(&existing[..bi]);
            s.push_str(block);
            s.push_str(&existing[end_full..]);
            (s, "replaced")
        }
        _ => (append_block(existing, block), "appended"),
    }
}

fn append_block(existing: &str, block: &str) -> String {
    if existing.trim().is_empty() {
        format!("{block}\n")
    } else if existing.ends_with("\n\n") {
        format!("{existing}{block}\n")
    } else if existing.ends_with('\n') {
        format!("{existing}\n{block}\n")
    } else {
        format!("{existing}\n\n{block}\n")
    }
}

// ─────────────────────────────────────────────────────────────────────────
// 步驟5：全域 doctrine 自動部署（adr-005 精神；設計審查 2026-07-01-step5-...）。
// vault `general/_meta/global-agent-config.md` 為唯一源 → 整檔部署到本機
// ~/.claude/CLAUDE.md（H1 `# CLAUDE.md`）與 ~/.codex/AGENTS.md（H1 `# AGENTS.md`）。
// fail-closed：任何解析/render/safety 失敗一律不寫；寫入用 temp+原子 rename + 備份。
// ─────────────────────────────────────────────────────────────────────────

const DOCTRINE_BEGIN_TOKEN: &str = "AMAGI-DOCTRINE:BEGIN";
const DOCTRINE_END_TOKEN: &str = "AMAGI-DOCTRINE:END";

/// 全域 doctrine 部署結果，回報前端。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeployResult {
    pub claude_path: String,
    pub codex_path: String,
    pub backup_made: bool,
    /// 部署提醒（Codex override 存在、AGENTS.md 逼近 32 KiB 等），非錯誤。
    pub warnings: Vec<String>,
}

/// `<file>.<suffix>`：附加副檔名，**不吃掉原 `.md`**（有別於 `with_extension`：CLAUDE.md → CLAUDE.md.bak）。
fn append_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut s = path.as_os_str().to_os_string();
    s.push(".");
    s.push(suffix);
    PathBuf::from(s)
}

/// 從標準檔原文抽「可部署本體」（`AMAGI-DOCTRINE:BEGIN/END` 之間、不含標記行）。
/// 恰好一組 begin/end 且 begin<end，否則 Err（畸形 → fail-closed，不寫任何檔）。
fn extract_doctrine_body(source: &str) -> Result<String, AppError> {
    let begin_n = source.matches(DOCTRINE_BEGIN_TOKEN).count();
    let end_n = source.matches(DOCTRINE_END_TOKEN).count();
    if begin_n != 1 || end_n != 1 {
        return Err(AppError::InvalidPath(format!(
            "global-agent-config.md 的 AMAGI-DOCTRINE 標記需恰好一組（begin={begin_n}, end={end_n}）"
        )));
    }
    let b = source.find(DOCTRINE_BEGIN_TOKEN).unwrap();
    let e = source.find(DOCTRINE_END_TOKEN).unwrap();
    if b >= e {
        return Err(AppError::InvalidPath("AMAGI-DOCTRINE:BEGIN 必須在 END 之前".into()));
    }
    let body_start = source[b..].find('\n').map(|i| b + i + 1)
        .ok_or_else(|| AppError::InvalidPath("AMAGI-DOCTRINE:BEGIN 行格式異常".into()))?;
    let body_end = source[..e].rfind('\n').map(|i| i + 1).unwrap_or(0);
    if body_end <= body_start {
        return Err(AppError::InvalidPath("AMAGI-DOCTRINE 本體為空".into()));
    }
    Ok(source[body_start..body_end].trim_end().to_string())
}

/// render 可部署本體為某目標全域檔內容：
/// ① 第一個非空行須是 H1 → 取代為 `target_h1`（否則畸形 Err，不 silently prepend）
/// ② 本體內 `AMAGI-VAULT` 佔位（恰好一組）→ 換成 `build_pointer_block(vault_path)` 真實內容
/// ③ 驗證輸出：AMAGI-VAULT 恰好一組、無殘留 AMAGI-DOCTRINE 標記
fn render_global_doctrine(body: &str, target_h1: &str, vault_path: &str) -> Result<String, AppError> {
    let lines: Vec<&str> = body.lines().collect();
    let h1_idx = lines.iter().position(|l| !l.trim().is_empty())
        .ok_or_else(|| AppError::InvalidPath("doctrine 本體為空".into()))?;
    if !lines[h1_idx].trim_start().starts_with("# ") {
        return Err(AppError::InvalidPath("doctrine 本體第一個非空行不是 H1（標準檔畸形）".into()));
    }
    if body.matches(BEGIN_MARKER).count() != 1 || body.matches(END_MARKER).count() != 1 {
        return Err(AppError::InvalidPath("doctrine 本體內 AMAGI-VAULT 標記需恰好一組".into()));
    }
    let mut out: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
    out[h1_idx] = target_h1.to_string();
    let content = out.join("\n");
    let block = build_pointer_block(vault_path);
    let (content, _) = splice_managed_block(&content, &block);
    if content.matches(BEGIN_MARKER).count() != 1 || content.matches(END_MARKER).count() != 1 {
        return Err(AppError::InvalidPath("render 後 AMAGI-VAULT 標記數異常".into()));
    }
    if content.contains(DOCTRINE_BEGIN_TOKEN) || content.contains(DOCTRINE_END_TOKEN) {
        return Err(AppError::InvalidPath("render 後不應殘留 AMAGI-DOCTRINE 標記".into()));
    }
    Ok(content)
}

/// 原子寫入全域檔（Codex #1/#5）：首次 `.predeploy.bak`（create-new，永不覆寫，保留跨機導入前原始版）
/// + rolling `.bak`（可覆寫，最近一次）+ temp 檔 + 原子 rename。回傳是否有做備份。
fn write_global_atomic(target: &Path, content: &str) -> Result<bool, AppError> {
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|e| AppError::Io(e.to_string()))?;
    }
    let mut backup_made = false;
    if target.exists() {
        let orig = std::fs::read(target).map_err(|e| AppError::Io(e.to_string()))?;
        let predeploy = append_suffix(target, "predeploy.bak");
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&predeploy) {
            Ok(mut f) => {
                use std::io::Write;
                f.write_all(&orig).map_err(|e| AppError::Io(e.to_string()))?;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(AppError::Io(e.to_string())),
        }
        std::fs::write(append_suffix(target, "bak"), &orig).map_err(|e| AppError::Io(e.to_string()))?;
        backup_made = true;
    }
    let tmp = append_suffix(target, "tmp");
    std::fs::write(&tmp, content).map_err(|e| AppError::Io(e.to_string()))?;
    std::fs::rename(&tmp, target).map_err(|e| AppError::Io(e.to_string()))?;
    Ok(backup_made)
}

/// 步驟5 主流程：讀 vault 標準檔 → 抽本體 → render 兩目標 → safety → 原子寫入兩全域檔。
/// 全部 render+safety 在任何寫入前完成（fail-closed）；第二檔寫入失敗嘗試回滾第一檔。
pub fn deploy_global_doctrine(data_dir: &Path) -> Result<DeployResult, AppError> {
    let vault_path = get_vault_config(data_dir).vault_path
        .ok_or_else(|| AppError::InvalidPath("尚未設定 vault 路徑，無法部署全域 doctrine".into()))?;
    let source_path = Path::new(&vault_path)
        .join("general").join("_meta").join("global-agent-config.md");
    let source = std::fs::read_to_string(&source_path).map_err(|e| AppError::InvalidPath(
        format!("讀不到 vault 全域標準檔 general/_meta/global-agent-config.md（{e}）")))?;
    let claude_md = fs_utils::global_claude_md_path()
        .ok_or_else(|| AppError::Io("無法取得 ~/.claude/CLAUDE.md 路徑".into()))?;
    let codex_agents = fs_utils::global_codex_agents_md_path()
        .ok_or_else(|| AppError::Io("無法取得 ~/.codex/AGENTS.md 路徑".into()))?;
    deploy_doctrine_to(&source, &vault_path, &claude_md, &codex_agents)
}

/// 可測核心：render 兩目標 + safety（皆在任何寫入前）→ 原子寫入；第二檔失敗交易式回滾第一檔。
fn deploy_doctrine_to(source: &str, vault_path: &str, claude_md: &Path, codex_agents: &Path) -> Result<DeployResult, AppError> {
    let body = extract_doctrine_body(source)?;
    let claude_content = render_global_doctrine(&body, "# CLAUDE.md", vault_path)?;
    let codex_content = render_global_doctrine(&body, "# AGENTS.md", vault_path)?;
    // safety（含 masked snippet，Codex #4）；兩份都過才寫，fail-closed。
    for (name, c) in [("~/.claude/CLAUDE.md", &claude_content), ("~/.codex/AGENTS.md", &codex_content)] {
        let s = safety_filter::check(c);
        if !s.is_safe {
            let hits: Vec<String> = s.hits.iter().map(|h| format!("{}：{}", h.label, h.masked)).collect();
            return Err(AppError::SafetyBlocked(format!(
                "全域 doctrine 疑似含敏感內容（於 {name}），未寫入任何檔案。命中：{}。若為 commit SHA／雜湊等誤判，請修 vault 標準檔（不提供強制略過）。",
                hits.join("；")
            )));
        }
    }
    // 部署提醒（非錯誤）：Codex override 優先、AGENTS.md 逼近 32 KiB 截斷（Codex #7）。
    let mut warnings = Vec::new();
    if let Some(codex_dir) = codex_agents.parent() {
        let ov = codex_dir.join("AGENTS.override.md");
        if ov.is_file() && std::fs::read_to_string(&ov).map(|s| !s.trim().is_empty()).unwrap_or(false) {
            warnings.push("偵測到 ~/.codex/AGENTS.override.md（非空）：Codex 會優先讀它，本次部署的 AGENTS.md 可能不生效。".to_string());
        }
    }
    if codex_content.as_bytes().len() > 24 * 1024 {
        warnings.push(format!("~/.codex/AGENTS.md 部署後約 {} KiB，接近 Codex 預設 32 KiB 上限，可能被截斷。", codex_content.as_bytes().len() / 1024));
    }
    // 原子寫入：claude 先、codex 後；codex 失敗交易式回滾 claude（Codex #2）：
    // 原本存在 → 從 .bak 還原；原本不存在 → 刪除新建檔。回滾失敗誠實回報 partial state。
    let claude_existed = claude_md.exists();
    let b1 = write_global_atomic(claude_md, &claude_content)?;
    let b2 = match write_global_atomic(codex_agents, &codex_content) {
        Ok(b) => b,
        Err(e) => {
            let rolled_back = if claude_existed {
                std::fs::copy(append_suffix(claude_md, "bak"), claude_md).is_ok()
            } else {
                std::fs::remove_file(claude_md).is_ok()
            };
            return Err(AppError::Io(if rolled_back {
                format!("~/.codex/AGENTS.md 寫入失敗（{e}）；已回滾 ~/.claude/CLAUDE.md，請重試「同步全域」。")
            } else {
                format!("~/.codex/AGENTS.md 寫入失敗（{e}），且 ~/.claude/CLAUDE.md 回滾失敗——可能為半部署狀態，請檢查 {} 及其 .predeploy.bak／.bak。", claude_md.to_string_lossy())
            }));
        }
    };
    Ok(DeployResult {
        claude_path: claude_md.to_string_lossy().to_string(),
        codex_path: codex_agents.to_string_lossy().to_string(),
        backup_made: b1 || b2,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_append_to_existing_preserves_content() {
        let existing = "# 天城人格\n忠誠理性\n";
        let block = build_pointer_block("E:\\vault");
        let (out, action) = splice_managed_block(existing, &block);
        assert_eq!(action, "appended");
        assert!(out.starts_with("# 天城人格\n忠誠理性\n"));
        assert!(out.contains("路徑：E:\\vault"));
        assert_eq!(out.matches(BEGIN_MARKER).count(), 1);
    }

    #[test]
    fn test_replace_is_idempotent_and_keeps_persona() {
        let persona = "# 天城人格\n忠誠理性\n";
        let block1 = build_pointer_block("E:\\old");
        let (c1, _) = splice_managed_block(persona, &block1);

        // 換路徑重設：應只替換區塊，不堆疊、不動人格
        let block2 = build_pointer_block("D:\\new");
        let (c2, action) = splice_managed_block(&c1, &block2);
        assert_eq!(action, "replaced");
        assert!(c2.starts_with("# 天城人格\n忠誠理性\n"));
        assert!(c2.contains("路徑：D:\\new"));
        assert!(!c2.contains("路徑：E:\\old"));
        assert_eq!(c2.matches(BEGIN_MARKER).count(), 1);
        assert_eq!(c2.matches(END_MARKER).count(), 1);
    }

    #[test]
    fn test_append_to_empty() {
        let block = build_pointer_block("C:\\v");
        let (out, action) = splice_managed_block("", &block);
        assert_eq!(action, "appended");
        assert!(out.contains("# Amagi-Vault 知識庫"));
    }

    #[test]
    fn test_detect_doctrine_source() {
        // ④ 偵測邏輯的可測核心：源檔存在→true、不存在→false（純函式，不觸全域檔寫入）
        let base = std::env::temp_dir().join(format!("amagi-doctrinesrc-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&base).unwrap();
        assert!(!detect_doctrine_source(&base), "無源檔應為 false");
        let meta = base.join("general").join("_meta");
        std::fs::create_dir_all(&meta).unwrap();
        assert!(!detect_doctrine_source(&base), "只有 _meta 目錄、無檔仍為 false");
        std::fs::write(meta.join("global-agent-config.md"), "# doctrine").unwrap();
        assert!(detect_doctrine_source(&base), "源檔就位應為 true");
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn test_get_vault_status() {
        let base = std::env::temp_dir().join(format!("amagi-vstatus-{}", uuid::Uuid::new_v4()));
        let data_dir = base.join("data");
        std::fs::create_dir_all(&data_dir).unwrap();

        // 未設定 → 需引導
        let st = get_vault_status(&data_dir);
        assert!(!st.configured && !st.is_git_repo && st.vault_path.is_none());

        // 已設定、但非 git repo → 應提示掛 git
        let vault = base.join("vault");
        std::fs::create_dir_all(&vault).unwrap();
        json_store::write_json(
            &config_path(&data_dir),
            &VaultConfig {
                vault_path: Some(vault.to_string_lossy().to_string()),
                pointer_written: true,
            },
        )
        .unwrap();
        let st = get_vault_status(&data_dir);
        assert!(st.configured && !st.is_git_repo);

        // 掛上 git → is_git_repo
        std::fs::create_dir_all(vault.join(".git")).unwrap();
        let st = get_vault_status(&data_dir);
        assert!(st.configured && st.is_git_repo);

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn test_ensure_project_path_outside_vault() {
        // bridge 等寫入型 command 共用閘：vault 已設 → 根/子路徑拒、外部過；未設 → 放行
        let base = std::env::temp_dir().join(format!("amagi-vguard-cfg-{}", uuid::Uuid::new_v4()));
        let data_dir = base.join("data");
        let vault = base.join("vault");
        let proj = base.join("proj");
        std::fs::create_dir_all(&data_dir).unwrap();
        std::fs::create_dir_all(vault.join("projects").join("x")).unwrap();
        std::fs::create_dir_all(&proj).unwrap();
        json_store::write_json(&config_path(&data_dir), &VaultConfig {
            vault_path: Some(vault.to_string_lossy().to_string()),
            pointer_written: true,
        }).unwrap();

        assert!(ensure_project_path_outside_vault(vault.to_str().unwrap(), &data_dir).is_err(), "vault 根應拒");
        assert!(ensure_project_path_outside_vault(
            vault.join("projects").join("x").to_str().unwrap(), &data_dir).is_err(), "vault 子路徑應拒");
        assert!(ensure_project_path_outside_vault(proj.to_str().unwrap(), &data_dir).is_ok(), "vault 外應過");

        // vault 未設定（另一個乾淨 data_dir）→ 放行
        let data2 = base.join("data2");
        std::fs::create_dir_all(&data2).unwrap();
        assert!(ensure_project_path_outside_vault(vault.to_str().unwrap(), &data2).is_ok(), "vault 未設應放行");

        let _ = std::fs::remove_dir_all(&base);
    }

    // ── 步驟5：全域 doctrine 部署 ──────────────────────────────
    #[test]
    fn test_extract_doctrine_body_ok_and_malformed() {
        let src = "---\ntitle: x\n---\n# 說明\n> note\n<!-- AMAGI-DOCTRINE:BEGIN (x) -->\n# 天城人格\n內容一\n<!-- AMAGI-VAULT:BEGIN (Amagi Core 管理，勿手改) -->\n（佔位）\n<!-- AMAGI-VAULT:END -->\n<!-- AMAGI-DOCTRINE:END -->\n";
        let body = extract_doctrine_body(src).unwrap();
        assert!(body.starts_with("# 天城人格"), "本體應從人格 H1 起");
        assert!(body.contains("AMAGI-VAULT:BEGIN"), "本體含 vault 佔位");
        assert!(!body.contains("AMAGI-DOCTRINE"), "本體不含 DOCTRINE 標記行");
        assert!(!body.contains("# 說明"), "說明區不入本體");
        // 畸形 → Err（fail-closed）
        assert!(extract_doctrine_body("# 無標記").is_err(), "無標記應 Err");
        assert!(extract_doctrine_body(&format!("{src}\n<!-- AMAGI-DOCTRINE:BEGIN -->")).is_err(), "重複 begin 應 Err");
    }

    #[test]
    fn test_render_global_doctrine_replaces_h1_and_vault_block() {
        let body = "# 天城人格\n忠誠\n<!-- AMAGI-VAULT:BEGIN (Amagi Core 管理，勿手改) -->\n（佔位）\n<!-- AMAGI-VAULT:END -->";
        let out = render_global_doctrine(body, "# CLAUDE.md", "C:\\v").unwrap();
        assert!(out.starts_with("# CLAUDE.md"), "第一個 H1 應被取代");
        assert!(!out.contains("# 天城人格"), "原 H1 應消失");
        assert!(out.contains("路徑：C:\\v"), "AMAGI-VAULT 佔位應換成真實 pointer block");
        assert!(!out.contains("（佔位）"), "佔位文字應被取代");
        assert_eq!(out.matches(BEGIN_MARKER).count(), 1, "AMAGI-VAULT 應恰好一組");
        assert!(!out.contains("AMAGI-DOCTRINE"), "不應殘留 DOCTRINE 標記");
        // 畸形：本體第一非空行非 H1 → Err
        let bad = "內容非標題\n<!-- AMAGI-VAULT:BEGIN (Amagi Core 管理，勿手改) -->\nx\n<!-- AMAGI-VAULT:END -->";
        assert!(render_global_doctrine(bad, "# CLAUDE.md", "C:\\v").is_err(), "第一非空行非 H1 應 Err");
    }

    #[test]
    fn test_append_suffix_keeps_md() {
        let p = Path::new("C:\\x\\CLAUDE.md");
        assert_eq!(append_suffix(&p, "bak").to_string_lossy().replace('\\', "/"), "C:/x/CLAUDE.md.bak");
    }

    #[test]
    fn test_write_global_atomic_replaces_existing_and_backs_up() {
        // 實測 Codex #1：既有檔能否被 write_global_atomic 原子替換（Windows rename replace-existing）。
        let dir = std::env::temp_dir().join(format!("amagi-wga-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("CLAUDE.md");
        std::fs::write(&target, "原始內容").unwrap();
        let b = write_global_atomic(&target, "新內容 v1").unwrap();
        assert!(b, "既有檔應有備份");
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "新內容 v1", "既有檔應被替換");
        assert_eq!(std::fs::read_to_string(dir.join("CLAUDE.md.predeploy.bak")).unwrap(), "原始內容", "首次原始備份");
        assert_eq!(std::fs::read_to_string(dir.join("CLAUDE.md.bak")).unwrap(), "原始內容", "rolling 備份");
        // 第二次：predeploy 永不覆寫、rolling 更新為前一版、temp 不殘留
        write_global_atomic(&target, "新內容 v2").unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("CLAUDE.md.predeploy.bak")).unwrap(), "原始內容", "predeploy 永不覆寫");
        assert_eq!(std::fs::read_to_string(dir.join("CLAUDE.md.bak")).unwrap(), "新內容 v1", "rolling = 前一版");
        assert!(!dir.join("CLAUDE.md.tmp").exists(), "temp 不殘留");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_write_global_atomic_new_target_no_backup() {
        let dir = std::env::temp_dir().join(format!("amagi-wga2-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("AGENTS.md");
        let b = write_global_atomic(&target, "全新").unwrap();
        assert!(!b, "新檔無備份");
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "全新");
        assert!(!dir.join("AGENTS.md.predeploy.bak").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_deploy_rollback_on_second_target_failure() {
        // Codex #2：第二檔寫入失敗時，第一檔須交易式回滾為原始內容。
        let dir = std::env::temp_dir().join(format!("amagi-deploy-rb-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = "<!-- AMAGI-DOCTRINE:BEGIN -->\n# T\n內容\n<!-- AMAGI-VAULT:BEGIN (Amagi Core 管理，勿手改) -->\n（佔位）\n<!-- AMAGI-VAULT:END -->\n<!-- AMAGI-DOCTRINE:END -->";
        let claude = dir.join("CLAUDE.md");
        std::fs::write(&claude, "原始 claude").unwrap();
        // 第二目標故意設成「目錄」→ write_global_atomic 把既有目標當檔讀取失敗 → 觸發回滾
        let codex_dir = dir.join("AGENTS.md");
        std::fs::create_dir_all(&codex_dir).unwrap();
        let r = deploy_doctrine_to(source, "C:\\v", &claude, &codex_dir);
        assert!(r.is_err(), "第二檔（目錄）寫入失敗應 Err");
        assert_eq!(std::fs::read_to_string(&claude).unwrap(), "原始 claude", "第一檔應交易式回滾為原始");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_deploy_rollback_deletes_new_first_file_when_absent() {
        // Codex R2 Low：第一檔原本不存在、第二檔失敗 → 回滾應刪除新建的第一檔（回到「不存在」）。
        let dir = std::env::temp_dir().join(format!("amagi-deploy-rb2-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = "<!-- AMAGI-DOCTRINE:BEGIN -->\n# T\n內容\n<!-- AMAGI-VAULT:BEGIN (Amagi Core 管理，勿手改) -->\n（佔位）\n<!-- AMAGI-VAULT:END -->\n<!-- AMAGI-DOCTRINE:END -->";
        let claude = dir.join("CLAUDE.md"); // 不預先建立 → 原本不存在
        let codex_dir = dir.join("AGENTS.md");
        std::fs::create_dir_all(&codex_dir).unwrap(); // 第二目標為目錄 → 強制失敗
        let r = deploy_doctrine_to(source, "C:\\v", &claude, &codex_dir);
        assert!(r.is_err(), "第二檔寫入失敗應 Err");
        assert!(!claude.exists(), "第一檔原本不存在 → 回滾應刪除新建檔");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
