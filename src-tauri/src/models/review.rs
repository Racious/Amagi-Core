use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

/// 同步範圍：專案層或全域（~/.codex / ~/.claude）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum SyncScope {
    Project,
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
    /// 預設 Project；切換為 Global 時寫入 ~/.codex/skills / ~/.claude/commands
    #[serde(default)]
    pub sync_scope: SyncScope,
    /// Agent 寫入 .amagi/pending/ 的來源檔路徑，同步後歸檔用
    #[serde(default)]
    pub source_pending_file: Option<String>,
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
