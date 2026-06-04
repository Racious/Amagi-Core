//! 衝突／危險模式偵測。
//!
//! 掃描記憶、技能、設定等內容，標出「違反老爺全域規則」或「危險操作」的字樣。
//! 與 safety_filter 互補：safety_filter 抓「機密」，conflict_filter 抓「規則違反／危險指令」。
//!
//! 規則來源：老爺全域 CLAUDE.md 的禁止事項 + 通用危險 git 操作。

use once_cell::sync::Lazy;
use regex::Regex;

/// 偵測規則：(正規表達式, 人話理由)
static CONFLICT_PATTERNS: Lazy<Vec<(Regex, &'static str)>> = Lazy::new(|| {
    vec![
        // ── 老爺全域規則：git 作者相關 ────────────────────
        (
            Regex::new(r"(?i)git\s+config\s+--local\s+user\.(name|email)").unwrap(),
            "禁止設定 git config --local user.name/email（應改用 --author 旗標，否則污染 repo 作者、影響 Sourcetree）",
        ),
        (
            Regex::new(r"(?i)git\s+config\s+--global\s+user\.(name|email)").unwrap(),
            "禁止動 git config --global user.name/email（會影響老爺整台機器的提交身分）",
        ),
        (
            Regex::new(r"(?i)co-authored-by").unwrap(),
            "全域規則禁止加 Co-Authored-By 行",
        ),
        // ── 危險 git 操作 ─────────────────────────────────
        (
            Regex::new(r"(?i)--no-verify").unwrap(),
            "危險：--no-verify 會跳過 git hook 檢查",
        ),
        (
            Regex::new(r"(?i)git\s+push\s+(--force|-f)(\s|$)").unwrap(),
            "危險：git push --force 強制覆寫遠端歷史",
        ),
        (
            Regex::new(r"(?i)git\s+reset\s+--hard").unwrap(),
            "危險：git reset --hard 會丟棄未提交變更",
        ),
        (
            Regex::new(r"(?i)git\s+clean\s+-[a-z]*f").unwrap(),
            "危險：git clean -f 會刪除未追蹤檔案",
        ),
    ]
});

/// 單一衝突命中
#[derive(Debug, Clone)]
pub struct Conflict {
    /// 人話理由
    pub reason: String,
    /// 命中的原始片段
    pub matched: String,
}

/// 偵測結果
#[derive(Debug, Clone)]
pub struct ConflictResult {
    pub has_conflict: bool,
    pub conflicts: Vec<Conflict>,
}

/// 掃描內容，回傳所有命中的衝突／危險模式
pub fn check(text: &str) -> ConflictResult {
    let mut conflicts = Vec::new();
    for (pattern, reason) in CONFLICT_PATTERNS.iter() {
        if let Some(m) = pattern.find(text) {
            conflicts.push(Conflict {
                reason: reason.to_string(),
                matched: m.as_str().to_string(),
            });
        }
    }
    ConflictResult {
        has_conflict: !conflicts.is_empty(),
        conflicts,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 這是 Gomoku 記憶當初真的記錯的內容——必須抓得到
    #[test]
    fn test_detects_real_gomoku_git_config_bug() {
        let bad = r#"
            確認 local git config 已設定正確（git config --local）。
            設定指令：
            git config --local user.name "あまぎ"
            git config --local user.email "amagi.core@gmail.com"
        "#;
        let result = check(bad);
        assert!(result.has_conflict);
        assert!(result.conflicts.iter().any(|c| c.reason.contains("--local")));
    }

    #[test]
    fn test_detects_global_config() {
        let result = check("git config --global user.email amagi.core@gmail.com");
        assert!(result.has_conflict);
    }

    #[test]
    fn test_detects_co_authored_by() {
        let result = check("Co-Authored-By: Someone <x@y.com>");
        assert!(result.has_conflict);
    }

    #[test]
    fn test_detects_dangerous_git() {
        assert!(check("git push --force origin main").has_conflict);
        assert!(check("git reset --hard HEAD~3").has_conflict);
        assert!(check("git clean -fd").has_conflict);
        assert!(check("git commit --no-verify -m x").has_conflict);
    }

    /// 正確做法不該被誤判
    #[test]
    fn test_correct_author_usage_is_clean() {
        let good = r#"
            commit 時用 --author 旗標指定，不動任何 config。
            git commit --author="あまぎ <amagi.core@gmail.com>" -m "訊息"
        "#;
        let result = check(good);
        assert!(!result.has_conflict, "正確的 --author 用法不該被標衝突");
    }

    #[test]
    fn test_normal_content_is_clean() {
        let result = check("在 gameStore.ts 新增 undo() action，撤回上一步棋。");
        assert!(!result.has_conflict);
    }
}
