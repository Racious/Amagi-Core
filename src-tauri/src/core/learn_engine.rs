use chrono::Utc;
use uuid::Uuid;
use crate::models::review::{ReviewItem, ReviewItemType, RiskLevel, ReviewStatus, SyncScope};
use crate::core::safety_filter;

pub fn generate_candidates(
    project_id: &str,
    changed_files: &[String],
    diff_stat: &str,
    diff_text: &str,
) -> Vec<ReviewItem> {
    let mut candidates = Vec::new();

    // 偵測疑似機密：不再「一次擋全部」，而是加一筆「待確認」封鎖項
    //（帶規則名與遮罩片段供判斷），其餘正常候選照常產生。
    // 依檔切段逐檔檢查：卡片列出命中檔案路徑，使用者可直接定位修正；
    // 且同一規則在多個檔案命中會各自列出（整份掃描時每規則只報第一筆）。
    let file_hits = scan_sensitive_by_file(diff_text);
    if !file_hits.is_empty() {
        candidates.push(blocked_item(project_id, &file_hits));
    }

    let has_readme = changed_files.iter().any(|f| {
        let lower = f.to_lowercase();
        lower == "readme.md" || lower.ends_with("/readme.md")
    });
    let readme_lines = count_added_lines(diff_text, "README.md");
    if has_readme && readme_lines > 10 {
        candidates.push(make_memory(
            project_id,
            "project_rule",
            "README 文件定位規則",
            "此專案的 README 有大幅度更新，建議保存文件定位說明或 README 寫作規範。",
            RiskLevel::Low,
            vec!["AGENTS.md".into(), "CLAUDE.md".into()],
        ));
    }

    let has_package = changed_files.iter().any(|f| {
        matches!(
            f.to_lowercase().as_str(),
            "package.json" | "cargo.toml" | "pom.xml" | "build.gradle" | "pyproject.toml"
        )
    });
    if has_package {
        candidates.push(make_memory(
            project_id,
            "tech_stack",
            "技術棧或建構指令更新",
            "專案依賴設定檔有變更，建議更新技術棧說明或常用建構指令記憶。",
            RiskLevel::Low,
            vec!["AGENTS.md".into()],
        ));
    }

    let has_workflow = changed_files.iter().any(|f| {
        f.contains(".github/workflows") || f.contains(".gitlab-ci")
    });
    if has_workflow {
        candidates.push(make_memory(
            project_id,
            "ci_cd_workflow",
            "CI/CD 流程更新",
            "CI/CD 工作流程設定有變更，建議保存 release 或部署流程記憶。",
            RiskLevel::Medium,
            vec!["AGENTS.md".into()],
        ));
    }

    let has_tauri_conf = changed_files.iter().any(|f| {
        f.to_lowercase().contains("tauri.conf")
    });
    if has_tauri_conf {
        candidates.push(make_memory(
            project_id,
            "tauri_config",
            "Tauri 設定更新",
            "tauri.conf.json 有變更，建議保存 Tauri 設定注意事項或 release 前檢查流程。",
            RiskLevel::Medium,
            vec!["AGENTS.md".into()],
        ));
        candidates.push(make_skill(
            project_id,
            "tauri-release-checklist",
            "建立 Tauri release 前檢查流程技能，確保每次發版前完成必要步驟。",
        ));
    }

    let has_agents_md = changed_files.iter().any(|f| {
        let lower = f.to_lowercase();
        lower == "agents.md" || lower == "claude.md"
    });
    if has_agents_md {
        candidates.push(make_memory(
            project_id,
            "agent_rule",
            "Agent 規則更新",
            "AGENTS.md 或 CLAUDE.md 有變更，建議同步更新全域或專案 Agent 規則。",
            RiskLevel::Medium,
            vec!["AGENTS.md".into(), "CLAUDE.md".into()],
        ));
    }

    let _ = diff_stat;
    candidates
}

