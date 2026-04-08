use std::sync::Arc;

use chrono::Utc;
use serde_json::json;
use uuid::Uuid;

use crate::{
    domain::{
        error::AppError,
        events::{
            EventEnvelope, EXECUTION_ARTIFACT_REGISTERED, EXECUTION_CANCELED, EXECUTION_COMPLETED,
            EXECUTION_FAILED, EXECUTION_RUN_CLOSED, EXECUTION_RUN_OPENED, EXECUTION_STARTED,
            EXECUTION_STEP_COMPLETED, EXECUTION_STEP_REGISTERED,
        },
        model::{
            CommandAccepted, ExecutionArtifact, ExecutionLog, ExecutionRun, ExecutionStatus,
            ExecutionStep, StepStatus,
        },
    },
    ports::{EventPublisher, ExecutionRepository, IdempotencyRepository},
};

#[derive(Debug, Clone, Copy)]
struct StepsSummary {
    total: usize,
    pending: usize,
    running: usize,
    completed: usize,
    failed: usize,
}

#[derive(Debug, Clone)]
pub struct OpenRunCommand {
    pub run_id: Option<String>,
    pub workspace_id: String,
    pub ops_session_id: String,
    pub ready_for_execution: bool,
    pub idempotency_key: String,
    pub correlation_id: String,
}

#[derive(Debug, Clone)]
pub struct StartRunCommand {
    pub run_id: String,
    pub idempotency_key: String,
    pub correlation_id: String,
}

#[derive(Debug, Clone)]
pub struct AddStepCommand {
    pub run_id: String,
    pub step_id: Option<String>,
    pub name: String,
    pub detail: Option<String>,
    pub idempotency_key: String,
    pub correlation_id: String,
}

#[derive(Debug, Clone)]
pub struct CompleteStepCommand {
    pub run_id: String,
    pub step_id: String,
    pub status: String,
    pub detail: Option<String>,
    pub idempotency_key: String,
    pub correlation_id: String,
}

#[derive(Debug, Clone)]
pub struct AddArtifactCommand {
    pub run_id: String,
    pub artifact_id: Option<String>,
    pub artifact_type: String,
    pub storage_ref: String,
    pub idempotency_key: String,
    pub correlation_id: String,
}

#[derive(Debug, Clone)]
pub struct CancelRunCommand {
    pub run_id: String,
    pub reason: Option<String>,
    pub idempotency_key: String,
    pub correlation_id: String,
}

#[derive(Debug, Clone)]
pub struct CloseRunCommand {
    pub run_id: String,
    pub reason: Option<String>,
    pub idempotency_key: String,
    pub correlation_id: String,
}

pub struct ExecutorService {
    repository: Arc<dyn ExecutionRepository>,
    idempotency: Arc<dyn IdempotencyRepository>,
    publisher: Arc<dyn EventPublisher>,
}

impl ExecutorService {
    pub fn new(
        repository: Arc<dyn ExecutionRepository>,
        idempotency: Arc<dyn IdempotencyRepository>,
        publisher: Arc<dyn EventPublisher>,
    ) -> Self {
        Self {
            repository,
            idempotency,
            publisher,
        }
    }

