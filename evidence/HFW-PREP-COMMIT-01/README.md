# HFW-PREP-COMMIT-01 — preparation.result.commit unknown field

## Bug

User Prepare on BCNC failed at commit with toast:

`preparation.result.commit input contains an unknown field.`

Live role id: `5b2a8484-d54b-40f2-b475-3aa572cdc803` (BCNC GROUP / Senior React Frontend Developer). Prior job state: `action_required` / `adapter_error` / `preparation.result.commit`.

## Root cause

Adapter `preparation.result.commit` allowlist:

`preparationId, eventDate, company, title, location, url, job, result, canonicalEvaluation, artifactPlan`

Rust `AdapterConfig::commit_preparation` dumped full `PreparationRoleInput`, which also includes context-only fields:

`trackerId, reportPath, reportSha256, upstreamRevision, evaluationCompatibilityFingerprint, artifactCompatibilityFingerprint`

Those are valid for `preparation.context.get` but illegal for commit → `assertInputKeys` throws.

## Fix

1. Build commit payload with only allowlisted keys (role identity + eventDate + job + result + canonicalEvaluation + artifactPlan).
2. Extend `v2_full_cv_flows_from_context_through_binding_to_commit` to assert commit input keys ⊆ allowlist and forbid context-only fields.
3. Desktop version bump `0.1.1` → `0.1.2`.

Non-goal: do not widen the adapter commit allowlist.

## Live before → after

| Signal | Before | After |
| --- | --- | --- |
| Commit toast | unknown field | (fixed binary 0.1.2; ready for Retry/Prepare) |
| BCNC prep | action_required / adapter_error / preparation.result.commit (historical) | `not_started`, latest cancelled jobs, eval `needs_decision` |
| Stuck reset | n/a | dry-run: no candidates (already clean after prior cleanup) |
| Desktop version | 0.1.1 | 0.1.2 |
| codesign | — | `--deep --strict` OK |

## Evidence files

- `before-error.txt` — toast / root cause (no career data)
- `after-live-db.txt` / `after-captured-at.txt` — BCNC prep + adapter_ready
- `codesign-verify.txt`
- `desktop-version.txt`

## Validation

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib v2_full_cv_flows_from_context_through_binding_to_commit
corepack pnpm build:extension && corepack pnpm tauri build
codesign --verify --deep --strict /Users/leo/Desktop/HereForWork.app
node scripts/reset-stuck-preparations.mjs --dry-run --no-evidence --force-role-id 5b2a8484-d54b-40f2-b475-3aa572cdc803
```
