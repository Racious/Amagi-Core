use std::path::Path;
use chrono::Utc;
use uuid::Uuid;
use crate::AppError;
use crate::models::review::{ReviewItem, ReviewItemType, RiskLevel, ReviewStatus, SyncScope};
use crate::models::sync::PendingSkipped;
use crate::core::safety_filter;

/// pending 投遞通道規格：技能與記憶共用同一套解析骨架（frontmatter → ReviewItem），
/// 只在檔名前綴、產出型別、預設 category 與 sync_targets 上不同。
struct PendingKind {
    /// 只掃 `<prefix>*.md`
    prefix: &'static str,
    /// 人話名稱，用於日誌／通知訊息
    label: &'static str,
    item_type: ReviewItemType,
    /// frontmatter 未給 `category`（或給空值）時的預設。
    /// **不可為空字串**——`agent_exporter` 的 vault 記憶 loader 以
    /// `title.is_empty() || category.is_empty() || created.is_none()` 為格式守門
    /// （見 `agent_exporter.rs:208`），category 空的記憶會寫進 vault 卻被靜默跳過（記憶隱形）。
    default_category: &'static str,
    sync_targets: &'static [&'static str],
}

const SKILL_KIND: PendingKind = PendingKind {
    prefix: "skill-",
    label: "技能",
    item_type: ReviewItemType::Skill,
    default_category: "skill",
    sync_targets: &[".codex/skills", ".claude/commands"],
};

/// 記憶投遞通道（P1，2026-08-17）：AI 完成任務後在 after-task-review 階段寫
/// `.amagi/pending/memory-*.md`，經此掃入審核佇列 → 老爺核可 → 同步落 vault。
/// 補上「AI → Core 記憶」這條原本不存在的入口（原僅技能可投遞）。
const MEMORY_KIND: PendingKind = PendingKind {
    prefix: "memory-",
    label: "記憶",
    item_type: ReviewItemType::Memory,
    default_category: "agent_note",
    // 與 learn_engine::make_memory 的記憶候選一致（衍生物落點）
    sync_targets: &["AGENTS.md", "CLAUDE.md"],
};

/// 一次掃描的產物：入列候選 ＋ 被安全過濾擋下而未入列的檔（N3）。
pub struct PendingScan {
    pub items: Vec<ReviewItem>,
    pub skipped: Vec<PendingSkipped>,
}

/// 掃描 <project>/.amagi/pending/skill-*.md，
/// 解析 Agent 寫入的技能草稿，回傳候選 ReviewItem。
pub fn scan_pending_skills(
    project_path: &str,
    project_id: &str,
    existing_sources: &[String],  // 已在佇列中的來源檔路徑，避免重複加入
) -> Result<PendingScan, AppError> {
    scan_pending_kind(project_path, project_id, existing_sources, &SKILL_KIND)
}

/// 掃描 <project>/.amagi/pending/memory-*.md，
/// 解析 Agent 寫入的記憶草稿，回傳候選 ReviewItem（P1）。
pub fn scan_pending_memories(
    project_path: &str,
    project_id: &str,
    existing_sources: &[String],
) -> Result<PendingScan, AppError> {
    scan_pending_kind(project_path, project_id, existing_sources, &MEMORY_KIND)
}