    pub async fn open_run(&self, cmd: OpenRunCommand) -> Result<CommandAccepted, AppError> {
        if cmd.workspace_id.trim().is_empty() || cmd.ops_session_id.trim().is_empty() {
            return Err(AppError::Validation(
                "workspace_id e ops_session_id sao obrigatorios".to_string(),
            ));
        }

        let run_id = cmd
            .run_id
            .clone()
            .filter(|v| !v.trim().is_empty())
            .unwrap_or_else(|| format!("run_{}", Uuid::new_v4().simple()));

        let scope = format!("execution:open:{}", run_id);
        if let Some(existing) = self.idempotency.get(&scope, &cmd.idempotency_key).await? {
            return Ok(existing);
        }

        if self.repository.get_run(&run_id).await?.is_some() {
            return Err(AppError::Conflict(format!("run {} ja existe", run_id)));
        }

        let now = Utc::now();
        self.repository
            .create_run(ExecutionRun {
                run_id: run_id.clone(),
                workspace_id: cmd.workspace_id.clone(),
                ops_session_id: cmd.ops_session_id.clone(),
                status: ExecutionStatus::Open,
                ready_for_execution: cmd.ready_for_execution,
                started_at: None,
                ended_at: None,
                created_at: now,
                updated_at: now,
                correlation_id: cmd.correlation_id.clone(),
            })
            .await?;

        self.publisher
            .publish(EventEnvelope {
                event_id: Uuid::new_v4().to_string(),
                event_type: EXECUTION_RUN_OPENED.to_string(),
                event_version: "1.0".to_string(),
                workspace_id: cmd.workspace_id.clone(),
                run_id: run_id.clone(),
                occurred_at: Utc::now(),
                correlation_id: cmd.correlation_id.clone(),
                payload: json!({
                    "ops_session_id": cmd.ops_session_id,
                    "status": "open",
                    "ready_for_execution": cmd.ready_for_execution,
                }),
            })
            .await?;

        let accepted = CommandAccepted {
            status: "accepted".to_string(),
            workspace_id: cmd.workspace_id,
            run_id,
            resource_id: None,
            correlation_id: cmd.correlation_id,
        };

        self.idempotency
            .put(&scope, &cmd.idempotency_key, accepted.clone())
            .await?;

        Ok(accepted)
    }

    pub async fn start_run(&self, cmd: StartRunCommand) -> Result<CommandAccepted, AppError> {
        if cmd.run_id.trim().is_empty() {
            return Err(AppError::Validation("run_id e obrigatorio".to_string()));
        }

        let scope = format!("execution:start:{}", cmd.run_id);
        if let Some(existing) = self.idempotency.get(&scope, &cmd.idempotency_key).await? {
            return Ok(existing);
        }

        let run = self.ensure_run_mutable(&cmd.run_id).await?;
        if !run.ready_for_execution {
            return Err(AppError::Validation(
                "run nao pode iniciar sem ready_for_execution=true".to_string(),
            ));
        }

        let started = self
            .repository
            .update_run_status(
                &cmd.run_id,
                ExecutionStatus::Running.as_wire(),
                Some(Utc::now()),
                None,
                &cmd.correlation_id,
            )
            .await?;

        self.repository
            .add_log(
                &started.run_id,
                &started.workspace_id,
                "info",
                "execution run started",
                &cmd.correlation_id,
            )
            .await?;

        self.publisher
            .publish(EventEnvelope {
                event_id: Uuid::new_v4().to_string(),
                event_type: EXECUTION_STARTED.to_string(),
                event_version: "1.0".to_string(),
                workspace_id: started.workspace_id.clone(),
                run_id: started.run_id.clone(),
                occurred_at: Utc::now(),
                correlation_id: cmd.correlation_id.clone(),
                payload: json!({
                    "status": started.status.as_wire(),
                    "started_at": started.started_at,
                }),
            })
            .await?;

        let accepted = CommandAccepted {
            status: "accepted".to_string(),
            workspace_id: started.workspace_id,
            run_id: started.run_id,
            resource_id: None,
            correlation_id: cmd.correlation_id,
        };

        self.idempotency
            .put(&scope, &cmd.idempotency_key, accepted.clone())
            .await?;

        Ok(accepted)
    }

