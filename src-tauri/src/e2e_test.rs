//! 端對端整合測試（對真實臨時檔案系統落地）。
//!
//! 不同於各模組內的單元測試（零件級），這裡把整條流水線串起來，
//! 用一個「假 Git 專案」+ 獨立的「假 AppData」目錄，實際走一遍：
//!
//!   add_project → init_project → learn(generate_candidates)
//!     → review_queue(add/list/accept) → conflict gate → agent_exporter::sync
//!     → 驗證真的落地的檔案內容 → mark_synced
//!
//! 邊界說明：Tauri command 層（sync_agent_files 等）需要執行中的
//! App + State<AppState>，無法在純測試環境構造；故這裡測「command 實際呼叫的
//! 底層函式」，等同 command 的核心行為。command 的衝突卡控判斷另由
//! commands::sync_commands::tests 覆蓋。
//!
//! 全程只碰臨時目錄，**不觸碰** 老爺真實的 ~/.claude、~/.codex（故技能一律用
//! Project scope 測試，不測 Global scope 以免污染家目錄）。

#![cfg(test)]

use std::path::PathBuf;
use uuid::Uuid;

use crate::core::{agent_exporter, conflict_filter, learn_engine, project_manager, review_queue};
use crate::models::review::{ReviewItem, ReviewItemType, ReviewStatus, RiskLevel, SyncScope};
use crate::models::project::Project;
use crate::utils::fs_utils;

/// 測試沙盒：一個假 Git 專案 + 一個假 AppData 目錄；Drop 時自動清除。
struct Sandbox {
    repo: PathBuf,
    data_dir: PathBuf,
}

impl Sandbox {
    fn new(tag: &str) -> Self {
        let base = std::env::temp_dir().join(format!("amagi-e2e-{}-{}", tag, Uuid::new_v4()));
        let repo = base.join("repo");
        let data_dir = base.join("appdata");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::create_dir_all(&data_dir).unwrap();
        // 讓 is_git_repo 通過：建立 .git 目錄
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        Sandbox { repo, data_dir }
    }

    fn repo_str(&self) -> String {
        self.repo.to_string_lossy().to_string()
    }

    fn read(&self, rel: &str) -> String {
        std::fs::read_to_string(self.repo.join(rel))
            .unwrap_or_else(|e| panic!("讀不到 {}：{}", rel, e))
    }

    fn exists(&self, rel: &str) -> bool {
        self.repo.join(rel).exists()
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        // 清掉整個 base（repo 與 appdata 共同的父層）
        if let Some(base) = self.repo.parent() {
            let _ = std::fs::remove_dir_all(base);
        }
    }
}

fn mem_item(project_id: &str, category: &str, title: &str, content: &str, scope: SyncScope) -> ReviewItem {
    ReviewItem {
        id: Uuid::new_v4().to_string(),
        project_id: project_id.to_string(),
        item_type: ReviewItemType::Memory,
        category: category.to_string(),
        title: title.to_string(),
        content: content.to_string(),
        risk: RiskLevel::Low,
        status: ReviewStatus::Accepted,
        sync_targets: vec![],
        sync_scope: scope,
        source_pending_file: None,
        created_at: chrono::Utc::now(),
        reviewed_at: None,
    }
}

fn skill_item(project_id: &str, title: &str, content: &str) -> ReviewItem {
    ReviewItem {
        id: Uuid::new_v4().to_string(),
        project_id: project_id.to_string(),
        item_type: ReviewItemType::Skill,
        category: "skill".to_string(),
        title: title.to_string(),
        content: content.to_string(),
        risk: RiskLevel::Low,
        status: ReviewStatus::Accepted,
        sync_targets: vec![],
        sync_scope: SyncScope::Project,
        source_pending_file: None,
        created_at: chrono::Utc::now(),
        reviewed_at: None,
    }
}

/// 模擬 commands::sync_commands::scan_item_conflicts（私有，無法直接呼叫）。
fn gate(items: &[ReviewItem]) -> Vec<&ReviewItem> {
    items
        .iter()
        .filter(|i| conflict_filter::check(&i.content).has_conflict)
        .collect()
}

