//! 差異匯出：列出異動檔、對勾選檔產生 diff 文字（JetBrains 風 patch 格式）。
//!
//! 框1（Edited）：修改／改名 → 局部 unified diff（`git diff HEAD -- <path>`）。
//! 框2（AddedDeleted）：新增／刪除 → 整檔。
//!   - 刪除：git diff 整排 `-`。
//!   - 新增（未追蹤）：**自合成**（讀檔、整排 `+`），不呼叫 git、不動 index。

use std::collections::HashMap;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::AppError;
use crate::core::git_scanner;
use crate::models::diff::{ChangedFile, ChangedStatus, DiffBundle, DiffGroup};

/// 每個框的輸出總量上限（位元組）
const MAX_TOTAL_BYTES: usize = 512 * 1024;
/// 單一新檔自合成內容上限
const MAX_NEW_FILE_BYTES: u64 = 256 * 1024;

// ── 對外：列出異動檔 ───────────────────────────────────────
pub fn list_changed_files(project_path: &str) -> Result<Vec<ChangedFile>, AppError> {
    let raw = git_scanner::status_porcelain(project_path)?;
    Ok(parse_porcelain(&raw))
}

// ── 對外：對勾選檔產生 diff ────────────────────────────────
pub fn generate_diff_text(project_path: &str, paths: &[String]) -> Result<DiffBundle, AppError> {
    let listed = parse_porcelain(&git_scanner::status_porcelain(project_path)?);
    let status_map: HashMap<&str, &ChangedFile> =
        listed.iter().map(|c| (c.path.as_str(), c)).collect();

    let sha = git_scanner::head_sha(project_path).unwrap_or_else(|| "0".repeat(40));
    let ms = current_millis();

    let mut edited = String::new();
    let mut added_deleted = String::new();
    let mut skipped: Vec<String> = Vec::new();

    for p in paths {
        // 安全：路徑須合法，且必須是 git status 實際回報的檔（防任意路徑）
        git_scanner::validate_rel_path(p)?;
        let cf = match status_map.get(p.as_str()) {
            Some(c) => *c,
            None => continue, // 不在異動清單中 → 不處理
        };

        match cf.status {
            ChangedStatus::Modified | ChangedStatus::Renamed => {
                let body = git_scanner::diff_one(project_path, p)?;
                if body.trim().is_empty() {
                    continue;
                }
                push_block(&mut edited, &wrap_index(p, &rewrite_headers(&body, &sha, ms)));
            }
            ChangedStatus::Deleted => {
                let body = git_scanner::diff_one(project_path, p)?;
                if body.trim().is_empty() {
                    continue;
                }
                push_block(&mut added_deleted, &wrap_index(p, &rewrite_headers(&body, &sha, ms)));
            }
            ChangedStatus::Added | ChangedStatus::Untracked => {
                match synthesize_new_file(project_path, p, ms) {
                    Synth::Patch(s) => push_block(&mut added_deleted, &wrap_index(p, &s)),
                    Synth::Skip(reason) => skipped.push(format!("{}（{}）", p, reason)),
                }
            }
        }
    }

    let (edited_patch, t1) = cap(edited);
    let (added_deleted_patch, t2) = cap(added_deleted);

    Ok(DiffBundle {
        edited_patch: edited_patch.trim_end().to_string(),
        added_deleted_patch: added_deleted_patch.trim_end().to_string(),
        skipped,
        truncated: t1 || t2,
    })
}

// ── 解析 git status --porcelain ────────────────────────────
fn parse_porcelain(raw: &str) -> Vec<ChangedFile> {
    let mut out = Vec::new();
    for line in raw.lines() {
        if line.len() < 3 {
            continue;
        }
        let bytes = line.as_bytes();
        let x = bytes[0] as char;
        let y = bytes[1] as char;
        let rest = line[3..].trim_end();

        let (status, group) = match classify(x, y) {
            Some(v) => v,
            None => continue, // 忽略（如 !! ignored）
        };

        // 改名格式：`old -> new`，取 new（現存檔）
        let path_raw = if let Some(idx) = rest.find(" -> ") {
            &rest[idx + 4..]
        } else {
            rest
        };
        let path = unquote(path_raw);

        let staged = x != ' ' && x != '?';
        out.push(ChangedFile { path, status, group, staged });
    }
    out
}