    pub async fn add_step(&self, cmd: AddStepCommand) -> Result<CommandAccepted, AppError> {
        if cmd.run_id.trim().is_empty() || cmd.name.trim().is_empty() {
            return Err(AppError::Validation(
                "run_id e name sao obrigatorios".to_string(),
            ));
        }

        let step_id = cmd
            .step_id
            .clone()
            .filter(|v| !v.trim().is_empty())
            .unwrap_or_else(|| format!("stp_{}", Uuid::new_v4().simple()));

        let scope = format!("execution:step:{}:{}", cmd.run_id, step_id);
        if let Some(existing) = self.idempotency.get(&scope, &cmd.idempotency_key).await? {
            return Ok(existing);
        }

        let run = self.ensure_run_mutable(&cmd.run_id).await?;
        if run.status != ExecutionStatus::Running {
            return Err(AppError::Conflict(
                "step so pode ser criado quando run estiver running".to_string(),
            ));
        }

        let inserted = self
            .repository
            .add_step(
                &cmd.run_id,
                &run.workspace_id,
                &step_id,
                cmd.name.trim(),
                StepStatus::Running.as_wire(),
                cmd.detail.as_deref(),
                &cmd.correlation_id,
            )
            .await?;

        self.publisher
            .publish(EventEnvelope {
                event_id: Uuid::new_v4().to_string(),
                event_type: EXECUTION_STEP_REGISTERED.to_string(),
                event_version: "1.0".to_string(),
                workspace_id: run.workspace_id.clone(),
                run_id: run.run_id.clone(),
                occurred_at: Utc::now(),
                correlation_id: cmd.correlation_id.clone(),
                payload: json!({
                    "step_id": inserted,
                    "name": cmd.name,
                    "status": "running",
                }),
            })
            .await?;

        let accepted = CommandAccepted {
            status: "accepted".to_string(),
            workspace_id: run.workspace_id,
            run_id: run.run_id,
            resource_id: Some(inserted),
            correlation_id: cmd.correlation_id,
        };

        self.idempotency
            .put(&scope, &cmd.idempotency_key, accepted.clone())
            .await?;

        Ok(accepted)
    }

    pub async fn complete_step(
        &self,
        cmd: CompleteStepCommand,
    ) -> Result<CommandAccepted, AppError> {
        if cmd.run_id.trim().is_empty()
            || cmd.step_id.trim().is_empty()
            || cmd.status.trim().is_empty()
        {
            return Err(AppError::Validation(
                "run_id, step_id e status sao obrigatorios".to_string(),
            ));
        }

        let status = StepStatus::from_wire(cmd.status.trim())
            .ok_or_else(|| AppError::Validation(format!("status invalido: {}", cmd.status)))?;

        if status != StepStatus::Completed && status != StepStatus::Failed {
            return Err(AppError::Validation(
                "complete_step aceita somente completed/failed".to_string(),
            ));
        }

        let scope = format!("execution:step-complete:{}:{}", cmd.run_id, cmd.step_id);
        if let Some(existing) = self.idempotency.get(&scope, &cmd.idempotency_key).await? {
            return Ok(existing);
        }

        let run = self.ensure_run_mutable(&cmd.run_id).await?;

        let completed = self
            .repository
            .complete_step(
                &cmd.run_id,
                &cmd.step_id,
                status.as_wire(),
                cmd.detail.as_deref(),
                &cmd.correlation_id,
            )
            .await?;

        if status == StepStatus::Failed {
            self.repository
                .update_run_status(
                    &cmd.run_id,
                    ExecutionStatus::Failed.as_wire(),
                    run.started_at,
                    Some(Utc::now()),
                    &cmd.correlation_id,
                )
                .await?;

            self.publisher
                .publish(EventEnvelope {
                    event_id: Uuid::new_v4().to_string(),
                    event_type: EXECUTION_FAILED.to_string(),
                    event_version: "1.0".to_string(),
                    workspace_id: run.workspace_id.clone(),
                    run_id: run.run_id.clone(),
                    occurred_at: Utc::now(),
                    correlation_id: cmd.correlation_id.clone(),
                    payload: json!({
                        "step_id": completed.step_id,
                        "status": "failed",
                        "detail": completed.detail,
                    }),
                })
                .await?;
        } else {
            self.publisher
                .publish(EventEnvelope {
                    event_id: Uuid::new_v4().to_string(),
                    event_type: EXECUTION_STEP_COMPLETED.to_string(),
                    event_version: "1.0".to_string(),
                    workspace_id: run.workspace_id.clone(),
                    run_id: run.run_id.clone(),
                    occurred_at: Utc::now(),
                    correlation_id: cmd.correlation_id.clone(),
                    payload: json!({
                        "step_id": completed.step_id,
                        "status": completed.status.as_wire(),
                    }),
                })
                .await?;
        }

        let accepted = CommandAccepted {
            status: "accepted".to_string(),
            workspace_id: run.workspace_id,
            run_id: run.run_id,
            resource_id: Some(completed.step_id),
            correlation_id: cmd.correlation_id,
        };

        self.idempotency
            .put(&scope, &cmd.idempotency_key, accepted.clone())
            .await?;

        Ok(accepted)
    }

