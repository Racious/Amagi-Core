use std::process::Command;
use crate::AppError;
use crate::models::sync::ScanResult;

const ALLOWED_ARGS: &[&[&str]] = &[
    &["status", "--short"],
    &["diff", "--stat"],
    &["diff"],
    &["log", "-5", "--oneline"],
    &["rev-parse", "--show-toplevel"],
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
    let output = Command::new("git")
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

#[cfg(test)]
mod tests {
    use super::*;

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
