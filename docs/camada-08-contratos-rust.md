# Camada 08 - Contratos v1 (Rust)

## Objetivo

Definir os contratos da Camada 08 (Execucao Tecnica) no formato contract-first, separando:
- API sincrona para inicializacao de execucao, acompanhamento de run e controle operacional tecnico
- Eventos assincronos para integracao com orquestracao, memoria e monitoramento de entrega

## Estrategia recomendada

1. Contract-first como regra de desenvolvimento.
2. Dois contratos formais:
- OpenAPI para comandos e consultas sincronas de execucao tecnica.
- Catalogo de eventos (AsyncAPI ou schema JSON versionado) para ciclo de vida da execucao.
3. Implementacao em Rust com arquitetura hexagonal e contratos compartilhados em crate dedicado.

## Arquitetura recomendada (Hexagonal)

Modelo alvo:
- aplicacao isolada da Camada 08 em Rust
- nucleo de dominio/aplicacao sem dependencia de framework HTTP, CI/CD especifico ou runtime especifico
- adaptadores conectando o nucleo a infraestrutura de execucao, persistencia e mensageria

Ports de saida:
- `ExecutionRunRepository`
- `ExecutionStepRepository`
- `ExecutionArtifactRepository`
- `ExecutionLogRepository`
- `IdempotencyRepository`
- `EventPublisher`
- `RuntimeAdapter`
- `Clock`
- `IdGenerator`

Adapters de entrada:
- HTTP REST (`axum`)
- Consumer de eventos da Camada 07 para inicio de execucao quando `execution_ready`

Adapters de saida:
- PostgreSQL (`sqlx`) para runs, steps, logs e artefatos
- Broker Kafka (`rdkafka`) para eventos de execucao
- adaptadores de runtime (containers, scripts, workers)
- observabilidade (`tracing`)

## Fronteira da Camada 08

Responsavel por:
- Executar tecnicamente o plano aprovado em ambiente controlado.
- Controlar progresso de execucao por run e por etapa.
- Registrar logs, artefatos de build/deploy e status final.
- Publicar eventos tecnicos para rastreabilidade operacional.

Nao responsavel por:
- Pesquisar e consolidar baseline (Camada 03).
- Persistir memoria geral do workspace (Camada 04).
- Definir escopo funcional e documental (Camadas 05 e 06).
- Decidir gates de fluxo operacional (Camada 07).

## API sincrona (OpenAPI v1)

Base path:
- `/v1/execution`

Endpoints minimos:
- `POST /v1/execution/workspaces/{workspace_id}/runs` (abrir run de execucao tecnica)
- `POST /v1/execution/runs/{run_id}/start` (iniciar run)
- `POST /v1/execution/runs/{run_id}/steps` (registrar etapa de execucao)
- `POST /v1/execution/runs/{run_id}/steps/{step_id}/complete` (concluir etapa)
- `POST /v1/execution/runs/{run_id}/artifacts` (registrar artefato tecnico)
- `POST /v1/execution/runs/{run_id}/cancel` (cancelar run)
- `POST /v1/execution/runs/{run_id}/close` (encerrar run)
- `GET /v1/execution/runs/{run_id}` (consultar run)
- `GET /v1/execution/runs/{run_id}/steps` (consultar etapas)
- `GET /v1/execution/runs/{run_id}/logs` (consultar logs de execucao)

Cabecalhos obrigatorios em comandos (`POST`):
- `Idempotency-Key`
- `X-Correlation-Id`

Header opcional para enforce de escopo por workspace:
- `X-Workspace-Id`

## Catalogo de eventos (v1)

Eventos minimos:
- `C8.ExecutionRunOpened`
- `C8.ExecutionStarted`
- `C8.ExecutionStepRegistered`
- `C8.ExecutionStepCompleted`
- `C8.ExecutionArtifactRegistered`
- `C8.ExecutionFailed`
- `C8.ExecutionCanceled`
- `C8.ExecutionCompleted`
- `C8.ExecutionRunClosed`

Consumidores esperados:
- Camada 04 (memoria e historico)
- Camada 07 (retroalimentacao de estado operacional)
- sistemas de observabilidade e operacao

## Envelope canonico de evento

Campos obrigatorios:
- `event_id` (UUID)
- `event_type` (string)
- `event_version` (string, ex.: `1.0`)
- `workspace_id` (string)
- `run_id` (string)
- `occurred_at` (RFC3339 UTC)
- `correlation_id` (string)
- `payload` (objeto tipado por evento)

