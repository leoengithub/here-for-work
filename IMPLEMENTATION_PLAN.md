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
  Evaluated row 103 and report/PDF artifacts with matching hashes. Those hashes establish
  artifact identity, not complete content truth.
- Under specific user authorization, the Ashby vertical slice filled and verified only
  Name and Email, persisted those observed values through career-ops, and released the
  page at `review_required`. LinkedIn, GitHub, declarations, sensitive controls, and
  Submit remained untouched.
- Live duplicate-tab and dynamic-mount races are covered by active-tab selection,
  bounded content-script and form-readiness retries, and rejection of empty snapshots.
  The extension still has no submit operation.
- Greenhouse and Lever have fixture coverage but still require their separate live
  no-finalization observations.

## Approved next dependency order

The original iterations below record how the personal proof was built. They do not
override the approved target sequencing now defined in `PRODUCT.md` and `MVP_SHAPE.md`.
Continue from the current implementation in this dependency order:

1. **Version the career-ops capability boundary.** Define HereForWork-owned typed
   orchestration for existing upstream `scan`, `discover`, full A–G evaluation, report/CV
   artifact reads, and preparation freshness. Do not patch, fork, vendor, add an
   HereForWork entry point to career-ops, invent another engine, or silently rewrite
   `auto_pdf_score_threshold`.
2. **Build the pre-Queue pipeline.** Orchestrate career-ops discovery, source-local
   normalization/deduplication, liveness, blocker checks, and full evaluation for every
   live, unique, nonblocked role. HereForWork owns replay idempotency and cross-run/
   cross-source identity reconciliation at typed ingestion, not another dedupe engine.
   Require a complete report and canonical native 1–5 score before Queue. Let career-ops
   generate CV/PDF artifacts according to its supported threshold, initially `3.5`.
   Use career-ops' supported batch/pipeline parallelism and model routing for volume:
   fast/economy first, low-confidence escalation, and an audit sample.
3. **Project evaluated decisions in Queue.** Add the native score, concise evidence,
   blockers or gaps, compensation context, and material uncertainty without percentages,
   probabilities, or HereForWork scoring. A staged `not_scored` record is not queueable.
4. **Make Prepare artifact-aware.** Reuse current report/CV artifacts and generate or
   refresh only missing, failed, or stale work. Explicit user preparation may request a
   CV/PDF for a below-threshold role before the browser lane begins.
5. **Add single-driver browser fallback.** Keep the extension primary and require one
   result per planned field plus settled read-back for every required fillable. Release
   its lease before the unchanged career-ops `apply` Playwright path takes ownership after
   an eligible hard pre-fill failure. Partial fill, authentication, CAPTCHA, anti-bot, or
   uncertain state goes to visible human handoff; retain ordered copy/paste recovery and
   the hard no-submit boundary.
6. **Close and prove the terminal workflow.** After physical submission and explicit user
   confirmation, retry only the canonical Applied writer. Applied ends HereForWork's
   scope. Add focused contract, recovery, and live no-finalization evidence, including
   Greenhouse and Lever, without creating follow-up or CRM behavior.

Scheduling migration remains independently gated by `SCHEDULING_MIGRATION.md`. Pipeline
implementation does not transfer executor authority or authorize a source cutover.

The verified interface inventory and compatibility gates for this sequence are in
[`contracts/career-ops-capabilities.md`](contracts/career-ops-capabilities.md). The
following isolated tasks refine the six target steps into acceptance-sized work.

### Dependency-ordered target tasks

```text
Capability boundary
  -> Pre-Queue pipeline
       -> Queue projection
       -> Artifact-aware Prepare
            -> Extension completion + lease + fallback
                 -> Terminal Applied proofs
```

#### Capability boundary

Deliver the versioned capability manifest and exact-revision probes described in the
capability contract. No discovery/evaluation/browser operation is enabled by this task.

Acceptance:

- Exact upstream revision and declared version are reported separately.
- Every capability is `supported`, `degraded`, or `unavailable` from a strict probe.
- Missing/changed interfaces fail closed without parsing CLI prose.
- Product-required but unsupported capabilities are visible blockers.
- No career-ops files or configuration are changed.

#### Pre-Queue pipeline

Depends on supported typed discovery, liveness, full A-G evaluation, evaluated-result
read, and canonical result-receipt capabilities. Orchestrate runs, retries, replay
idempotency, and cross-run/cross-source identity in HereForWork while career-ops retains
source-local normalization/deduplication and all evaluation authority.

Acceptance:

- Every queue candidate is live, unique, nonblocked, and has a complete A-G report plus
  canonical native score 1–5.
- Capped, partial, stale, malformed, or outage-stopped discovery never publishes as a
  successful empty run.
- Reverse-ATS discovery never writes cache or other state into the career-ops checkout;
  an isolated HFW-owned execution/copy/cache boundary or a verified zero-checkout-write
  upstream mode is proven before it is enabled.
- `not_scored`, missing-report, score/report disagreement, suspicious, dead, blocked,
  and previously applied fixtures remain out of Queue with typed reasons.
- Parallel work honors the supported upstream concurrency/model contract; there is no
  HFW score, model override, low-confidence escalation, or audit sampling unless the
  corresponding capability is supported.
- Canonical effects are replay-safe and no scheduled-source authority changes.

#### Queue projection

Depends on the typed evaluated-role projection from Pre-Queue.

Acceptance:

- Cards show the native 1–5 score, concise source-backed strengths, blockers/gaps,
  compensation context, and material uncertainty from the validated report result.
