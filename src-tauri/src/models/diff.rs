use serde::{Deserialize, Serialize};

/// 檔案異動狀態
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ChangedStatus {
    Modified,
    Added,
    Deleted,
    Renamed,
    Untracked,
}

/// 顯示分組：框1（局部異動）／框2（整檔新增刪除）
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DiffGroup {
    /// 框1：修改、改名
    Edited,
    /// 框2：新增、刪除
    AddedDeleted,
}

/// 單一異動檔
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangedFile {
    pub path: String,
    pub status: ChangedStatus,
    pub group: DiffGroup,
    /// 是否已暫存（git index）
    pub staged: bool,
}

/// 產生的 diff 文字（兩框）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffBundle {
    /// 框1：異動（修改／改名）
    pub edited_patch: String,
    /// 框2：新增／刪除
    pub added_deleted_patch: String,
    /// 被略過的檔（二進位／過大），含原因
    pub skipped: Vec<String>,
    /// 是否因總量上限而截斷
    pub truncated: bool,
}