/// 依 XY 碼判定狀態與分組（優先序：未追蹤→改名→刪除→新增→修改）
fn classify(x: char, y: char) -> Option<(ChangedStatus, DiffGroup)> {
    if x == '?' && y == '?' {
        return Some((ChangedStatus::Untracked, DiffGroup::AddedDeleted));
    }
    if x == '!' && y == '!' {
        return None; // ignored
    }
    let has = |c: char| x == c || y == c;
    if has('R') {
        Some((ChangedStatus::Renamed, DiffGroup::Edited))
    } else if has('D') {
        Some((ChangedStatus::Deleted, DiffGroup::AddedDeleted))
    } else if has('A') {
        Some((ChangedStatus::Added, DiffGroup::AddedDeleted))
    } else if has('M') || has('T') {
        Some((ChangedStatus::Modified, DiffGroup::Edited))
    } else {
        None
    }
}

/// 去除 git 對特殊字元路徑的雙引號包裹（最小處理）
fn unquote(p: &str) -> String {
    if p.len() >= 2 && p.starts_with('"') && p.ends_with('"') {
        p[1..p.len() - 1].to_string()
    } else {
        p.to_string()
    }
}

// ── 格式化 ─────────────────────────────────────────────────

/// 包上 JetBrains 風的 `Index:` 表頭與分隔線
fn wrap_index(path: &str, body: &str) -> String {
    let sep = "=".repeat(67);
    let body = if body.ends_with('\n') { body.to_string() } else { format!("{}\n", body) };
    format!("Index: {}\n{}\n{}", path, sep, body)
}

/// 將 git diff 的 `--- a/..` / `+++ b/..` 兩行補上 `(revision SHA)` / `(date ms)`
fn rewrite_headers(body: &str, sha: &str, ms: u128) -> String {
    let ended_nl = body.ends_with('\n');
    let mut lines: Vec<String> = Vec::new();
    for line in body.split('\n') {
        if line.starts_with("--- ") && line != "--- /dev/null" {
            lines.push(format!("{}\t(revision {})", line, sha));
        } else if line.starts_with("+++ ") && line != "+++ /dev/null" {
            lines.push(format!("{}\t(date {})", line, ms));
        } else {
            lines.push(line.to_string());
        }
    }
    let mut joined = lines.join("\n");
    if ended_nl && !joined.ends_with('\n') {
        joined.push('\n');
    }
    joined
}

enum Synth {
    Patch(String),
    Skip(String),
}

/// 自合成新檔的「整排 +」patch（不呼叫 git）
fn synthesize_new_file(project_path: &str, rel_path: &str, ms: u128) -> Synth {
    let full = Path::new(project_path).join(rel_path);
    // 防呆：路徑為目錄時（理論上 -uall 已展開，仍保險）明確標示，不誤報「讀取失敗」
    if full.is_dir() {
        return Synth::Skip("目錄（未展開）".into());
    }
    let bytes = match std::fs::read(&full) {
        Ok(b) => b,
        Err(_) => return Synth::Skip("讀取失敗".into()),
    };
    if bytes.len() as u64 > MAX_NEW_FILE_BYTES {
        return Synth::Skip(format!("過大 {} bytes", bytes.len()));
    }
    if is_binary(&bytes) {
        return Synth::Skip(format!("二進位 {} bytes", bytes.len()));
    }

    let content = String::from_utf8_lossy(&bytes);
    // 計算行數（內容以 \n 結尾時，最後的空段不計）
    let mut lines: Vec<&str> = content.split('\n').collect();
    if content.ends_with('\n') {
        lines.pop();
    }
    let n = lines.len();

    let mut s = String::new();
    s.push_str("--- /dev/null\n");
    s.push_str(&format!("+++ b/{}\t(date {})\n", rel_path, ms));
    if n == 0 {
        s.push_str("@@ -0,0 +0,0 @@\n");
    } else {
        s.push_str(&format!("@@ -0,0 +1,{} @@\n", n));
        for l in lines {
            s.push('+');
            s.push_str(l);
            s.push('\n');
        }
    }
    Synth::Patch(s)
}

fn is_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8000).any(|&b| b == 0)
}

fn push_block(buf: &mut String, block: &str) {
    if !buf.is_empty() {
        buf.push('\n');
    }
    buf.push_str(block);
}