/// 規則式候選去重：同一 diff 重按「學習變更」不得重複入列（冪等）。
/// 指紋＝(project_id, item_type, category, title, content) 完整相等——規則式候選內容確定性，
/// Blocked 項內容含遮罩片段，diff 不同→指紋不同→視為新項合法入列。
/// 僅對佇列中 `Pending`／`Accepted`（尚待處理）比對；`Ignored` 不擋——
/// 老爺忽略過的建議日後仍可因新一輪學習重新出現（要永久壓制屬另一產品決策）。
/// 批內同指紋亦僅保留第一筆。
pub fn dedup_against_queue(
    candidates: Vec<ReviewItem>,
    existing: &[ReviewItem],
) -> Vec<ReviewItem> {
    let fingerprint = |i: &ReviewItem| {
        format!(
            "{}\u{1f}{:?}\u{1f}{}\u{1f}{}\u{1f}{}",
            i.project_id, i.item_type, i.category, i.title, i.content
        )
    };
    let mut seen: std::collections::HashSet<String> = existing
        .iter()
        .filter(|i| matches!(i.status, ReviewStatus::Pending | ReviewStatus::Accepted))
        .map(fingerprint)
        .collect();
    candidates
        .into_iter()
        .filter(|c| seen.insert(fingerprint(c)))
        .collect()
}