fn scan_pending_kind(
    project_path: &str,
    project_id: &str,
    existing_sources: &[String],
    kind: &PendingKind,
) -> Result<PendingScan, AppError> {
    let pending_dir = Path::new(project_path).join(".amagi").join("pending");

    if !pending_dir.exists() {
        return Ok(PendingScan { items: vec![], skipped: vec![] });
    }

    let entries = std::fs::read_dir(&pending_dir)
        .map_err(|e| AppError::Io(e.to_string()))?;

    let mut items = Vec::new();
    let mut skipped = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();

        // 只處理 <prefix>*.md，忽略 README / AGENT_INSTRUCTIONS 等說明檔
        let name = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");
        if !name.starts_with(kind.prefix) || !name.ends_with(".md") {
            continue;
        }

        let path_str = path.to_string_lossy().to_string();

        // 若已在佇列中，跳過避免重複
        if existing_sources.contains(&path_str) {
            continue;
        }

        let raw = std::fs::read_to_string(&path)
            .map_err(|e| AppError::Io(e.to_string()))?;

        // 安全過濾：若含敏感資料，拒絕加入。
        // N3：除日誌外一律回報 skipped（僅檔名／通道／規則名稱，不帶命中值），
        // 否則此檔對老爺完全隱形——AI 以為已投遞、老爺以為沒有候選。
        let safety = safety_filter::check(&raw);
        if !safety.is_safe {
            eprintln!("[AMAGI] pending {} '{}' 含敏感資料，已略過", kind.label, name);
            let mut labels: Vec<String> = safety.hits.iter().map(|h| h.label.clone()).collect();
            labels.dedup();
            skipped.push(PendingSkipped {
                file_name: name.to_string(),
                kind: kind.label.to_string(),
                labels,
            });
            continue;
        }

        let (frontmatter, body) = split_frontmatter(&raw);
        // title/category 皆須「非空白才採用」：`title:` 後面留空會讓 extract_field 回
        // Some("")，若直接採用則寫出的記憶檔會被 vault loader 的格式守門跳過（見 PendingKind）。
        let title = extract_field(&frontmatter, "title")
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| name.trim_end_matches(".md").to_string());
        let category = extract_field(&frontmatter, "category")
            .map(|c| c.trim().to_string())
            .filter(|c| !c.is_empty())
            .unwrap_or_else(|| kind.default_category.to_string());
        let scope_str = extract_field(&frontmatter, "scope").unwrap_or_default();
        // 三層標籤驅動：project / shared / global；未知值保守 fallback Project（Codex 3b-2 #1）
        let sync_scope = match scope_str.to_lowercase().as_str() {
            "global" => SyncScope::Global,
            "shared" => SyncScope::Shared,
            _ => SyncScope::Project,
        };

        let item = ReviewItem {
            // id 一律後端產：不接受 pending 檔提供的任何 id，避免非 ASCII／惡意 id
            // 讓 `id_frag` 全部退成 "x" 而集中碰撞（見 agent_exporter.rs:676）。
            id: Uuid::new_v4().to_string(),
            project_id: project_id.to_string(),
            item_type: kind.item_type.clone(),
            category,
            title,
            content: body.trim().to_string(),
            risk: RiskLevel::Low,
            status: ReviewStatus::Pending,
            sync_targets: kind.sync_targets.iter().map(|s| s.to_string()).collect(),
            sync_scope,
            source_pending_file: Some(path_str),
            blocked_hits: vec![],
            created_at: Utc::now(),
            reviewed_at: None,
        };

        items.push(item);
    }

    Ok(PendingScan { items, skipped })
}

// ── 內部工具函數 ───────────────────────────────────────

/// 分割 YAML frontmatter（--- 之間）與正文
fn split_frontmatter(content: &str) -> (String, String) {
    let lines: Vec<&str> = content.lines().collect();
    if lines.first().map(|l| l.trim()) != Some("---") {
        return (String::new(), content.to_string());
    }

    let end = lines[1..].iter().position(|l| l.trim() == "---");
    match end {
        Some(i) => {
            let fm = lines[1..=i].join("\n");
            let body = lines[i + 2..].join("\n");
            (fm, body)
        }
        None => (String::new(), content.to_string()),
    }
}

