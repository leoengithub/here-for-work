# HereForWork implementation plan

Status: packaged personal-proof candidate implemented; Ashby review-only vertical slice passes
Date: 2026-08-30
Depends on: `PRODUCT.md`, `MVP_SHAPE.md`, and `STACK_SELECTION.md`

This plan implements a personal proof before distribution. Every iteration must produce a usable vertical result, retain the hard no-finalization boundary, and consider whether private operational knowledge belongs in the Obsidian vault. Stable behavior and interfaces belong in this repository; personal credentials and private career data do not.

## Live checkpoint — 2026-08-30

- Iterations 0–6 are implemented and packaged for Apple Silicon.
- The selected ordinary Chrome profile reconnects to the signed native host, and a real
  Ashby application page completed the inspect/release connection proof with zero fills
  and no terminal action.
- Iteration 4 completed a real Codex preparation through career-ops, producing canonical
  Evaluated row 103 and verified report/PDF artifacts with matching hashes.
- Under specific user authorization, the Ashby vertical slice filled and verified only
  Name and Email, persisted those observed values through career-ops, and released the
  page at `review_required`. LinkedIn, GitHub, declarations, sensitive controls, and
  Submit remained untouched.
- Live duplicate-tab and dynamic-mount races are covered by active-tab selection,
  bounded content-script and form-readiness retries, and rejection of empty snapshots.
  The extension still has no submit operation.
- Greenhouse and Lever have fixture coverage but still require their separate live
  no-finalization observations.

## Cross-cutting rules

- career-ops remains canonical for verified facts, evaluation, generated artifacts, grounded answers, and application history.
- HereForWork owns operational persistence, scheduling, queue UX, preparation state, retries, notifications, and browser handoff.
- The extension only inspects, fills allowlisted safe fields, and verifies. It never performs the final page action.
- Exact answers are drafted only after a live form snapshot is inspected.
- Untrusted job, company, recruiter, and form content is data, never instructions.
- Real career-ops, scheduled tasks, Gmail, credentials, Chrome profiles, and application history are read-only until an iteration explicitly requests and receives mutation authority.
- No private dataset, profile content, mailbox content, or generated artifact is committed.
- Before dependencies, builds, browser QA, or E2E, recheck disk space and the shared pnpm store according to `AGENTS.md`.

## Iteration 0 — Repository and toolchain foundation

Objective: create the smallest reproducible workspace after explicit setup approval.

Deliverables:

- Repository-local package metadata declaring Corepack pnpm.
- React/TypeScript/Vite renderer and Tauri 2 core.
- Cargo workspace prepared for the app core and future native host.
- Formatting, linting, typechecking, Rust checks, and unit-test commands.
- App identifier, data-directory name, database filename, protocol namespace, and extension identity recorded as compatibility identifiers.
- No ATS logic and no career-ops writes.

Preconditions requiring user intervention:

- Approve installing Rust through the official toolchain.
- Recheck at least 8 GiB free.
- Verify `pnpm store path` resolves under `/Users/leo/Library/pnpm/store` after the local package declaration exists.

Gate:

- A packaged, locally signed Apple Silicon app opens one React window.
- Closing and reopening the window works without duplicate core processes.
- No local web server is required in the packaged build.

Vault decision: no note unless toolchain/signing setup reveals private account information.

## Iteration 1 — Operational core and real proof import

Objective: display the real two-workflow proof dataset through a safe read-only import.

Deliverables:

- Initial SQLite schema and transactional migration runner.
- Tables for source occurrences, normalized roles, queue items, runs, steps, source cursors, and schema metadata.
- Versioned read-only import contract.
- Import of findings from both scheduled searches since 2026-08-29 08:00 Europe/Madrid from a local private source.
- Deterministic normalization, probable-duplicate presentation, and provenance.
- Queue groups: strong matches, other new roles, and needs decision.
- Redacted synthetic fixtures for repository tests; actual findings remain outside Git.

Gate:

- All approximately 32 source findings are accounted for.
- Likely overlap is visible and reversible rather than silently merged.
- Re-import is idempotent.
- No source-provided eligibility label can bypass career-ops evaluation.
- Restart reproduces the same queue.

Vault decision: a private proof-run note is appropriate only if it records sensitive role URLs or reconciliation observations that should not be committed. Stable normalization rules return to the repository.

## Iteration 2 — Packaged lifecycle, recovery, and activity

Objective: prove that HereForWork is a reliable local product rather than a development window.

Deliverables:

- Single-instance background core and disposable window lifecycle.
- Opt-in login launch with no window and no menu bar.
- Explicit background enable/disable and Quit behaviors.
- Run/step leases, crash recovery, bounded retries, and cancellation.
- Structured local activity log with redaction.
- Pre-migration backup, integrity checks, export, and restore preflight.
- Native notification permission requested in context.

