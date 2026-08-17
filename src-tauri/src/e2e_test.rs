//! 端對端整合測試（對真實臨時檔案系統落地）。
//!
//! 不同於各模組內的單元測試（零件級），這裡把整條流水線串起來，
//! 用一個「假 Git 專案」+ 獨立的「假 AppData」目錄，實際走一遍：
//!
//!   add_project → init_project → learn(generate_candidates)
//!     → review_queue(add/list/accept) → conflict gate → agent_exporter::sync
//!     → 驗證真的落地的檔案內容 → 出列（vault-first：入庫即出列，不留 Synced）
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

    /// 假 vault 根（與 repo、appdata 同在 base 下），供 Phase 3a 記憶落 vault 測試。
    fn vault_dir(&self) -> PathBuf {
        self.repo.parent().unwrap().join("vault")
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
        blocked_hits: vec![],
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
        blocked_hits: vec![],
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
    let result = project_manager::init_project(&project, Some(&sb.vault_dir())).unwrap();
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

    // 初始 AGENTS.md 應帶「開發工作流薄錨」（doctrine 全文在全域，此處宣告遵循＋指向軌跡檔）
    assert!(sb.read("AGENTS.md").contains("開發工作流"), "AGENTS.md 缺工作流薄錨");
    assert!(sb.read("AGENTS.md").contains(".amagi/workflow-state.md"), "AGENTS.md 薄錨應指向軌跡檔");

    // keystone：init 產出的 agent 檔應帶 vault 知識庫指針（Layer 2，路徑無關）
    let agents_init = sb.read("AGENTS.md");
    assert!(agents_init.contains("AMAGI-VAULT-PROJECT:BEGIN"), "AGENTS.md 缺 vault 指針");
    assert!(agents_init.contains("projects/"), "AGENTS.md 指針缺邏輯資料夾名");
    assert!(agents_init.contains("~/.codex/AGENTS.md"), "AGENTS.md 應引用 Codex 全局錨點");
    let claude_init = sb.read("CLAUDE.md");
    assert!(claude_init.contains("AMAGI-VAULT-PROJECT:BEGIN"), "CLAUDE.md 缺 vault 指針");
    assert!(claude_init.contains("~/.claude/CLAUDE.md"), "CLAUDE.md 應引用 Claude 全局錨點");

    // Phase 1b：init 應補派生物 gitignore 規則
    let gi = sb.read(".gitignore");
    assert!(gi.contains(".amagi/") && gi.contains(".codex/skills/") && gi.contains(".claude/skills/"),
        "init 應補 .gitignore 派生物規則");
    assert!(sb.read("CLAUDE.md").contains(".amagi/workflow-state.md"), "CLAUDE.md 薄錨應指向軌跡檔");

    // 冪等：再 init 一次不該爆，且不重複建立既有檔
    let second = project_manager::init_project(&project, Some(&sb.vault_dir())).unwrap();
    assert!(second.created_dirs.is_empty(), "第二次 init 不應重建目錄");
}

