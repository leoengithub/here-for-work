# Implementation status

Date: 2026-08-31
Stage: generic public-HTTPS path live-proven; Greenhouse and Lever live observations remain

## Verified implementation evidence

- Apple Silicon `.app` launches without a local HTTP server.
- Closing the window leaves one background core; reopening restores it.
- Login launch is opt-in and has no menu-bar item.
- The private proof snapshot imported 32 source occurrences into 30 normalized roles.
- Schema v13 stores an optional source publication date on each source occurrence. Queue projects a date only when non-null occurrences agree; re-imported omission clears the occurrence value and conflicts omit the role-level age.
- Canonical career-ops history now reconciles all 104 Markdown records through
  career-ops' own rebuildable SQLite index. HereForWork no longer redirects tracker
  reads and merge synchronization into a private derived database.
- Codex CLI and Claude CLI both passed the same structured, tool-free conformance probe
  from an empty HereForWork-owned working directory.
- Finder-launched app processes provide a deterministic provider `PATH` beginning with
  the selected CLI's directory. The npm Codex launcher can resolve its sibling Node
  runtime without a terminal shell, and early CLI exits preserve bounded stderr instead
  of collapsing to an opaque broken-pipe error.
- Codex accepts both provider contracts through its strict structured-output path. A
  recursive regression test rejects unsupported `oneOf`, open objects, optional object
  properties, and untyped constants or enums before packaging.
- Two missed discovery sources consolidate to one catch-up run each. The existing
  scheduled tasks remain active and unchanged pending a parallel-observation cutover.
- SQLite integrity check returned `ok`; a restorable backup and readable JSON summary
  were created in local app data.
- The package contains the MV3 extension and signed native messaging host.
- Schema v10 treats the current discovery sources as staged and persists best-effort
  resolved application URLs. Missed windows are preserved
  as `action_required`, not inaccurately presented as executable queued work.
- Active-source runs use ordered discover, reconcile, and notify steps with leases,
  crash recovery, three attempts per step, idempotent completion, cursor advancement only
  after full success, and quiet zero-result success.
- Opening an older operational database creates a pre-migration SQLite backup.
- The app can produce a bounded diagnostics export that omits role details, application
  URLs, activity messages, provider output, and the selected Chrome profile.
- Restore preflight opens the latest backup read-only and reports integrity, schema,
  role count, and run count without replacing the live database.
- Provider command construction and native-message framing have executable-independent
  boundary tests, including exact-origin injection and oversized-frame rejection.
- Failed preparation commits retain one validated provider result in a private
  mode-0600 recovery file. Retries reuse it instead of paying for or accepting a
  different model result. Long provider commands run asynchronously so the window and
  Cancel action remain responsive.
- The renderer follows the approved full-width Queue / Applications information
  architecture with secondary System controls and no Activity tab. Applications keeps
  one current row per role and opens the formatted career-ops Markdown report in a
  right-side details panel rather than showing a question-answer log.
- Queue cards reserve the first line for the role title, keep source-backed age and
  decision metadata together below it, and reserve the right edge for Dismiss then
  Prepare. Group descriptions and the repeated footer explanation are removed. Provider
  selection and background checks now live in System; Queue's upload icon remains an
  explicit selected-file JSON import rather than an automatic refresh.
- Queue filters are initialized from verified career-ops preferences, remain editable in
  System, and apply to both current unprepared roles and future imports. The bounded
  local filter does not replace career-ops evaluation or scoring.
- Prepare evaluates the role before committing artifacts. Viable matches continue into
  report/CV generation and an automatic new-tab browser handoff when authorization is
  unknown or bounded research is inconclusive. Confirmed authorization or legitimacy
  incompatibilities are discarded; other material fit or legitimacy uncertainty returns
  to Needs decision without generating artifacts.
- Applications Dismiss is limited to failed or ready-for-review work. It records
  Discarded through the canonical adapter before removing generated artifacts and local
  preparation/browser state. Failed canonical writes preserve that local work for retry.
- Duplicate provenance remains persisted for reconciliation and diagnostics, but source-occurrence count is intentionally hidden from the decision-focused Queue.
- Safe form fills are not released for human review until the validated answer snapshot
  has been persisted through career-ops' fixed application-answer writer. Persistence
  failure retries that canonical write without inspecting or filling the page again.
- JSON Schema contracts generate compiler-checked TypeScript declarations through the
  repository-local `generate:contracts` command.
- Schema v16 stores sanitized preparation failure stage, detail, retry policy, and CV
  provenance plus
  transactional, deduped failure and released-form outcome events. Visible windows use
  actionable in-app notices; hidden windows use informational macOS delivery, and a
  fully quit app expires undelivered outcomes rather than replaying them after restart.
