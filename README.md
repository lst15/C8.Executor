# C8.Executor

Implementacao da Camada 08 (Execucao Tecnica) da plataforma Uranometria em Rust, com arquitetura hexagonal e contratos v1.

## Stack

- Rust + Tokio
- Axum (HTTP)
- Serde/JSON
- Tracing
- PostgreSQL (`sqlx`)
- Kafka (`rdkafka`)

## Endpoints

- `GET /health`
- `GET /openapi.json`
- `POST /v1/execution/workspaces/{workspace_id}/runs`
- `POST /v1/execution/runs/{run_id}/start`
- `POST /v1/execution/runs/{run_id}/steps`
- `POST /v1/execution/runs/{run_id}/steps/{step_id}/complete`
- `POST /v1/execution/runs/{run_id}/artifacts`
- `POST /v1/execution/runs/{run_id}/cancel`
- `POST /v1/execution/runs/{run_id}/close`
- `GET /v1/execution/runs/{run_id}`
- `GET /v1/execution/runs/{run_id}/steps`
- `GET /v1/execution/runs/{run_id}/logs`

Headers obrigatorios nos endpoints `POST`:

- `Idempotency-Key`
- `X-Correlation-Id`

Header opcional para enforce de escopo por workspace:

- `X-Workspace-Id`

## Eventos publicados

- `C8.ExecutionRunOpened`
- `C8.ExecutionStarted`
- `C8.ExecutionStepRegistered`
- `C8.ExecutionStepCompleted`
- `C8.ExecutionArtifactRegistered`
- `C8.ExecutionFailed`
- `C8.ExecutionCanceled`
- `C8.ExecutionCompleted`
- `C8.ExecutionRunClosed`

## Como executar

1. Suba infraestrutura local (Postgres + Kafka):

```bash
docker compose up -d
```

2. Carregue as variaveis de ambiente:

```bash
cp .env.example .env
set -a; source .env; set +a
```

3. Inicie a API:

```bash
cargo run --bin c8-executor
```

Servidor padrao: `http://localhost:8087`

## Regras tecnicas implementadas

- Idempotencia por comando (`scope + Idempotency-Key`).
- `start` exige `ready_for_execution = true`.
- Steps so podem ser criados com run em `running`.
- `complete_step` aceita apenas `completed` ou `failed`.
- Falha de step pode marcar run como `failed`.
- `close` encerra a run e publica evento de fechamento.

## Documento oficial da camada

Fonte canonical local: `../docs/camada-08-contratos-rust.md`