// ───────────────────────────────────────────────────────────
// 2. learn → review queue → accept → sync 記憶 → AGENTS.md / CLAUDE.md
// ───────────────────────────────────────────────────────────
#[test]
fn e2e_learn_review_sync_memory() {
    let sb = Sandbox::new("memory");
    let project = project_manager::add_project(&sb.repo_str(), &sb.data_dir).unwrap();
    project_manager::init_project(&project, Some(&sb.vault_dir())).unwrap();

    // learn：README 大改 + package.json + CLAUDE.md（觸發 project_rule / tech_stack / agent_rule）
    let changed = vec![
        "README.md".to_string(),
        "package.json".to_string(),
        "CLAUDE.md".to_string(),
    ];
    let added: String = (0..15).map(|i| format!("+第 {} 行新內容\n", i)).collect();
    let diff = format!("diff --git a/README.md b/README.md\n{}", added);
    let candidates = learn_engine::generate_candidates(&project.id, &changed, "", &diff, &Default::default());

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
    let sync = agent_exporter::sync_agent_files(
        &sb.repo_str(),
        project.vault_folder.as_deref(),
        Some(&sb.vault_dir()),
        &accepted,
        &accepted,
    ).unwrap();
    assert!(sync.written_files.iter().any(|f| f.ends_with("AGENTS.md")));
    assert!(sync.written_files.iter().any(|f| f.ends_with("CLAUDE.md")));
    assert!(sync.blocked_conflicts.is_empty());

    // Phase 3 階段2：AGENTS.md / CLAUDE.md 內聯本專案記憶「索引」（非僅指標；非舊式全文 inline）
    let agents = sb.read("AGENTS.md");
    assert!(agents.starts_with("# AGENTS.md"));
    assert!(agents.contains("Auto-generated by AMAGI Core"));
    assert!(agents.contains("本專案記憶"), "AGENTS.md 應內聯本專案記憶索引段");
    assert!(agents.contains("agent/memory/"), "應仍標示記憶細節路徑");
    assert!(!agents.contains("Project Rules"), "不應是舊式全文 inline（只內聯索引）");
    assert!(sb.exists("AGENTS.bak"), "覆寫前應留 AGENTS.bak 備份");

    let claude = sb.read("CLAUDE.md");
    assert!(claude.contains("AMAGI-VAULT-PROJECT") || claude.contains("agent/memory/"),
        "CLAUDE.md 應為指標");

    // 記憶落 vault：寫出 MEMORY.md 索引 + 至少一筆個別記憶檔
    assert!(sync.written_files.iter().any(|f| f.ends_with("MEMORY.md")),
        "應寫 vault 記憶索引 MEMORY.md");
    assert!(sync.written_files.iter().any(|f| {
        let p = f.replace('\\', "/");
        p.contains("/agent/memory/") && p.ends_with(".md") && !p.ends_with("MEMORY.md")
    }), "應寫個別記憶檔到 vault agent/memory/");

    // vault-first（Phase 3）：入庫成功後「出列」（不再 mark_synced 長留佇列帳本）
    review_queue::remove_items_of_type(&sb.data_dir, &ids, ReviewItemType::Memory).unwrap();
    let after = review_queue::list_items(&sb.data_dir, Some(&project.id));
    assert!(after.iter().all(|i| i.item_type != ReviewItemType::Memory), "記憶入庫後應出列");
}