- Preparation commits now use the NDJSON adapter as the transaction boundary and invoke
  only fixed upstream career-ops CLIs. A private restart-safe journal, exclusive
  publication, drift checks, exact tracker post-verification, and conservative rollback
  provide compensating atomicity. Concurrent external writers remain outside that
  transaction and unsafe rollback becomes `manual_repair_required`.
- System can store a local, hash-bound user-reviewed PDF fallback. It is used only after
  HTML and fact checks pass and PDF rendering fails; its provenance stays visibly
  `user_reviewed_fallback` and never claims tailoring.
- The System control reuses the project's registry-derived shadcn `Field`, `Input`, and
  `Button` primitives; the registry check found no need for a new modal, card, or
  project-owned component.
- Opening an older personal-proof database creates a pre-migration backup before
  migrating to schema v10. The live database and backup copies pass SQLite integrity
  and restore-preflight checks.
- The selected ordinary Chrome profile and private extension reconnect through the
  signed native host without WebDriver, CDP, remote debugging, or automation flags.
- A real Ashby `/application` page was inspected: 43 fields were identified, only two
  classified safe for verified facts, and 41 left to the user.
- The original authorized real-form proof filled and read back only verified Name and Email.
  The current safe-field contract also permits phone, current location, LinkedIn, GitHub,
  portfolio, and other discrete facts when career-ops traces them exactly to verified
  CV/profile sources. Work-authorization declarations, sponsorship, past-work answers,
  diversity controls, file inputs, and the terminal action remain untouched. The
  browser session reached `review_required`; the user-owned Submit button was not clicked.
- Live recovery testing found and fixed three extension races: a stale native port after
  reload, duplicate identical ATS tabs selecting an older content script, and a new tab
  exposing its URL before its content script or dynamic form was ready. Selection now
  prefers the active matching tab, transport and form readiness have bounded retries,
  and application inspection waits through the bounded dynamic-mount window. If no
  compatible fields appear, the application is released for manual review with zero fills
  instead of being rejected; the connection diagnostic still reports an empty page as a failure.
- The packaged app completed a real Codex preparation for that role. career-ops owns
  report `reports/103-ashby-2026-08-30.md`, a two-page A4 PDF that passed bounded
  career-ops fact checks, canonical Evaluated tracker row 103, and matching SHA-256
  references in HereForWork. The hashes establish artifact identity, not complete truth.
- Provider result context is bound by HereForWork to the exact synchronous invocation
  context instead of relying on a model to reproduce a 64-character hash. The adapter
  still recomputes the context before committing, so a real career-ops source change
  during generation remains a fail-closed stale-result error.
- Context hashes use recursively canonicalized JSON, so the same typed preparation
  context survives the browser-to-Rust JSON round trip regardless of object key order.
  Version and hash mismatches now identify which invariant failed.
- career-ops health now checks the executable for its pinned Playwright Chromium, not
  merely the presence of `generate-pdf.mjs`; the missing runtime found by the live proof
  is installed on this Mac.
- The approved rebuildable `src-tauri/target/debug` directory was removed before the
  current validation run. No source or preserved evidence was removed; subsequent builds
  recreated only the artifacts they needed.
- A user-authorized KoreLabs proof exercised the generic multi-step form path. The first
  step filled and read back only verified first name, last name, and email. Later live
  steps classified phone as sensitive and location and profile links as suggestions;
  no such field was auto-filled. The grounded role-motivation answer was drafted only
  after its live question appeared. The final CV upload remained empty and the visible
  **Submit application** control remained untouched.
- A user-confirmed external application was recorded through the canonical Applied
  writer without reopening its form. The rebuilt packaged app reconciled the new tracker
  row, increased its hidden-history count, and removed the role from Queue. No browser
  command or page action was issued during reconciliation.
- Multi-step inspection now clears prior field markers and excludes controls hidden by
  attributes, ARIA, inline style, or computed style. A refill reuses an existing
  `review_required` browser session instead of reopening the original URL and resetting
  the form.

## Current validation

The current adapter persists artifact and context hashes and reports bounded validation
outcomes. It does not yet carry structured, source-backed CV change provenance with
classified changes, source references, unresolved additions, and an explicit review or
block result. Therefore the implementation may not describe a CV as fully verified or
proven truthful. Match scoring remains career-ops-owned and must be presented on its native
1–5 scale without percentage or probability conversion.

- React and extension TypeScript checks pass.
- Thirty extension-focused Vitest tests and ten adapter tests pass, including hidden
  multi-step controls, post-snapshot visibility changes, and JSON key-order-independent
  context hashing.
- Forty-one Rust core/native-host tests pass across all targets: 37 library tests and four
  native-host tests.
