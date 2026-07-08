use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

/// 同步範圍：專案層或全域。sync 一律進 vault（Phase 3a/3c）；scope 影響 vault 落點層級
/// 與日後分發預設，不再於 sync 當下直接寫 ~/.codex / ~/.claude。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum SyncScope {
    Project,
    /// 跨專案共用（Phase 3b-2）：記憶落 vault `shared/agent/memory/`。
    Shared,
    Global,
}

impl Default for SyncScope {
    fn default() -> Self {
        SyncScope::Project
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum ReviewItemType {
    Memory,
    Skill,
    Blocked,
    /// vault 知識頁候選（adr-002 D8）：接受後由 wiki_exporter 寫入 vault。
    Wiki,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum ReviewStatus {
    Pending,
    Accepted,
    Ignored,
    Synced,
}

/// 封鎖卡的結構化命中（adr-007 §4.1）：權威資料，content 顯示文字由此 render。
/// `value_digest` 為身分（正規化後完整命中字串的 SHA-256）；`masked` 僅顯示。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockedHit {
    pub file_path: Option<String>,
    pub rule_label: String,
    pub masked: String,
    pub value_digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewItem {
    pub id: String,
    pub project_id: String,
    pub item_type: ReviewItemType,
    pub category: String,
    pub title: String,
    pub content: String,
    pub risk: RiskLevel,
    pub status: ReviewStatus,
    pub sync_targets: Vec<String>,
    /// 預設 Project；技能/記憶 sync 一律進 vault（`_skills` / `agent/memory`），
    /// 實際分發到 .codex/.claude 由 Skills 頁選擇性處理（Phase 3a/3c 後）
    #[serde(default)]
    pub sync_scope: SyncScope,
    /// Agent 寫入 .amagi/pending/ 的來源檔路徑，同步後歸檔用
    #[serde(default)]
    pub source_pending_file: Option<String>,
    /// Blocked 卡的結構化命中（adr-007 §4.1）；舊卡反序列化為空 → UI 禁用「誤判」鈕。
    #[serde(default)]
    pub blocked_hits: Vec<BlockedHit>,
    pub created_at: DateTime<Utc>,
    pub reviewed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewApplyResult {
    pub accepted_ids: Vec<String>,
    pub written_files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewQueueData {
    pub items: Vec<ReviewItem>,
}

impl Default for ReviewQueueData {
    fn default() -> Self {
        Self { items: Vec::new() }
    }
}