// ───────────────────────────────────────────────────────────
// 1a2. P1+P3 端到端：AI 投遞記憶 → 掃入 → 核可 → 落 vault → 可刪除
//      （「冰箱會滿」的自動化證據；UI 層另走實機驗證）
// ───────────────────────────────────────────────────────────
#[test]
fn e2e_agent_memory_pending_to_vault_and_delete() {
    use crate::core::pending_scanner;
    use crate::models::review::SyncScope;

    let sb = Sandbox::new("pmem");
    let project = project_manager::add_project(&sb.repo_str(), &sb.data_dir).unwrap();
    project_manager::init_project(&project, Some(&sb.vault_dir())).unwrap();
    let vault = sb.vault_dir();
    let vf = project.vault_folder.clone()
        .unwrap_or_else(|| agent_exporter::project_vault_folder(&sb.repo_str()));

    // ── ① AI 投遞：三層 scope 各一筆 ＋ 一筆含假金鑰（應被安全過濾擋下）──
    let pending = sb.repo.join(".amagi").join("pending");
    std::fs::write(pending.join("memory-proj.md"),
        "---\ntitle: 專案踩坑\ncategory: gotcha\n---\n這個專案的 build 要先跑 codegen\n").unwrap();
    std::fs::write(pending.join("memory-share.md"),
        "---\ntitle: 跨專案踩坑\nscope: shared\n---\nPS5.1 寫 JSON 帶 BOM 會讓 app 靜默清空資料\n").unwrap();
    std::fs::write(pending.join("memory-leak.md"),
        "---\ntitle: 帶金鑰的記憶\n---\nkey: sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\n").unwrap();
    // 技能通道同時存在 → 驗證兩通道互不干擾、既有行為不退化
    std::fs::write(pending.join("skill-build.md"),
        "---\ntitle: 建置流程\n---\n## 步驟\n1. codegen\n2. build\n").unwrap();

    let mem_scan = pending_scanner::scan_pending_memories(&sb.repo_str(), &project.id, &[]).unwrap();
    let skill_scan = pending_scanner::scan_pending_skills(&sb.repo_str(), &project.id, &[]).unwrap();

    // 驗收③：安全擋下必須可見（不靜默）
    assert_eq!(mem_scan.items.len(), 2, "只有兩筆乾淨記憶該入列");
    assert_eq!(mem_scan.skipped.len(), 1, "含金鑰那筆必須回報為 skipped");
    assert_eq!(mem_scan.skipped[0].file_name, "memory-leak.md");
    assert_eq!(mem_scan.skipped[0].kind, "記憶");
    assert!(!mem_scan.skipped[0].labels.is_empty(), "須帶命中規則名稱");
    assert!(!format!("{:?}", mem_scan.skipped[0]).contains("sk-ant-api03"),
        "回報不得含敏感原文");
    // 驗收⑥：既有技能通道不退化
    assert_eq!(skill_scan.items.len(), 1, "技能通道應獨立撈到 skill-build.md");
    assert_eq!(skill_scan.items[0].item_type, ReviewItemType::Skill);

    // scope 標籤生效（驗收②的前半）
    let scope_of = |t: &str| mem_scan.items.iter().find(|i| i.title == t).unwrap().sync_scope.clone();
    assert_eq!(scope_of("專案踩坑"), SyncScope::Project);
    assert_eq!(scope_of("跨專案踩坑"), SyncScope::Shared);

    // ── ② 進佇列 → 核可 ──
    review_queue::add_items(&sb.data_dir, mem_scan.items.clone()).unwrap();
    let queued = review_queue::list_items(&sb.data_dir, Some(&project.id));
    let ids: Vec<String> = queued.iter().map(|i| i.id.clone()).collect();
    let accepted = review_queue::accept_items(&sb.data_dir, &ids).unwrap();
    assert!(gate(&accepted).is_empty(), "乾淨投遞不該被衝突閘卡住");

    // ── ③ 同步落 vault（專案層走 sync_agent_files；shared 走 sync_shared_memory）──
    let proj_mem: Vec<ReviewItem> = accepted.iter()
        .filter(|i| i.sync_scope == SyncScope::Project).cloned().collect();
    agent_exporter::sync_agent_files(
        &sb.repo_str(), project.vault_folder.as_deref(), Some(&vault), &accepted, &proj_mem,
    ).unwrap();
    agent_exporter::sync_shared_memory(&vault, &accepted).unwrap();

    // 驗收①②：三層落點 ＋ 索引重建 ＋ 專案內聯更新
    let proj_items = agent_exporter::load_project_memory_from_vault(&vault, &vf);
    assert!(proj_items.iter().any(|i| i.title == "專案踩坑"), "專案層記憶應落 vault");
    let shared_items = agent_exporter::load_shared_memory_from_vault(&vault);
    assert!(shared_items.iter().any(|i| i.title == "跨專案踩坑"), "shared 記憶應落 vault");
    assert!(vault.join(&vf).join("agent/memory/MEMORY.md").is_file(), "專案層索引須存在");
    assert!(vault.join("shared/agent/memory/MEMORY.md").is_file(), "shared 索引須存在");
    assert!(sb.read("CLAUDE.md").contains("專案踩坑") || sb.read("AGENTS.md").contains("專案踩坑"),
        "專案 AGENTS/CLAUDE 內聯應含新記憶");

    // ── ④ 歸檔 pending（N1）：來源檔不得殘留，否則下輪重複入列 ──
    // **走 production 函式本身**（`archive_pending_sources`，非手寫 rename）——
    // 它是自由函式、不需 State，故 e2e 可直接呼叫 command 實際使用的同一條路徑。
    // 並刻意先在 history 種一個同名檔，一併驗撞名唯一化（W1 的核心情境）。
    let history = sb.repo.join(".amagi").join("history");
    std::fs::create_dir_all(&history).unwrap();
    std::fs::write(history.join("memory-proj.md"), "先前歸檔的同名檔").unwrap();

    let archive_refs: Vec<&ReviewItem> = accepted.iter()
        .filter(|i| i.source_pending_file.is_some())
        .collect();
    let warns = crate::commands::sync_commands::archive_pending_sources(
        &history, &archive_refs, "20260817-120000");

    assert!(warns.is_empty(), "正常歸檔不應有警告，實得 {warns:?}");
    assert!(!pending.join("memory-proj.md").exists(), "已同步的來源檔不應留在 pending");
    assert!(!pending.join("memory-share.md").exists(), "已同步的來源檔不應留在 pending");
    // 撞名唯一化：既有歷史檔內容不得被覆蓋，新檔以時間戳落另一個名字
    assert_eq!(
        std::fs::read_to_string(history.join("memory-proj.md")).unwrap(),
        "先前歸檔的同名檔",
        "既有 history 檔不得被覆蓋");
    assert!(history.join("memory-proj-20260817-120000.md").is_file(),
        "撞名時新檔應以時間戳唯一化落檔");
    assert!(history.join("memory-share.md").is_file(), "未撞名者以原名歸檔");
    // 被擋下的那筆**留在原處**（每次學習會再提醒，直到修好）
    assert!(pending.join("memory-leak.md").is_file(), "被安全擋下的檔應留在 pending");

    // 驗收①末：再次掃描不重複入列（已歸檔者消失、被擋者仍只回報 skipped）
    let rescan = pending_scanner::scan_pending_memories(&sb.repo_str(), &project.id, &[]).unwrap();
    assert!(rescan.items.is_empty(), "已歸檔的投遞不應重複入列，實得 {:?}",
        rescan.items.iter().map(|i| &i.title).collect::<Vec<_>>());
    assert_eq!(rescan.skipped.len(), 1, "被擋下的檔應持續回報");

    // ── ⑤ 驗收④：可刪除（三層皆可，刪後索引更新、不復活）──
    let proj_id_to_del = proj_items.iter().find(|i| i.title == "專案踩坑").unwrap().id.clone();
    agent_exporter::delete_memory_file(
        &vault, &SyncScope::Project, Some(&vf), &proj_id_to_del).unwrap();
    let after_proj = agent_exporter::load_project_memory_from_vault(&vault, &vf);
    assert!(!after_proj.iter().any(|i| i.title == "專案踩坑"), "刪除後不得再出現");
    let idx = std::fs::read_to_string(vault.join(&vf).join("agent/memory/MEMORY.md")).unwrap();
    assert!(!idx.contains("專案踩坑"), "索引須反映刪除，實得：{idx}");

    let shared_id_to_del = shared_items.iter().find(|i| i.title == "跨專案踩坑").unwrap().id.clone();
    agent_exporter::delete_memory_file(&vault, &SyncScope::Shared, None, &shared_id_to_del).unwrap();
    assert!(!agent_exporter::load_shared_memory_from_vault(&vault)
        .iter().any(|i| i.title == "跨專案踩坑"), "shared 刪除後不得再出現");

    // 刪除後再同步一次：**不得復活**（vault 為唯一權威，佇列已出列無帳本可重建）
    review_queue::remove_items_of_type(&sb.data_dir, &ids, ReviewItemType::Memory).unwrap();
    let remaining_queue = review_queue::list_items(&sb.data_dir, Some(&project.id));
    let proj_mem2: Vec<ReviewItem> = remaining_queue.iter()
        .filter(|i| i.item_type == ReviewItemType::Memory && i.sync_scope == SyncScope::Project)
        .cloned().collect();
    agent_exporter::sync_agent_files(
        &sb.repo_str(), project.vault_folder.as_deref(), Some(&vault), &[], &proj_mem2,
    ).unwrap();
    assert!(!agent_exporter::load_project_memory_from_vault(&vault, &vf)
        .iter().any(|i| i.title == "專案踩坑"), "已刪記憶不得因再同步而復活");
}

