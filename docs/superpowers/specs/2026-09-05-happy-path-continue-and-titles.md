# Happy path: Continue anyway + native title open

Date: 2026-09-05
Status: approved by product owner
Branch: `hfw-happy-path`

## Problem

1. Role titles are `<a target="_blank">`. Packaged Tauri (CSP `default-src 'self'`) never opens the listing. Click looks dead.
2. `cv_fact_check_failed` only offers Prepare again. User wants an explicit override that keeps the role valid and continues into form fill.

## Locked decisions

### Title open

- Keep `<a href>` + link role for accessibility and existing tests.
- In the desktop app, click calls a new Tauri command `open_external_url`.
- Command accepts public `https` URLs with a host only. Reject `http`, `javascript:`, `file:`, missing host.
- Open with `/usr/bin/open` (same pattern as `open_preparation_artifact`).
- Surfaces: Queue `RoleRow`, Needs attention, Applications row, Details heading.
- Missing URL stays plain text.

### Continue anyway

- Show **Continue anyway** only when preparation is `action_required` and `errorClass === "cv_fact_check_failed"`.
- Keep **Prepare again** (fresh provider run). Do not replace it.
- Inline confirm required. Confirm copy must include the stored invented-claim `errorDetail` and state the CV will not be marked fact-checked.
- Command: `continue_unverified_preparation(preparationId)`.
- Does **not** weaken `verify-cv-facts.mjs`. The failed verdict stays recorded.
- Completes the **same** preparation id (reuse staged provider result / HTML). Never invent a second CV generator or tracker.
- Publish path:
  1. If staged tailored HTML from this preparation still exists, generate PDF from it.
  2. Else if the hash-bound user-reviewed fallback is configured and valid, use that.
  3. Else fail visibly; Prepare again remains available.
- Provenance source: `user_accepted_unverified`.
  - When publishing staged tailored HTML: `tailored: true`.
  - When publishing fallback: `tailored: false` plus existing fallback hash/recovery fields.
- Warning required: `code=cv_fact_check_failed`, `recoveredBy=user_accepted_unverified`, detail = bounded invented-claim summary.
- UI and activity must say user-accepted / unverified. Never “fact-checked”, “verified”, or “passed fact checks”.
- After successful complete, queue the application browser session (same as a normal completed prepare).
- Sensitive fields still skipped. No submit command.

## Non-goals

- Changing `verify-cv-facts` allow/deny lists.
- Auto-accepting fact-check failures.
- Desktop rebuild (Manager ships 0.1.8 after merge).
- Submit / send.
