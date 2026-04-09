# Homologacao Manual C8 (2026-04-09)

Base local: `http://localhost:8087`

## Resultado

- Health e OpenAPI: `200`
- Open run: `202`
- Idempotencia de open run: `202` com resposta reaproveitada
- Start run com `ready_for_execution=true`: `202`
- Add step: `202`
- Complete step com status invalido: `400` (esperado)
- Complete step com `completed`: `202`
- Add artifact: `202`
- Get run / steps / logs: `200`
- Close run: `202`
- Mutacao apos close (add step): `409` (esperado)
- Scope mismatch via `X-Workspace-Id`: `400` (esperado)
- Header obrigatorio ausente (`Idempotency-Key`): `400` (esperado)
- Start run com `ready_for_execution=false`: `400` (esperado)

## Observacoes

- Stack C8 validada em Docker com Postgres + Kafka + app (`docker compose up -d --build`).
- Bootstrap de schema Postgres corrigido para comandos SQL separados (compatibilidade SQLx).
- Guard de mutabilidade confirmou bloqueio para run `closed`, alinhado ao comportamento esperado da camada.