// ───────────────────────────────────────────────────────────
// 1b. 自訂 vault_folder 應貫穿 init（發現1 修復回歸測試）
// ───────────────────────────────────────────────────────────
#[test]
fn e2e_custom_vault_folder_in_init_scaffold() {
    let sb = Sandbox::new("customvf");
    let mut project = project_manager::add_project(&sb.repo_str(), &sb.data_dir).unwrap();
    // 模擬自訂/遷移後的 mapping，與 repo basename 不同
    project.vault_folder = Some("projects/custom-mapping".to_string());

    project_manager::init_project(&project, Some(&sb.vault_dir())).unwrap();
    assert!(sb.read("AGENTS.md").contains("projects/custom-mapping/"),
        "init AGENTS.md 應指向自訂 vault_folder，而非 repo basename");
    assert!(sb.read("CLAUDE.md").contains("projects/custom-mapping/"),
        "init CLAUDE.md 應指向自訂 vault_folder");
}

#[test]
fn e2e_custom_vault_folder_in_sync() {
    let sb = Sandbox::new("customvfsync");
    let project = project_manager::add_project(&sb.repo_str(), &sb.data_dir).unwrap();
    project_manager::init_project(&project, Some(&sb.vault_dir())).unwrap();

    let vf = Some("projects/custom-mapping");
    let mem = mem_item(&project.id, "tech_stack", "Tech Stack", "Rust + Tauri", SyncScope::Project);
    let rule = mem_item(&project.id, "agent_rule", "Agent 規則", "先驗證再 commit", SyncScope::Project);

    // sync 以自訂 vault_folder 寫出（不退回 basename）
    let sync = agent_exporter::sync_agent_files(&sb.repo_str(), vf, Some(&sb.vault_dir()), &[mem.clone(), rule.clone()], &[mem.clone(), rule.clone()]).unwrap();
    assert!(sync.written_files.iter().any(|f| f.ends_with("AGENTS.md")));
    assert!(sb.read("AGENTS.md").contains("projects/custom-mapping/"),
        "sync AGENTS.md 應指向自訂 vault_folder");
    assert!(sb.read("CLAUDE.md").contains("projects/custom-mapping/"),
        "sync CLAUDE.md 應指向自訂 vault_folder");

    // preview 也應反映自訂 vault_folder
    let previews = agent_exporter::preview_sync_diff(&sb.repo_str(), vf, Some(&sb.vault_dir()), &[mem.clone(), rule.clone()], &[mem, rule]);
    assert!(previews.iter().any(|p| p.new_content.contains("projects/custom-mapping/")),
        "preview new_content 應指向自訂 vault_folder");
}