Gate:

- A packaged app survives window closure while a synthetic job completes.
- Sleep/restart yields one consolidated recovery run.
- Notification opens the intended local screen.
- Interrupted migration or work item is recoverable without duplicate effects.

Fallback gate:

- If Tauri's autostart plugin cannot provide reliable lifecycle behavior, implement a narrow macOS `SMAppService` bridge and repeat the package test before proceeding.

Vault decision: save private signing-account instructions only if needed; never commit credentials or certificate material.

## Iteration 3 — Read-only career-ops adapter

Objective: replace filesystem inference with a typed, capability-negotiated boundary.

Deliverables in HereForWork:

- Versioned NDJSON adapter client over stdio.
- JSON Schema contracts and generated TypeScript types.
- Explicit Node and career-ops path onboarding.
- Capability, health, history, liveness, and evaluation read models.
- Error normalization into completed, action-required, retryable, permanent, and conflict outcomes.

Required separate authority:

- Adding or changing the corresponding adapter entry point in `/Users/leo/Work/career-ops` is a different repository mutation and requires explicit approval before execution.

Gate:

- HereForWork can re-evaluate and reconcile the proof roles without changing canonical history.
- Adapter restart and malformed output cannot corrupt operational state.
- Every result carries career-ops provenance and input hashes.

Vault decision: no note unless path setup or local profile facts are private; protocol decisions stay in both repositories' stable documentation.

## Iteration 4 — Provider-neutral preparation

Objective: prepare one real role through either subscription CLI without provider coupling.

Deliverables:

- Provider capability interface.
- Codex noninteractive provider.
- Claude noninteractive provider.
- Exact executable-path selection and health UI.
- Structured schema output, timeouts, cancellation, redacted logs, and ephemeral sessions.
- career-ops-owned report/CV preparation returning artifact references and hashes.
- Manual browser handoff; no extension yet.

Gate:

- The same preparation contract passes with Codex and Claude using fixtures.
- One configured provider completes a real preparation from the packaged app.
- Missing login or moved executable produces a clear user-intervention state.
- Provider output alone cannot write application history.

Vault decision: authentication remains in provider-owned storage. Record only personal provider preferences in the vault if they should not be committed.

## Iteration 5 — Scheduling and source ownership

Objective: demonstrate consolidated catch-up without prematurely disabling either current workflow.

Deliverables:

- Per-source timezone-aware schedules and successful-coverage cursors.
- Consolidated missed-window calculation.
- Offline and authentication-required states.
- Notification deduplication through an outbox.
- Manual Run now and retry controls.
- Source-adapter seam for eventual Gmail alert ingestion and web discovery.

Gate:

- Multiple missed nominal times create one catch-up interval.
- Zero-result success is quiet.
- A source failure does not advance its cursor.
- Existing schedules remain active and unchanged until replacement evidence is accepted.

Cutover checkpoint requiring user approval:

- Compare at least one parallel observation period.
- Reconcile duplicates and misses.
- Explicitly choose which old scheduled task to pause or retain. HereForWork never changes it implicitly.

Vault decision: source credentials and private mailbox setup belong in the vault or Keychain-backed onboarding, while schedule semantics remain in the repository.

## Iteration 6 — Native messaging and selected Chrome profile

Objective: establish the browser trust boundary without touching an ATS form.

Deliverables:

- Small Rust native-messaging host.
- User-only Unix-domain-socket transport to the running core.
- Exact extension-origin, protocol-version, request-size, and session-capability validation.
- Manifest V3 extension service worker and isolated content script.
- Chrome profile onboarding:
  - select an existing profile;
  - create one in ordinary Chrome and connect it;
  - or accept/edit a suggested name.
- Stable connection keyed by extension ID plus a profile-local installation UUID, not display name.
- A separately paired test profile, named by the user, for automated fixtures; tests never touch the selected personal profile.
- Detection and refusal of automation-marked production sessions.

Gate:

- The user-selected ordinary Chrome profile reconnects after restart.
- The app cannot communicate with an unapproved extension ID or a different profile installation.
- A malicious page message cannot invoke a privileged operation.
- No WebDriver, CDP, remote-debugging port, localhost server, or arbitrary browser command exists. The user-approved permanent all-sites permission is isolated behind typed native commands for one expected public HTTPS application URL.

User intervention:

- The user installs or approves the private extension, selects the profile, and signs in to required sites manually.

Vault decision: do not store profile cookies or credentials. A private note may record which profile the user selected, but the app's stable local identifier remains operational state.

## Iteration 7 — Ashby vertical slice

Objective: complete one real preparation-to-review flow on the ATS most represented in the proof dataset.

Deliverables:

