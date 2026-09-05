# Queue Attention, Titles, and Applied Label Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Needs attention cards get reason-routed actions, job titles open the listing URL everywhere they look clickable, Queue order is strong → other → Needs a decision → Needs attention, and the external Applied CTA is labeled **Applied**.

**Architecture:** Keep career-ops as the only Discarded/Applied writer. Add `applicationUrl` on pre-queue rows. Add a role-scoped `retry_evaluation` store+Tauri path that resets attempt budget only for stale/executor attention reasons and returns the role to `awaiting_evaluation`. Reuse existing `dismiss_role` for SKIP/not-Evaluated. Render attention **after** `groupOrder` Queue sections. Replace user-visible **I applied elsewhere** only.

**Tech Stack:** React/`App.tsx`, Vitest, Rust store + Tauri commands, existing Queue dismiss + evaluation sync worker.

**Spec:** `docs/superpowers/specs/2026-09-04-queue-attention-and-titles-design.md`

## Global Constraints

- Never submit or send an application.
- SKIP / `canonical_status_not_evaluated` Dismiss → canonical **Discarded** via existing `dismiss_role`.
- Retry evaluation is **per card** only; do not invent a second group retry.
- Keep group **Retry history sync** for `global_reconcile` / `reconcile_application_history`.
- CTA label **Applied**; confirm **Keep role** / **Record Applied**; command `mark_application_applied` unchanged.
- Queue order: `strong_match` → `other_new` → `needs_decision` → **Needs attention**.
- Title is `<a href={url} target="_blank" rel="noreferrer">` when URL exists; plain text when null (not a fake control).
- Do not weaken fact checks. Do not batch-eval the 47 held roles in this PR.
- Branch: `hfw-queue-attention-ux`; draft PR to `main`; no merge without user OK.

## File map

| File | Responsibility |
| --- | --- |
| `src/types.ts` | `PreQueueRoleSummary.applicationUrl` |
| `src-tauri/src/domain.rs` | same field on pre-queue DTO |
| `src-tauri/src/store.rs` | select URL; `retry_evaluation_for_role`; tests |
| `src-tauri/src/lib.rs` | Tauri `retry_evaluation` |
| `src/api.ts` | `retryEvaluation(roleId)` |
| `src/App.tsx` | helpers, attention actions, title links, group order, Applied label |
| `src/App.test.tsx` | fixtures + tests from spec |
| `src/dev/applications-preview.ts` / queue preview | attention fixtures with URLs + reasons |
| `MVP_SHAPE.md` | distill attention mix + title + Applied label |
| `evidence/HFW-QUEUE-ATTENTION-01/README.md` | sanitized |

---

### Task 1: Pre-queue URL + retry_evaluation store (TDD)

**Files:** `domain.rs`, `store.rs`, `lib.rs`, `types.ts`, `api.ts`

- [ ] Add `application_url` / `applicationUrl` to pre-queue dashboard query and TypeScript type.
- [ ] Failing tests:
  - `retry_evaluation_for_role` on `evaluation_result_invalid_or_stale` → state `awaiting_evaluation`, attempt 0, input_hash NULL.
  - same for `canonical_evaluation_missing_executor_unavailable`.
  - `canonical_status_not_evaluated` / SKIP → error (Dismiss path, not retry).
  - `global_reconcile` reason → error (use group Retry history sync).
- [ ] Implement `Store::retry_evaluation_for_role(&mut self, role_id: &str)`.
- [ ] Tauri `retry_evaluation(role_id)` → dashboard; `api.retryEvaluation`.
- [ ] `cargo test --lib retry_evaluation_`
- [ ] Commit: `feat: add per-role evaluation retry for stale attention holds`

Retry reason allowlist (exact):

```text
evaluation_result_invalid_or_stale
evaluation_receipt_pointer_unreadable
evaluation_result_capability_unavailable
canonical_evaluation_missing_executor_unavailable
canonical_evaluation_pending_executor
evaluation_executor_failed
evaluation_executor_receipt_invalid
evaluation_executor_url_missing
```

---

### Task 2: Attention action helper + fixtures (TDD)

**Files:** `App.tsx`, `App.test.tsx`, preview fixtures

```ts
export type AttentionCardAction = "dismiss" | "retry_evaluation" | null;

export function attentionCardAction(reason: string, recoveryScope: string): AttentionCardAction {
  if (reason === "canonical_status_not_evaluated") return "dismiss";
  if (RETRY_EVALUATION_REASONS.includes(reason)) return "retry_evaluation";
  if (recoveryScope === "global_reconcile") return null;
  return null;
}
```

- [ ] Unit tests for SKIP → dismiss, stale/executor → retry, history → null.
- [ ] Preview fixtures: one SKIP, one stale, one global_reconcile; each with `applicationUrl`.
- [ ] Commit: `feat: classify Needs attention card actions`

---

### Task 3: Attention UI + Queue order + titles + Applied label

**Files:** `App.tsx`, `App.test.tsx`, `MVP_SHAPE.md`

- [ ] `PreQueueAttentionGroup`: title link when URL; Dismiss (reuse Queue dismiss confirm pattern + `dismissQueueRole`); Retry evaluation (`retryEvaluation`); keep group Retry history sync.
- [ ] Move `<PreQueueAttentionGroup>` to **after** `groupOrder.map` in the Queue document.
- [ ] Applications row + Details: title/heading link when URL available (preparation summaries may need `applicationUrl` — add to `PreparationSummary` if missing rather than inventing a second lookup).
- [ ] Replace user-visible **I applied elsewhere** with **Applied** (2 label sites).
- [ ] Tests from spec (heading order, `getByRole('button', { name: 'Applied' })`, title `link` href, SKIP Dismiss, stale Retry).
- [ ] Distill `MVP_SHAPE.md` Queue / Applications sentences.
- [ ] `vitest run src/App.test.tsx` + `pnpm typecheck`
- [ ] evidence `HFW-QUEUE-ATTENTION-01`
- [ ] Commit: `feat: route Needs attention actions and link job titles`
- [ ] Push + draft PR to `main`

## Visual contract

- Target: spec copy and Dismiss-pattern inline confirm.
- Capture: Queue with all three attention kinds; Applications Applied CTA; title opens URL (don't claim final visual acceptance).
- Pass budget: 3.

## Escalate

- Preparation row has no URL field and adding it is a large dashboard change — stop and report.
- Dismiss from pre-queue cannot reuse `dismiss_role` for SKIP — stop.
