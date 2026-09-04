# HFW-QA-CORE-01 — Retry history sync, Prepare, Prepare again

Live Desktop **0.1.4** (`8853541`) plus read-only operational DB
`~/Library/Application Support/com.hereforwork.desktop/here-for-work.sqlite3`
(schema 25, captured 2026-09-04T20:40:35Z). No titles, companies, URLs, CV
text, or report bodies are copied here.

## 1. Retry history sync

### Repro

Needs attention at capture:

| role_id | reason | canonical_status | attempt | recovery.scope |
| --- | --- | --- | --- | --- |
| `155883aa-528c-4cb4-989d-e9b1dfb566f8` | `evaluation_result_invalid_or_stale` | Evaluated (tracker 152) | **3** | `global_reconcile` |
| `03660131-37a2-45a7-86b8-6a84c50027d5` | `canonical_status_not_evaluated` | SKIP (tracker 149) | 1 | `repair_career_ops` |
| `171be989-5c57-4567-be57-c2c05a8ec230` | `canonical_status_not_evaluated` | SKIP (tracker 153) | 1 | `repair_career_ops` |

The group **Retry history sync** button is visible because one row is
`evaluation_result_invalid_or_stale`. Product contract keeps SKIP /
`canonical_status_not_evaluated` diagnostic (`repair_career_ops`); gating was
not widened.

`last_history_reconcile_at` = `2026-09-04T18:43:29Z`, same second as the
holds. Career-ops tracker 152 is still `Evaluated` (path present; body not
read).

### Root cause

`claim_evaluation_sync` refuses the same `input_hash` once
`attempt >= 3`. `hold_evaluation` keeps `input_hash` and `attempt`.
User-triggered `reconcile_application_history` then counts the role as
`unchanged` and no-ops with no error.

Automatic background sync still needs that budget. The missing piece was a
user-retry reset.

### Fix

`reconcile_application_history` (Queue button only) calls
`reset_exhausted_global_reconcile_attempts` after a successful history
snapshot. It clears `attempt` / `input_hash` only for
`global_reconcile` reasons. SKIP / `repair_career_ops` rows stay as-is.

## 2. Prepare a role

### Repro

`adapter_ready=true`. Queue-eligible Evaluated + URL + receipt +
`not_started`:

- `801ef0ad-31a2-489f-867a-6824a0894468`
- `82efae54-7c29-4df4-b707-5ab2f12e10a5`

Both look filter-matchable (location/family tokens only; titles not stored).
BCNC `5b2a8484-d54b-40f2-b475-3aa572cdc803` is now **Applied** / terminal
(4 cancelled jobs). Not a Prepare candidate.

Past blockers (`adapter_ready`, capability hold, commit unknown fields,
awaiting canonical evaluation) are not present on these two roles.

### Root cause / fix

No live Prepare command bug on the Evaluated + receipt + URL path.
`begin_preparation` still requires a current receipt
(`ready` / `needs_decision`). UI Prepare stays gated on
`adapterStatus === "ready"` + application URL + evaluation.

Did **not** start a live provider run.

## 3. Prepare again

### Repro

Live `fresh_preparation_provider_run` job:

| field | value |
| --- | --- |
| role_id | `8acfb1ba-79a1-4e22-beb0-43c9c5e813e4` |
| job | `eb226f78-2d5f-4848-a88a-88360955dbb9` |
| status | `action_required` / `preparing_cv` |
| error | `cv_fact_check_failed` / `stage.fact_verification` |
| retry_policy | `fresh_preparation_provider_run` |
| eval | `needs_decision` + current receipt |

Same role already has two older **cancelled** jobs, including one cancelled
at the current job’s `created_at`. Cancel + new id already happened once.
The new draft failed fact-check again (`7 years` in sanitized detail). That
is an honest career-ops fact check, not a reuse of the poisoned id.

### Root cause / fix

Store contract was already correct: cancel `action_required` + new
preparation id. UI already labeled **Prepare again**.

Tightened Applications so:

- Details uses `stage` for `preparationRecoveryAction` (hides undo-cleanup)
- a successful Prepare again closes the stale failed Details sheet and drops
  the cancelled row from the list

Fact checks were not weakened.

## Tests

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib user_retry
cargo test --manifest-path src-tauri/Cargo.toml --lib fresh_preparation_provider_run_starts_a_new_preparation_id
cargo test --manifest-path src-tauri/Cargo.toml --lib evaluated_queue_role_with_receipt
cargo test --manifest-path src-tauri/Cargo.toml --lib every_emitted_attention_reason_has_a_typed_recovery_scope
corepack pnpm exec vitest --config vite.config.ts run src/App.test.tsx -t "Prepare again|undo cleanup|typed group retry"
```

## Ship note

Desktop rebuild is required for the user to see the Retry reset and the
Details-sheet Prepare again cleanup. Installed app remains **0.1.4**.
Manager ships; this branch does not bump the version or merge.

Do not paste personal career data, report bodies, or CV HTML here.
