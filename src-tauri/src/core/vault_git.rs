use std::path::Path;
use crate::AppError;
use crate::utils::proc;

/// vault 提交一律以「あまぎ」為作者（老爺全域 git 規範；committer 仍為全域身分）。
const AUTHOR: &str = "あまぎ <amagi.core@gmail.com>";

fn run_git(vault_root: &Path, args: &[&str]) -> Result<String, AppError> {
    let output = proc::command("git")
        .args(args)
        .current_dir(vault_root)
        .output()
        .map_err(|e| AppError::Git(e.to_string()))?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::Git(err.trim().to_string()));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// 工作區簡短狀態（git status --short）。
pub fn status_short(vault_root: &Path) -> Result<String, AppError> {
    run_git(vault_root, &["status", "--short"])
}

/// 拉取最新（fast-forward only，避免自動合併產生意外 merge commit）。
pub fn pull(vault_root: &Path) -> Result<String, AppError> {
    run_git(vault_root, &["pull", "--ff-only"])
}

/// 自管同步：add -A → commit（作者 あまぎ）→ push。
/// 無本地變更時略過 commit，仍嘗試推送既有未推送 commit。
pub fn sync(vault_root: &Path, message: &str) -> Result<String, AppError> {
    let dirty = !status_short(vault_root)?.trim().is_empty();
    if dirty {
        run_git(vault_root, &["add", "-A"])?;
        run_git(
            vault_root,
            &["commit", &format!("--author={AUTHOR}"), "-m", message],
        )?;
    }
    run_git(vault_root, &["push"])?;
    Ok(if dirty {
        "已提交並推送。".to_string()
    } else {
        "無本地變更，已推送既有 commit。".to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_on_fresh_repo() {
        let dir = std::env::temp_dir().join(format!("amagi-vgit-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let _ = proc::command("git").args(["init"]).current_dir(&dir).output();
        // 新建空 repo 的 status 應可成功取得（內容為空）
        assert!(status_short(&dir).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
