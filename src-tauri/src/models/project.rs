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
    /// 專案目錄是否仍可作為分發目標（與後端 distribute 的 is_dir 判斷一致）。
    /// false → 目錄不存在或被同名檔案取代（如「幽靈專案」：projects.json 有記錄但目錄已刪），
    /// 前端據此標示/停用分發，避免靜默分發失敗。
    pub path_exists: bool,
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
