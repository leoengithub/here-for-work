# HFW-PREP-CLEANUP-01 — Q7=B one-shot stuck preparation cleanup

Date: 2026-09-04

## Rules

Reset local preparation for roles that are stuck in `failed` /
`action_required` **and** lack `evaluation_sync` in (`ready`,
`needs_decision`), plus explicit `--force-role-id` overrides.

- Cancels blocking `action_required` preparation jobs
- Sets `roles.preparation_state` to `not_started`
- Expires pending `preparation_failed` notifications for touched roles
- Preserves `evaluation_sync`, `evaluation_receipts`, and canonical tracker fields
- Refuses Applied / Discarded terminal roles

## Implementation

| Piece | Path |
|---|---|
| Store API | `Store::list_stuck_preparation_cleanup_candidates`, `reset_stuck_preparation_for_role`, `reset_stuck_preparations` |
| CLI | `src-tauri/src/bin/reset-stuck-preparations.rs` |
| Wrapper | `scripts/reset-stuck-preparations.mjs` (`--dry-run` / `--apply`) |

## Live apply (authorized)

Touched:

| role_id | company | before | after | reason |
|---|---|---|---|---|
| `8acfb1ba-79a1-4e22-beb0-43c9c5e813e4` | coches.net / Adevinta Motor | failed + `action_required`/`artifact_inspection_unavailable` + needs_decision | not_started, no active prep job | force_role |
| `93d4ef6b-0e49-4513-baf8-58d47033f9fb` | Nunegal Consulting | failed + `action_required`/`artifact_commit_failed` + hidden | not_started, no active prep job | zombie_failed_prep |

Unchanged (as required):

| role_id | company | note |
|---|---|---|
| `5b2a8484-d54b-40f2-b475-3aa572cdc803` | BCNC GROUP | still failed prep but `needs_decision` (not forced) |
| `35f21536-fc15-4d9c-9c9e-f4bcc1cccb1f` | HASH | already not_started |
| `db09eb41-26d8-4c11-83cd-2559ad31bff5` | Ashby | Discarded terminal history left alone |
| `5583aeb6-55a6-413e-b0a2-d796c89bf48f` | KoreLabs | Applied terminal history left alone |

Idempotent re-apply returned zero candidates.

Snapshots: `dry-run-snapshot.json`, `apply-snapshot.json`.

## Validation

- `cargo test --manifest-path src-tauri/Cargo.toml stuck_preparation_cleanup` — 3 pass
- Live dry-run then apply via `node scripts/reset-stuck-preparations.mjs`
- `git -C /Users/leo/Work/career-ops status` — no modifications from this task
  (pre-existing unrelated dirty files only)
