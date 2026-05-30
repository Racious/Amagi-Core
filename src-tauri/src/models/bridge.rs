use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

/// File Bridge 流程的整體狀態
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum BridgeRunStatus {
    /// 等待 AI 執行當前步驟並寫回 result.md
    AwaitingResult,
    /// 所有步驟完成
    Done,
    /// 使用者中止
    Cancelled,
}

/// 單一步驟的狀態
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum BridgeStepStatus {
    Pending,
    Active,
    Done,
}

/// File Bridge 流程中的一個步驟
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeStep {
    pub id: String,
    pub name: String,
    /// 給 AI 的指示（這一步要做什麼）
    pub instruction: String,
    pub status: BridgeStepStatus,
    /// AI 從 result.md 寫回的結果
    pub result: Option<String>,
}

/// 一次完整的 File Bridge 流程執行
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeRun {
    pub id: String,
    pub project_id: String,
    pub project_path: String,
    pub workflow_id: String,
    pub workflow_name: String,
    /// 使用者輸入的任務描述
    pub task: String,
    pub steps: Vec<BridgeStep>,
    /// 目前進行到第幾步（索引）
    pub current_step: usize,
    pub status: BridgeRunStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