fn cap(s: String) -> (String, bool) {
    if s.len() > MAX_TOTAL_BYTES {
        let mut cut = MAX_TOTAL_BYTES;
        // 不切在多位元組字元中間
        while cut > 0 && !s.is_char_boundary(cut) {
            cut -= 1;
        }
        (format!("{}\n\n... 內容過長已截斷（共 {} bytes）", &s[..cut], s.len()), true)
    } else {
        (s, false)
    }
}

fn current_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_porcelain_groups() {
        let raw = " M src/a.ts\nA  src/b.ts\n D src/c.ts\nR  old.ts -> new.ts\n?? src/d.vue\n!! ignored.log\n";
        let files = parse_porcelain(raw);
        let find = |p: &str| files.iter().find(|f| f.path == p).unwrap();

        assert_eq!(files.len(), 5); // ignored 被排除
        assert_eq!(find("src/a.ts").status, ChangedStatus::Modified);
        assert_eq!(find("src/a.ts").group, DiffGroup::Edited);
        assert_eq!(find("src/b.ts").status, ChangedStatus::Added);
        assert_eq!(find("src/b.ts").group, DiffGroup::AddedDeleted);
        assert_eq!(find("src/c.ts").status, ChangedStatus::Deleted);
        assert_eq!(find("new.ts").status, ChangedStatus::Renamed); // 取改名後的新名
        assert_eq!(find("new.ts").group, DiffGroup::Edited);
        assert_eq!(find("src/d.vue").status, ChangedStatus::Untracked);
        assert_eq!(find("src/d.vue").group, DiffGroup::AddedDeleted);
    }

    #[test]
    fn test_staged_flag() {
        let files = parse_porcelain("M  staged.ts\n M unstaged.ts\n?? new.ts\n");
        let find = |p: &str| files.iter().find(|f| f.path == p).unwrap();
        assert!(find("staged.ts").staged);
        assert!(!find("unstaged.ts").staged);
        assert!(!find("new.ts").staged);
    }

    #[test]
    fn test_rewrite_headers() {
        let body = "diff --git a/x.ts b/x.ts\n--- a/x.ts\n+++ b/x.ts\n@@ -1 +1 @@\n-old\n+new\n";
        let out = rewrite_headers(body, "abc123", 1700000000000);
        assert!(out.contains("--- a/x.ts\t(revision abc123)"));
        assert!(out.contains("+++ b/x.ts\t(date 1700000000000)"));
        // 內文 hunk 保留
        assert!(out.contains("-old"));
        assert!(out.contains("+new"));
    }

    #[test]
    fn test_rewrite_headers_skips_dev_null() {
        // 刪除檔的 +++ /dev/null 不應被加 (date)
        let body = "--- a/gone.ts\n+++ /dev/null\n@@ -1 +0,0 @@\n-bye\n";
        let out = rewrite_headers(body, "sha", 123);
        assert!(out.contains("--- a/gone.ts\t(revision sha)"));
        assert!(out.contains("+++ /dev/null\n"));
        assert!(!out.contains("/dev/null\t(date"));
    }

    #[test]
    fn test_synthesize_new_file_text() {
        let dir = std::env::temp_dir().join(format!("amagi-synth-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("n.ts"), "line1\nline2\n").unwrap();

        match synthesize_new_file(dir.to_str().unwrap(), "n.ts", 999) {
            Synth::Patch(s) => {
                assert!(s.starts_with("--- /dev/null\n"));
                assert!(s.contains("+++ b/n.ts\t(date 999)"));
                assert!(s.contains("@@ -0,0 +1,2 @@"));
                assert!(s.contains("+line1"));
                assert!(s.contains("+line2"));
            }
            Synth::Skip(r) => panic!("不該略過：{}", r),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_synthesize_skips_binary() {
        let dir = std::env::temp_dir().join(format!("amagi-bin-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("b.png"), [0u8, 1, 2, 0, 255]).unwrap();

        match synthesize_new_file(dir.to_str().unwrap(), "b.png", 1) {
            Synth::Skip(r) => assert!(r.contains("二進位")),
            Synth::Patch(_) => panic!("二進位應被略過"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_wrap_index() {
        let out = wrap_index("src/a.ts", "body\n");
        assert!(out.starts_with("Index: src/a.ts\n"));
        assert!(out.contains(&"=".repeat(67)));
        assert!(out.ends_with("body\n"));
    }
}
