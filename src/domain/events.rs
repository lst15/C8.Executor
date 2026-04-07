use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventEnvelope {
    pub event_id: String,
    pub event_type: String,
    pub event_version: String,
    pub workspace_id: String,
    pub run_id: String,
    pub occurred_at: DateTime<Utc>,
    pub correlation_id: String,
    pub payload: Value,
}

pub const EXECUTION_RUN_OPENED: &str = "C8.ExecutionRunOpened";
pub const EXECUTION_STARTED: &str = "C8.ExecutionStarted";
pub const EXECUTION_STEP_REGISTERED: &str = "C8.ExecutionStepRegistered";
pub const EXECUTION_STEP_COMPLETED: &str = "C8.ExecutionStepCompleted";
pub const EXECUTION_ARTIFACT_REGISTERED: &str = "C8.ExecutionArtifactRegistered";
pub const EXECUTION_FAILED: &str = "C8.ExecutionFailed";
pub const EXECUTION_CANCELED: &str = "C8.ExecutionCanceled";
pub const EXECUTION_COMPLETED: &str = "C8.ExecutionCompleted";
pub const EXECUTION_RUN_CLOSED: &str = "C8.ExecutionRunClosed";
