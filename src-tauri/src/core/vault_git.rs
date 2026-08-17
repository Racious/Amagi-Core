//! vault 自管 git 同步（adr-008 app 強制層）。
//!
//! 紀律：main 單線＋推前 rebase；衝突一律不代合（A 案：安全退回＋引導），
//! 不 force push、不動 global config、不動 user.name/email。
//! 設計審查：reports/2026-07/2026-07-23-adr008-app-git-sync-design-review.md。

use std::path::{Path, PathBuf};
use crate::AppError;
use crate::utils::proc;

/// vault 提交一律以「あまぎ」為作者（老爺全域 git 規範；committer 仍為全域身分）。
const AUTHOR: &str = "あまぎ <amagi.core@gmail.com>";

/// push race 重試上限：一次＝完整 fetch→rebase→push cycle。
const MAX_PUSH_CYCLES: u32 = 3;

struct GitOutput {
    ok: bool,
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

/// 執行 git 並保留完整輸出與成敗（衝突分類需要自行判讀，不能非零即拋）。
fn run_git_capture(vault_root: &Path, args: &[&str]) -> Result<GitOutput, AppError> {
    let output = proc::command("git")
        .args(args)
        .current_dir(vault_root)
        .output()
        .map_err(|e| AppError::Git(e.to_string()))?;
    Ok(GitOutput {
        ok: output.status.success(),
        code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    })
}

/// 是否位於 git 工作樹內（`rev-parse --is-inside-work-tree`）。
/// 與檔案系統式 `.git` 目錄判斷不同：linked worktree（`.git` 為指標檔）也能正確辨識。
pub fn is_git_work_tree(path: &Path) -> bool {
    run_git_capture(path, &["rev-parse", "--is-inside-work-tree"])
        .map(|o| o.ok && o.stdout.trim() == "true")
        .unwrap_or(false)
}

/// 單一檔案相對於 vault git 的提交狀態（P3 刪除前的復原性判斷用）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileCommitState {
    /// 未被 git 追蹤：刪除後無從復原
    Untracked,
    /// 已追蹤但與 HEAD 有差異：git 只能復原到上一版
    Modified,
    /// 已追蹤且與 HEAD 一致：可從 git 歷史完整復原
    Committed,
}

/// 查單一路徑的提交狀態。
///
/// **刻意不解析 `git status` 輸出**：git 預設 `core.quotepath=true` 會把非 ASCII 檔名
/// 轉義成八進位並加引號（例 `"…/\345\245\227\345\233\236….md"`），而記憶檔名幾乎必然
/// 含中文 → 以原始 UTF-8 路徑做字串比對會**永遠不命中**，把未提交的新檔誤判為「已提交、
/// 可復原」，正好與「保守判斷」的要求相反（2026-08-17 實機驗證抓到，自動化測試未覆蓋）。
/// 改為直接對該路徑發問，讓 git 自己處理路徑編碼。
pub fn file_commit_state(vault_root: &Path, rel_path: &str) -> Result<FileCommitState, AppError> {
    // 是否被追蹤：ls-files --error-unmatch 對未追蹤路徑回非零
    let tracked = run_git_capture(vault_root, &["ls-files", "--error-unmatch", "--", rel_path])?;
    if !tracked.ok {
        return Ok(FileCommitState::Untracked);
    }
    // 與 HEAD 是否一致：diff --quiet 相同回 0、有差異回 1
    let diff = run_git_capture(vault_root, &["diff", "--quiet", "HEAD", "--", rel_path])?;
    if diff.ok {
        Ok(FileCommitState::Committed)
    } else {
        Ok(FileCommitState::Modified)
    }
}

/// 執行 git，非零即拋（沿用原介面，適合不需分類失敗原因的呼叫）。
fn run_git(vault_root: &Path, args: &[&str]) -> Result<String, AppError> {
    let out = run_git_capture(vault_root, args)?;
    if !out.ok {
        return Err(AppError::Git(out.stderr.trim().to_string()));
    }
    Ok(out.stdout)
}