- No percentage, probability, advisory `rank:`, or HFW-derived confidence appears.
- Missing evidence does not become positive copy; nonqueueable results are absent.
- Existing compact card, action placement, keyboard, 200% zoom, and shadcn policy remain
  intact with synthetic fixtures only.

#### Artifact-aware Prepare

Depends on Pre-Queue plus supported artifact inspection/provenance/freshness.

Acceptance:

- Current valid report/CV artifacts are reused; only missing, failed, or typed-stale work
  is generated or refreshed.
- A below-threshold role generates its missing CV/PDF only after explicit Prepare.
- The effective upstream threshold and any mismatch are visible; HFW never rewrites it.
- File existence or modification time alone never claims freshness or provenance.
- Retry resumes at the failed step and preserves already validated artifacts.

#### Extension completion, lease, and fallback

Depends on Artifact-aware Prepare and a supported review-only browser fallback capability.

Acceptance:

- Extension success has exactly one result per planned field and correct settled read-back
  for every required fillable.
- The single-driver lease is durable and released before fallback starts.
- Only eligible hard pre-fill failures auto-fallback; partial fill, authentication,
  CAPTCHA, anti-bot, and uncertain state become visible human handoff.
- Fallback returns typed field results and an ordered grounded copy/paste plan when safe
  autofill cannot finish.
- No transport, extension, fallback, fixture, or test contains a submit/finalize action.
- Redacted generic fixtures pass before separately authorized Greenhouse and Lever live
  review-only proofs.

#### Terminal Applied proofs

Depends on the browser workflow reaching human review; the tracking writer itself may be
contract-tested independently.

Acceptance:

- Only physical user submission plus explicit confirmation invokes Applied.
- A tracking failure retains the idempotency key and retries only the canonical writer.
- Tracking retry never opens, inspects, fills, or reuses a browser session.
- Canonical/history reconciliation proves one Applied effect and terminal HFW state.
- No follow-up, outreach, reply-watch, interview workflow, or post-Applied state is added.

## Cross-cutting rules

- career-ops remains canonical for verified facts, native 1–5 match scoring, generated artifacts and their provenance, grounded answers, and application history. HereForWork does not rescore or convert match output into a percentage or probability.
- HereForWork owns operational persistence, queue UX, preparation state, retries, notifications, and browser handoff. Scheduling is its end-state responsibility; current executor authority remains with the existing tasks until the per-source migration contract passes and the user approves cutover.
- The extension only inspects, fills allowlisted safe fields, and verifies. It never performs the final page action.
- Exact form answers are drafted only after a live form snapshot is inspected. Discovery
  evaluation and its report occur before Queue; they do not pre-author unseen form answers.
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

Boundary supersession: this iteration's deterministic normalization was built for the
legacy proof importer. In the approved target, career-ops owns source-local discovery
normalization and deduplication; HereForWork retains typed replay idempotency and
cross-run/cross-source identity reconciliation only.

Gate:

- All approximately 32 source findings are accounted for.
- Likely overlap is visible and reversible rather than silently merged.
- Re-import is idempotent.
- No source-provided eligibility label can bypass career-ops evaluation.
- Restart reproduces the same queue.

Vault decision: a private proof-run note is appropriate only if it records sensitive role URLs or reconciliation observations that should not be committed. Stable typed-ingestion and identity-reconciliation rules return to the repository; source-local discovery normalization remains career-ops behavior.

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

Supersession note:

- The earlier path of adding or changing an adapter entry point in career-ops is no
  longer permitted. career-ops is an external dependency and remains unchanged;
  integration lives entirely in HereForWork against fixed, supported upstream behavior
  and capabilities.

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

Objective: demonstrate consolidated catch-up and migration evidence without prematurely disabling either current workflow. [SCHEDULING_MIGRATION.md](SCHEDULING_MIGRATION.md) is the authoritative contract for stages, per-source authority, promotion, and rollback.

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

- Establish versioned typed result ingestion from the authoritative executor.
- Complete a 14-day strictly read-only shadow with all expected windows accounted for, zero silent loss, zero duplicate notifications or shadow-caused Gmail/canonical effects, crash/retry idempotency, safe cursor behavior, and catch-up evidence.
- Reconcile every duplicate, miss, and result difference before promotion.
- Canary Frontend Role Scan first. Canary EU Job Radar only after typed, idempotent Gmail-effect receipts exist.
- Explicitly approve the source handoff and choose which legacy task to pause. Preserve its configuration and cursor for rollback; HereForWork never changes it implicitly.

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

## Iteration 10 — Multi-role preparation pipeline

Objective: let the user keep selecting viable roles while HereForWork completes each preparation-to-review workflow safely in the background.

Deliverables:

- Durable FIFO preparation queue with two concurrent report/CV workers.
- Immediate per-role transition from Queue to Applications with queued, preparing, and action-required states.
- Serialized career-ops canonical writes even when provider work overlaps.
- A separate one-application browser lane that orders inspection, grounded drafting, safe filling, verification, and release.
- Background Chrome tab opening that does not steal focus.
- Role-scoped cancellation, retry, failure recovery, and notifications; one failed role never stalls later work.
- No global UI busy state for Prepare.

Gate:

- Three rapid Prepare selections leave Queue usable, run no more than two provider preparations at once, and preserve the third durably.
- Browser commands for a later role are not leased until the earlier active application reaches review-required or action-required.
- Restart preserves queued work and converts an interrupted active provider invocation into a truthful action-required state.
- No browser path or test can submit, finalize, or activate the terminal page control.

Vault decision: this is a stable implementation contract and belongs in repository documentation. The Obsidian product notebook only needs a concise decision mirror; it must not contain operational job data or generated artifacts.

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
