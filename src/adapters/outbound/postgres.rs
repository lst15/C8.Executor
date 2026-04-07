use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool, postgres::PgPoolOptions};

use crate::{
    domain::{
        error::AppError,
        model::{
            CommandAccepted, ExecutionArtifact, ExecutionLog, ExecutionRun, ExecutionStatus,
            ExecutionStep, StepStatus,
        },
    },
    ports::{ExecutionRepository, IdempotencyRepository},
};

pub struct PostgresPorts {
    pool: PgPool,
}

impl PostgresPorts {
    pub async fn connect(database_url: &str) -> Result<Self, AppError> {
        let pool = PgPoolOptions::new()
            .max_connections(10)
            .connect(database_url)
            .await
            .map_err(|e| AppError::Dependency(format!("postgres connect error: {e}")))?;

        Ok(Self { pool })
    }

    pub async fn ensure_schema(&self) -> Result<(), AppError> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS c8_execution_runs (
                run_id TEXT PRIMARY KEY,
                workspace_id TEXT NOT NULL,
                ops_session_id TEXT NOT NULL,
                status TEXT NOT NULL,
                ready_for_execution BOOLEAN NOT NULL,
                started_at TIMESTAMPTZ NULL,
                ended_at TIMESTAMPTZ NULL,
                created_at TIMESTAMPTZ NOT NULL,
                updated_at TIMESTAMPTZ NOT NULL,
                correlation_id TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_c8_runs_workspace
                ON c8_execution_runs (workspace_id, updated_at DESC);

            CREATE TABLE IF NOT EXISTS c8_execution_steps (
                step_id TEXT PRIMARY KEY,
                run_id TEXT NOT NULL,
                workspace_id TEXT NOT NULL,
                name TEXT NOT NULL,
                status TEXT NOT NULL,
                detail TEXT NULL,
                created_at TIMESTAMPTZ NOT NULL,
                completed_at TIMESTAMPTZ NULL,
                correlation_id TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_c8_steps_run
                ON c8_execution_steps (run_id, created_at ASC);

            CREATE TABLE IF NOT EXISTS c8_execution_artifacts (
                artifact_id TEXT PRIMARY KEY,
                run_id TEXT NOT NULL,
                workspace_id TEXT NOT NULL,
                artifact_type TEXT NOT NULL,
                storage_ref TEXT NOT NULL,
                created_at TIMESTAMPTZ NOT NULL,
                correlation_id TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_c8_artifacts_run
                ON c8_execution_artifacts (run_id, created_at ASC);

            CREATE TABLE IF NOT EXISTS c8_execution_logs (
                log_id TEXT PRIMARY KEY,
                run_id TEXT NOT NULL,
                workspace_id TEXT NOT NULL,
                level TEXT NOT NULL,
                message TEXT NOT NULL,
                occurred_at TIMESTAMPTZ NOT NULL,
                correlation_id TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_c8_logs_run
                ON c8_execution_logs (run_id, occurred_at ASC);

            CREATE TABLE IF NOT EXISTS c8_execution_idempotency (
                scope TEXT NOT NULL,
                idempotency_key TEXT NOT NULL,
                response JSONB NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                PRIMARY KEY (scope, idempotency_key)
            );
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Dependency(format!("postgres schema init error: {e}")))?;

        Ok(())
    }
}

#[derive(Debug, FromRow)]
struct RunRow {
    run_id: String,
    workspace_id: String,
    ops_session_id: String,
    status: String,
    ready_for_execution: bool,
    started_at: Option<DateTime<Utc>>,
    ended_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    correlation_id: String,
}

#[derive(Debug, FromRow)]
struct StepRow {
    step_id: String,
    run_id: String,
    workspace_id: String,
    name: String,
    status: String,
    detail: Option<String>,
    created_at: DateTime<Utc>,
    completed_at: Option<DateTime<Utc>>,
    correlation_id: String,
}

#[derive(Debug, FromRow)]
struct ArtifactRow {
    artifact_id: String,
    run_id: String,
    workspace_id: String,
    artifact_type: String,
    storage_ref: String,
    created_at: DateTime<Utc>,
    correlation_id: String,
}

#[derive(Debug, FromRow)]
struct LogRow {
    log_id: String,
    run_id: String,
    workspace_id: String,
    level: String,
    message: String,
    occurred_at: DateTime<Utc>,
    correlation_id: String,
}

impl TryFrom<RunRow> for ExecutionRun {
    type Error = AppError;

    fn try_from(value: RunRow) -> Result<Self, Self::Error> {
        Ok(Self {
            run_id: value.run_id,
            workspace_id: value.workspace_id,
            ops_session_id: value.ops_session_id,
            status: ExecutionStatus::from_wire(&value.status)
                .ok_or_else(|| AppError::Dependency(format!("invalid run status in db: {}", value.status)))?,
            ready_for_execution: value.ready_for_execution,
            started_at: value.started_at,
            ended_at: value.ended_at,
            created_at: value.created_at,
            updated_at: value.updated_at,
            correlation_id: value.correlation_id,
        })
    }
}

impl TryFrom<StepRow> for ExecutionStep {
    type Error = AppError;

    fn try_from(value: StepRow) -> Result<Self, Self::Error> {
        Ok(Self {
            step_id: value.step_id,
            run_id: value.run_id,
            workspace_id: value.workspace_id,
            name: value.name,
            status: StepStatus::from_wire(&value.status)
                .ok_or_else(|| AppError::Dependency(format!("invalid step status in db: {}", value.status)))?,
            detail: value.detail,
            created_at: value.created_at,
            completed_at: value.completed_at,
            correlation_id: value.correlation_id,
        })
    }
}

impl From<ArtifactRow> for ExecutionArtifact {
    fn from(value: ArtifactRow) -> Self {
        Self {
            artifact_id: value.artifact_id,
            run_id: value.run_id,
            workspace_id: value.workspace_id,
            artifact_type: value.artifact_type,
            storage_ref: value.storage_ref,
            created_at: value.created_at,
            correlation_id: value.correlation_id,
        }
    }
}

impl From<LogRow> for ExecutionLog {
    fn from(value: LogRow) -> Self {
        Self {
            log_id: value.log_id,
            run_id: value.run_id,
            workspace_id: value.workspace_id,
            level: value.level,
            message: value.message,
            occurred_at: value.occurred_at,
            correlation_id: value.correlation_id,
        }
    }
}

#[async_trait]
impl ExecutionRepository for PostgresPorts {
    async fn create_run(&self, run: ExecutionRun) -> Result<(), AppError> {
        sqlx::query(
            r#"
            INSERT INTO c8_execution_runs (
                run_id, workspace_id, ops_session_id, status, ready_for_execution,
                started_at, ended_at, created_at, updated_at, correlation_id
            ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
            "#,
        )
        .bind(run.run_id)
        .bind(run.workspace_id)
        .bind(run.ops_session_id)
        .bind(run.status.as_wire())
        .bind(run.ready_for_execution)
        .bind(run.started_at)
        .bind(run.ended_at)
        .bind(run.created_at)
        .bind(run.updated_at)
        .bind(run.correlation_id)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Dependency(format!("postgres create run error: {e}")))?;

        Ok(())
    }

    async fn get_run(&self, run_id: &str) -> Result<Option<ExecutionRun>, AppError> {
        let row = sqlx::query_as::<_, RunRow>(
            r#"
            SELECT run_id, workspace_id, ops_session_id, status, ready_for_execution,
                   started_at, ended_at, created_at, updated_at, correlation_id
            FROM c8_execution_runs
            WHERE run_id = $1
            "#,
        )
        .bind(run_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::Dependency(format!("postgres get run error: {e}")))?;

        match row {
            Some(v) => Ok(Some(v.try_into()?)),
            None => Ok(None),
        }
    }

    async fn update_run_status(
        &self,
        run_id: &str,
        status: &str,
        started_at: Option<DateTime<Utc>>,
        ended_at: Option<DateTime<Utc>>,
        correlation_id: &str,
    ) -> Result<ExecutionRun, AppError> {
        let row = sqlx::query_as::<_, RunRow>(
            r#"
            UPDATE c8_execution_runs
            SET status = $2,
                started_at = $3,
                ended_at = $4,
                updated_at = NOW(),
                correlation_id = $5
            WHERE run_id = $1
            RETURNING run_id, workspace_id, ops_session_id, status, ready_for_execution,
                      started_at, ended_at, created_at, updated_at, correlation_id
            "#,
        )
        .bind(run_id)
        .bind(status)
        .bind(started_at)
        .bind(ended_at)
        .bind(correlation_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::Dependency(format!("postgres update run status error: {e}")))?;

        let row = row.ok_or_else(|| AppError::NotFound(format!("run {} nao encontrada", run_id)))?;
        row.try_into()
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
        sqlx::query(
            r#"
            INSERT INTO c8_execution_steps (
                step_id, run_id, workspace_id, name, status, detail,
                created_at, completed_at, correlation_id
            ) VALUES ($1,$2,$3,$4,$5,$6,NOW(),NULL,$7)
            "#,
        )
        .bind(step_id)
        .bind(run_id)
        .bind(workspace_id)
        .bind(name)
        .bind(status)
        .bind(detail)
        .bind(correlation_id)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Dependency(format!("postgres add step error: {e}")))?;

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
        let row = sqlx::query_as::<_, StepRow>(
            r#"
            UPDATE c8_execution_steps
            SET status = $3,
                detail = COALESCE($4, detail),
                completed_at = NOW(),
                correlation_id = $5
            WHERE run_id = $1 AND step_id = $2
            RETURNING step_id, run_id, workspace_id, name, status, detail,
                      created_at, completed_at, correlation_id
            "#,
        )
        .bind(run_id)
        .bind(step_id)
        .bind(status)
        .bind(detail)
        .bind(correlation_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::Dependency(format!("postgres complete step error: {e}")))?;

        let row =
            row.ok_or_else(|| AppError::NotFound(format!("step {} nao encontrada", step_id)))?;
        row.try_into()
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
        sqlx::query(
            r#"
            INSERT INTO c8_execution_artifacts (
                artifact_id, run_id, workspace_id, artifact_type, storage_ref,
                created_at, correlation_id
            ) VALUES ($1,$2,$3,$4,$5,NOW(),$6)
            "#,
        )
        .bind(artifact_id)
        .bind(run_id)
        .bind(workspace_id)
        .bind(artifact_type)
        .bind(storage_ref)
        .bind(correlation_id)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Dependency(format!("postgres add artifact error: {e}")))?;

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
        let log_id = format!("log_{}", uuid::Uuid::new_v4().simple());

        sqlx::query(
            r#"
            INSERT INTO c8_execution_logs (
                log_id, run_id, workspace_id, level, message, occurred_at, correlation_id
            ) VALUES ($1,$2,$3,$4,$5,NOW(),$6)
            "#,
        )
        .bind(&log_id)
        .bind(run_id)
        .bind(workspace_id)
        .bind(level)
        .bind(message)
        .bind(correlation_id)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Dependency(format!("postgres add log error: {e}")))?;

