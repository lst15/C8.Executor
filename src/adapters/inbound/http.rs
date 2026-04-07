use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};

use crate::{
    application::service::{
        AddArtifactCommand, AddStepCommand, CancelRunCommand, CloseRunCommand,
        CompleteStepCommand, ExecutorService, OpenRunCommand, StartRunCommand,
    },
    domain::{
        error::AppError,
        model::{ExecutionLog, ExecutionRun, ExecutionStep},
    },
};

#[derive(Clone)]
pub struct AppState {
    service: Arc<ExecutorService>,
}

pub fn build_router(service: Arc<ExecutorService>) -> Router {
    let state = AppState { service };

    Router::new()
        .route("/health", get(health))
        .route(
            "/v1/execution/workspaces/{workspace_id}/runs",
            post(open_run),
        )
        .route("/v1/execution/runs/{run_id}/start", post(start_run))
        .route("/v1/execution/runs/{run_id}/steps", post(add_step).get(get_steps))
        .route(
            "/v1/execution/runs/{run_id}/steps/{step_id}/complete",
            post(complete_step),
        )
        .route("/v1/execution/runs/{run_id}/artifacts", post(add_artifact))
        .route("/v1/execution/runs/{run_id}/cancel", post(cancel_run))
        .route("/v1/execution/runs/{run_id}/close", post(close_run))
        .route("/v1/execution/runs/{run_id}", get(get_run))
        .route("/v1/execution/runs/{run_id}/logs", get(get_logs))
        .with_state(state)
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    service: &'static str,
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        service: "C8.Executor",
    })
}

#[derive(Debug, Deserialize)]
struct OpenRunBody {
    run_id: Option<String>,
    ops_session_id: String,
    ready_for_execution: bool,
}

