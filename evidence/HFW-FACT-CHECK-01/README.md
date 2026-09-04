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
   `fresh_preparation_id` (list + Details).
2. `begin_preparation` cancels the failed job and allocates a **new** preparation
   id for those policies (same-id retry unchanged for other policies).
3. Adapter surfaces a bounded fact-check reason in `error_detail` / message
   (e.g. unsupported metric-like claims: `8 years`), without weakening the fact
   check and without treating fact failure as PDF fallback.
4. Desktop ship **0.1.3**; live BCNC force-reset for a clean Prepare again.

## Live role

- role_id: `5b2a8484-d54b-40f2-b475-3aa572cdc803` (BCNC GROUP / Senior React Frontend Developer)
- prior prep: `618e5711-7105-4b95-91ea-1670f3580f59` (cancelled by force reset)

## Live before → after

| Signal | Before | After |
| --- | --- | --- |
| BCNC prep | action_required / cv_fact_check_failed / fresh_preparation_provider_run | `not_started`, no blocking non-cancelled job |
| error_detail | generic `Preparation commit failed with cv_fact_check_failed` | bounded invented-claim summary (new failures) |
| Recovery CTA | Details/Dismiss only | Prepare again (new preparation id) |
| Desktop version | 0.1.2 | 0.1.3 |
| codesign | — | `--deep --strict` OK |

## Evidence files

- `before-error.txt` — sanitized failure signals (mentions `8 years`, no CV body)
- `after-live-db.txt` / `after-captured-at.txt` — BCNC preparable + adapter_ready
- `codesign-verify.txt`
- `desktop-version.txt`

## Validation

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib fresh_preparation
node --test packages/career-ops-adapter/preparation-transaction.test.mjs
corepack pnpm test -- src/App.test.tsx
corepack pnpm tauri build
codesign --verify --deep --strict /Users/leo/Desktop/HereForWork.app
node scripts/reset-stuck-preparations.mjs --apply --force-role-id 5b2a8484-d54b-40f2-b475-3aa572cdc803
```

Do not paste personal career data, full provider transcripts, or raw CV HTML here.

## Post-ship (2026-09-04)

- Desktop installed: `/Users/leo/Desktop/HereForWork.app` CFBundleShortVersionString `0.1.3`
- Draft PR: https://github.com/leoengithub/here-for-work/pull/10
- Live BCNC already `not_started` / `Evaluated` with prior fact-check jobs `cancelled` (force-reset candidates empty — no blocking failed prep)
