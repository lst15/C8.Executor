use std::{collections::HashMap, sync::Mutex};

use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;

use crate::{
    domain::{
        error::AppError,
        events::EventEnvelope,
        model::{
            CommandAccepted, ExecutionArtifact, ExecutionLog, ExecutionRun, ExecutionStatus,
            ExecutionStep, StepStatus,
        },
    },
    ports::{EventPublisher, ExecutionRepository, IdempotencyRepository},
};

#[derive(Default)]
struct State {
    runs: HashMap<String, ExecutionRun>,
    steps: HashMap<String, ExecutionStep>,
    artifacts: HashMap<String, ExecutionArtifact>,
    logs: HashMap<String, ExecutionLog>,
    idempotency: HashMap<(String, String), CommandAccepted>,
    events: Vec<EventEnvelope>,
}

pub struct InMemoryPorts {
    state: Mutex<State>,
}

impl InMemoryPorts {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(State::default()),
        }
    }

    pub fn published_events(&self) -> Result<Vec<EventEnvelope>, AppError> {
        let guard = self.state.lock().map_err(|_| AppError::Internal)?;
        Ok(guard.events.clone())
    }
}

#[async_trait]
impl ExecutionRepository for InMemoryPorts {
    async fn create_run(&self, run: ExecutionRun) -> Result<(), AppError> {
        let mut guard = self.state.lock().map_err(|_| AppError::Internal)?;
        guard.runs.insert(run.run_id.clone(), run);
        Ok(())
    }

    async fn get_run(&self, run_id: &str) -> Result<Option<ExecutionRun>, AppError> {
        let guard = self.state.lock().map_err(|_| AppError::Internal)?;
        Ok(guard.runs.get(run_id).cloned())
    }

    async fn update_run_status(
        &self,
        run_id: &str,
        status: &str,
        started_at: Option<chrono::DateTime<chrono::Utc>>,
        ended_at: Option<chrono::DateTime<chrono::Utc>>,
        correlation_id: &str,
    ) -> Result<ExecutionRun, AppError> {
        let mut guard = self.state.lock().map_err(|_| AppError::Internal)?;
        let run = guard
            .runs
            .get_mut(run_id)
            .ok_or_else(|| AppError::NotFound(format!("run {} nao encontrada", run_id)))?;

        run.status = ExecutionStatus::from_wire(status)
            .ok_or_else(|| AppError::Validation(format!("status invalido: {}", status)))?;
        run.started_at = started_at;
        run.ended_at = ended_at;
        run.updated_at = Utc::now();
        run.correlation_id = correlation_id.to_string();

        Ok(run.clone())
    }

    async fn add_step(
        &self,
        run_id: &str,
        workspace_id: &str,
        step_id: &str,
        name: &str,
        status: &str,
        detail: Option<&str>,
        correlation_id: &str,
    ) -> Result<String, AppError> {
        let mut guard = self.state.lock().map_err(|_| AppError::Internal)?;
        if guard.steps.contains_key(step_id) {
            return Err(AppError::Conflict(format!("step {} ja existe", step_id)));
        }

        let step = ExecutionStep {
            step_id: step_id.to_string(),
            run_id: run_id.to_string(),
            workspace_id: workspace_id.to_string(),
            name: name.to_string(),
            status: StepStatus::from_wire(status)
                .ok_or_else(|| AppError::Validation(format!("status invalido: {}", status)))?,
            detail: detail.map(|v| v.to_string()),
            created_at: Utc::now(),
            completed_at: None,
            correlation_id: correlation_id.to_string(),
        };

        guard.steps.insert(step_id.to_string(), step);
        Ok(step_id.to_string())
    }

    async fn complete_step(
        &self,
        run_id: &str,
        step_id: &str,
        status: &str,
        detail: Option<&str>,
        correlation_id: &str,
    ) -> Result<ExecutionStep, AppError> {
        let mut guard = self.state.lock().map_err(|_| AppError::Internal)?;
        let step = guard
            .steps
            .get_mut(step_id)
            .ok_or_else(|| AppError::NotFound(format!("step {} nao encontrada", step_id)))?;

        if step.run_id != run_id {
            return Err(AppError::Validation(
                "step nao pertence a run informada".to_string(),
            ));
        }

        step.status = StepStatus::from_wire(status)
            .ok_or_else(|| AppError::Validation(format!("status invalido: {}", status)))?;
        step.detail = detail
            .map(|v| v.to_string())
            .or_else(|| step.detail.clone());
        step.completed_at = Some(Utc::now());
        step.correlation_id = correlation_id.to_string();

        Ok(step.clone())
    }

