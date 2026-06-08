use crate::AppError;
use crate::models::sync::ScanResult;

const ALLOWED_ARGS: &[&[&str]] = &[
    &["status", "--short"],
    &["status", "--porcelain"],
    &["diff", "--stat"],
    &["diff"],
    &["log", "-5", "--oneline"],
    &["rev-parse", "--show-toplevel"],
    &["rev-parse", "HEAD"],
    &["branch", "--show-current"],
];

const MAX_DIFF_BYTES: usize = 512 * 1024;

fn is_allowed(args: &[&str]) -> bool {
    ALLOWED_ARGS.iter().any(|allowed| *allowed == args)
}

fn run_git(project_path: &str, args: &[&str]) -> Result<String, AppError> {
    if !is_allowed(args) {
        return Err(AppError::CommandNotAllowed(format!("git {}", args.join(" "))));
    }
    let output = crate::utils::proc::command("git")
        .args(args)
        .current_dir(project_path)
        .output()
        .map_err(|e| AppError::Git(e.to_string()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(AppError::Git(stderr));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

pub fn scan(project_id: &str, project_path: &str) -> Result<ScanResult, AppError> {
    let branch = run_git(project_path, &["branch", "--show-current"])
        .unwrap_or_default()
        .trim()
        .to_string();

    let status_short = run_git(project_path, &["status", "--short"])
        .unwrap_or_default();

    let diff_stat = run_git(project_path, &["diff", "--stat"])
        .unwrap_or_default();

    let diff_text_raw = run_git(project_path, &["diff"])
        .unwrap_or_default();

    let diff_text = if diff_text_raw.len() > MAX_DIFF_BYTES {
        format!(
            "{}\n\n... output truncated ({} bytes total)",
            &diff_text_raw[..MAX_DIFF_BYTES],
            diff_text_raw.len()
        )
    } else {
        diff_text_raw
    };

    let recent_log = run_git(project_path, &["log", "-5", "--oneline"])
        .unwrap_or_default();

    let changed_files = parse_changed_files(&diff_stat);

    Ok(ScanResult {
        project_id: project_id.to_string(),
        branch,
        status_short,
        diff_stat,
        diff_text,
        recent_log,
        changed_files,
    })
}

fn parse_changed_files(diff_stat: &str) -> Vec<String> {
    diff_stat
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.contains('|') {
                Some(trimmed.split('|').next()?.trim().to_string())
            } else {
                None
            }
        })
        .collect()
}

// ── 差異匯出用：唯讀查詢 ───────────────────────────────────

/// `git status --porcelain` 原始輸出（供解析異動檔清單）
pub fn status_porcelain(project_path: &str) -> Result<String, AppError> {
    run_git(project_path, &["status", "--porcelain"])
}

/// 取得 HEAD 的完整 SHA；若無任何 commit 則回傳 None
pub fn head_sha(project_path: &str) -> Option<String> {
    run_git(project_path, &["rev-parse", "HEAD"])
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// 驗證相對路徑安全（防跳脫、旗標注入、絕對路徑）
pub fn validate_rel_path(p: &str) -> Result<(), AppError> {
    let unsafe_path = p.is_empty()
        || p.starts_with('-')          // 旗標注入
        || p.starts_with('/')          // 絕對路徑
        || p.starts_with('\\')
        || p.contains(':')             // 磁碟機代號 C:\ 等
        || p.split(['/', '\\']).any(|c| c == ".."); // 目錄跳脫
    if unsafe_path {
        return Err(AppError::CommandNotAllowed(format!("不安全的路徑：{}", p)));
    }
    Ok(())
}

/// 對單一（已驗證的）相對路徑取 `git diff HEAD -- <path>`。
/// 僅建構 `diff HEAD --` + 經驗證的路徑，維持唯讀、不動 index。
pub fn diff_one(project_path: &str, rel_path: &str) -> Result<String, AppError> {
    validate_rel_path(rel_path)?;
    let args = ["diff", "HEAD", "--", rel_path];
    let output = crate::utils::proc::command("git")
        .args(args)
        .current_dir(project_path)
        .output()
        .map_err(|e| AppError::Git(e.to_string()))?;
    // 注意：diff 在「有差異」時 exit code 仍為 0；僅在真正錯誤才非 0。
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(AppError::Git(stderr));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_rel_path() {
        assert!(validate_rel_path("src/stores/reader.ts").is_ok());
        assert!(validate_rel_path("../etc/passwd").is_err());   // 跳脫
        assert!(validate_rel_path("/etc/passwd").is_err());     // 絕對
        assert!(validate_rel_path("C:\\Windows").is_err());     // 磁碟機
        assert!(validate_rel_path("--output=x").is_err());      // 旗標注入
        assert!(validate_rel_path("a/../../b").is_err());       // 中段跳脫
    }

    #[test]
    fn test_allowlist_permits_porcelain_and_head() {
        assert!(is_allowed(&["status", "--porcelain"]));
        assert!(is_allowed(&["rev-parse", "HEAD"]));
    }

    #[test]
    fn test_allowlist_rejects_commit() {
        assert!(!is_allowed(&["commit", "-m", "test"]));
    }

    #[test]
    fn test_allowlist_rejects_push() {
        assert!(!is_allowed(&["push", "origin", "main"]));
    }

    #[test]
    fn test_allowlist_rejects_reset() {
        assert!(!is_allowed(&["reset", "--hard"]));
    }

    #[test]
    fn test_allowlist_rejects_clean() {
        assert!(!is_allowed(&["clean", "-fd"]));
    }

    #[test]
    fn test_allowlist_permits_status() {
        assert!(is_allowed(&["status", "--short"]));
    }

    #[test]
    fn test_allowlist_permits_diff() {
        assert!(is_allowed(&["diff"]));
    }

    #[test]
    fn test_allowlist_permits_log() {
        assert!(is_allowed(&["log", "-5", "--oneline"]));
    }
}
