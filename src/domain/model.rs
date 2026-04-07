use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ExecutionStatus {
    Open,
    Running,
    Failed,
    Canceled,
    Completed,
    Closed,
}

impl ExecutionStatus {
    pub fn from_wire(value: &str) -> Option<Self> {
        match value {
            "open" => Some(Self::Open),
            "running" => Some(Self::Running),
            "failed" => Some(Self::Failed),
            "canceled" => Some(Self::Canceled),
            "completed" => Some(Self::Completed),
            "closed" => Some(Self::Closed),
            _ => None,
        }
    }

    pub fn as_wire(&self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Running => "running",
            Self::Failed => "failed",
            Self::Canceled => "canceled",
            Self::Completed => "completed",
            Self::Closed => "closed",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionRun {
    pub run_id: String,
    pub workspace_id: String,
    pub ops_session_id: String,
    pub status: ExecutionStatus,
    pub ready_for_execution: bool,
    pub started_at: Option<DateTime<Utc>>,
    pub ended_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub correlation_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum StepStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

impl StepStatus {
    pub fn from_wire(value: &str) -> Option<Self> {
        match value {
            "pending" => Some(Self::Pending),
            "running" => Some(Self::Running),
            "completed" => Some(Self::Completed),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }

    pub fn as_wire(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionStep {
    pub step_id: String,
    pub run_id: String,
    pub workspace_id: String,
    pub name: String,
    pub status: StepStatus,
    pub detail: Option<String>,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub correlation_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionArtifact {
    pub artifact_id: String,
    pub run_id: String,
    pub workspace_id: String,
    pub artifact_type: String,
    pub storage_ref: String,
    pub created_at: DateTime<Utc>,
    pub correlation_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionLog {
    pub log_id: String,
    pub run_id: String,
    pub workspace_id: String,
    pub level: String,
    pub message: String,
    pub occurred_at: DateTime<Utc>,
    pub correlation_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandAccepted {
    pub status: String,
    pub workspace_id: String,
    pub run_id: String,
    pub resource_id: Option<String>,
    pub correlation_id: String,
}