    pub async fn add_artifact(&self, cmd: AddArtifactCommand) -> Result<CommandAccepted, AppError> {
        if cmd.run_id.trim().is_empty()
            || cmd.artifact_type.trim().is_empty()
            || cmd.storage_ref.trim().is_empty()
        {
            return Err(AppError::Validation(
                "run_id, artifact_type e storage_ref sao obrigatorios".to_string(),
            ));
        }

        let artifact_id = cmd
            .artifact_id
            .clone()
            .filter(|v| !v.trim().is_empty())
            .unwrap_or_else(|| format!("art_{}", Uuid::new_v4().simple()));

        let scope = format!("execution:artifact:{}:{}", cmd.run_id, artifact_id);
        if let Some(existing) = self.idempotency.get(&scope, &cmd.idempotency_key).await? {
            return Ok(existing);
        }

        let run = self.ensure_run_exists(&cmd.run_id).await?;

        let inserted = self
            .repository
            .add_artifact(
                &cmd.run_id,
                &run.workspace_id,
                &artifact_id,
                cmd.artifact_type.trim(),
                cmd.storage_ref.trim(),
                &cmd.correlation_id,
            )
            .await?;

        self.publisher
            .publish(EventEnvelope {
                event_id: Uuid::new_v4().to_string(),
                event_type: EXECUTION_ARTIFACT_REGISTERED.to_string(),
                event_version: "1.0".to_string(),
                workspace_id: run.workspace_id.clone(),
                run_id: run.run_id.clone(),
                occurred_at: Utc::now(),
                correlation_id: cmd.correlation_id.clone(),
                payload: json!({
                    "artifact_id": inserted,
                    "artifact_type": cmd.artifact_type,
                    "storage_ref": cmd.storage_ref,
                }),
            })
            .await?;

        let accepted = CommandAccepted {
            status: "accepted".to_string(),
            workspace_id: run.workspace_id,
            run_id: run.run_id,
            resource_id: Some(inserted),
            correlation_id: cmd.correlation_id,
        };

        self.idempotency
            .put(&scope, &cmd.idempotency_key, accepted.clone())
            .await?;

        Ok(accepted)
    }

    pub async fn cancel_run(&self, cmd: CancelRunCommand) -> Result<CommandAccepted, AppError> {
        if cmd.run_id.trim().is_empty() {
            return Err(AppError::Validation("run_id e obrigatorio".to_string()));
        }

        let scope = format!("execution:cancel:{}", cmd.run_id);
        if let Some(existing) = self.idempotency.get(&scope, &cmd.idempotency_key).await? {
            return Ok(existing);
        }

        let run = self.ensure_run_mutable(&cmd.run_id).await?;

        let canceled = self
            .repository
            .update_run_status(
                &cmd.run_id,
                ExecutionStatus::Canceled.as_wire(),
                run.started_at,
                Some(Utc::now()),
                &cmd.correlation_id,
            )
            .await?;

        self.publisher
            .publish(EventEnvelope {
                event_id: Uuid::new_v4().to_string(),
                event_type: EXECUTION_CANCELED.to_string(),
                event_version: "1.0".to_string(),
                workspace_id: canceled.workspace_id.clone(),
                run_id: canceled.run_id.clone(),
                occurred_at: Utc::now(),
                correlation_id: cmd.correlation_id.clone(),
                payload: json!({
                    "reason": cmd.reason,
                    "status": canceled.status.as_wire(),
                }),
            })
            .await?;

        let accepted = CommandAccepted {
            status: "accepted".to_string(),
            workspace_id: canceled.workspace_id,
            run_id: canceled.run_id,
            resource_id: None,
            correlation_id: cmd.correlation_id,
        };

        self.idempotency
            .put(&scope, &cmd.idempotency_key, accepted.clone())
            .await?;

        Ok(accepted)
    }

