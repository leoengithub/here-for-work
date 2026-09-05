# Queue attention, titles, and Applied label

Date: 2026-09-04  
Status: approved; implementation plan ready  
Branch target: `hfw-queue-attention-ux` (draft PR → `main`)

## Problem

Needs attention cards show a reason and no action. Job titles look clickable but often do nothing. **I applied elsewhere** is the wrong label for an external Applied confirm. Queue shows Needs attention above Needs a decision. Meanwhile ~47 discovered roles never reached career-ops evaluation (separate PATH/executor work).

## Locked decisions

| Item | Choice |
| --- | --- |
| Needs attention actions | Mix by reason (D) |
| SKIP / not-Evaluated Dismiss | Canonical **Discarded** (A), same as Queue Dismiss |
| Retry eval | **Per card** only (A) |
| Applied CTA | **Applied** (was I applied elsewhere) |
| Queue group order | strong matches → other new → **Needs a decision** → **Needs attention** |
| Job title | Opens the listing URL everywhere it appears and looks interactive |

## Needs attention card contract

Always: title is a link to `applicationUrl` when present (`target=_blank`, `rel=noreferrer`).

| Reason class | Actions |
| --- | --- |
| `canonical_status_not_evaluated` / SKIP / non-Evaluated status | **Dismiss** → inline confirm → career-ops Discarded |
| `evaluation_result_invalid_or_stale`, `canonical_evaluation_missing_executor_unavailable`, other executor-hold attention reasons | **Retry evaluation** (this role only) |
| `global_reconcile` / history match | Keep group **Retry history sync**; no invented per-row history button |
| Else | Open listing only |

Retry evaluation requeues that role into evaluating. It does not batch-eval other roles. Dismiss never deletes career-ops reports; it follows existing Queue dismiss rules.

## Titles

Any role title that uses title styling (Queue `RoleRow`, Needs attention, Applications row, Details heading) must be an `<a>` to the application/listing URL when one exists. If URL is missing, keep plain text (not a fake control).

## Applied label

Replace user-visible **I applied elsewhere** with **Applied**. Confirm copy and **Keep role** / **Record Applied** stay. Command name `mark_application_applied` unchanged.

## Non-goals

- Auto-eval of the 47 held roles inside this UI PR (career-ops chunked review is a separate Manager workstream).
- Weakening fact checks or adding submit.
- Queue Mark Applied.

## Tests

- Attention fixture: SKIP shows Dismiss; stale shows Retry evaluation; history-reconcile still shows group retry.
- Title links present in Queue + attention + Applications when URL set; absent when null.
- Applied button accessible name is `Applied`.
- Group heading order: Needs a decision appears before Needs attention in the Queue document.
