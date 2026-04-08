use async_trait::async_trait;

use crate::domain::{
    error::AppError,
    events::EventEnvelope,
    model::{CommandAccepted, ExecutionArtifact, ExecutionLog, ExecutionRun, ExecutionStep},
};

#[async_trait]
pub trait ExecutionRepository: Send + Sync {
    async fn create_run(&self, run: ExecutionRun) -> Result<(), AppError>;
    async fn get_run(&self, run_id: &str) -> Result<Option<ExecutionRun>, AppError>;
    async fn update_run_status(
        &self,
        run_id: &str,
        status: &str,
        started_at: Option<chrono::DateTime<chrono::Utc>>,
        ended_at: Option<chrono::DateTime<chrono::Utc>>,
        correlation_id: &str,
    ) -> Result<ExecutionRun, AppError>;

    async fn add_step(
        &self,
        run_id: &str,
        workspace_id: &str,
        step_id: &str,
        name: &str,
        status: &str,
        detail: Option<&str>,
        correlation_id: &str,
    ) -> Result<String, AppError>;
    async fn complete_step(
        &self,
        run_id: &str,
        step_id: &str,
        status: &str,
        detail: Option<&str>,
        correlation_id: &str,
    ) -> Result<ExecutionStep, AppError>;

    async fn add_artifact(
        &self,
        run_id: &str,
        workspace_id: &str,
        artifact_id: &str,
        artifact_type: &str,
        storage_ref: &str,
        correlation_id: &str,
    ) -> Result<String, AppError>;

    async fn add_log(
        &self,
        run_id: &str,
        workspace_id: &str,
        level: &str,
        message: &str,
        correlation_id: &str,
    ) -> Result<String, AppError>;

    async fn list_steps(&self, run_id: &str) -> Result<Vec<ExecutionStep>, AppError>;
    async fn list_logs(&self, run_id: &str) -> Result<Vec<ExecutionLog>, AppError>;
    async fn list_artifacts(&self, run_id: &str) -> Result<Vec<ExecutionArtifact>, AppError>;
    async fn count_failed_steps(&self, run_id: &str) -> Result<i64, AppError>;
}

#[async_trait]
pub trait IdempotencyRepository: Send + Sync {
    async fn get(&self, scope: &str, key: &str) -> Result<Option<CommandAccepted>, AppError>;
    async fn put(&self, scope: &str, key: &str, value: CommandAccepted) -> Result<(), AppError>;
}

#[async_trait]
pub trait EventPublisher: Send + Sync {
    async fn publish(&self, event: EventEnvelope) -> Result<(), AppError>;
}