#[test]
fn e2e_gitignore_idempotent_and_preserves() {
    let sb = Sandbox::new("gi");
    let repo = sb.repo_str();
    // 預置既有 .gitignore（自訂規則）
    std::fs::write(format!("{}/.gitignore", repo), "node_modules\ndist\n").unwrap();

    // 第一次：補派生物規則
    assert!(project_manager::ensure_gitignore_rules(&repo).unwrap(), "首次應寫入");
    let gi = std::fs::read_to_string(format!("{}/.gitignore", repo)).unwrap();
    assert!(gi.contains("node_modules") && gi.contains("dist"), "既有規則應保留");
    assert!(gi.contains(".amagi/") && gi.contains(".codex/skills/") && gi.contains(".claude/skills/"),
        "派生物規則應補上");

    // 第二次：冪等，不再變更
    assert!(!project_manager::ensure_gitignore_rules(&repo).unwrap(), "二次不該再寫");
    let gi2 = std::fs::read_to_string(format!("{}/.gitignore", repo)).unwrap();
    assert_eq!(gi, gi2, "二次呼叫內容不變");
}

#[test]
fn e2e_gitignore_partial_and_no_false_ignores() {
    let sb = Sandbox::new("gi2");
    let repo = sb.repo_str();
    // 既有已含一條規則、且「無結尾換行」
    std::fs::write(format!("{}/.gitignore", repo), "target\n.amagi/").unwrap();

    assert!(project_manager::ensure_gitignore_rules(&repo).unwrap(), "應補缺少的兩條");
    let gi = std::fs::read_to_string(format!("{}/.gitignore", repo)).unwrap();

    assert!(gi.contains("target"), "既有規則保留");
    assert_eq!(gi.matches(".amagi/").count(), 1, ".amagi/ 不該重複");
    assert!(gi.contains(".codex/skills/") && gi.contains(".claude/skills/"), "補上另外兩條");
    // 不誤 ignore 根檔或整個目錄
    assert!(!gi.contains("AGENTS.md") && !gi.contains("CLAUDE.md"), "不該 ignore 根 agent 檔");
    assert!(!gi.lines().any(|l| matches!(l.trim(), ".claude/" | ".codex/")),
        "不該 ignore 整個 .claude/ 或 .codex/");
}