// ───────────────────────────────────────────────────────────
// 1. add_project → init_project：專案註冊與初始化骨架
// ───────────────────────────────────────────────────────────
#[test]
fn e2e_add_and_init_project() {
    let sb = Sandbox::new("init");

    // add_project
    let project = project_manager::add_project(&sb.repo_str(), &sb.data_dir).unwrap();
    assert_eq!(project.path, sb.repo_str());
    assert!(!project.initialized);

    // 重複加入應被拒
    assert!(project_manager::add_project(&sb.repo_str(), &sb.data_dir).is_err());

    // list / get
    let listed = project_manager::list_projects(&sb.data_dir);
    assert_eq!(listed.len(), 1);
    assert!(project_manager::get_project(&project.id, &sb.data_dir).is_some());

    // init_project：建立 .amagi 骨架 + 模板檔
    let result = project_manager::init_project(&project).unwrap();
    assert!(!result.created_dirs.is_empty());

    for d in ["memory", "pending", "skills", "history", "artifacts", "state"] {
        assert!(sb.exists(&format!(".amagi/{}", d)), "缺少目錄 .amagi/{}", d);
    }
    for f in [
        ".amagi/config.json",
        ".amagi/workflow-state.md",
        ".amagi/after-task-review.md",
        ".amagi/pending/AGENT_INSTRUCTIONS.md",
        "AGENTS.md",
        "CLAUDE.md",
    ] {
        assert!(sb.exists(f), "缺少檔案 {}", f);
    }

    // 初始 AGENTS.md / CLAUDE.md 應含「自我分步」紀律
    assert!(sb.read("AGENTS.md").contains("自我分步"));
    assert!(sb.read("CLAUDE.md").contains("不可跳步"));

    // 冪等：再 init 一次不該爆，且不重複建立既有檔
    let second = project_manager::init_project(&project).unwrap();
    assert!(second.created_dirs.is_empty(), "第二次 init 不應重建目錄");
}

// ───────────────────────────────────────────────────────────
// 2. learn → review queue → accept → sync 記憶 → AGENTS.md / CLAUDE.md
// ───────────────────────────────────────────────────────────
#[test]
fn e2e_learn_review_sync_memory() {
    let sb = Sandbox::new("memory");
    let project = project_manager::add_project(&sb.repo_str(), &sb.data_dir).unwrap();
    project_manager::init_project(&project).unwrap();

    // learn：README 大改 + package.json + CLAUDE.md（觸發 project_rule / tech_stack / agent_rule）
    let changed = vec![
        "README.md".to_string(),
        "package.json".to_string(),
        "CLAUDE.md".to_string(),
    ];
    let added: String = (0..15).map(|i| format!("+第 {} 行新內容\n", i)).collect();
    let diff = format!("diff --git a/README.md b/README.md\n{}", added);
    let candidates = learn_engine::generate_candidates(&project.id, &changed, "", &diff);

    assert!(candidates.iter().any(|c| c.category == "project_rule"));
    assert!(candidates.iter().any(|c| c.category == "tech_stack"));
    assert!(candidates.iter().any(|c| c.category == "agent_rule"));

    // 進審核佇列
    review_queue::add_items(&sb.data_dir, candidates.clone()).unwrap();
    let queued = review_queue::list_items(&sb.data_dir, Some(&project.id));
    assert_eq!(queued.len(), candidates.len());
    // 跨專案過濾
    assert!(review_queue::list_items(&sb.data_dir, Some("other-project")).is_empty());

    // 接受全部
    let ids: Vec<String> = queued.iter().map(|i| i.id.clone()).collect();
    let accepted = review_queue::accept_items(&sb.data_dir, &ids).unwrap();
    assert_eq!(accepted.len(), ids.len());
    assert!(accepted.iter().all(|i| i.status == ReviewStatus::Accepted));

    // 衝突閘門：這批乾淨，應全數放行
    assert!(gate(&accepted).is_empty(), "乾淨記憶不該被卡");

    // sync → 落地 AGENTS.md / CLAUDE.md
    let sync = agent_exporter::sync_agent_files(&sb.repo_str(), &accepted).unwrap();
    assert!(sync.written_files.iter().any(|f| f.ends_with("AGENTS.md")));
    assert!(sync.written_files.iter().any(|f| f.ends_with("CLAUDE.md")));
    assert!(sync.blocked_conflicts.is_empty());

    // AGENTS.md 內容：被 build_agents_md 覆寫（含分類標頭）
    let agents = sb.read("AGENTS.md");
    assert!(agents.starts_with("# AGENTS.md"));
    assert!(agents.contains("Auto-generated by AMAGI Core"));
    assert!(agents.contains("Project Rules"));
    assert!(agents.contains("Tech Stack"));
    // 覆寫前的初始模板應備份；注意 write_with_backup 用 with_extension("bak")，
    // 故 AGENTS.md 的備份檔名是 AGENTS.bak（取代副檔名，非附加）。
    assert!(sb.exists("AGENTS.bak"), "覆寫前應留 AGENTS.bak 備份");

    // CLAUDE.md：只收 agent_rule
    let claude = sb.read("CLAUDE.md");
    assert!(claude.contains("Agent 規則更新"));

    // mark_synced
    review_queue::mark_synced(&sb.data_dir, &ids).unwrap();
    let after = review_queue::list_items(&sb.data_dir, Some(&project.id));
    assert!(after.iter().all(|i| i.status == ReviewStatus::Synced));
}