#[derive(Debug, Deserialize)]
struct AddStepBody {
    step_id: Option<String>,
    name: String,
    detail: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CompleteStepBody {
    status: String,
    detail: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AddArtifactBody {
    artifact_id: Option<String>,
    artifact_type: String,
    storage_ref: String,
}

#[derive(Debug, Deserialize)]
struct CancelBody {
    reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CloseBody {
    reason: Option<String>,
}

#[derive(Debug, Serialize)]
struct RunResponse {
    run_id: String,
    run: ExecutionRun,
}

#[derive(Debug, Serialize)]
struct StepsResponse {
    run_id: String,
    steps: Vec<ExecutionStep>,
}

#[derive(Debug, Serialize)]
struct LogsResponse {
    run_id: String,
    logs: Vec<ExecutionLog>,
}

async fn open_run(
    State(state): State<AppState>,
    Path(workspace_id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<OpenRunBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    let idempotency_key = required_header(&headers, "Idempotency-Key")?;
    let correlation_id = required_header(&headers, "X-Correlation-Id")?;

    let result = state
        .service
        .open_run(OpenRunCommand {
            run_id: body.run_id,
            workspace_id,
            ops_session_id: body.ops_session_id,
            ready_for_execution: body.ready_for_execution,
            idempotency_key,
            correlation_id,
        })
        .await?;

    Ok((
        StatusCode::ACCEPTED,
        Json(serde_json::to_value(result).map_err(|_| AppError::Internal)?),
    ))
}

async fn start_run(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
    headers: HeaderMap,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    let idempotency_key = required_header(&headers, "Idempotency-Key")?;
    let correlation_id = required_header(&headers, "X-Correlation-Id")?;

    state
        .service
        .enforce_workspace_scope(&run_id, optional_header(&headers, "X-Workspace-Id").as_deref())
        .await?;

    let result = state
        .service
        .start_run(StartRunCommand {
            run_id,
            idempotency_key,
            correlation_id,
        })
        .await?;

    Ok((
        StatusCode::ACCEPTED,
        Json(serde_json::to_value(result).map_err(|_| AppError::Internal)?),
    ))
}

async fn add_step(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<AddStepBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    let idempotency_key = required_header(&headers, "Idempotency-Key")?;
    let correlation_id = required_header(&headers, "X-Correlation-Id")?;

    state
        .service
        .enforce_workspace_scope(&run_id, optional_header(&headers, "X-Workspace-Id").as_deref())
        .await?;

    let result = state
        .service
        .add_step(AddStepCommand {
            run_id,
            step_id: body.step_id,
            name: body.name,
            detail: body.detail,
            idempotency_key,
            correlation_id,
        })
        .await?;

    Ok((
        StatusCode::ACCEPTED,
        Json(serde_json::to_value(result).map_err(|_| AppError::Internal)?),
    ))
}

async fn complete_step(
    State(state): State<AppState>,
    Path((run_id, step_id)): Path<(String, String)>,
    headers: HeaderMap,
    Json(body): Json<CompleteStepBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    let idempotency_key = required_header(&headers, "Idempotency-Key")?;
    let correlation_id = required_header(&headers, "X-Correlation-Id")?;

    state
        .service
        .enforce_workspace_scope(&run_id, optional_header(&headers, "X-Workspace-Id").as_deref())
        .await?;

    let result = state
        .service
        .complete_step(CompleteStepCommand {
            run_id,
            step_id,
            status: body.status,
            detail: body.detail,
            idempotency_key,
            correlation_id,
        })
        .await?;

    Ok((
        StatusCode::ACCEPTED,
        Json(serde_json::to_value(result).map_err(|_| AppError::Internal)?),
    ))
}

async fn add_artifact(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<AddArtifactBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    let idempotency_key = required_header(&headers, "Idempotency-Key")?;
    let correlation_id = required_header(&headers, "X-Correlation-Id")?;

    state
        .service
        .enforce_workspace_scope(&run_id, optional_header(&headers, "X-Workspace-Id").as_deref())
        .await?;

    let result = state
        .service
        .add_artifact(AddArtifactCommand {
            run_id,
            artifact_id: body.artifact_id,
            artifact_type: body.artifact_type,
            storage_ref: body.storage_ref,
            idempotency_key,
            correlation_id,
        })
        .await?;

    Ok((
        StatusCode::ACCEPTED,
        Json(serde_json::to_value(result).map_err(|_| AppError::Internal)?),
    ))
}

async fn cancel_run(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<CancelBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    let idempotency_key = required_header(&headers, "Idempotency-Key")?;
    let correlation_id = required_header(&headers, "X-Correlation-Id")?;

    state
        .service
        .enforce_workspace_scope(&run_id, optional_header(&headers, "X-Workspace-Id").as_deref())
        .await?;

    let result = state
        .service
        .cancel_run(CancelRunCommand {
            run_id,
            reason: body.reason,
            idempotency_key,
            correlation_id,
        })
        .await?;

    Ok((
        StatusCode::ACCEPTED,
        Json(serde_json::to_value(result).map_err(|_| AppError::Internal)?),
    ))
}

async fn close_run(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<CloseBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    let idempotency_key = required_header(&headers, "Idempotency-Key")?;
    let correlation_id = required_header(&headers, "X-Correlation-Id")?;

    state
        .service
        .enforce_workspace_scope(&run_id, optional_header(&headers, "X-Workspace-Id").as_deref())
        .await?;

    let result = state
        .service
        .close_run(CloseRunCommand {
            run_id,
            reason: body.reason,
            idempotency_key,
            correlation_id,
        })
        .await?;

    Ok((
        StatusCode::ACCEPTED,
        Json(serde_json::to_value(result).map_err(|_| AppError::Internal)?),
    ))
}

async fn get_run(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<RunResponse>, AppError> {
    state
        .service
        .enforce_workspace_scope(&run_id, optional_header(&headers, "X-Workspace-Id").as_deref())
        .await?;

    let run = state.service.get_run(&run_id).await?;
    Ok(Json(RunResponse { run_id, run }))
}

async fn get_steps(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<StepsResponse>, AppError> {
    state
        .service
        .enforce_workspace_scope(&run_id, optional_header(&headers, "X-Workspace-Id").as_deref())
        .await?;

    let steps = state.service.list_steps(&run_id).await?;
    Ok(Json(StepsResponse { run_id, steps }))
}

async fn get_logs(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<LogsResponse>, AppError> {
    state
        .service
        .enforce_workspace_scope(&run_id, optional_header(&headers, "X-Workspace-Id").as_deref())
        .await?;

    let logs = state.service.list_logs(&run_id).await?;
    Ok(Json(LogsResponse { run_id, logs }))
}

fn required_header(headers: &HeaderMap, key: &'static str) -> Result<String, AppError> {
    let value = headers
        .get(key)
        .ok_or_else(|| AppError::Validation(format!("header {} e obrigatorio", key)))?;

    let parsed = value
        .to_str()
        .map_err(|_| AppError::Validation(format!("header {} invalido", key)))?
        .trim()
        .to_string();

    if parsed.is_empty() {
        return Err(AppError::Validation(format!("header {} vazio", key)));
    }

    Ok(parsed)
}

fn optional_header(headers: &HeaderMap, key: &'static str) -> Option<String> {
    headers
        .get(key)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}
