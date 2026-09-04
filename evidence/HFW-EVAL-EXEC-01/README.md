# HFW-EVAL-EXEC-01 — typed evaluation executor live sample

Date: 2026-09-04

Authorized sample only (three roles). Remaining awaiting roles were not evaluated.

| role_id | company | outcome | tracker | native score | evaluation_sync |
|---|---|---|---|---|---|
| `5b2a8484-d54b-40f2-b475-3aa572cdc803` | BCNC GROUP | typed receipt | 147 | 3.6 | `needs_decision` / `canonical_evaluation_verified` |
| `35f21536-fc15-4d9c-9c9e-f4bcc1cccb1f` | HASH | explicit fail-closed diagnostic | — | — | `needs_attention` / `evaluation_executor_failed` |
| `93d4ef6b-0e49-4513-baf8-58d47033f9fb` | Nunegal Consulting | typed receipt | 148 | 2.7 | `hidden` / `canonical_evaluation_not_viable` |

HASH failed because the stored application URL is a generic careers listing (`https://hash.ai/careers`) with no evaluable JD. That is a visible diagnostic, not silent loss.

Artifacts: `summary.json`, `receipt-*.json`, `evaluation-sync-states.json`.