// ───────────────────────────────────────────────────────────
// 3. 技能同步 → .claude/skills、.codex/skills、.amagi/skills 原生格式
// ───────────────────────────────────────────────────────────
#[test]
fn e2e_skill_sync_native_format() {
    let sb = Sandbox::new("skill");
    let project = project_manager::add_project(&sb.repo_str(), &sb.data_dir).unwrap();
    project_manager::init_project(&project).unwrap();

    let content = "## 描述\n替遊戲狀態加入悔棋功能\n\n## 何時使用\n- 需要撤回上一步\n- 觸發關鍵字：undo、悔棋\n\n## 步驟\n1. 在 store 新增 history 陣列\n2. 實作 undo()";
    let skill = skill_item(&project.id, "新增悔棋功能", content);

    let sync = agent_exporter::sync_agent_files(&sb.repo_str(), std::slice::from_ref(&skill)).unwrap();
    assert!(sync.blocked_conflicts.is_empty());

    let slug = fs_utils::slugify(&skill.title);
    let claude_skill = format!(".claude/skills/{}/SKILL.md", slug);
    let codex_skill = format!(".codex/skills/{}/SKILL.md", slug);
    let amagi_skill = format!(".amagi/skills/{}.md", slug);

    assert!(sb.exists(&claude_skill), "缺少 {}", claude_skill);
    assert!(sb.exists(&codex_skill), "缺少 {}", codex_skill);
    assert!(sb.exists(&amagi_skill), "缺少 {}", amagi_skill);

    // 原生 frontmatter：name / description / when_to_use 自動觸發欄位
    let md = sb.read(&claude_skill);
    assert!(md.starts_with("---\n"));
    assert!(md.contains("name: \"新增悔棋功能\""));
    assert!(md.contains("description:"));
    assert!(md.contains("when_to_use:"));
    assert!(md.contains("undo")); // when_to_use 應帶觸發關鍵字
    assert!(md.contains("## 步驟")); // 內文保留

    // 三份內容一致
    assert_eq!(sb.read(&codex_skill), md);
    assert_eq!(sb.read(&amagi_skill), md);
}

// ───────────────────────────────────────────────────────────
// 4. 衝突卡控：含 git config --local 的記憶應被閘門擋下
// ───────────────────────────────────────────────────────────
#[test]
fn e2e_conflict_gate_blocks_bad_memory() {
    let project_id = "p-conflict";

    // 真實出過包的內容（Gomoku 當初記錯的 git config --local）
    let bad = mem_item(
        project_id,
        "feedback",
        "git 作者設定",
        "請先執行 git config --local user.name \"あまぎ\" 再 commit。",
        SyncScope::Project,
    );
    let good = mem_item(
        project_id,
        "feedback",
        "git 作者設定（正確）",
        "commit 用 git commit --author=\"あまぎ <amagi.core@gmail.com>\"，不動任何 config。",
        SyncScope::Project,
    );

    let batch = vec![bad.clone(), good.clone()];
    let blocked = gate(&batch);

    // 只有壞的被擋
    assert_eq!(blocked.len(), 1);
    assert_eq!(blocked[0].id, bad.id);

    // 確認理由講得出來
    let reasons = conflict_filter::check(&bad.content);
    assert!(reasons.conflicts.iter().any(|c| c.reason.contains("--local")));
}

// ───────────────────────────────────────────────────────────
// 5. 安全過濾：diff 含疑似金鑰時，learn 只產出 Blocked 候選
// ───────────────────────────────────────────────────────────
#[test]
fn e2e_safety_filter_blocks_secret_in_learn() {
    let changed = vec!["config.ts".to_string()];
    let diff = "diff --git a/config.ts b/config.ts\n+const apiKey = \"sk-abcdef0123456789secrettoken\"";
    let candidates = learn_engine::generate_candidates("p-secret", &changed, "", diff);

    let blocked = candidates
        .iter()
        .find(|c| c.item_type == ReviewItemType::Blocked)
        .expect("應有封鎖項");
    // 改為待確認：留在審核佇列供檢視，內容帶規則名
    assert_eq!(blocked.status, ReviewStatus::Pending);
    assert!(blocked.content.contains("API key"));
}