    async fn add_artifact(
        &self,
        run_id: &str,
        workspace_id: &str,
        artifact_id: &str,
        artifact_type: &str,
        storage_ref: &str,
        correlation_id: &str,
    ) -> Result<String, AppError> {
        let mut guard = self.state.lock().map_err(|_| AppError::Internal)?;
        if guard.artifacts.contains_key(artifact_id) {
            return Err(AppError::Conflict(format!(
                "artifact {} ja existe",
                artifact_id
            )));
        }

        let artifact = ExecutionArtifact {
            artifact_id: artifact_id.to_string(),
            run_id: run_id.to_string(),
            workspace_id: workspace_id.to_string(),
            artifact_type: artifact_type.to_string(),
            storage_ref: storage_ref.to_string(),
            created_at: Utc::now(),
            correlation_id: correlation_id.to_string(),
        };

        guard.artifacts.insert(artifact_id.to_string(), artifact);
        Ok(artifact_id.to_string())
    }

    async fn add_log(
        &self,
        run_id: &str,
        workspace_id: &str,
        level: &str,
        message: &str,
        correlation_id: &str,
    ) -> Result<String, AppError> {
        let mut guard = self.state.lock().map_err(|_| AppError::Internal)?;
        let log_id = format!("log_{}", Uuid::new_v4().simple());
        let log = ExecutionLog {
            log_id: log_id.clone(),
            run_id: run_id.to_string(),
            workspace_id: workspace_id.to_string(),
            level: level.to_string(),
            message: message.to_string(),
            occurred_at: Utc::now(),
            correlation_id: correlation_id.to_string(),
        };

        guard.logs.insert(log_id.clone(), log);
        Ok(log_id)
    }

    async fn list_steps(&self, run_id: &str) -> Result<Vec<ExecutionStep>, AppError> {
        let guard = self.state.lock().map_err(|_| AppError::Internal)?;
        let mut list: Vec<ExecutionStep> = guard
            .steps
            .values()
            .filter(|s| s.run_id == run_id)
            .cloned()
            .collect();
        list.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        Ok(list)
    }

    async fn list_logs(&self, run_id: &str) -> Result<Vec<ExecutionLog>, AppError> {
        let guard = self.state.lock().map_err(|_| AppError::Internal)?;
        let mut list: Vec<ExecutionLog> = guard
            .logs
            .values()
            .filter(|l| l.run_id == run_id)
            .cloned()
            .collect();
        list.sort_by(|a, b| a.occurred_at.cmp(&b.occurred_at));
        Ok(list)
    }

    async fn list_artifacts(&self, run_id: &str) -> Result<Vec<ExecutionArtifact>, AppError> {
        let guard = self.state.lock().map_err(|_| AppError::Internal)?;
        let mut list: Vec<ExecutionArtifact> = guard
            .artifacts
            .values()
            .filter(|a| a.run_id == run_id)
            .cloned()
            .collect();
        list.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        Ok(list)
    }

    async fn count_failed_steps(&self, run_id: &str) -> Result<i64, AppError> {
        let guard = self.state.lock().map_err(|_| AppError::Internal)?;
        let count = guard
            .steps
            .values()
            .filter(|s| s.run_id == run_id && s.status == StepStatus::Failed)
            .count() as i64;
        Ok(count)
    }
}

#[async_trait]
impl IdempotencyRepository for InMemoryPorts {
    async fn get(&self, scope: &str, key: &str) -> Result<Option<CommandAccepted>, AppError> {
        let guard = self.state.lock().map_err(|_| AppError::Internal)?;
        Ok(guard
            .idempotency
            .get(&(scope.to_string(), key.to_string()))
            .cloned())
    }

    async fn put(&self, scope: &str, key: &str, value: CommandAccepted) -> Result<(), AppError> {
        let mut guard = self.state.lock().map_err(|_| AppError::Internal)?;
        guard
            .idempotency
            .insert((scope.to_string(), key.to_string()), value);
        Ok(())
    }
}

#[async_trait]
impl EventPublisher for InMemoryPorts {
    async fn publish(&self, event: EventEnvelope) -> Result<(), AppError> {
        let mut guard = self.state.lock().map_err(|_| AppError::Internal)?;
        guard.events.push(event);
        Ok(())
    }
}