        Ok(log_id)
    }

    async fn list_steps(&self, run_id: &str) -> Result<Vec<ExecutionStep>, AppError> {
        let rows = sqlx::query_as::<_, StepRow>(
            r#"
            SELECT step_id, run_id, workspace_id, name, status, detail,
                   created_at, completed_at, correlation_id
            FROM c8_execution_steps
            WHERE run_id = $1
            ORDER BY created_at ASC
            "#,
        )
        .bind(run_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Dependency(format!("postgres list steps error: {e}")))?;

        rows.into_iter().map(TryInto::try_into).collect()
    }

    async fn list_logs(&self, run_id: &str) -> Result<Vec<ExecutionLog>, AppError> {
        let rows = sqlx::query_as::<_, LogRow>(
            r#"
            SELECT log_id, run_id, workspace_id, level, message, occurred_at, correlation_id
            FROM c8_execution_logs
            WHERE run_id = $1
            ORDER BY occurred_at ASC
            "#,
        )
        .bind(run_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Dependency(format!("postgres list logs error: {e}")))?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn list_artifacts(&self, run_id: &str) -> Result<Vec<ExecutionArtifact>, AppError> {
        let rows = sqlx::query_as::<_, ArtifactRow>(
            r#"
            SELECT artifact_id, run_id, workspace_id, artifact_type, storage_ref,
                   created_at, correlation_id
            FROM c8_execution_artifacts
            WHERE run_id = $1
            ORDER BY created_at ASC
            "#,
        )
        .bind(run_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Dependency(format!("postgres list artifacts error: {e}")))?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn count_failed_steps(&self, run_id: &str) -> Result<i64, AppError> {
        let count = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*)::bigint
            FROM c8_execution_steps
            WHERE run_id = $1 AND status = 'failed'
            "#,
        )
        .bind(run_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AppError::Dependency(format!("postgres count failed steps error: {e}")))?;

        Ok(count)
    }
}