// ───────────────────────────────────────────────────────────
// 6. remove_project：移除後清單應變空
// ───────────────────────────────────────────────────────────
#[test]
fn e2e_remove_project() {
    let sb = Sandbox::new("remove");
    let project = project_manager::add_project(&sb.repo_str(), &sb.data_dir).unwrap();
    assert_eq!(project_manager::list_projects(&sb.data_dir).len(), 1);

    project_manager::remove_project(&project.id, &sb.data_dir).unwrap();
    assert!(project_manager::list_projects(&sb.data_dir).is_empty());

    // 移除不存在的 → Err
    assert!(project_manager::remove_project("ghost-id", &sb.data_dir).is_err());
}

// 防止 unused import 警告（Project 型別在 add_project 回傳處已用到，這裡顯式標註）
#[allow(dead_code)]
fn _assert_project_type(p: Project) -> String {
    p.id
}

// ───────────────────────────────────────────────────────────
// 7. 差異匯出：真實 git repo，改/增/刪三檔 → 列檔 + 產生 diff（兩框）
// ───────────────────────────────────────────────────────────
use crate::core::diff_export;
use crate::models::diff::{ChangedStatus, DiffGroup};

/// 在 repo 執行 git（純測試用；commit 以 -c 內聯身分，不寫任何 config）
fn git(repo: &std::path::Path, args: &[&str]) {
    let ok = std::process::Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    assert!(ok, "git {:?} 失敗", args);
}

#[test]
fn e2e_diff_export_real_git() {
    let base = std::env::temp_dir().join(format!("amagi-diff-{}", Uuid::new_v4()));
    let repo = base.join("repo");
    std::fs::create_dir_all(&repo).unwrap();

    // 初始化 + 首個 commit（a.txt 之後會改、c.txt 之後會刪）
    git(&repo, &["init", "-q"]);
    std::fs::write(repo.join("a.txt"), "l1\nl2\n").unwrap();
    std::fs::write(repo.join("c.txt"), "gone\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &[
        "-c", "user.email=test@amagi.local",
        "-c", "user.name=amagi-test",
        "commit", "-q", "-m", "init",
    ]);

    // 製造三種異動：改 a.txt、新增 b.txt（未追蹤）、刪 c.txt
    std::fs::write(repo.join("a.txt"), "l1\nCHANGED\n").unwrap();
    std::fs::write(repo.join("b.txt"), "new1\nnew2\n").unwrap();
    std::fs::remove_file(repo.join("c.txt")).unwrap();

    let repo_str = repo.to_string_lossy().to_string();

    // ── 列出異動檔：分組正確 ──
    let listed = diff_export::list_changed_files(&repo_str).unwrap();
    let find = |p: &str| listed.iter().find(|f| f.path == p).expect(p);
    assert_eq!(find("a.txt").status, ChangedStatus::Modified);
    assert_eq!(find("a.txt").group, DiffGroup::Edited);
    assert_eq!(find("b.txt").status, ChangedStatus::Untracked);
    assert_eq!(find("b.txt").group, DiffGroup::AddedDeleted);
    assert_eq!(find("c.txt").status, ChangedStatus::Deleted);
    assert_eq!(find("c.txt").group, DiffGroup::AddedDeleted);

    // ── 全選三檔產生 diff ──
    let all = vec!["a.txt".to_string(), "b.txt".to_string(), "c.txt".to_string()];
    let bundle = diff_export::generate_diff_text(&repo_str, &all).unwrap();

    // 框1（異動）：含 a.txt 的修改
    assert!(bundle.edited_patch.contains("Index: a.txt"));
    assert!(bundle.edited_patch.contains("+CHANGED"));
    assert!(!bundle.edited_patch.contains("Index: b.txt")); // 新檔不在框1
    // 框2（新增/刪除）：新增 b.txt（整排 +）、刪除 c.txt（整排 -）
    assert!(bundle.added_deleted_patch.contains("Index: b.txt"));
    assert!(bundle.added_deleted_patch.contains("+new1"));
    assert!(bundle.added_deleted_patch.contains("Index: c.txt"));
    assert!(bundle.added_deleted_patch.contains("-gone"));

    // ── 範圍卡控：只勾 a.txt，其餘不得出現 ──
    let only_a = vec!["a.txt".to_string()];
    let b2 = diff_export::generate_diff_text(&repo_str, &only_a).unwrap();
    assert!(b2.edited_patch.contains("Index: a.txt"));
    assert!(b2.added_deleted_patch.is_empty(), "未勾選的新增/刪除不該出現");

    // ── 安全：不在清單內的路徑被忽略；跳脫路徑被擋 ──
    let ignored = diff_export::generate_diff_text(&repo_str, &vec!["not-listed.txt".to_string()]).unwrap();
    assert!(ignored.edited_patch.is_empty() && ignored.added_deleted_patch.is_empty());
    assert!(diff_export::generate_diff_text(&repo_str, &vec!["../escape".to_string()]).is_err());

    let _ = std::fs::remove_dir_all(&base);
}
