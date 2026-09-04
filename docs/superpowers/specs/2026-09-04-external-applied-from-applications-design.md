# External Applied from Applications

Date: 2026-09-04  
Status: approved design (pending spec review)  
Branch target: `hfw-mark-applied-elsewhere` (draft PR → `main`)

## Problem

Some Applications rows never reach HereForWork’s browser confirm path. Example: a LinkedIn Easy Apply role that was prepared (or failed preparation) in HereForWork, then submitted directly on LinkedIn. The user has no way to record canonical **Applied** from the Applications tab, so the role stays stuck while career-ops still shows Evaluated (or equivalent non-Applied).

Existing **I submitted this application** only works when an application browser session exists after HereForWork opened/filled the form.

## Goal

Let the user mark an Applications role as Applied when they applied outside HereForWork, using the same career-ops Applied writer and terminal retirement rules, without inventing a second tracker.

## Non-goals

- Queue CTA for Mark Applied (deferred).
- Deleting generated preparation artifacts on external Applied.
- Auto-detecting LinkedIn / Easy Apply.
- Changing fact-check, Prepare, or browser-submit Applied flows beyond coexistence.
- Any submit/send automation.

## Locked decisions

| Decision | Choice |
| --- | --- |
| Surface | Applications tab only |
| Row eligibility | Any Applications row that is **idle** (see gates) |
| Artifacts | **Keep** generated prep files |
| Active work | **Block** while prep or browser work is active; user Cancels first |
| Confirm UX | Inline confirmation (Dismiss pattern) |
| CTA label | **I applied elsewhere** |
| Architecture | New role-scoped command; reuse career-ops Applied writer |

## Approaches considered

1. Widen session-bound `confirm_application_applied` with a synthetic session — rejected (couples external path to browser model).
2. **New `mark_application_applied(roleId)`** + shared Applied writer — **chosen**.
3. Career-ops / Settings only — rejected (misses the Applications moment).

## UX

### List + Details

- CTA: **I applied elsewhere**
- Placement: Applications row actions and preparation Details, beside existing recovery/dismiss actions when eligible.
- Inline confirm (Dismiss-style alert in the row / detail), not a modal unless the existing Dismiss pattern already uses one (match Dismiss).

### Confirm copy (intent)

Record **Applied** in career-ops. Preparation files stay. HereForWork retires local work for this role. The role leaves Applications.

Buttons: **Keep role** / **Record Applied**.

### Coexistence with browser confirm

- When the latest application session is outcome-confirmable (`I submitted this application` already shown), **hide** **I applied elsewhere** on that row.
- Otherwise, for idle Applications rows without that session path (failed prep, completed prep with no live session, etc.), show **I applied elsewhere**.

## Eligibility gates

Show **I applied elsewhere** only when all hold:

1. Role appears in Applications (has a preparation job in the Applications list).
2. Preparation status is idle for this action: `action_required` or `completed` (not `queued`, not `preparing`).
3. No active browser application work for that role/preparation (`queued`, `preparing`, `waiting_for_extension`, `filling`, or equivalent in-progress statuses). Outcome-waiting / tracking-pending states follow existing Applied UX instead of this CTA where applicable.
4. Not already Applied / Discarded / Rejected terminal in local + canonical sense.
5. Not mid-dismissal, not mid-Applied-recording.

Hide while Applied tracking is pending after a prior confirm; show retry only for the failed tracking write (existing pattern).

## Backend

### Command

`mark_application_applied(role_id, user_confirmed: true)` (Tauri + frontend API).

Reject if `user_confirmed` is not true.

### Preconditions (store)

- Role has tracker id and is not already terminal Applied/Discarded/Rejected.
- Preparation for the role is idle per gates above; otherwise return a clear invalid-state error (“Cancel active work first”).
- No active browser lease/fill for the role.

### Effect

1. Begin idempotent adapter effect for Applied (same career-ops writer as session confirm; `userConfirmed` required).
2. On success: set canonical Applied; retire local prep/browser/evaluation sync for the role as today’s Applied reconciliation does; **do not** delete generated artifact files; remove role from Applications.
3. On writer failure: mark tracking-pending / fail effect for retry only; do not reopen forms; keep Applications row.

### Idempotency

Same as existing Applied path: safe retry after success must not duplicate harmful side effects; second confirm on already-Applied with matching HereForWork effect is a no-op success.

## Product / docs touchpoints

- Distill into `MVP_SHAPE.md` Applications section: **I applied elsewhere** vs Dismiss vs browser **I submitted this application**.
- `NOTIFICATIONS.md` only if toast/native copy changes.
- Evidence folder when shipping: `evidence/HFW-MARK-APPLIED-01/` (sanitized).

## Test plan (acceptance)

- Idle failed prep row: CTA visible → confirm → career-ops Applied → row gone → files still on disk.
- Idle completed prep without browser session: same.
- Queued/preparing prep: CTA hidden; after Cancel → CTA appears.
- Active browser fill: CTA hidden.
- Browser awaiting “I submitted this application”: existing CTA preferred; external CTA policy per coexistence rule.
- Writer failure: row remains; retry tracking only.
- No second independent Applied store; tracker is career-ops.

## Open follow-ups (explicitly deferred)

- Queue-side external Applied before Prepare.
- Optional artifact cleanup choice.
- Richer “applied via LinkedIn” provenance in notes (only if career-ops writer already supports bounded note metadata without schema invention).
