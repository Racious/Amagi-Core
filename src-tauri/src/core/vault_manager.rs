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

fn build_pointer_block(vault_path: &str) -> String {
    format!(
        "{BEGIN_MARKER}\n# Amagi-Vault 知識庫\n路徑：{vault_path}\n對話開始時讀取該路徑 index.md 與最近 3 份 daily/，規則依該路徑 CLAUDE.md。\n{END_MARKER}"
    )
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
}