- Rust formatting and Clippy with warnings denied pass.
- The all-sites extension and ad-hoc-signed Apple Silicon app build successfully.
- The current package passes strict code-signature verification and contains the answer,
  preparation, browser, and provider schemas plus the rebuilt extension.

## Extension evidence

- The user-approved permanent all-sites host permission covers application domains that are not knowable in advance; the content script runs only on HTTPS pages and remains inert until a typed command targets the expected URL.
- Synthetic fixtures for Ashby, Greenhouse, Lever, and a custom generic form pass inspection tests.
- Any public HTTPS application URL is accepted. The adapter attempts source-to-form resolution and persists the resolved URL; inaccessible or login-gated sources retain the original URL without blocking preparation.
- Terminal controls are excluded from snapshots.
- Only `safe_verified` instructions can be filled; other fields skip independently.
- Read-back verification is required.
- Grounded suggestions and verified fills are written to the career-ops report before
  extension ownership is released; sensitive or skipped fields are not recorded as used.
- A stale snapshot fails closed.
- The terminal page action is guarded during extension ownership and released only for
  human review. No finalization operation exists in source or transport.
- `navigator.webdriver` causes a fail-closed manual-completion state; production Chrome
  is never launched with WebDriver, CDP, remote debugging, or automation flags.
- Concurrent popup/service-worker startup coalesces Installation ID creation into one
  stable profile-local value.
- Opening the popup actively refreshes the native hello when Chrome retained a stale
  port across an app/package restart, restoring the polling loop without automation
  control or a page-originated command path.
- Duplicate matching application URLs prefer the selected Chrome tab, then the newest match; tracking query parameters do not break matching.
  Inspection waits for both the content-script receiver and a non-empty dynamically
  mounted form; neither wait can introduce a finalization command.
- Recovery attempts remain visible for audit, but only the newest browser session for a
  preparation exposes the user-confirmed outcome control.

## Manual intervention checkpoints

The browser pairing checkpoint is complete on this Mac with the user-selected ordinary
Chrome profile. A new installation still follows these steps:

1. Open `chrome://extensions` in the preferred ordinary Chrome profile.
2. Enable Developer mode and load the unpacked extension from the path shown in the
   app's System screen.
3. Copy the resulting 32-letter extension ID into HereForWork, select any existing or
   user-created/named profile, open the extension popup and copy its profile-local
   Installation ID, then
   choose **Connect selected profile**.
4. Choose **Enable alerts** and accept the macOS notification prompt if notifications
   are desired.

No LinkedIn or ATS credentials are requested by HereForWork. Sign-in remains inside the
chosen ordinary Chrome profile. The current live page is public and required no login.

The Ashby and KoreLabs transmission checkpoints are complete under the user's specific
authorization. The latest packaged-app refill reused the existing KoreLabs tab, inspected
the terminal upload step, found no compatible visible fields, and released it for manual
review. The CV upload is still empty, the canonical row remains **Evaluated**, and neither
**Submit Application** nor **Submit application** was touched. Future real forms require
their own scoped authority before transmitting personal data.

## Deliberately not cut over

- The two existing Codex/ChatGPT scheduled workflows are not paused or edited. They remain
  the operationally authoritative executors for their respective sources; HereForWork's
  local scheduling, cursor, lease, and catch-up machinery is migration-readiness evidence,
  not active executor authority.
- No source has completed the approved versioned-result baseline, 14-day read-only shadow,
  per-source canary, and explicit promotion in `SCHEDULING_MIGRATION.md`. Frontend Role Scan
  must canary first. EU Job Radar additionally requires typed, idempotent Gmail-effect
  receipts before its canary.
- No Gmail state, career-ops profile source, credentials, or scheduled task was mutated.
  Canonical history changed only through the explicit Prepare action described above.
- Real report/CV generation plus Ashby and generic KoreLabs inspect, draft, fill, verify,
  persist, and release-to-review paths have been exercised. No application was submitted.
- Developer ID notarization and stable extension distribution remain post-personal-proof.

## Remaining proof boundary

The typed writable adapter runs only fixed career-ops entry points. The generic public-HTTPS
path is live-proven with per-field fallback. The next checkpoints are observed Greenhouse
and Lever variants, each ending at review without submission.
Scheduled workflows, Gmail, credentials, and career-ops profile facts remain outside
automated test mutation. The required 14-day shadow, promotion evidence, and rollback
exercise have not been completed, so no scheduling cutover may be inferred from the
implemented scheduler machinery.

## Obsidian decision

This iteration has private operational evidence: the selected Chrome profile, live Ashby
and KoreLabs URLs, canonical report/tracker references, field values, and recovery
observations. The non-sensitive outcome and private references belong in the Obsidian
personal-proof journal; only generalized behavior and redacted counts stay here.
