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
    /// 從 .amagi/pending/ 撈到的 Agent 記憶草稿數（P1 記憶投遞通道）
    pub pending_memory_count: usize,
    /// 被 safety_filter 擋下、**未**入列的 pending 投遞檔（N3）。
    /// 原實作僅 `eprintln!` 到 stderr、UI 完全無感——AI 以為已投遞、老爺以為沒有候選，
    /// 該筆知識無聲消失。故一律回報給前端呈現。
    pub pending_skipped: Vec<PendingSkipped>,
    pub candidate_ids: Vec<String>,
}

/// 被安全過濾擋下的 pending 投遞檔摘要（N3 可見化）。
/// 刻意**不帶內容也不帶遮罩值**——只給檔名、通道類型與命中的規則名稱，
/// 讓老爺知道「哪個檔因為什麼規則沒進來」，而不把敏感字串再搬一次到 UI／日誌。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingSkipped {
    pub file_name: String,
    /// 通道類型人話名稱（「技能」／「記憶」）
    pub kind: String,
    /// 命中的安全規則名稱（如「API Key」），不含命中值
    pub labels: Vec<String>,
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
