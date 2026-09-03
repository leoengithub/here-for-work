# HereForWork

> You’re here for work, not the headache of finding work.

HereForWork is a local-first, review-before-submit job-search companion. It is intended to reduce repetitive discovery, evaluation, preparation, and form-filling work while keeping every application decision and final submission with the candidate.

## Status

Personal-proof implementation. The stack and implementation direction are recorded in
[STACK_SELECTION.md](STACK_SELECTION.md) and [IMPLEMENTATION_PLAN.md](IMPLEMENTATION_PLAN.md).

The current source uses schema-v22 SQLite migrations. The last recorded Apple Silicon
proof package is at:

```text
src-tauri/target/release/bundle/macos/HereForWork.app
```

That earlier package includes the local React/Tauri queue, real private proof import,
canonical-history reconciliation, scheduling/catch-up records, Codex and Claude CLI
conformance, provider-neutral report/CV preparation, grounded live-form answers, login
launch, notifications, backup/export, a native messaging host, and an unpacked all-sites
extension with generic form support plus Ashby/Greenhouse/Lever detection. The extension is paired with the selected ordinary
Chrome profile on this Mac; a new installation still requires that one manual step.

The browser flow does not release an inspected form for manual review until verified
fills and grounded suggestions have been written through career-ops' fixed
application-answer writer. If that write fails, retry resumes at persistence and does
not inspect or fill the page again.

The recorded proof package also contains run and browser-command leases with bounded retries,
pre-migration backup, restore preflight, redacted diagnostics, provider/native-host
boundary tests, and the approved full-width Queue / Applications information
architecture with secondary System controls. That previously recorded package predates the
current typed incremental-import documentation update; no app rebuild or push is claimed
here for these changes. It was installed at `/Users/leo/Desktop/HereForWork.app` for the
local personal proof.
The real Ashby inspection, Codex preparation, career-ops answer persistence, and explicitly
authorized fill/read-back/release paths now pass. Only verified Name and Email were filled;
all other fields and the user-owned terminal action remained untouched. Greenhouse and
Lever have isolated synthetic public-form inspect/read-back evidence with Submit untouched,
but not native-host/extension E2E evidence because the QA browser exposes WebDriver. The
source and extension manifest now include the narrowly scoped Chrome `scripting` permission
needed for the no-finalization guard. It is used only in the already selected application
tab: the guard is installed in the page's `MAIN` world before inspection/fill and restores
the original `submit`/`requestSubmit` descriptors and listeners only after verified
`release_for_review`. It adds no browser command, host access, or submission capability.
That previously recorded package contains the exact manifest and passed a read-only launch
and navigation smoke check through Queue, Applications, and System; no application or form
action was executed.

The first integration uses career-ops as the sole authority for verified profile data, native 1–5 match scoring, application artifacts and their provenance, grounded answers, and canonical tracking. HereForWork owns orchestration, the unified review queue, user-triggered preparation, notifications, retries, and the browser workflow. Scheduling is an end-state HereForWork responsibility; the existing scheduled tasks remain the authoritative executors until each source completes the staged, explicitly approved migration in [SCHEDULING_MIGRATION.md](SCHEDULING_MIGRATION.md).

## Product boundaries