/// 解開 git C-style quoted 路徑（`core.quotePath` 預設下，非 ASCII／特殊字元路徑會被
/// `"..."` 包裹並以 `\ddd` 八進位位元組、`\t` 等跳脫）。非引號包裹者原樣回傳。
fn unquote_git_path(s: &str) -> String {
    let inner = match s.strip_prefix('"').and_then(|t| t.strip_suffix('"')) {
        Some(i) => i,
        None => return s.to_string(),
    };
    let mut bytes: Vec<u8> = Vec::with_capacity(inner.len());
    let mut chars = inner.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            let mut buf = [0u8; 4];
            bytes.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
            continue;
        }
        match chars.next() {
            Some('n') => bytes.push(b'\n'),
            Some('t') => bytes.push(b'\t'),
            Some('r') => bytes.push(b'\r'),
            Some('\\') => bytes.push(b'\\'),
            Some('"') => bytes.push(b'"'),
            Some(d @ '0'..='7') => {
                // 八進位 \ddd（1~3 位）→ 一個位元組；UTF-8 中文即由連續多組組成
                let mut v = d as u32 - '0' as u32;
                for _ in 0..2 {
                    match chars.peek() {
                        Some(&n @ '0'..='7') => {
                            v = v * 8 + (n as u32 - '0' as u32);
                            chars.next();
                        }
                        _ => break,
                    }
                }
                bytes.push(v as u8);
            }
            Some(other) => {
                let mut buf = [0u8; 4];
                bytes.extend_from_slice(other.encode_utf8(&mut buf).as_bytes());
            }
            None => {}
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

/// 由 `+++ `／`--- ` 行取路徑（quoted 亦可）；`/dev/null` 回 None。
fn path_from_marker(rest: &str) -> Option<String> {
    let t = rest.trim_end();
    if t == "/dev/null" {
        return None;
    }
    let unq = unquote_git_path(t);
    unq.strip_prefix("b/")
        .or_else(|| unq.strip_prefix("a/"))
        .map(|p| p.to_string())
        .or(Some(unq))
}

/// 段內以 `+++ b/<path>` 行定位路徑（每檔一行、無「路徑含空白」歧義；quoted 亦解）。
/// 刪檔（`+++ /dev/null`）退回 `--- a/<path>`。找不到 → None（交由標頭解析 fallback）。
fn path_from_body(body: &str) -> Option<String> {
    let mut minus: Option<String> = None;
    for line in body.lines() {
        if let Some(rest) = line.strip_prefix("+++ ") {
            match path_from_marker(rest) {
                Some(p) => return Some(p),
                None => return minus, // +++ /dev/null（刪檔）→ 用 a 側
            }
        } else if let Some(rest) = line.strip_prefix("--- ") {
            minus = path_from_marker(rest);
        }
    }
    None
}

/// 由 `diff --git ...` 標頭取 b/ 側路徑（fallback 用；quoted 與未引號形式皆試）。
fn path_from_header(line: &str) -> Option<String> {
    let rest = line.strip_prefix("diff --git ")?;
    // quoted b 側：`... "b/<escaped>"`（行尾必為closing quote）
    if let Some(i) = rest.rfind("\"b/") {
        let token = &rest[i..];
        if token.ends_with('"') {
            return Some(unquote_git_path(token).trim_start_matches("b/").to_string());
        }
    }
    // 未引號：取最後一個 ` b/` 分隔（路徑含空白時有歧義，故僅作 fallback；主解析走 +++ 行）
    rest.rfind(" b/").map(|i| rest[i + 3..].to_string())
}

/// 把 diff 依 `diff --git ` 標頭切段，逐檔跑安全過濾。
/// 路徑定位：優先段內 `+++ b/` 行（無歧義、quoted 可解），標頭解析作 fallback。
/// 回傳 (檔案路徑, 該檔命中清單)；標頭前的內容（無檔可歸；含整段無標頭的純文字）
/// 以 `None` 表示，維持舊行為（仍檢查、卡片不標路徑）。
fn scan_sensitive_by_file(diff_text: &str) -> Vec<(Option<String>, Vec<safety_filter::SafetyHit>)> {
    // 切段：每段 = (標頭行, 內容行集)。切段條件放寬為 `diff --git ` 前綴，
    // quoted 標頭（`diff --git "a/…" "b/…"`，中文檔名 core.quotePath 預設即此形）也能開新段。
    let mut chunks: Vec<(Option<String>, String)> = Vec::new();
    let mut cur_header: Option<String> = None;
    let mut cur_body = String::new();
    for line in diff_text.lines() {
        if line.starts_with("diff --git ") {
            if !cur_body.trim().is_empty() || cur_header.is_some() {
                chunks.push((cur_header.take(), std::mem::take(&mut cur_body)));
            }
            cur_header = Some(line.to_string());
        } else {
            cur_body.push_str(line);
            cur_body.push('\n');
        }
    }
    if !cur_body.trim().is_empty() || cur_header.is_some() {
        chunks.push((cur_header, cur_body));
    }

    let mut out = Vec::new();
    for (header, body) in chunks {
        let r = safety_filter::check(&body);
        if !r.is_safe {
            let path = match &header {
                Some(h) => path_from_body(&body).or_else(|| path_from_header(h)),
                None => None, // 標頭前內容：維持不標路徑的舊行為
            };
            out.push((path, r.hits));
        }
    }
    out
}

fn blocked_item(
    project_id: &str,
    file_hits: &[(Option<String>, Vec<safety_filter::SafetyHit>)],
) -> ReviewItem {
    let mut lines = vec![
        "AMAGI 偵測到這次變更中有下列疑似機密，已擋下自動保存。".to_string(),
        "請確認是否為誤判：".to_string(),
        String::new(),
    ];
    for (path, hits) in file_hits {
        if let Some(p) = path {
            lines.push(format!("📄 {p}"));
        }
        for h in hits {
            lines.push(format!("• {}：{}", h.label, h.masked));
        }
        lines.push(String::new());
    }
    lines.push("處置建議：".to_string());
    lines.push("- 開啟上列檔案，把敏感行移除或改讀環境變數（真值放不進版控的檔案）後，再點「確認丟棄」。".to_string());
    lines.push("- 若是誤判（例如 commit SHA、雜湊值），直接點「確認丟棄」即可，不影響其他候選。".to_string());
    lines.push("- 若機密已 commit／push，請先作廢並更換該金鑰，再處理檔案；切勿同步進 AGENTS.md／CLAUDE.md。".to_string());

    ReviewItem {
        id: Uuid::new_v4().to_string(),
        project_id: project_id.to_string(),
        item_type: ReviewItemType::Blocked,
        category: "sensitive".to_string(),
        title: "疑似敏感內容（待確認）".to_string(),
        content: lines.join("\n"),
        risk: RiskLevel::High,
        // 改為 Pending：留在審核佇列供老爺檢視判斷，而非自動忽略
        status: ReviewStatus::Pending,
        sync_targets: vec![],
        sync_scope: SyncScope::Project,
        source_pending_file: None,
        created_at: Utc::now(),
        reviewed_at: None,
    }
}

fn make_memory(
    project_id: &str,
    category: &str,
    title: &str,
    content: &str,
    risk: RiskLevel,
    sync_targets: Vec<String>,
) -> ReviewItem {
    ReviewItem {
        id: Uuid::new_v4().to_string(),
        project_id: project_id.to_string(),
        item_type: ReviewItemType::Memory,
        category: category.to_string(),
        title: title.to_string(),
        content: content.to_string(),
        risk,
        status: ReviewStatus::Pending,
        sync_targets,
        sync_scope: SyncScope::Project,
        source_pending_file: None,
        created_at: Utc::now(),
        reviewed_at: None,
    }
}

fn make_skill(project_id: &str, title: &str, content: &str) -> ReviewItem {
    ReviewItem {
        id: Uuid::new_v4().to_string(),
        project_id: project_id.to_string(),
        item_type: ReviewItemType::Skill,
        category: "skill".to_string(),
        title: title.to_string(),
        content: content.to_string(),
        risk: RiskLevel::Medium,
        status: ReviewStatus::Pending,
        sync_targets: vec![
            ".codex/skills".into(),
            ".claude/commands".into(),
        ],
        sync_scope: SyncScope::Project,
        source_pending_file: None,
        created_at: Utc::now(),
        reviewed_at: None,
    }
}

fn count_added_lines(diff_text: &str, filename: &str) -> usize {
    let lower_filename = filename.to_lowercase();
    let mut in_file = false;
    let mut count = 0usize;
    for line in diff_text.lines() {
        if line.starts_with("diff --git") {
            in_file = line.to_lowercase().contains(&lower_filename);
        }
        if in_file && line.starts_with('+') && !line.starts_with("+++") {
            count += 1;
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_readme_rule_fires() {
        let files = vec!["README.md".to_string()];
        // 產生 15 個獨立的 added lines（每行以 + 開頭）
        let added_lines: String = (0..15).map(|i| format!("+新增內容第{}行\n", i)).collect();
        let big_diff = format!("diff --git a/README.md b/README.md\n{}", added_lines);
        let candidates = generate_candidates("proj1", &files, "", &big_diff);
        assert!(candidates.iter().any(|c| c.category == "project_rule"));
    }

    #[test]
    fn test_sensitive_adds_reviewable_blocked_item() {
        let files = vec!["README.md".to_string()];
        let diff = "api_key=sk-secret123abc";
        let candidates = generate_candidates("proj1", &files, "", diff);
        let blocked = candidates
            .iter()
            .find(|c| c.item_type == ReviewItemType::Blocked)
            .expect("應有一筆封鎖項");
        // 改為待確認（Pending），留在佇列供檢視，而非自動忽略
        assert_eq!(blocked.status, ReviewStatus::Pending);
        // 內容應帶規則名，讓使用者看得出觸發原因
        assert!(blocked.content.contains("API key"));
    }

    #[test]
    fn test_workflow_rule_fires() {
        let files = vec![".github/workflows/release.yml".to_string()];
        let candidates = generate_candidates("proj1", &files, "", "normal diff content");
        assert!(candidates.iter().any(|c| c.category == "ci_cd_workflow"));
    }

    /// 封鎖卡附命中檔案路徑：依 diff 標頭定位，多檔命中各自列出（含各自規則）。
    #[test]
    fn test_blocked_item_lists_hit_file_paths() {
        let diff = concat!(
            "diff --git a/config.env b/config.env\n",
            "+password=supersecret99\n",
            "diff --git a/src/api.ts b/src/api.ts\n",
            "+const k = 'x'; // api_key=sk-live-abcdef\n",
            "diff --git a/README.md b/README.md\n",
            "+一般說明文字，無敏感內容\n",
        );
        let candidates = generate_candidates("proj1", &[], "", diff);
        let blocked = candidates.iter()
            .find(|c| c.item_type == ReviewItemType::Blocked)
            .expect("應有一筆封鎖項");
        assert!(blocked.content.contains("📄 config.env"), "應列出 config.env：\n{}", blocked.content);
        assert!(blocked.content.contains("📄 src/api.ts"), "應列出 src/api.ts");
        assert!(!blocked.content.contains("README.md"), "無命中的檔案不得列出");
        // 各檔規則歸位：password 行歸 config.env 段、api_key 歸 api.ts 段
        let cfg_pos = blocked.content.find("📄 config.env").unwrap();
        let api_pos = blocked.content.find("📄 src/api.ts").unwrap();
        let pw_pos = blocked.content.find("密碼（password）").unwrap();
        let key_pos = blocked.content.find("API key").unwrap();
        assert!(cfg_pos < pw_pos && pw_pos < api_pos, "password 命中應列在 config.env 段內");
        assert!(api_pos < key_pos, "api_key 命中應列在 src/api.ts 段內");
        // 遮罩原則不變：完整密文不得出現
        assert!(!blocked.content.contains("supersecret99"));
        assert!(!blocked.content.contains("sk-live-abcdef"));
    }

    /// 無 diff 標頭的內容（如貼上的純文字）：仍檢查、仍封鎖，但不標檔案路徑（維持舊行為）。
    #[test]
    fn test_blocked_item_headerless_diff_no_path() {
        let candidates = generate_candidates("proj1", &[], "", "api_key=sk-secret123abc");
        let blocked = candidates.iter()
            .find(|c| c.item_type == ReviewItemType::Blocked)
            .expect("無標頭內容命中仍應封鎖");
        assert!(blocked.content.contains("API key"));
        assert!(!blocked.content.contains("📄"), "無標頭內容不標檔案路徑");
    }

    fn first_blocked(diff: &str) -> ReviewItem {
        generate_candidates("proj1", &[], "", diff)
            .into_iter()
            .find(|c| c.item_type == ReviewItemType::Blocked)
            .expect("應有封鎖項")
    }

    /// Codex 審查核心案例：core.quotePath 預設下中文檔名被 C-style quote（八進位跳脫），
    /// quoted 標頭仍須切段、路徑仍須解出（「設定.env」= \350\250\255\345\256\232.env）。
    #[test]
    fn test_blocked_path_quoted_chinese_filename() {
        let diff = concat!(
            r#"diff --git "a/\350\250\255\345\256\232.env" "b/\350\250\255\345\256\232.env""#, "\n",
            r#"--- "a/\350\250\255\345\256\232.env""#, "\n",
            r#"+++ "b/\350\250\255\345\256\232.env""#, "\n",
            "+password=supersecret99\n",
        );
        let blocked = first_blocked(diff);
        assert!(blocked.content.contains("📄 設定.env"),
            "quoted 中文路徑應被解出並列在卡片：\n{}", blocked.content);
        assert!(!blocked.content.contains(r"\350"), "不得殘留未解碼的八進位跳脫");
    }

    /// rename：取 b/ 側（現況路徑）。
    #[test]
    fn test_blocked_path_rename_takes_b_side() {
        let diff = concat!(
            "diff --git a/old.env b/new.env\n",
            "--- a/old.env\n",
            "+++ b/new.env\n",
            "+password=supersecret99\n",
        );
        assert!(first_blocked(diff).content.contains("📄 new.env"), "rename 應取 b 側路徑");
    }

    /// 刪檔：`+++ /dev/null` → 退回 a/ 側路徑（被刪內容含敏感行仍須定位）。
    #[test]
    fn test_blocked_path_deleted_file_falls_back_to_a_side() {
        let diff = concat!(
            "diff --git a/gone.env b/gone.env\n",
            "--- a/gone.env\n",
            "+++ /dev/null\n",
            "-password=supersecret99\n",
        );
        assert!(first_blocked(diff).content.contains("📄 gone.env"), "刪檔應以 a 側定位");
    }

    /// 路徑含空白：未引號標頭有 ` b/` 歧義，主解析走 `+++ b/` 行（無歧義）。
    #[test]
    fn test_blocked_path_with_spaces_via_marker_line() {
        let diff = concat!(
            "diff --git a/my dir/x.env b/my dir/x.env\n",
            "--- a/my dir/x.env\n",
            "+++ b/my dir/x.env\n",
            "+password=supersecret99\n",
        );
        assert!(first_blocked(diff).content.contains("📄 my dir/x.env"),
            "含空白路徑應由 +++ 行正確解出");
    }

    /// 連續標頭（如 binary diff 無內容行）：不得吃掉下一段的歸屬。
    #[test]
    fn test_blocked_path_consecutive_headers() {
        let diff = concat!(
            "diff --git a/logo.png b/logo.png\n",
            "diff --git a/config.env b/config.env\n",
            "--- a/config.env\n",
            "+++ b/config.env\n",
            "+password=supersecret99\n",
        );
        let blocked = first_blocked(diff);
        assert!(blocked.content.contains("📄 config.env"), "命中應歸 config.env");
        assert!(!blocked.content.contains("logo.png"), "無命中的 binary 檔不得列出");
    }

    /// 模擬「同一 diff 重按學習」：第一輪入列後（Pending），第二輪相同候選應全數被擋。
    #[test]
    fn test_relearn_same_diff_is_idempotent() {
        let files = vec!["README.md".to_string(), "tauri.conf.json".to_string()];
        let added: String = (0..15).map(|i| format!("+第{}行\n", i)).collect();
        let diff = format!("diff --git a/README.md b/README.md\n{}", added);

        let round1 = generate_candidates("proj1", &files, "", &diff);
        assert!(!round1.is_empty());
        // 覆蓋記憶與規則技能兩種型別
        assert!(round1.iter().any(|c| c.item_type == ReviewItemType::Memory));
        assert!(round1.iter().any(|c| c.item_type == ReviewItemType::Skill));

        // 第一輪照常入列
        let queued = dedup_against_queue(round1.clone(), &[]);
        assert_eq!(queued.len(), round1.len(), "佇列為空時不應擋任何候選");

        // 第二輪：同 diff 再學一次 → 全數命中指紋、不重複入列
        let round2 = generate_candidates("proj1", &files, "", &diff);
        assert!(dedup_against_queue(round2, &queued).is_empty(), "重按學習應冪等");
    }

    /// Accepted（已核可待同步）同樣視為既有，不得重複入列。
    #[test]
    fn test_dedup_blocks_against_accepted() {
        let files = vec!["README.md".to_string()];
        let added: String = (0..15).map(|i| format!("+第{}行\n", i)).collect();
        let diff = format!("diff --git a/README.md b/README.md\n{}", added);

        let mut existing = generate_candidates("proj1", &files, "", &diff);
        for item in &mut existing {
            item.status = ReviewStatus::Accepted;
        }
        let round2 = generate_candidates("proj1", &files, "", &diff);
        assert!(dedup_against_queue(round2, &existing).is_empty(), "Accepted 也應擋");
    }

    /// Ignored 不擋：老爺忽略過的建議，新一輪學習仍可重新出現。
    #[test]
    fn test_dedup_allows_reappearance_after_ignored() {
        let files = vec!["README.md".to_string()];
        let added: String = (0..15).map(|i| format!("+第{}行\n", i)).collect();
        let diff = format!("diff --git a/README.md b/README.md\n{}", added);

        let mut existing = generate_candidates("proj1", &files, "", &diff);
        for item in &mut existing {
            item.status = ReviewStatus::Ignored;
        }
        let round2 = generate_candidates("proj1", &files, "", &diff);
        assert!(!dedup_against_queue(round2, &existing).is_empty(), "Ignored 不應擋重新建議");
    }

    /// 指紋含內容：Blocked 項遮罩片段不同（不同機密）→ 視為新項，不得誤擋。
    #[test]
    fn test_dedup_keeps_blocked_with_different_content() {
        let files = vec!["config.rs".to_string()];
        let existing = generate_candidates("proj1", &files, "", "api_key=sk-secret123abc");
        let next = generate_candidates("proj1", &files, "", "api_key=sk-another456xyz");
        let blocked_next: Vec<_> = next
            .into_iter()
            .filter(|c| c.item_type == ReviewItemType::Blocked)
            .collect();
        assert!(!blocked_next.is_empty());
        let kept = dedup_against_queue(blocked_next, &existing);
        assert!(!kept.is_empty(), "遮罩內容不同的 Blocked 項是新發現，不得誤擋");
    }

    /// 不同專案的相同候選互不干擾（指紋含 project_id）。
    #[test]
    fn test_dedup_scoped_by_project() {
        let files = vec!["README.md".to_string()];
        let added: String = (0..15).map(|i| format!("+第{}行\n", i)).collect();
        let diff = format!("diff --git a/README.md b/README.md\n{}", added);

        let existing = generate_candidates("proj1", &files, "", &diff);
        let other = generate_candidates("proj2", &files, "", &diff);
        let kept = dedup_against_queue(other.clone(), &existing);
        assert_eq!(kept.len(), other.len(), "不同專案不得互擋");
    }

    /// 批內同指紋僅保留第一筆（防未來規則重複產出）。
    #[test]
    fn test_dedup_within_batch() {
        let dup = make_memory(
            "proj1", "project_rule", "同標題", "同內容",
            RiskLevel::Low, vec!["AGENTS.md".into()],
        );
        let batch = vec![dup.clone(), dup];
        assert_eq!(dedup_against_queue(batch, &[]).len(), 1, "批內重複應僅留一筆");
    }
}
