# Scheduling migration

Status: approved migration direction; no source has been cut over
Date: 2026-08-31
Depends on: `PRODUCT.md`, `MVP_SHAPE.md`, and `IMPLEMENTATION_PLAN.md`

This document is the authoritative migration contract for moving discovery scheduling
from the existing Codex/ChatGPT scheduled tasks to HereForWork. It defines authority,
evidence, promotion, and rollback. It does not implement a result schema, change a task,
or authorize Gmail or canonical-tracker mutation.

## Current authority and target ownership

HereForWork owning scheduling is the approved end-state product responsibility. It is not
a claim that operational authority has already moved.

Today, each existing Codex/ChatGPT scheduled task remains active and is the authoritative
executor for its source. Until a source is explicitly promoted, HereForWork may own local
orchestration state, visibility, comparison evidence, and migration readiness, but it
must not claim executor authority for that source or duplicate its Gmail or canonical
tracking effects.

Authority transfers one source at a time only after the relevant stages and measurable
gates below pass and the user explicitly approves that source's cutover. The corresponding
legacy task may then be paused, but its configuration and cursor must be retained for
rollback. HereForWork never pauses, retires, deletes, or edits an existing task implicitly.

## Single-mutator invariant

For each source and covered window, exactly one executor may be authorized to mutate Gmail
or canonical career-ops tracking. A shadow run is strictly read-only: it may compare and
record evidence in HereForWork-owned operational state, but it must not change Gmail,
advance the authoritative source cursor, write canonical application history, or emit a
second user notification for the same result.

Cutover and rollback must change authority as one controlled operation. Do not enable one
executor's mutating path until the other executor's equivalent path is confirmed inactive.

## Migration stages

### 1. Contract and baseline

- Inventory each source's schedule, timezone, successful-coverage cursor, catch-up rules,
  notification behavior, and possible Gmail or canonical effects.
- Preserve the current task configuration and cursor as rollback inputs.
- Establish versioned typed run results from the authoritative executor and account for
  every expected window, including zero-result, incomplete, failed, and recovered runs.
- Record the authority holder per source explicitly. Typed ingestion is observation, not
  executor promotion.

### 2. Typed result ingestion

- HereForWork consumes the versioned typed results produced by the authoritative scheduled
  task. The contract is defined in `contracts/discovery-run.md` and
  `contracts/discovery-run.schema.json`; the supported repository producer/sealer and
  exact source instructions are in `contracts/discovery-run-producer.md`.
- Begin with user-triggered manual Refresh while the existing selected-file schema-v1
  importer remains available. Then shadow-consume the same immutable run files. Optional
  automatic file consumption is considered only after the shadow evidence is accepted;
  consumption alone never transfers executor authority.
- Replayed or retried results must be idempotent in HereForWork-owned state.
- Invalid, unsupported, incomplete, or missing results remain visible and do not advance a
  successful-coverage cursor.
- No source execution, Gmail mutation, canonical write, or duplicate notification is
  authorized in this stage.

### 3. Fourteen-day read-only shadow

Run HereForWork's equivalent discovery path in strictly read-only shadow mode for 14 days.
The period is acceptable only when every expected window is accounted for. A missed window
does not disappear from the evidence: it must be recovered through catch-up or remain an
explicit unresolved failure.

Promotion evidence must show:

- zero silent loss across expected windows;
- zero duplicate notifications and zero shadow-caused Gmail or canonical effects;
- explained and reconciled differences in discovered, normalized, and classified results;
- crash and retry behavior that is idempotent;
- cursor advancement only after complete successful coverage;
- catch-up evidence for sleep, shutdown, temporary source failure, and delayed execution;
- preservation of the legacy task configuration and cursor for rollback.

The 14-day duration alone is not a gate. Unaccounted windows or unresolved discrepancies
extend the observation period until the evidence is complete.

### 4. Frontend Role Scan canary

Frontend Role Scan is the first canary because it does not require the EU Job Radar Gmail
mutation path. After the shadow gates pass, a canary still requires explicit user approval.
That approval must designate HereForWork as the sole executor for the source and explicitly
pause, but not delete, the corresponding legacy task.

During the canary, continue measuring window coverage, result reconciliation, notification
deduplication, retry idempotency, cursor safety, and catch-up. Do not expand authority to a
second source merely because the first source is running.

### 5. EU Job Radar canary

EU Job Radar may enter canary only after Frontend Role Scan has stable canary evidence and
career-ops provides typed, idempotent receipts for every Gmail effect. Those receipts must
make retries and reconciliation safe enough to prove that the same Gmail action cannot be
applied twice. The general shadow and promotion gates still apply; receipt support is an
additional prerequisite, not a replacement for them.

The EU Job Radar cutover requires its own explicit user approval and single-mutator handoff.
Until then, its existing scheduled task remains authoritative.

### 6. End-state ownership

HereForWork becomes the scheduling authority only for sources that have individually
completed the approved canary and promotion. Product-level ownership is complete only when
all intended sources have been explicitly promoted. A source that has not passed remains on
its legacy executor without blocking proven sources from progressing.

## Promotion decision

A source may be promoted only when all applicable stage evidence is persisted and reviewed,
no discrepancy remains unexplained, rollback inputs are usable, the single-mutator handoff
is defined, and the user explicitly approves the cutover. Current implementation of local
schedules, catch-up records, leases, or cursors is readiness evidence only; it is not proof
of migration completion.

## Rollback

Rollback is required for any of these triggers:

- an incorrect Gmail or canonical-tracking effect;
- cursor advancement after incomplete work;
- unexplained queue or result loss;
- duplicate user notifications;
- an expected window that is neither completed nor recovered;
- loss of idempotency under crash or retry;
- inability to reconcile the canary with the preserved baseline.

On rollback, stop HereForWork's mutating authority for that source before re-enabling the
legacy executor. Reconcile the last complete coverage window and any ambiguous side effects,
then resume from the preserved task configuration and a reviewed cursor. Never run both
mutating executors to repair a gap. Do not delete canary evidence or the failed executor's
state; retain it for diagnosis and a later promotion decision.

## Deferred implementation work

The typed discovery-run result contract and a local structured-draft producer/sealer are
implemented. This decision still does not implement automatic inbox consumption, the
career-ops exporter itself, the Gmail-effect receipt schema, adapter changes, task
controls, or migration UI. Each existing scheduled task still needs an explicit,
executor-verified update to construct its source-specific draft and invoke the producer;
that update does not authorize a scheduler cutover.