#[async_trait]
impl IdempotencyRepository for PostgresPorts {
    async fn get(&self, scope: &str, key: &str) -> Result<Option<CommandAccepted>, AppError> {
        let row = sqlx::query_scalar::<_, serde_json::Value>(
            r#"
            SELECT response
            FROM c8_execution_idempotency
            WHERE scope = $1 AND idempotency_key = $2
            "#,
        )
        .bind(scope)
        .bind(key)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::Dependency(format!("postgres idempotency get error: {e}")))?;

        match row {
            Some(v) => {
                let parsed = serde_json::from_value::<CommandAccepted>(v).map_err(|e| {
                    AppError::Dependency(format!("idempotency payload decode error: {e}"))
                })?;
                Ok(Some(parsed))
            }
            None => Ok(None),
        }
    }

    async fn put(&self, scope: &str, key: &str, value: CommandAccepted) -> Result<(), AppError> {
        let payload = serde_json::to_value(value)
            .map_err(|e| AppError::Dependency(format!("idempotency payload encode error: {e}")))?;

        sqlx::query(
            r#"
            INSERT INTO c8_execution_idempotency (scope, idempotency_key, response)
            VALUES ($1,$2,$3)
            ON CONFLICT (scope, idempotency_key) DO NOTHING
            "#,
        )
        .bind(scope)
        .bind(key)
        .bind(payload)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Dependency(format!("postgres idempotency put error: {e}")))?;

        Ok(())
    }
}