    pub async fn close_run(&self, cmd: CloseRunCommand) -> Result<CommandAccepted, AppError> {
        if cmd.run_id.trim().is_empty() {
            return Err(AppError::Validation("run_id e obrigatorio".to_string()));
        }

        let scope = format!("execution:close:{}", cmd.run_id);
        if let Some(existing) = self.idempotency.get(&scope, &cmd.idempotency_key).await? {
            return Ok(existing);
        }

        let run = self.ensure_run_exists(&cmd.run_id).await?;

        if run.status == ExecutionStatus::Closed {
            return Err(AppError::Conflict("run ja encerrada".to_string()));
        }

        let failed_steps = self.repository.count_failed_steps(&cmd.run_id).await?;
        let status = if failed_steps > 0 {
            ExecutionStatus::Failed
        } else if run.status == ExecutionStatus::Canceled {
            ExecutionStatus::Canceled
        } else {
            ExecutionStatus::Completed
        };

        let ended = self
            .repository
            .update_run_status(
                &cmd.run_id,
                status.as_wire(),
                run.started_at,
                Some(Utc::now()),
                &cmd.correlation_id,
            )
            .await?;

        let steps = self.repository.list_steps(&cmd.run_id).await?;
        let artifacts = self.repository.list_artifacts(&cmd.run_id).await?;
        let duration_ms = duration_ms(ended.started_at, ended.ended_at);
        let steps_summary = summarize_steps(&steps);

        if ended.status == ExecutionStatus::Completed {
            let completed_payload =
                build_final_payload(&ended, duration_ms, steps_summary, &artifacts, None);
            self.publisher
                .publish(EventEnvelope {
                    event_id: Uuid::new_v4().to_string(),
                    event_type: EXECUTION_COMPLETED.to_string(),
                    event_version: "1.0".to_string(),
                    workspace_id: ended.workspace_id.clone(),
                    run_id: ended.run_id.clone(),
                    occurred_at: Utc::now(),
                    correlation_id: cmd.correlation_id.clone(),
                    payload: completed_payload,
                })
                .await?;
        } else if ended.status == ExecutionStatus::Failed {
            let error_summary = cmd
                .reason
                .clone()
                .unwrap_or_else(|| "execution failed due to one or more failed steps".to_string());
            let failed_payload = build_final_payload(
                &ended,
                duration_ms,
                steps_summary,
                &artifacts,
                Some(error_summary),
            );
            self.publisher
                .publish(EventEnvelope {
                    event_id: Uuid::new_v4().to_string(),
                    event_type: EXECUTION_FAILED.to_string(),
                    event_version: "1.0".to_string(),
                    workspace_id: ended.workspace_id.clone(),
                    run_id: ended.run_id.clone(),
                    occurred_at: Utc::now(),
                    correlation_id: cmd.correlation_id.clone(),
                    payload: failed_payload,
                })
                .await?;
        }

        let closed = self
            .repository
            .update_run_status(
                &cmd.run_id,
                ExecutionStatus::Closed.as_wire(),
                ended.started_at,
                ended.ended_at,
                &cmd.correlation_id,
            )
            .await?;

        self.publisher
            .publish(EventEnvelope {
                event_id: Uuid::new_v4().to_string(),
                event_type: EXECUTION_RUN_CLOSED.to_string(),
                event_version: "1.0".to_string(),
                workspace_id: closed.workspace_id.clone(),
                run_id: closed.run_id.clone(),
                occurred_at: Utc::now(),
                correlation_id: cmd.correlation_id.clone(),
                payload: json!({
                    "status": closed.status.as_wire(),
                    "reason": cmd.reason,
                }),
            })
            .await?;

        let accepted = CommandAccepted {
            status: "accepted".to_string(),
            workspace_id: closed.workspace_id,
            run_id: closed.run_id,
            resource_id: None,
            correlation_id: cmd.correlation_id,
        };

        self.idempotency
            .put(&scope, &cmd.idempotency_key, accepted.clone())
            .await?;

        Ok(accepted)
    }