Exemplo:

```json
{
  "event_id": "f0c5ad5b-58e4-48f5-b8cb-c73dbef6db99",
  "event_type": "C8.ExecutionCompleted",
  "event_version": "1.0",
  "workspace_id": "ws_123",
  "run_id": "run_441",
  "occurred_at": "2026-04-07T21:30:00Z",
  "correlation_id": "corr_081",
  "payload": {
    "status": "completed",
    "steps_total": 12,
    "steps_failed": 0,
    "artifacts_total": 5
  }
}
```

## Contrato minimo de saida da Camada 08

A Camada 08 conclui a execucao tecnica e devolve estado tecnico auditavel.

Evento de referencia de encerramento:
- `C8.ExecutionCompleted` ou `C8.ExecutionFailed`

Campos obrigatorios de saida:
- `workspace_id`
- `run_id`
- `status`
- `duration_ms`
- `steps_summary`
- `artifacts[]`
- `error_summary` (quando houver falha)

Resultado esperado nas camadas consumidoras:
- rastreabilidade completa da execucao
- capacidade de auditoria e reproducao operacional

## Estrutura Rust recomendada

```text
crates/
  c8-contracts/
    src/
      commands.rs
      events.rs
      envelope.rs
      errors.rs
      execution.rs
      lib.rs
apps/
  c8-execution-service/
    src/
      domain/
      application/
      ports/
      adapters/
        inbound/http/
        inbound/messaging/
        outbound/persistence/
        outbound/messaging/
        outbound/runtime/
      main.rs
```

### Crate `c8-contracts`

Responsavel por:
- Tipos de comando e consulta de execucao.
- Tipos de evento de ciclo de execucao.
- Envelope comum e erros padronizados.
- Schemas de run, step e artefatos tecnicos.

## Modelo de dados minimo (persistencia)

Tabelas minimas sugeridas:
- `c8_execution_runs`
- `c8_execution_steps`
- `c8_execution_artifacts`
- `c8_execution_logs`
- `c8_execution_timeline`
- `c8_execution_idempotency`

Campos minimos por run:
- `run_id`
- `workspace_id`
- `ops_session_id`
- `status` (`open`, `running`, `failed`, `canceled`, `completed`, `closed`)
- `started_at`
- `ended_at` (opcional)
- `correlation_id`

Regras obrigatorias:
- Toda entidade vinculada a `workspace_id` e `run_id`.
- Toda etapa deve ter status e timestamps consistentes.
- Todo artefato deve conter tipo e referencia de armazenamento.
- Toda mudanca de status da run deve ser registrada na timeline.

## Regras de execucao e encerramento

1. Run nao pode iniciar sem sessao operacional em estado `execution_ready`.
2. Step nao pode ser concluido antes de ser registrado.
3. Falha critica de step pode marcar run como `failed`.
4. Cancelamento deve preservar logs e estado parcial da execucao.
5. Encerramento de run deve registrar resumo final tecnico.

## Regras de qualidade de contrato

1. Validacao de payload na borda (Camada 08).
2. Idempotencia por chave + escopo de operacao.
3. Erros padronizados:
- `code`
- `message`
- `details`
- `trace_id`
4. Correlacao obrigatoria em logs e eventos por `X-Correlation-Id`.

## Regras de versionamento de contrato

1. Toda rota e evento em namespace `v1`.
2. Alteracoes em `v1` devem ser aditivas.
3. Remocao/renomeacao de campo exige nova versao (`v2`).
4. Eventos devem manter compatibilidade para consumidores existentes.

## Stack Rust sugerida

- `axum` para HTTP.
- `serde` + `validator` para parsing/validacao.
- `utoipa` para OpenAPI.
- `tracing` para logs estruturados.
- `sqlx` para persistencia e idempotencia.
- `rdkafka` para event bus.
- `schemars` para schema JSON.

## Proximos passos (implementacao)

1. Criar `crates/c8-contracts` com comandos/eventos/envelope/erros/execution.
2. Publicar OpenAPI v1 e catalogo de eventos v1 com exemplos.
3. Implementar `apps/c8-execution-service` em arquitetura hexagonal.
4. Integrar gatilho de inicio via evento C7 (estado `execution_ready`).
5. Integrar publicacao de status final para C4 e C7.
