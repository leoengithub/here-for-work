# HFW-FACT-CHECK-01 — Fact-check recovery (Prepare again)

## Problem

BCNC Prepare failed with `cv_fact_check_failed` / `fresh_preparation_provider_run`
after career-ops `verify-cv-facts.mjs` blocked an invented metric-like claim
(`8 years`). Applications only offered Details/Dismiss because Retry was gated to
`retry_same_preparation` | `repair_runtime_then_retry`. Re-queuing the same
preparation id would have reused the poisoned provider result via
`recover_preparation_result`.

## Fix

1. UI exposes **Prepare again** for `fresh_preparation_provider_run` /
   `fresh_preparation_id`.
2. `begin_preparation` cancels the failed job and allocates a **new** preparation
   id for those policies (same-id retry unchanged for other policies).
3. Adapter surfaces a bounded fact-check reason in `error_detail` / message
   (e.g. unsupported metric-like claims), without weakening the fact check.
4. Desktop ship **0.1.3**; live BCNC force-reset for a clean Prepare again.

## Live role

- role_id: `5b2a8484-d54b-40f2-b475-3aa572cdc803` (BCNC GROUP / Senior React Frontend Developer)
- tracker: 147

## Verification (sanitized)

- Vitest `src/App.test.tsx` — Prepare again CTA + recovery action mapping
- Node `preparation-transaction.test.mjs` — bounded invented-claim diagnostics
- Rust `fresh_preparation_*` store tests — cancel + new id
- Reset: `node scripts/reset-stuck-preparations.mjs --apply --force-role-id 5b2a8484-d54b-40f2-b475-3aa572cdc803`

Do not paste personal career data, full provider transcripts, or raw CV HTML here.
