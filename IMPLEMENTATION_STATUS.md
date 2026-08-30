# Implementation status

Date: 2026-08-30
Stage: generic public-HTTPS path implemented; unknown-site, Greenhouse, and Lever live observations remain

## Verified implementation evidence

- Apple Silicon `.app` launches without a local HTTP server.
- Closing the window leaves one background core; reopening restores it.
- Login launch is opt-in and has no menu-bar item.
- The private proof snapshot imported 32 source occurrences into 30 normalized roles.
- Canonical career-ops history reconciled 99 records and hid six deterministic matches,
  leaving 24 new roles. Import and reconciliation were read-only; the later explicit
  Prepare action created canonical Evaluated row 103 through career-ops' fixed writers.
- Codex CLI and Claude CLI both passed the same structured, tool-free conformance probe
  from an empty HereForWork-owned working directory.
- Codex accepts both provider contracts through its strict structured-output path. A
  recursive regression test rejects unsupported `oneOf`, open objects, optional object
  properties, and untyped constants or enums before packaging.
- Two missed discovery sources consolidate to one catch-up run each. The existing
  scheduled tasks remain active and unchanged pending a parallel-observation cutover.
- SQLite integrity check returned `ok`; a restorable backup and readable JSON summary
  were created in local app data.
- The package contains the MV3 extension and signed native messaging host.
- Schema v9 treats the current discovery sources as staged. Missed windows are preserved
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
- The renderer follows the approved full-width Queue / Applications / Activity
  information architecture. Prepare creates a career-ops report and verified tailored CV;
  Applications can display the report and open the verified CV; Discard and Undo use
  canonical writers. Active provider work can be cancelled safely and retried.
- Duplicate provenance remains visible as a source-occurrence count on each normalized
  role rather than being hidden by consolidation.
- Safe form fills are not released for human review until the validated answer snapshot
  has been persisted through career-ops' fixed application-answer writer. Persistence
  failure retries that canonical write without inspecting or filling the page again.
- JSON Schema contracts generate compiler-checked TypeScript declarations through the
  repository-local `generate:contracts` command.
- Opening an older personal-proof database creates a pre-migration backup before
  migrating to schema v9. The live database and backup copies pass SQLite integrity
  and restore-preflight checks.
- The selected ordinary Chrome profile and private extension reconnect through the
  signed native host without WebDriver, CDP, remote debugging, or automation flags.
- A real Ashby `/application` page was inspected: 43 fields were identified, only two
  classified safe for verified facts, and 41 left to the user.
- The authorized real-form proof filled and read back only verified Name and Email.
  LinkedIn, GitHub, work-authorization declarations, sponsorship, past-work answers,
  diversity controls, file inputs, and the terminal action remained untouched. The
  browser session reached `review_required`; the user-owned Submit button was not clicked.
- Live recovery testing found and fixed three extension races: a stale native port after
  reload, duplicate identical ATS tabs selecting an older content script, and a new tab
  exposing its URL before its content script or dynamic form was ready. Selection now
  prefers the active matching tab, transport and form readiness have bounded retries,
  and application inspection waits through the bounded dynamic-mount window. If no
  compatible fields appear, the application is released for manual review with zero fills
  instead of being rejected; the connection diagnostic still reports an empty page as a failure.
- The packaged app completed a real Codex preparation for that role. career-ops owns
  report `reports/103-ashby-2026-08-30.md`, a two-page A4 verified PDF, canonical
  Evaluated tracker row 103, and matching SHA-256 references in HereForWork.
- career-ops health now checks the executable for its pinned Playwright Chromium, not
  merely the presence of `generate-pdf.mjs`; the missing runtime found by the live proof
  is installed on this Mac.
- The approved rebuildable `src-tauri/target/debug` directory was removed before the
  current validation run. No source or preserved evidence was removed; subsequent builds
  recreated only the artifacts they needed.

## Current validation

- React and extension TypeScript checks pass.
- Twenty-eight extension-focused Vitest tests and nine adapter tests pass in the generic-form change set; the full suite is re-run before packaging.
- Thirty-one Rust core/native-host tests pass across all targets: 27 library tests and
  four native-host tests.
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

The Ashby transmission checkpoint is complete under the user's specific authorization.
HereForWork filled only verified Name and Email, released the page for review, and did
not touch **Submit Application**. Future real forms require their own scoped authority
before transmitting personal data.

## Deliberately not cut over

- The two existing scheduled workflows are not paused or edited.
- No Gmail state, career-ops profile source, credentials, or scheduled task was mutated.
  Canonical history changed only through the explicit Prepare action described above.
- Real report/CV generation and the complete Ashby inspect, draft, fill, verify, persist,
  and release-to-review path have been exercised. No application was submitted.
- Developer ID notarization and stable extension distribution remain post-personal-proof.

## Remaining proof boundary

The typed writable adapter runs only fixed career-ops entry points. The generic public-HTTPS
path is implemented with per-field fallback. The next checkpoints are one unknown-site live
observation plus observed Greenhouse and Lever variants, each ending at review without submission.
Scheduled workflows, Gmail, credentials, and career-ops profile facts remain outside
automated test mutation.

## Obsidian decision

This iteration has private operational evidence: the selected Chrome profile, live
Ashby URL, canonical report/tracker references, field values, and recovery observations.
Record the non-sensitive outcome and private references in the Obsidian personal-proof
journal; keep only generalized behavior and redacted counts in this repository.