    pub async fn get_run(&self, run_id: &str) -> Result<ExecutionRun, AppError> {
        self.repository
            .get_run(run_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("run {} nao encontrada", run_id)))
    }

    pub async fn list_steps(&self, run_id: &str) -> Result<Vec<ExecutionStep>, AppError> {
        let _ = self.get_run(run_id).await?;
        self.repository.list_steps(run_id).await
    }

    pub async fn list_logs(&self, run_id: &str) -> Result<Vec<ExecutionLog>, AppError> {
        let _ = self.get_run(run_id).await?;
        self.repository.list_logs(run_id).await
    }

    pub async fn list_artifacts(&self, run_id: &str) -> Result<Vec<ExecutionArtifact>, AppError> {
        let _ = self.get_run(run_id).await?;
        self.repository.list_artifacts(run_id).await
    }

    pub async fn enforce_workspace_scope(
        &self,
        run_id: &str,
        header_workspace_id: Option<&str>,
    ) -> Result<(), AppError> {
        let Some(header_workspace_id) = header_workspace_id else {
            return Ok(());
        };

        let run = self.get_run(run_id).await?;
        if run.workspace_id == header_workspace_id {
            return Ok(());
        }

        Err(AppError::Validation(format!(
            "X-Workspace-Id {} nao corresponde ao workspace {}",
            header_workspace_id, run.workspace_id
        )))
    }

    async fn ensure_run_exists(&self, run_id: &str) -> Result<ExecutionRun, AppError> {
        self.get_run(run_id).await
    }

    async fn ensure_run_mutable(&self, run_id: &str) -> Result<ExecutionRun, AppError> {
        let run = self.get_run(run_id).await?;
        if matches!(
            run.status,
            ExecutionStatus::Closed | ExecutionStatus::Canceled | ExecutionStatus::Failed
        ) {
            return Err(AppError::Conflict(format!(
                "run {} nao pode ser alterada em status {}",
                run_id
                ,
                run.status.as_wire()
            )));
        }
        Ok(run)
    }
}

fn summarize_steps(steps: &[ExecutionStep]) -> StepsSummary {
    let mut summary = StepsSummary {
        total: steps.len(),
        pending: 0,
        running: 0,
        completed: 0,
        failed: 0,
    };

    for step in steps {
        match step.status {
            StepStatus::Pending => summary.pending += 1,
            StepStatus::Running => summary.running += 1,
            StepStatus::Completed => summary.completed += 1,
            StepStatus::Failed => summary.failed += 1,
        }
    }

    summary
}

fn duration_ms(
    started_at: Option<chrono::DateTime<chrono::Utc>>,
    ended_at: Option<chrono::DateTime<chrono::Utc>>,
) -> i64 {
    match (started_at, ended_at) {
        (Some(started), Some(ended)) => ended
            .signed_duration_since(started)
            .num_milliseconds()
            .max(0),
        _ => 0,
    }
}