- Scheduled runs reuse career-ops discovery: `scan` finds roles through configured portals/APIs and broad agentic search, while `discover` expands a company into ATS portal sources. career-ops owns source-local normalization and deduplication. HereForWork orchestrates runs and retries, makes typed ingestion replay-idempotent, and reconciles identity across runs and sources without creating another discovery, deduplication, evaluation, or scoring engine.
- Every live, unique, nonblocked role receives the full career-ops A–G evaluation before Queue. Each valid evaluation writes a complete report; career-ops may also generate CV/PDF artifacts according to its supported `auto_pdf_score_threshold`. HereForWork reads and validates the effective value instead of substituting one. The upstream fallback is `3.0` when that key is absent; this personal career-ops configuration is explicitly set to the approved `3.5`, and the capabilities check reports it as `configured`.
- **Prepare application** validates and reuses current report/CV artifacts. It generates or refreshes only missing, failed, or stale work, including the CV/PDF for a below-threshold role the user explicitly chooses to prepare.
- Application answers are drafted only after the live form has been inspected.
- Every viable role remains available without an arbitrary daily cap.
- The desktop-first queue is a direct list without a lightweight detail pane.
- Queue roles use compact, non-expandable cards with the title on its own line, wrapping decision information below, and quiet Dismiss before primary Prepare. They show the native career-ops 1–5 score with concise evidence, blockers or gaps, compensation context, and material uncertainty, but omit ATS, preparation state, and source-count hygiene. A real source publication date may appear as `Today`, `1 day ago`, or `N days ago`, and disappears when missing, invalid, future-dated, or conflicting. HereForWork never converts the score to a percentage or probability or computes its own score.
- Queue, Applications, and System remain available in a fixed 56-pixel header. Dismiss records canonical Discarded state immediately, then offers a session-only 30-second Undo in a fixed stack of up to three independent notices; notices pause while hovered or keyboard focused and never return after restart.
- Preparation failures and released forms use durable, deduped outcome notifications. Visible windows show actionable in-app notices; hidden windows receive informational macOS notifications. Fully quitting never replays an undelivered outcome on restart. See [NOTIFICATIONS.md](NOTIFICATIONS.md).
- Preparation provider, background checks, and Queue filters live in System. Filters start from verified career-ops profile preferences and apply to current unprepared roles and future imports.
- Queue's upload icon opens a file picker and imports the selected discovery JSON. It routes legacy schema-v1 snapshots and typed `hereforwork.discovery-run` envelopes to their respective importers; it does not imply automatic refresh.
- Applications keeps one current row per role; Details opens a formatted career-ops report preview instead of exposing a question-answer log.
- Suspicious findings are excluded before Queue. Preparation keeps a suspicious live result as a safety backstop; unknown authorization and `Proceed with Caution` legitimacy continue with their warnings preserved. Dismiss in Applications is limited to failed or ready-for-review work; it records Discarded and deletes only generated preparation artifacts after inline confirmation.
- Prepare is a durable background queue: two report/CV jobs may run concurrently while Queue stays interactive, and Applications shows queued and in-progress roles immediately. Browser inspection and filling remain FIFO and one application at a time; one failure cannot block later roles.
- The browser extension is the primary form driver, not the primary product interface. Success requires exactly one result for every planned field and a correct settled read-back for every required fillable value. Expired transport or handshake, a wrong or missing tab, zero compatible fields, missing results, read-back mismatch, and unsupported multi-page, modal, or iframe flows are driver failures that may eventually transfer to a separate review-only fallback; today they remain visible recovery states because that public fallback contract is not yet callable.
- Only one browser driver owns a preparation at a time. The extension releases its lease before any future review-only fallback driver may take over, but HereForWork does not invoke career-ops' private Playwright/apply internals as that fallback. Automatic fallback therefore remains capability-blocked today. Partial fill, authentication, CAPTCHA, or uncertain page state requires visible human handoff. If safe autofill still cannot complete, HereForWork preserves an ordered copy/paste recovery path. No driver may submit.
- Any public HTTPS application URL is accepted; source listings receive best-effort application-link resolution and unknown sites use conservative generic inspection.
- ATS-specific adapters improve reliability but never act as a support allowlist.
- Verified profile facts including phone, LinkedIn, GitHub, portfolio, and location are safe when directly grounded. The extension may attach only the manifest-matched career-ops preparation PDF to one unambiguous CV/resume control and preserves a file the user already selected; sensitive, unknown, ambiguous, unsupported, or unverifiable fields are skipped while safe fields continue. A CV may be described as having passed bounded career-ops fact checks, but not as fully verified or proven truthful until structured, source-backed change provenance exists.
- Career-ops-grounded narrative drafts are filled and read back with their source provenance, detected language, and detected length policy preserved for review. Matching annual compensation controls may receive career-ops' structured `compensation.application_answer`; other currencies or periods remain untouched and HereForWork never scrapes compensation prose.
- CV uploads use the truthful public filename `Leonardo_Gomez_Frontend_Engineer.pdf`; internal preparation provenance still distinguishes tailored generation from the user-reviewed fallback.
- HereForWork never submits an application or sends a message for the user.
- After the user physically submits and confirms the outcome, HereForWork records canonical Applied through career-ops. A tracking failure retries only that writer and never reopens or repeats the form. Applied ends the HereForWork workflow; follow-up, outreach, reply monitoring, interviews, and post-application CRM are out of scope, although the evaluation report may contain interview preparation.
- When canonical Applied is reconciled, active role-scoped preparation and browser work is retired transactionally and idempotently: queued/preparing work is cancelled, pending or leased browser commands are terminalized, and application sessions are released as `applied_recorded`. Completed preparation rows, artifact paths/hashes, and completed browser evidence remain preserved.

Read [PRODUCT.md](PRODUCT.md) before making product or interface decisions.

Read [MVP_SHAPE.md](MVP_SHAPE.md) before proposing technical architecture or implementation.

Discovery producers use the versioned [discovery-run contract](contracts/discovery-run.md).
The existing schema-v1 snapshot remains supported by the legacy selected-file importer;
the dedicated `import_discovery_run` operation accepts digest-sealed version-1 envelopes,
records durable `(sourceId, runId, digest)` replay identity, source/finding identity,
diagnostics, and per-source successful-coverage cursors, with replay, conflict, rollback,
and out-of-order bounds. Manual typed Refresh is implemented and does not imply that the
historical backlog has been imported. The last global legacy boundary observed was
`2026-09-01T11:53:14+02:00`; later Markdown/TSV artifacts lack deterministic run/finding
IDs and digests, so no personal database import was performed. Scheduled tasks remain
authoritative and must first emit valid typed envelopes for a real incremental proof;
producer/exporter output, shadow consumption, automatic sync, and cutover remain gated.

Current source validation on 2026-09-03 passed: frontend 93 tests, adapter 53,
discovery-contract 15, Rust 125 plus 4, typechecks, contract checks, extension build,
Clippy, and formatting. This documentation update does not claim a new app build or push.

## Local validation

Use Corepack pnpm and the shared store configured in `pnpm-workspace.yaml`:

```sh
corepack pnpm typecheck
corepack pnpm typecheck:extension
corepack pnpm check:contracts
corepack pnpm test
corepack pnpm test:adapter
corepack pnpm test:discovery-contract
cargo test --manifest-path src-tauri/Cargo.toml --all-targets
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

Build the extension and app:

```sh
corepack pnpm build:extension
corepack pnpm tauri build
```

career-ops' PDF writer normally requires the Chromium revision pinned by its
installed Playwright package:

```sh
cd /path/to/career-ops
./node_modules/.bin/playwright install chromium
```

The adapter health check reports this runtime separately. As a recovery-only option,
System can save an absolute path to a user-reviewed PDF. HereForWork hash-binds the file
when saved and revalidates it on use. It is used only when PDF rendering fails after HTML
and fact checks, and successful preparations identify it as an untailored user-reviewed
fallback. The personal path and PDF are never committed to this repository.

Do not run automated browser tests against the selected production Chrome profile.
Live-form testing uses ordinary Chrome and ends at review; the user performs the final
page action.
