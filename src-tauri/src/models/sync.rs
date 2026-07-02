use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanResult {
    pub project_id: String,
    pub branch: String,
    pub status_short: String,
    pub diff_stat: String,
    pub diff_text: String,
    pub recent_log: String,
    pub changed_files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LearnResult {
    pub project_id: String,
    pub candidates_generated: usize,
    /// 僅計真正被 safety_filter 封鎖的候選（ReviewItemType::Blocked）
    pub blocked_count: usize,
    /// 從 .amagi/pending/ 撈到的 Agent 技能草稿數（與封鎖無關，分開呈現）
    pub pending_skill_count: usize,
    pub candidate_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncResult {
    pub project_id: String,
    pub written_files: Vec<String>,
    pub skipped_files: Vec<String>,
    /// 若非空，代表同步「被擋下」（偵測到衝突，尚未寫入任何檔案）。
    /// 老爺需修正衝突，或以 force 重新同步放行。
    pub blocked_conflicts: Vec<ItemConflict>,
}

/// 某個待同步項目偵測到的衝突
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemConflict {
    pub item_id: String,
    pub item_title: String,
    /// 人話理由清單
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDiffPreview {
    pub file_path: String,
    pub current_content: Option<String>,
    pub new_content: String,
    pub is_new_file: bool,
}
