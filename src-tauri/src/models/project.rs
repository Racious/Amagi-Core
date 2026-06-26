use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    pub path: String,
    pub created_at: DateTime<Utc>,
    pub last_scanned_at: Option<DateTime<Utc>>,
    pub initialized: bool,
    /// 對應的 vault 知識資料夾（相對 vault root，如 "projects/amagi-core"）。
    #[serde(default)]
    pub vault_folder: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectInfo {
    pub id: String,
    pub name: String,
    pub path: String,
    pub is_git_repo: bool,
    pub current_branch: Option<String>,
    pub initialized: bool,
    pub pending_review_count: usize,
    pub vault_folder: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitResult {
    pub project_id: String,
    pub created_dirs: Vec<String>,
    pub created_files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectsData {
    pub projects: Vec<Project>,
}

impl Default for ProjectsData {
    fn default() -> Self {
        Self { projects: Vec::new() }
    }
}