- Ashby page detection and versioned fixture set.
- Typed live `FormSnapshot` with provenance and hash.
- career-ops answer drafting bound to that snapshot.
- Field classification: safe verified, sensitive, unknown, unsupported, unverifiable.
- Allowlisted fill plan and read-back verification.
- Review summary and explicit release-to-user state.
- Defensive finalization guard active only during extension ownership.
- User confirmation followed by canonical career-ops outcome recording.

Gate:

- Safe fields continue when another field is skipped.
- Changed snapshots invalidate stale drafts.
- Sensitive and uncertain fields require user review.
- Automation never triggers page finalization, navigation, or the terminal control.
- The user physically completes the final page action and separately confirms the outcome.
- A post-confirmation tracking failure retries only the canonical writer.

Vault decision: real-form observations containing personal answers stay private. Selector and behavior conclusions are distilled into redacted fixtures and repository documentation.

## Iteration 8 — Generic web forms, then Greenhouse and Lever hardening

Objective: make generic public-HTTPS inspection the baseline, then improve known ATS reliability through evidence rather than nominal support gates.

Sequence:

1. Generic unknown-host fixtures and best-effort source-to-application URL resolution.
2. Greenhouse, supported by observed proof data.
3. Lever, retained as a named reliability track despite no initial observed role.

Deliverables per ATS:

- Detector, inspector, filler, verifier, and fixtures.
- Permanent all-sites permission review, with inspection remaining inert until a typed expected-URL command.
- Accessibility and keyboard checks.
- Live no-finalization evidence.
- Per-field fallback for drifted variants, custom employer domains, and controls that cannot be verified safely.

Gate per ATS:

- At least one real form reaches verified Review required without page finalization.
- Unsupported controls are reported, not guessed.
- ATS drift falls back to generic inspection; each unsafe or unsupported field fails closed independently.

Vault decision: preserve private live URLs and personal observations outside Git; commit only generalized adapter evidence.

## Iteration 9 — Personal-proof reliability closure

Objective: make the owner build recoverable and trustworthy enough for daily use.

Deliverables:

- Full migration/backup/export/restore rehearsal.
- Retry and crash fault injection.
- Bounded, redacted diagnostics export.
- Keyboard and VoiceOver closure across app and extension.
- Resource, startup, idle, and long-running measurements.
- Manual upgrade and native-host re-registration test.
- Explicit uninstall inventory; no automatic deletion of career-ops or Chrome data.

Gate:

- A multi-day personal trial has no unexplained queue loss, duplicate canonical effects, or finalization invariant violation.
- Recovery procedures are usable without development tools.
- Remaining limitations are visible in the app.

Vault decision: a private trial journal is useful here. Stable defects, recovery steps, and product conclusions must be summarized into repository issues or documentation before implementation relies on them.

## Later — Trusted-user distribution

Deferred until the personal proof is accepted:

- Developer ID signing and notarization.
- Universal Apple Silicon/Intel decision based on target users.
- DMG installation and upgrade UX.
- Stable extension distribution/update channel.
- Credential onboarding for direct discovery sources.
- Signed diagnostics and support workflow.
- Auto-update evaluation.

Phone access, execution while the Mac is offline, email-receipt matching, guaranteed complete autofill across every web control, and detailed visual design remain deferred.

## Test matrix

| Layer | Primary approach | Production-data rule |
|---|---|---|
| React view models | Vitest, Testing Library, accessibility assertions | Synthetic only |
| Rust core | Cargo unit/integration tests, property tests, temporary SQLite | Temporary stores only |
| Migrations | Version fixtures, interruption and corruption cases | Copies only |
| Career-ops adapter | Contract fixtures and temporary career-ops checkout/copy | Never real writes |
| Providers | Fake process runner plus installed-CLI health smoke | Redacted output |
| Tauri IPC | Mock runtime and packaged command tests | No remote content |
| App E2E | Tauri/WebdriverIO against the app test build | Separate test state |
| Extension | Unit tests plus Playwright on synthetic ATS pages | Dedicated test Chrome profile |
| Live ATS | Manual ordinary-Chrome verification | No automated final action |
| Accessibility | Automated checks, keyboard, VoiceOver | App and extension independently |

Tauri provides a mock runtime and WebDriver-based desktop testing, including a macOS-capable embedded service. These tests apply only to the app/test browser environments, never the selected production Chrome profile. See [Tauri tests](https://v2.tauri.app/develop/tests/) and [WebDriver](https://v2.tauri.app/develop/tests/webdriver/).

## First implementation authorization request

When implementation begins, request approval for this exact initial scope:

1. Install the official Rust toolchain.
2. Create the repository-local pnpm and Cargo workspace metadata.
3. Scaffold only the packaged Tauri/React lifecycle slice from Iteration 0.
4. Install only dependencies needed for that slice.
5. Build and verify a local packaged app.

Do not bundle the proof dataset, modify career-ops, install the Chrome extension, sign in to any service, or alter scheduled tasks in that first scope.