// ───────────────────────────────────────────────────────────
// 3. 技能同步 → vault _skills/<slug>/SKILL.md（Phase 3c·A：單一來源，不自動分發）
// ───────────────────────────────────────────────────────────
#[test]
fn e2e_skill_sync_native_format() {
    let sb = Sandbox::new("skill");
    let project = project_manager::add_project(&sb.repo_str(), &sb.data_dir).unwrap();
    project_manager::init_project(&project, Some(&sb.vault_dir())).unwrap();

    let content = "## 描述\n替遊戲狀態加入悔棋功能\n\n## 何時使用\n- 需要撤回上一步\n- 觸發關鍵字：undo、悔棋\n\n## 步驟\n1. 在 store 新增 history 陣列\n2. 實作 undo()";
    let skill = skill_item(&project.id, "新增悔棋功能", content);

    let sync = agent_exporter::sync_agent_files(
        &sb.repo_str(),
        project.vault_folder.as_deref(),
        Some(&sb.vault_dir()),
        std::slice::from_ref(&skill),
        &[],
    ).unwrap();
    assert!(sync.blocked_conflicts.is_empty());

    let slug = fs_utils::slugify(&skill.title);
    // 技能正本落 vault _skills（Phase 3c·A）
    let skill_path = sb.vault_dir().join("_skills").join(&slug).join("SKILL.md");
    assert!(skill_path.is_file(), "缺少 vault _skills/{}/SKILL.md", slug);
    assert!(sync.written_files.iter().any(|f| {
        let p = f.replace('\\', "/");
        p.contains("/_skills/") && p.ends_with("SKILL.md")
    }), "written_files 應含 vault _skills 正本");
    // sync 不再寫 .amagi/.codex/.claude（分發改由 Skills 頁）
    assert!(!sb.exists(&format!(".amagi/skills/{}.md", slug)), "3c 後不寫 .amagi/skills");
    assert!(!sb.exists(&format!(".codex/skills/{}/SKILL.md", slug)), "sync 不自動分發 .codex");
    assert!(!sb.exists(&format!(".claude/skills/{}/SKILL.md", slug)), "sync 不自動分發 .claude");

    // 原生 frontmatter：name / description / when_to_use 自動觸發欄位
    let md = std::fs::read_to_string(&skill_path).unwrap();
    assert!(md.starts_with("---\n"));
    assert!(md.contains("name: \"新增悔棋功能\""));
    assert!(md.contains("description:"));
    assert!(md.contains("when_to_use:"));
    assert!(md.contains("undo")); // when_to_use 應帶觸發關鍵字
    assert!(md.contains("## 步驟")); // 內文保留
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
    let candidates = learn_engine::generate_candidates("p-secret", &changed, "", diff, &Default::default());

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

    // 種佇列項：本專案 2 筆 + 他專案 1 筆——驗證 remove_project「連帶清佇列」的接線
    // （既有測試只驗專案消失、未驗佇列被清；此為補上的整合覆蓋）。
    let mk = |pid: &str, id: &str| ReviewItem {
        id: id.into(), project_id: pid.into(), item_type: ReviewItemType::Memory,
        category: "feedback".into(), title: id.into(), content: "x".into(),
        risk: RiskLevel::Low, status: ReviewStatus::Pending, sync_targets: vec![],
        sync_scope: SyncScope::Project, source_pending_file: None,
        blocked_hits: vec![],
        created_at: chrono::Utc::now(), reviewed_at: None,
    };
    review_queue::add_items(&sb.data_dir, vec![
        mk(&project.id, "own-1"), mk(&project.id, "own-2"), mk("other-proj", "keep-1"),
    ]).unwrap();

    project_manager::remove_project(&project.id, &sb.data_dir).unwrap();
    assert!(project_manager::list_projects(&sb.data_dir).is_empty());
    // 接線驗證：本專案佇列項應全清（孤兒項不再殘留）、他專案項不得被誤清。
    let remaining = review_queue::list_items(&sb.data_dir, None);
    assert!(!remaining.iter().any(|i| i.project_id == project.id), "移除專案應連帶清其佇列殘項");
    assert!(remaining.iter().any(|i| i.id == "keep-1"), "他專案佇列項不得被誤清");

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

// ───────────────────────────────────────────────────────────
// 8. 文件路由器：串真實 project + vault config + vault_folder 解析 → 落點
//    （補 Phase 2e 指令層零實機呼叫缺口；模擬 command 實際呼叫的底層鏈）
// ───────────────────────────────────────────────────────────
use crate::core::doc_router;
use crate::core::vault_manager::{self, VaultConfig};
use crate::utils::json_store;

#[test]
fn e2e_doc_router_routes_by_type_through_real_vault_config() {
    let sb = Sandbox::new("docrouter");
    // 在 sandbox 內建 temp vault（絕不碰真實 vault/家目錄）
    let vault = sb.data_dir.join("vault");
    std::fs::create_dir_all(&vault).unwrap();

    // 模擬指令層：寫 vault.json → get_vault_config 讀回 vault_root（與 set_vault_path 同落點，
    // 但不寫 ~/.claude、~/.codex，避免污染家目錄）
    let cfg = VaultConfig {
        vault_path: Some(vault.to_string_lossy().to_string()),
        pointer_written: true,
    };
    json_store::write_json(&sb.data_dir.join("vault.json"), &cfg).unwrap();
    let read = vault_manager::get_vault_config(&sb.data_dir);
    let vault_root = std::path::PathBuf::from(read.vault_path.expect("vault_root 應讀回"));

    // 真實 add_project → 取得 vault_folder；模擬指令層 resolve_project_folder 的 fallback
    let project = project_manager::add_project(&sb.repo_str(), &sb.data_dir).unwrap();
    let pf = project
        .vault_folder
        .clone()
        .unwrap_or_else(|| agent_exporter::project_vault_folder(&project.path));

    // adr → 專案 knowledge 桶
    let adr = "---\ntitle: 測試決策\ntype: adr\n---\n# 內文";
    let r1 = doc_router::route_document(&vault_root, Some(&pf), adr, None).unwrap();
    assert!(r1.written && !r1.skipped);
    assert_eq!(r1.decision.bucket, "knowledge");
    assert!(r1.destination.starts_with(&format!("{}/knowledge/", pf)));
    assert!(vault_root.join(&r1.destination).is_file(), "adr 應落地 knowledge 桶");

    // review → 專案 reports 桶
    let review = "---\ntitle: 某審查\ntype: review\n---\nx";
    let r2 = doc_router::route_document(&vault_root, Some(&pf), review, None).unwrap();
    assert_eq!(r2.decision.bucket, "reports");
    assert!(vault_root.join(&r2.destination).is_file());

    // handoff → 專案交接活頁 handoff.md（需專案、檔名固定、覆寫式快照）
    let handoff = "---\ntitle: 交接\ntype: handoff\n---\nx";
    let r3 = doc_router::route_document(&vault_root, Some(&pf), handoff, Some("ignored.md")).unwrap();
    assert_eq!(r3.decision.bucket, "handoff");
    assert_eq!(r3.destination, format!("{}/handoff.md", pf));
    assert!(vault_root.join(&r3.destination).is_file());
    // 覆寫式：再寫一次覆蓋舊內容、不略過（與其餘桶非破壞語意不同）
    let handoff2 = "---\ntitle: 交接\ntype: handoff\n---\nY2";
    let r3b = doc_router::route_document(&vault_root, Some(&pf), handoff2, None).unwrap();
    assert!(r3b.written && !r3b.skipped);
    assert!(std::fs::read_to_string(vault_root.join(&r3b.destination)).unwrap().contains("Y2"));
    // handoff 不再落頂層 daily：缺專案報錯
    assert!(doc_router::route_document(&vault_root, None, handoff, None).is_err());

    // 缺 type → 兜底 knowledge + fallback 標記
    let notype = "---\ntitle: 無型別\n---\nx";
    let r4 = doc_router::route_document(&vault_root, Some(&pf), notype, None).unwrap();
    assert!(r4.decision.is_fallback && r4.decision.bucket == "knowledge");

    // 非破壞：同 adr 再路由一次 → 略過、不覆寫
    let r5 = doc_router::route_document(&vault_root, Some(&pf), adr, None).unwrap();
    assert!(r5.skipped && !r5.written);

    // preview 乾跑不寫檔
    let prev = "---\ntitle: 只預覽\ntype: spec\n---\nx";
    let (d, dest) = doc_router::preview_route(Some(&pf), prev, None).unwrap();
    assert_eq!(d.bucket, "knowledge");
    assert!(!vault_root.join(&dest).exists(), "preview 不應寫檔");
}

// ───────────────────────────────────────────────────────────
// 9. init_project_vault 建三桶（2e-後：產生器改建 knowledge/reports，不再回退舊 pages/）
// ───────────────────────────────────────────────────────────
#[test]
fn e2e_init_project_vault_builds_three_buckets() {
    let sb = Sandbox::new("vaultinit");
    let vault = sb.data_dir.join("vault");
    std::fs::create_dir_all(&vault).unwrap();

    let mut project = project_manager::add_project(&sb.repo_str(), &sb.data_dir).unwrap();
    project.vault_folder = Some("projects/test-proj".to_string());

    let res = project_manager::init_project_vault(&project, &vault).unwrap();
    let base = vault.join("projects/test-proj");

    // 三桶：knowledge/、reports/ 主動建（含 .gitkeep）；不再建舊 pages/ 結構
    assert!(base.join("knowledge").is_dir(), "應建 knowledge/ 桶");
    assert!(base.join("reports").is_dir(), "應建 reports/ 桶");
    assert!(base.join("knowledge/.gitkeep").exists());
    assert!(base.join("reports/.gitkeep").exists());
    assert!(!base.join("pages").exists(), "不再建舊 pages/ 結構（2e-後 修正）");
    assert!(res.created_dirs.iter().any(|d| d.ends_with("knowledge")));

    // index.md 為三桶表，不含舊 pages 表
    let idx = std::fs::read_to_string(base.join("index.md")).unwrap();
    assert!(idx.contains("三桶"), "index 應為三桶結構說明");
    assert!(idx.contains("knowledge/") && idx.contains("reports/"));
    assert!(!idx.contains("pages/adr"), "index 不再含舊 pages 表");

    // 非破壞冪等：再 init 一次不重建目錄
    let res2 = project_manager::init_project_vault(&project, &vault).unwrap();
    assert!(res2.created_dirs.is_empty(), "二次 init 不應重建桶");
}
