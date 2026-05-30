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
    pub blocked_count: usize,
    pub candidate_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncResult {
    pub project_id: String,
    pub written_files: Vec<String>,
    pub skipped_files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDiffPreview {
    pub file_path: String,
    pub current_content: Option<String>,
    pub new_content: String,
    pub is_new_file: bool,
}