fn build_final_payload(
    run: &ExecutionRun,
    duration_ms: i64,
    steps_summary: StepsSummary,
    artifacts: &[ExecutionArtifact],
    error_summary: Option<String>,
) -> serde_json::Value {
    json!({
        "workspace_id": run.workspace_id,
        "run_id": run.run_id,
        "status": run.status.as_wire(),
        "duration_ms": duration_ms,
        "steps_summary": {
            "total": steps_summary.total,
            "pending": steps_summary.pending,
            "running": steps_summary.running,
            "completed": steps_summary.completed,
            "failed": steps_summary.failed,
        },
        "artifacts": artifacts,
        "steps_total": steps_summary.total,
        "steps_failed": steps_summary.failed,
        "artifacts_total": artifacts.len(),
        "error_summary": error_summary,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::Value;

    use crate::domain::events::EXECUTION_COMPLETED;
    use crate::{
        adapters::outbound::in_memory::InMemoryPorts,
        application::service::{
            AddArtifactCommand, AddStepCommand, CloseRunCommand, CompleteStepCommand,
            ExecutorService, OpenRunCommand, StartRunCommand,
        },
    };

    #[tokio::test]
    async fn should_be_idempotent_for_open_run() {
        let ports = Arc::new(InMemoryPorts::new());
        let service = ExecutorService::new(ports.clone(), ports.clone(), ports);

        let cmd = OpenRunCommand {
            run_id: Some("run_001".to_string()),
            workspace_id: "ws_001".to_string(),
            ops_session_id: "os_001".to_string(),
            ready_for_execution: true,
            idempotency_key: "idem-open-001".to_string(),
            correlation_id: "corr-open-001".to_string(),
        };

        let first = service.open_run(cmd.clone()).await.expect("first ok");
        let second = service.open_run(cmd).await.expect("second cached");

        assert_eq!(first.run_id, second.run_id);
    }

    #[tokio::test]
    async fn should_require_ready_for_execution_before_start() {
        let ports = Arc::new(InMemoryPorts::new());
        let service = ExecutorService::new(ports.clone(), ports.clone(), ports);

        service
            .open_run(OpenRunCommand {
                run_id: Some("run_002".to_string()),
                workspace_id: "ws_001".to_string(),
                ops_session_id: "os_002".to_string(),
                ready_for_execution: false,
                idempotency_key: "idem-open-002".to_string(),
                correlation_id: "corr-open-002".to_string(),
            })
            .await
            .expect("open ok");

        let fail = service
            .start_run(StartRunCommand {
                run_id: "run_002".to_string(),
                idempotency_key: "idem-start-fail".to_string(),
                correlation_id: "corr-start-fail".to_string(),
            })
            .await;

        assert!(fail.is_err());
    }

    #[tokio::test]
    async fn should_emit_completed_payload_with_execution_summary() {
        let ports = Arc::new(InMemoryPorts::new());
        let service = ExecutorService::new(ports.clone(), ports.clone(), ports.clone());

        service
            .open_run(OpenRunCommand {
                run_id: Some("run_003".to_string()),
                workspace_id: "ws_001".to_string(),
                ops_session_id: "os_003".to_string(),
                ready_for_execution: true,
                idempotency_key: "idem-open-003".to_string(),
                correlation_id: "corr-open-003".to_string(),
            })
            .await
            .expect("open ok");

        service
            .start_run(StartRunCommand {
                run_id: "run_003".to_string(),
                idempotency_key: "idem-start-003".to_string(),
                correlation_id: "corr-start-003".to_string(),
            })
            .await
            .expect("start ok");

        service
            .add_step(AddStepCommand {
                run_id: "run_003".to_string(),
                step_id: Some("stp_003".to_string()),
                name: "build".to_string(),
                detail: None,
                idempotency_key: "idem-step-003".to_string(),
                correlation_id: "corr-step-003".to_string(),
            })
            .await
            .expect("step ok");

        service
            .complete_step(CompleteStepCommand {
                run_id: "run_003".to_string(),
                step_id: "stp_003".to_string(),
                status: "completed".to_string(),
                detail: None,
                idempotency_key: "idem-stepc-003".to_string(),
                correlation_id: "corr-stepc-003".to_string(),
            })
            .await
            .expect("step complete ok");

        service
            .add_artifact(AddArtifactCommand {
                run_id: "run_003".to_string(),
                artifact_id: Some("art_003".to_string()),
                artifact_type: "build".to_string(),
                storage_ref: "s3://bucket/build.zip".to_string(),
                idempotency_key: "idem-art-003".to_string(),
                correlation_id: "corr-art-003".to_string(),
            })
            .await
            .expect("artifact ok");

        service
            .close_run(CloseRunCommand {
                run_id: "run_003".to_string(),
                reason: Some("finished".to_string()),
                idempotency_key: "idem-close-003".to_string(),
                correlation_id: "corr-close-003".to_string(),
            })
            .await
            .expect("close ok");

        let events = ports.published_events().expect("events");
        let completed = events
            .iter()
            .find(|event| event.event_type == EXECUTION_COMPLETED)
            .expect("completed event");

        let payload = &completed.payload;
        assert_eq!(
            payload.get("workspace_id"),
            Some(&Value::String("ws_001".to_string()))
        );
        assert_eq!(
            payload.get("run_id"),
            Some(&Value::String("run_003".to_string()))
        );
        assert_eq!(
            payload.get("status"),
            Some(&Value::String("completed".to_string()))
        );
        assert!(payload.get("duration_ms").is_some());
        assert!(payload.get("steps_summary").is_some());
        assert!(payload
            .get("artifacts")
            .and_then(Value::as_array)
            .is_some_and(|arr| !arr.is_empty()));
    }
}