/// 從 frontmatter 字串取出 key: value
fn extract_field(fm: &str, key: &str) -> Option<String> {
    for line in fm.lines() {
        if let Some(rest) = line.strip_prefix(&format!("{}:", key)) {
            return Some(rest.trim().to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_frontmatter() {
        let content = "---\ntitle: 測試技能\nscope: project\n---\n## 步驟\n1. 做某事\n";
        let (fm, body) = split_frontmatter(content);
        assert!(fm.contains("title: 測試技能"));
        assert!(body.contains("## 步驟"));
    }

    #[test]
    fn test_extract_field() {
        let fm = "title: 我的技能\nscope: global\n";
        assert_eq!(extract_field(fm, "title"), Some("我的技能".to_string()));
        assert_eq!(extract_field(fm, "scope"), Some("global".to_string()));
        assert_eq!(extract_field(fm, "missing"), None);
    }

    #[test]
    fn test_no_frontmatter() {
        let content = "## 步驟\n1. 沒有 frontmatter";
        let (fm, body) = split_frontmatter(content);
        assert!(fm.is_empty());
        assert!(body.contains("## 步驟"));
    }

    #[test]
    fn test_scan_parses_three_scopes() {
        // scope: project / shared / global / 未標 → 對應 SyncScope，未知 fallback Project（Codex 3b-2 #1）
        let root = std::env::temp_dir().join(format!("amagi-pending-{}", Uuid::new_v4()));
        let pending = root.join(".amagi").join("pending");
        std::fs::create_dir_all(&pending).unwrap();
        std::fs::write(pending.join("skill-s.md"), "---\ntitle: 共用技能\nscope: shared\n---\n## 描述\n做某事").unwrap();
        std::fs::write(pending.join("skill-g.md"), "---\ntitle: 全域技能\nscope: global\n---\n## 描述\n做某事").unwrap();
        std::fs::write(pending.join("skill-u.md"), "---\ntitle: 未標技能\n---\n## 描述\n做某事").unwrap();

        let items = scan_pending_skills(root.to_str().unwrap(), "p1", &[]).unwrap().items;
        let scope_of = |t: &str| items.iter().find(|i| i.title == t).unwrap().sync_scope.clone();
        assert_eq!(scope_of("共用技能"), SyncScope::Shared);
        assert_eq!(scope_of("全域技能"), SyncScope::Global);
        assert_eq!(scope_of("未標技能"), SyncScope::Project);

        let _ = std::fs::remove_dir_all(&root);
    }

    // ── P1 記憶投遞通道（MEMORY_KIND）─────────────────────

    fn seed(files: &[(&str, &str)]) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("amagi-pmem-{}", Uuid::new_v4()));
        let pending = root.join(".amagi").join("pending");
        std::fs::create_dir_all(&pending).unwrap();
        for (name, body) in files {
            std::fs::write(pending.join(name), body).unwrap();
        }
        root
    }

    #[test]
    fn test_scan_memories_three_scopes_and_defaults() {
        let root = seed(&[
            ("memory-p.md", "---\ntitle: 專案記憶\n---\nPS5.1 寫 JSON 要無 BOM"),
            ("memory-s.md", "---\ntitle: 共用記憶\nscope: shared\ncategory: gotcha\n---\n跨專案踩坑"),
            ("memory-g.md", "---\ntitle: 全域記憶\nscope: global\n---\n永遠適用"),
            ("memory-u.md", "---\ntitle: 未知範圍\nscope: 亂寫\n---\n內容"),
        ]);
        let items = scan_pending_memories(root.to_str().unwrap(), "p1", &[]).unwrap().items;
        assert_eq!(items.len(), 4);
        let of = |t: &str| items.iter().find(|i| i.title == t).unwrap();

        // scope 三層 + 未知值保守 fallback Project
        assert_eq!(of("專案記憶").sync_scope, SyncScope::Project);
        assert_eq!(of("共用記憶").sync_scope, SyncScope::Shared);
        assert_eq!(of("全域記憶").sync_scope, SyncScope::Global);
        assert_eq!(of("未知範圍").sync_scope, SyncScope::Project, "未知 scope 須保守退回專案層");

        // 型別／category／sync_targets／來源檔
        assert!(items.iter().all(|i| i.item_type == ReviewItemType::Memory));
        assert_eq!(of("專案記憶").category, "agent_note", "未給 category 應套預設");
        assert_eq!(of("共用記憶").category, "gotcha", "frontmatter 指定的 category 應生效");
        assert_eq!(of("專案記憶").sync_targets, vec!["AGENTS.md", "CLAUDE.md"]);
        assert!(of("專案記憶").content.contains("無 BOM"), "正文應為記憶內容");
        assert!(of("專案記憶").source_pending_file.as_deref().unwrap().ends_with("memory-p.md"));
        assert_eq!(of("專案記憶").status, ReviewStatus::Pending);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_scan_memories_id_is_backend_generated() {
        // 不接受 pending 檔自帶 id：一律後端 UUID，避免非 ASCII id 讓 id_frag 全退成 "x" 碰撞
        let root = seed(&[("memory-x.md", "---\ntitle: 帶 id 的投遞\nid: 中文識別碼\n---\n內容")]);
        let items = scan_pending_memories(root.to_str().unwrap(), "p1", &[]).unwrap().items;
        assert_eq!(items.len(), 1);
        assert_ne!(items[0].id, "中文識別碼", "不得採用 pending 檔提供的 id");
        assert!(Uuid::parse_str(&items[0].id).is_ok(), "id 須為後端產生的 UUID，實得 {}", items[0].id);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_scan_memories_blank_title_and_category_fall_back() {
        // `title:`／`category:` 留空 → extract_field 回 Some("")；若直接採用，
        // 寫出的記憶檔會被 vault loader 格式守門（title/category 非空）靜默跳過＝記憶隱形。
        let root = seed(&[("memory-blank.md", "---\ntitle:\ncategory:\n---\n正文仍在")]);
        let items = scan_pending_memories(root.to_str().unwrap(), "p1", &[]).unwrap().items;
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "memory-blank", "空白 title 須 fallback 檔名");
        assert_eq!(items[0].category, "agent_note", "空白 category 須 fallback 預設");
        assert!(!items[0].category.is_empty(), "category 絕不可為空（loader 會跳過該檔）");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_scan_memories_ignores_other_prefixes_and_dedups_by_source() {
        // 兩通道互不干擾：memory 掃描不得撈到 skill-*/說明檔；已在佇列的來源檔跳過
        let root = seed(&[
            ("memory-a.md", "---\ntitle: 記憶 A\n---\n內容"),
            ("skill-b.md", "---\ntitle: 技能 B\n---\n內容"),
            ("AGENT_INSTRUCTIONS.md", "# 指引"),
            ("README.md", "# 說明"),
        ]);
        let mems = scan_pending_memories(root.to_str().unwrap(), "p1", &[]).unwrap().items;
        assert_eq!(mems.len(), 1, "只該撈到 memory-*.md，實得 {:?}",
            mems.iter().map(|i| &i.title).collect::<Vec<_>>());
        assert_eq!(mems[0].title, "記憶 A");

        let skills = scan_pending_skills(root.to_str().unwrap(), "p1", &[]).unwrap().items;
        assert_eq!(skills.len(), 1, "技能通道不得被記憶檔污染");
        assert_eq!(skills[0].item_type, ReviewItemType::Skill);
        assert_eq!(skills[0].category, "skill", "技能未給 category 仍為原值，行為不退化");

        // 來源檔去重
        let src = mems[0].source_pending_file.clone().unwrap();
        let again = scan_pending_memories(root.to_str().unwrap(), "p1", &[src]).unwrap().items;
        assert!(again.is_empty(), "已在佇列的來源檔不應重複入列");

        let _ = std::fs::remove_dir_all(&root);
    }

    // ── N3 安全擋下可見化 ────────────────────────────────

    #[test]
    fn test_blocked_pending_is_reported_not_silent() {
        // 含疑似金鑰的投遞檔：不得入列，但必須出現在 skipped（原實作僅 eprintln → UI 全無感）
        let root = seed(&[
            ("memory-leak.md", "---\ntitle: 帶密鑰的記憶\n---\n用這個 key: sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"),
            ("memory-ok.md", "---\ntitle: 乾淨記憶\n---\n改用環境變數讀取憑證"),
        ]);
        let scan = scan_pending_memories(root.to_str().unwrap(), "p1", &[]).unwrap();

        assert_eq!(scan.items.len(), 1, "只有乾淨的那筆該入列");
        assert_eq!(scan.items[0].title, "乾淨記憶");
        assert_eq!(scan.skipped.len(), 1, "被擋下的檔必須回報，不可靜默");
        let sk = &scan.skipped[0];
        assert_eq!(sk.file_name, "memory-leak.md");
        assert_eq!(sk.kind, "記憶");
        assert!(!sk.labels.is_empty(), "須帶命中的規則名稱讓老爺知道為什麼被擋");

        // 不得把敏感原文（或其片段）搬進回報結構
        let dumped = format!("{sk:?}");
        assert!(!dumped.contains("sk-ant-api03"), "回報不得含敏感原文，實得 {dumped}");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_clean_scan_reports_no_skipped() {
        let root = seed(&[("memory-a.md", "---\ntitle: 乾淨\n---\n正常內容")]);
        let scan = scan_pending_memories(root.to_str().unwrap(), "p1", &[]).unwrap();
        assert_eq!(scan.items.len(), 1);
        assert!(scan.skipped.is_empty(), "全乾淨時不應有 skipped");
        let _ = std::fs::remove_dir_all(&root);
    }
}