/// 取 git 內部路徑（`rev-parse --git-path`）。不可直拼 `vault_root/.git/<name>`：
/// linked worktree 下 `.git` 是含 gitdir 指標的檔案而非目錄，直拼會失準。
fn git_path(vault_root: &Path, name: &str) -> Result<PathBuf, AppError> {
    let out = run_git(vault_root, &["rev-parse", "--git-path", name])?;
    let p = PathBuf::from(out.trim());
    Ok(if p.is_absolute() { p } else { vault_root.join(p) })
}

/// rebase 中斷態偵測：`rebase-merge`（merge backend）與 `rebase-apply`（apply backend）皆查，
/// 任一存在即視為 rebase 進行中。
fn is_rebase_in_progress(vault_root: &Path) -> bool {
    ["rebase-merge", "rebase-apply"].iter().any(|name| {
        git_path(vault_root, name)
            .map(|p| p.exists())
            .unwrap_or(false)
    })
}

/// 未合併（衝突）檔清單。`-z` NUL 分隔，避免特殊檔名被引號/跳脫干擾。
fn unmerged_files(vault_root: &Path) -> Vec<String> {
    run_git(vault_root, &["diff", "--name-only", "--diff-filter=U", "-z"])
        .map(|s| {
            s.split('\0')
                .filter(|x| !x.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// 冪等設 vault repo 的同步紀律 config（adr-008；僅 `--local`，絕不動 global 或 user 身分）。
/// 非 git repo 時 `git config --local` 會失敗並回錯，由呼叫端決定語意（pull/sync 入口＝擋下）。
pub fn ensure_repo_config(vault_root: &Path) -> Result<(), AppError> {
    run_git(vault_root, &["config", "--local", "pull.rebase", "true"])?;
    run_git(vault_root, &["config", "--local", "rebase.autoStash", "true"])?;
    Ok(())
}

/// 組衝突錯誤：訊息含衝突檔清單；命中 daily/ 附合併引導（daily＝每日共用日誌，app 不代合）。
fn conflict_error(phase: &str, files: Vec<String>, abort_status: &str, base_msg: &str) -> AppError {
    let daily_hint = files.iter().any(|f| f.starts_with("daily/"));
    let mut message = base_msg.to_string();
    if !files.is_empty() {
        message.push_str(&format!("衝突檔：{}。", files.join("、")));
    }
    if daily_hint {
        message.push_str("（daily 為每日共用日誌、各專案各一區塊，通常保留雙方區塊即可。）");
    }
    AppError::GitConflict {
        phase: phase.to_string(),
        files,
        daily_hint,
        abort_status: abort_status.to_string(),
        message,
    }
}

/// rebase 衝突處置（A 案）：先取衝突檔清單→`rebase --abort` 安全退回→結構化錯誤。
/// abort 失敗必須明講「repo 可能停在中斷態」與手動修復指令（fail-safe，不可默默吞掉）。
fn handle_rebase_conflict(vault_root: &Path, phase: &str, commit_kept: bool) -> AppError {
    let files = unmerged_files(vault_root);
    let kept = if commit_kept {
        "本地 commit 保留、尚未推送。"
    } else {
        ""
    };
    match run_git(vault_root, &["rebase", "--abort"]) {
        Ok(_) => conflict_error(
            phase,
            files,
            "aborted",
            &format!("rebase 遇到衝突，已安全退回。{kept}請手動整合後再推。"),
        ),
        Err(_) => conflict_error(
            phase,
            files,
            "abort_failed",
            &format!(
                "rebase 遇到衝突且自動退回失敗，repo 可能停在 rebase 中斷態；\
                 請手動執行 git rebase --abort。{kept}"
            ),
        ),
    }
}

/// 工作區簡短狀態（git status --short）。
pub fn status_short(vault_root: &Path) -> Result<String, AppError> {
    run_git(vault_root, &["status", "--short"])
}

/// 拉取最新：`pull --rebase --autostash`（adr-008；本地有 commit 也能自動疊上）。
///
/// 失敗/異常分類（順序不可反）：
/// 1. rebase 中斷態 → 取衝突檔 → abort 退回 → `GitConflict(pull)`。
/// 2. **exit 0 也要查 unmerged**：autostash 套回衝突時 git 回傳成功（2026-07-24 本機實驗證實），
///    但工作區留下衝突標記、原變更同存 stash——此時不可謊報成功，也**不得** rebase --abort
///    （無 rebase 態可退）→ `GitConflict(pull-autostash)`。
/// 3. 其他失敗原樣回報。
pub fn pull(vault_root: &Path) -> Result<String, AppError> {
    ensure_repo_config(vault_root)?;
    let out = run_git_capture(vault_root, &["pull", "--rebase", "--autostash"])?;
    if !out.ok {
        if is_rebase_in_progress(vault_root) {
            return Err(handle_rebase_conflict(vault_root, "pull", false));
        }
        let files = unmerged_files(vault_root);
        if !files.is_empty() {
            return Err(autostash_conflict_error(files));
        }
        return Err(AppError::Git(out.stderr.trim().to_string()));
    }
    let files = unmerged_files(vault_root);
    if !files.is_empty() {
        return Err(autostash_conflict_error(files));
    }
    let msg = out.stdout.trim();
    Ok(if msg.is_empty() {
        out.stderr.trim().to_string()
    } else {
        msg.to_string()
    })
}

fn autostash_conflict_error(files: Vec<String>) -> AppError {
    conflict_error(
        "pull-autostash",
        files,
        "none",
        "遠端更新已套用，但本地未提交變更套回時發生衝突：檔案已含衝突標記，\
         原變更另存於 stash。請解開標記後 git add 該檔，再 git stash drop 清除暫存。",
    )
}

/// 自管同步（adr-008 顯式整合序）：add -A → commit（作者あまぎ）→
/// ≤3 次 cycle｛fetch → rebase @{u} → push --porcelain｝。
///
/// - rebase 衝突：立即 abort 終止（不吃重試次數），訊息明講 commit 已留在本機。
/// - push 被拒：fetch 後以 `merge-base --is-ancestor @{u} HEAD` 判別——upstream 已非
///   HEAD 祖先＝遠端前進（race）→ 重試；仍是祖先＝其他原因（認證/hook/保護分支）→ 不重試。
///   以祖先關係為客觀依據，不解析 stderr 字串（locale/版本差異風險）。
/// - detached HEAD／無 upstream：明確拒絕，不進迴圈。
pub fn sync(vault_root: &Path, message: &str) -> Result<String, AppError> {
    sync_impl(vault_root, message, &mut |_| {})
}

/// 測試縫（seam）：`between_rebase_and_push` 在每次 cycle 的 rebase 成功後、push 前呼叫，
/// 供整合測試確定性模擬 push race（生產路徑為 no-op）。
fn sync_impl(
    vault_root: &Path,
    message: &str,
    between_rebase_and_push: &mut dyn FnMut(u32),
) -> Result<String, AppError> {
    ensure_repo_config(vault_root)?;

    // detached 判定用 symbolic-ref：unborn branch（空 repo 首 commit 前）HEAD 仍指向分支名
    // 可通過（首 commit 會建立該分支）；真 detached 才失敗。rev-parse HEAD 在 unborn 會誤炸。
    if !run_git_capture(vault_root, &["symbolic-ref", "--quiet", "HEAD"])?.ok {
        return Err(AppError::Git(
            "目前處於 detached HEAD（未在任何分支上），已中止同步；請先切回分支。".into(),
        ));
    }

    // upstream 檢查置於 commit 前：無 upstream 的場景不先產生本地 commit（審查 R1 發現 1）。
    if !run_git_capture(vault_root, &["rev-parse", "--abbrev-ref", "@{u}"])?.ok {
        return Err(AppError::Git(
            "目前分支未設定追蹤分支（upstream），無法自動推送；\
             請先手動執行 git push -u origin <分支>。"
                .into(),
        ));
    }

    let dirty = !status_short(vault_root)?.trim().is_empty();
    if dirty {
        run_git(vault_root, &["add", "-A"])?;
        run_git(
            vault_root,
            &["commit", &format!("--author={AUTHOR}"), "-m", message],
        )?;
    }

    // commit 之後的任何失敗都必須明講「commit 已留在本機」（設計定稿；資料不丟、可重試）。
    let unpushed = |detail: &str| AppError::GitSyncUnpushed {
        phase: "integrate".into(),
        files: vec![],
        message: format!("{}。本地 commit 保留、尚未推送。", detail.trim()),
    };

    let mut cycles = 0u32;
    while cycles < MAX_PUSH_CYCLES {
        cycles += 1;
        let fetch = run_git_capture(vault_root, &["fetch"])?;
        if !fetch.ok {
            return Err(unpushed(&fetch.stderr));
        }

        let reb = run_git_capture(vault_root, &["rebase", "--autostash", "@{u}"])?;
        if !reb.ok {
            if is_rebase_in_progress(vault_root) {
                return Err(handle_rebase_conflict(vault_root, "sync-rebase", true));
            }
            return Err(unpushed(&reb.stderr));
        }

        between_rebase_and_push(cycles);

        let push = run_git_capture(vault_root, &["push", "--porcelain"])?;
        if push.ok {
            let integrated = if cycles > 1 {
                "（已自動整合遠端新變更後推送。）"
            } else {
                ""
            };
            return Ok(if dirty {
                format!("已提交並推送。{integrated}")
            } else {
                format!("無本地變更，已推送既有 commit。{integrated}")
            });
        }

        // push 失敗：fetch 取最新 upstream，客觀判別是否遠端前進（可重試）。
        let refetch = run_git_capture(vault_root, &["fetch"])?;
        if !refetch.ok {
            return Err(unpushed(&refetch.stderr));
        }
        // exit 0＝upstream 仍是 HEAD 祖先；1＝已非祖先（遠端前進）；128+＝引用/repo 錯誤，
        // 不得折成「可重試」（審查 R1 發現 3），原樣回報並帶保留提示。
        let anc = run_git_capture(
            vault_root,
            &["merge-base", "--is-ancestor", "@{u}", "HEAD"],
        )?;
        match anc.code {
            Some(0) => {
                // 遠端沒前進，push 卻失敗＝認證/hook/保護分支等，重試無意義。
                return Err(AppError::GitSyncUnpushed {
                    phase: "push".into(),
                    files: vec![],
                    message: format!(
                        "推送失敗（原因非遠端更新）：{}。本地 commit 保留、尚未推送。",
                        push.stderr.trim()
                    ),
                });
            }
            Some(1) => {
                // 遠端已前進（典型 push race）→ 下一輪 cycle 重新 rebase 再推。
            }
            _ => return Err(unpushed(&anc.stderr)),
        }
    }

    Err(AppError::GitSyncUnpushed {
        phase: "retry-exhausted".into(),
        files: vec![],
        message: format!(
            "遠端持續有新變更，自動整合重試 {MAX_PUSH_CYCLES} 次仍未推上；\
             本地 commit 保留、尚未推送，請稍後再試。"
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    /// 建測試根目錄（每測試獨立）。
    fn tmp_root(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("amagi-vgit-{tag}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// 測試用 git 執行（斷言成功）。
    fn git(dir: &Path, args: &[&str]) -> String {
        let out = proc::command("git").args(args).current_dir(dir).output().unwrap();
        assert!(
            out.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).to_string()
    }

    /// 測試 repo 需要 committer 身分：僅設定臨時 repo 的 local config（非使用者 repo）。
    fn set_identity(dir: &Path) {
        git(dir, &["config", "user.name", "amagi-test"]);
        git(dir, &["config", "user.email", "amagi-test@example.com"]);
    }

    fn write(dir: &Path, rel: &str, content: &str) {
        let p = dir.join(rel);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(p, content).unwrap();
    }

    /// 建 bare 遠端＋種子內容＋兩個 clone（皆已設 upstream 與測試身分）。
    fn setup_two_clones(tag: &str) -> (PathBuf, PathBuf, PathBuf) {
        let root = tmp_root(tag);
        let bare = root.join("bare.git");
        fs::create_dir_all(&bare).unwrap();
        git(&bare, &["init", "--bare"]);

        let a = root.join("a");
        git(&root, &["clone", bare.to_str().unwrap(), "a"]);
        set_identity(&a);
        write(&a, "seed.md", "seed\n");
        write(&a, "daily/2026-07-24.md", "# daily\nseed-line\n");
        git(&a, &["add", "-A"]);
        git(&a, &["commit", "-m", "seed"]);
        git(&a, &["push", "-u", "origin", "HEAD"]);

        let b = root.join("b");
        git(&root, &["clone", bare.to_str().unwrap(), "b"]);
        set_identity(&b);
        (root, a, b)
    }

    #[test]
    fn test_status_on_fresh_repo() {
        let dir = tmp_root("fresh");
        git(&dir, &["init"]);
        assert!(status_short(&dir).is_ok());
        let _ = fs::remove_dir_all(&dir);
    }

    /// (a) config 冪等：重複呼叫不報錯、值正確、僅 local 層。
    #[test]
    fn test_ensure_repo_config_idempotent() {
        let dir = tmp_root("cfg");
        git(&dir, &["init"]);
        ensure_repo_config(&dir).unwrap();
        ensure_repo_config(&dir).unwrap(); // 第二次不得報錯
        assert_eq!(git(&dir, &["config", "--local", "--get", "pull.rebase"]).trim(), "true");
        assert_eq!(git(&dir, &["config", "--local", "--get", "rebase.autoStash"]).trim(), "true");
        let _ = fs::remove_dir_all(&dir);
    }

    /// config 於非 git 目錄應回錯（pull/sync 入口語意＝擋下）。
    #[test]
    fn test_ensure_repo_config_non_repo_errors() {
        let dir = tmp_root("cfg-nonrepo");
        assert!(ensure_repo_config(&dir).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    /// (b) 不重疊變更：遠端已前進、本地有 commit → sync 自動 rebase 後推上，無需人工。
    #[test]
    fn test_sync_auto_rebase_non_overlapping() {
        let (root, a, b) = setup_two_clones("nonoverlap");
        write(&a, "from-a.md", "a\n");
        git(&a, &["add", "-A"]);
        git(&a, &["commit", "-m", "a-change"]);
        git(&a, &["push"]);

        write(&b, "from-b.md", "b\n");
        let msg = sync(&b, "b-change").unwrap();
        assert!(msg.contains("已提交並推送"), "unexpected: {msg}");

        // 遠端應同時含 a-change 與 b-change（rebase 疊上、線性歷史）
        let log = git(&b, &["log", "--oneline", "origin/HEAD"]);
        assert!(log.contains("a-change") && log.contains("b-change"), "log: {log}");
        assert!(status_short(&b).unwrap().trim().is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    /// (c) 同改一檔（daily）衝突：abort 安全退回——commit 保留、工作區乾淨、
    /// 無 rebase 中斷態、錯誤含衝突檔與 daily 提示。
    #[test]
    fn test_sync_conflict_aborts_safely() {
        let (root, a, b) = setup_two_clones("conflict");
        write(&a, "daily/2026-07-24.md", "# daily\nA-machine\n");
        git(&a, &["add", "-A"]);
        git(&a, &["commit", "-m", "a-daily"]);
        git(&a, &["push"]);

        write(&b, "daily/2026-07-24.md", "# daily\nB-machine\n");
        let err = sync(&b, "b-daily").unwrap_err();
        match &err {
            AppError::GitConflict { phase, files, daily_hint, abort_status, message } => {
                assert_eq!(phase, "sync-rebase");
                assert_eq!(abort_status, "aborted");
                assert!(*daily_hint);
                assert!(files.iter().any(|f| f == "daily/2026-07-24.md"), "files: {files:?}");
                assert!(message.contains("本地 commit 保留"), "msg: {message}");
            }
            other => panic!("expected GitConflict, got {other:?}"),
        }
        assert!(!is_rebase_in_progress(&b), "repo 不得停在 rebase 中斷態");
        assert!(status_short(&b).unwrap().trim().is_empty(), "abort 後工作區應乾淨");
        let log = git(&b, &["log", "--oneline"]);
        assert!(log.contains("b-daily"), "本地 commit 應保留：{log}");
        let _ = fs::remove_dir_all(&root);
    }

    /// (d) push race：rebase 後、push 前遠端又被插入 → 自動重試下一輪 cycle 後推上。
    #[test]
    fn test_sync_push_race_retries() {
        let (root, a, b) = setup_two_clones("race");
        write(&b, "from-b.md", "b\n");
        let a_dir = a.clone();
        let msg = sync_impl(&b, "b-change", &mut |cycle| {
            if cycle == 1 {
                // 模擬另一機在 B 的 rebase 與 push 之間插入不重疊 commit
                write(&a_dir, "race-a.md", "race\n");
                git(&a_dir, &["add", "-A"]);
                git(&a_dir, &["commit", "-m", "race-a"]);
                git(&a_dir, &["push"]);
            }
        })
        .unwrap();
        assert!(msg.contains("已自動整合遠端新變更"), "應經歷重試：{msg}");
        let log = git(&b, &["log", "--oneline", "origin/HEAD"]);
        assert!(log.contains("race-a") && log.contains("b-change"), "log: {log}");
        let _ = fs::remove_dir_all(&root);
    }

    /// (e) autostash 套回衝突：pull exit 0 但工作區留衝突標記（2026-07-24 實驗證實）——
    /// 不得謊報成功、不得 rebase --abort；原變更同存 stash。
    #[test]
    fn test_pull_autostash_apply_conflict() {
        let (root, a, b) = setup_two_clones("autostash");
        write(&a, "seed.md", "remote-edit\n");
        git(&a, &["add", "-A"]);
        git(&a, &["commit", "-m", "a-edit"]);
        git(&a, &["push"]);

        write(&b, "seed.md", "local-uncommitted\n"); // 未提交、與遠端同檔衝突
        let err = pull(&b).unwrap_err();
        match &err {
            AppError::GitConflict { phase, files, abort_status, .. } => {
                assert_eq!(phase, "pull-autostash");
                assert_eq!(abort_status, "none");
                assert!(files.iter().any(|f| f == "seed.md"), "files: {files:?}");
            }
            other => panic!("expected GitConflict(pull-autostash), got {other:?}"),
        }
        assert!(!is_rebase_in_progress(&b));
        let stash = git(&b, &["stash", "list"]);
        assert!(stash.contains("autostash"), "原變更應存於 stash：{stash}");
        let _ = fs::remove_dir_all(&root);
    }

    /// (f) 無 upstream：明確拒絕，不進重試迴圈；且**不得先產生本地 commit**（upstream 檢查在 commit 前）。
    #[test]
    fn test_sync_no_upstream_rejected() {
        let dir = tmp_root("noup");
        git(&dir, &["init"]);
        set_identity(&dir);
        write(&dir, "x.md", "x\n");
        let err = sync(&dir, "x").unwrap_err();
        match err {
            AppError::Git(m) => assert!(m.contains("upstream") || m.contains("追蹤分支"), "msg: {m}"),
            other => panic!("expected Git error, got {other:?}"),
        }
        // 檢查失敗須發生在 commit 前：repo 應仍無任何 commit（unborn branch，rev-parse HEAD 失敗）
        let head = run_git_capture(&dir, &["rev-parse", "--verify", "HEAD"]).unwrap();
        assert!(!head.ok, "無 upstream 場景不得先建立本地 commit");
        let _ = fs::remove_dir_all(&dir);
    }

    /// (i) commit 後 fetch 失敗：錯誤必為 GitSyncUnpushed 且明講「commit 已留在本機」；commit 確實保留。
    #[test]
    fn test_sync_post_commit_fetch_failure_mentions_kept_commit() {
        let (root, _a, b) = setup_two_clones("fetchfail");
        write(&b, "from-b.md", "b\n");
        // upstream 追蹤設定仍在（rev-parse @{u} 過），但 remote 路徑改為不存在 → commit 後 fetch 失敗
        git(&b, &["remote", "set-url", "origin", root.join("nonexistent.git").to_str().unwrap()]);
        let err = sync(&b, "b-change").unwrap_err();
        match &err {
            AppError::GitSyncUnpushed { phase, message, .. } => {
                assert_eq!(phase, "integrate");
                assert!(message.contains("本地 commit 保留"), "msg: {message}");
            }
            other => panic!("expected GitSyncUnpushed, got {other:?}"),
        }
        let log = git(&b, &["log", "--oneline"]);
        assert!(log.contains("b-change"), "本地 commit 應保留：{log}");
        let _ = fs::remove_dir_all(&root);
    }

    /// (j) is_git_work_tree：一般 clone、linked worktree 皆 true；非 repo false。
    #[test]
    fn test_is_git_work_tree_covers_linked_worktree() {
        let (root, a, _b) = setup_two_clones("iswt");
        assert!(is_git_work_tree(&a));
        let wt = root.join("wt2");
        git(&a, &["worktree", "add", wt.to_str().unwrap(), "-b", "wt2-branch"]);
        assert!(is_git_work_tree(&wt), "linked worktree 應判定為 git 工作樹");
        let plain = root.join("plain");
        fs::create_dir_all(&plain).unwrap();
        assert!(!is_git_work_tree(&plain));
        let _ = fs::remove_dir_all(&root);
    }

    /// (g) detached HEAD：明確拒絕，避免 commit 成孤兒。
    #[test]
    fn test_sync_detached_head_rejected() {
        let (root, _a, b) = setup_two_clones("detached");
        git(&b, &["checkout", "--detach"]);
        let err = sync(&b, "x").unwrap_err();
        match err {
            AppError::Git(m) => assert!(m.contains("detached"), "msg: {m}"),
            other => panic!("expected Git error, got {other:?}"),
        }
        let _ = fs::remove_dir_all(&root);
    }

    /// (h) git_path：linked worktree 下 `.git` 為指標檔，直拼 `.git/rebase-merge` 會失準；
    /// `rev-parse --git-path` 應解析到主 repo 的 worktrees 目錄下。
    #[test]
    fn test_git_path_resolves_in_linked_worktree() {
        let (root, a, _b) = setup_two_clones("worktree");
        let wt = root.join("wt");
        git(&a, &["worktree", "add", wt.to_str().unwrap(), "-b", "wt-branch"]);
        assert!(wt.join(".git").is_file(), "linked worktree 的 .git 應為檔案（gitdir 指標）");
        let p = git_path(&wt, "rebase-merge").unwrap();
        assert!(
            !p.starts_with(wt.join(".git")),
            "git_path 不得直拼工作區 .git：{}",
            p.display()
        );
        assert!(!is_rebase_in_progress(&wt));
        let _ = fs::remove_dir_all(&root);
    }

    /// (i) file_commit_state 三態——**檔名刻意含中文**：
    /// 這是 2026-08-17 實機驗證抓到的 bug 的迴歸測試。原實作解析 `git status --porcelain`
    /// 字串比對路徑，但 core.quotepath 預設把非 ASCII 檔名轉義成八進位並加引號，
    /// 導致中文檔名永遠不命中 → 未提交的新檔被誤判為「已提交、可從 git 復原」。
    #[test]
    fn test_file_commit_state_three_states_with_cjk_filename() {
        let (root, a, _b) = setup_two_clones("commitstate");
        let cjk = "shared/agent/memory/測試記憶-中文檔名-abcd1234.md";

        // ① 未追蹤：剛寫出、尚未 add
        write(&a, cjk, "---\ntitle: 測試\n---\n內容\n");
        assert_eq!(file_commit_state(&a, cjk).unwrap(), FileCommitState::Untracked,
            "新寫入且未 add 的中文檔名須判為 Untracked（原 bug 誤判為已提交）");

        // ② 已提交且乾淨
        git(&a, &["add", "-A"]);
        git(&a, &["commit", "-m", "add cjk memory"]);
        assert_eq!(file_commit_state(&a, cjk).unwrap(), FileCommitState::Committed);

        // ③ 已追蹤但有未提交變更
        write(&a, cjk, "---\ntitle: 測試\n---\n改過的內容\n");
        assert_eq!(file_commit_state(&a, cjk).unwrap(), FileCommitState::Modified);

        // 對照：純 ASCII 檔名三態亦正確（確保修法沒有只對中文生效）
        let ascii = "shared/agent/memory/ascii-memo-beef5678.md";
        write(&a, ascii, "x\n");
        assert_eq!(file_commit_state(&a, ascii).unwrap(), FileCommitState::Untracked);
        git(&a, &["add", "-A"]);
        git(&a, &["commit", "-m", "add ascii"]);
        assert_eq!(file_commit_state(&a, ascii).unwrap(), FileCommitState::Committed);

        // 完全不存在的路徑：視為未追蹤（保守，不宣稱可復原）
        assert_eq!(
            file_commit_state(&a, "shared/agent/memory/不存在-00000000.md").unwrap(),
            FileCommitState::Untracked);

        let _ = fs::remove_dir_all(&root);
    }
}
