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
}

fn config_path(data_dir: &Path) -> PathBuf {
    data_dir.join("vault.json")
}

pub fn get_vault_config(data_dir: &Path) -> VaultConfig {
    json_store::read_json_or_default(&config_path(data_dir))
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
        let labels: Vec<String> = safety.hits.iter().map(|h| h.label.clone()).collect();
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

    Ok(VaultSetResult {
        vault_path: path.to_string(),
        looks_like_vault,
        claude_md_path: claude_md.to_string_lossy().to_string(),
        backup_made,
        pointer_action,
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
    s.push_str("對話開始時讀取該路徑 index.md 與最近 3 份 daily/，規則依該路徑 CLAUDE.md。\n\n");
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
        let labels: Vec<String> = safety.hits.iter().map(|h| h.label.clone()).collect();
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
}
