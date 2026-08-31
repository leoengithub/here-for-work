# HereForWork

> You’re here for work, not the headache of finding work.

HereForWork is a local-first, review-before-submit job-search companion. It is intended to reduce repetitive discovery, evaluation, preparation, and form-filling work while keeping every application decision and final submission with the candidate.

## Status

Personal-proof implementation. The stack and implementation direction are recorded in
[STACK_SELECTION.md](STACK_SELECTION.md) and [IMPLEMENTATION_PLAN.md](IMPLEMENTATION_PLAN.md).

The last verified Apple Silicon package is built at:

```text
src-tauri/target/release/bundle/macos/HereForWork.app
```

It includes the local React/Tauri queue, schema-v10 SQLite migrations, real private proof
import, canonical-history reconciliation, scheduling/catch-up records, Codex and Claude
CLI conformance, provider-neutral report/CV preparation, grounded live-form answers,
login launch, notifications, backup/export, a native messaging host, and an unpacked
all-sites extension with generic form support plus Ashby/Greenhouse/Lever detection. The extension is paired with the selected ordinary
Chrome profile on this Mac; a new installation still requires that one manual step.

The browser flow does not release an inspected form for manual review until verified
fills and grounded suggestions have been written through career-ops' fixed
application-answer writer. If that write fails, retry resumes at persistence and does
not inspect or fill the page again.

The current package also contains run and browser-command leases with bounded retries,
pre-migration backup, restore preflight, redacted diagnostics, provider/native-host
boundary tests, and the approved full-width Queue / Applications information
architecture with secondary System controls. The rebuilt package passes automated checks and code-signature verification.
The real Ashby inspection, Codex preparation, career-ops answer persistence, and explicitly
authorized fill/read-back/release paths now pass. Only verified Name and Email were filled;
all other fields and the user-owned terminal action remained untouched. Greenhouse and
Lever still require separate live no-finalization observations.

The first integration will use career-ops as the engine for verified profile data, evaluation, application artifacts, grounded answers, and canonical tracking. HereForWork will own orchestration, the unified review queue, user-triggered preparation, notifications, retries, and the browser workflow.

## Product boundaries

- Scheduled runs discover, normalize, deduplicate, and classify roles.
- Full reports and tailored CVs begin after the user selects **Prepare application**.
- Application answers are drafted only after the live form has been inspected.
- Every viable role remains available without an arbitrary daily cap.
- The desktop-first queue is a direct list without a lightweight detail pane.
- Queue filters live in System, start from verified career-ops profile preferences, and apply to current unprepared roles and future imports.
- Applications keeps one current row per role; Details opens a formatted career-ops report preview instead of exposing a question-answer log.
- Suspicious findings are excluded before Queue. Preparation keeps a suspicious live result as a safety backstop; unknown authorization and `Proceed with Caution` legitimacy continue with their warnings preserved. Undo preparation records Discarded and deletes only the generated report and tailored-CV artifacts.
- The browser extension is a focused form-inspection, filling, and verification companion, not the primary interface.
- Any public HTTPS application URL is accepted; source listings receive best-effort application-link resolution and unknown sites use conservative generic inspection.
- ATS-specific adapters improve reliability but never act as a support allowlist.
- Verified CV/profile facts including phone, LinkedIn, GitHub, portfolio, and location are safe when directly grounded. The extension may attach only the manifest-verified tailored career-ops PDF to one unambiguous CV/resume control and preserves a file the user already selected; sensitive, unknown, ambiguous, unsupported, or unverifiable fields are skipped while safe fields continue.
- HereForWork never submits an application or sends a message for the user.

Read [PRODUCT.md](PRODUCT.md) before making product or interface decisions.

Read [MVP_SHAPE.md](MVP_SHAPE.md) before proposing technical architecture or implementation.

## Local validation

Use Corepack pnpm and the shared store configured in `pnpm-workspace.yaml`:

```sh
corepack pnpm typecheck
corepack pnpm typecheck:extension
corepack pnpm check:contracts
corepack pnpm test
corepack pnpm test:adapter
cargo test --manifest-path src-tauri/Cargo.toml --all-targets
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

Build the extension and app:

```sh
corepack pnpm build:extension
corepack pnpm tauri build
```

career-ops' verified PDF writer also requires the Chromium revision pinned by its
installed Playwright package:

```sh
cd /path/to/career-ops
./node_modules/.bin/playwright install chromium
```

The adapter health check reports this runtime separately so a missing browser cannot
masquerade as a ready PDF integration.

Do not run automated browser tests against the selected production Chrome profile.
Live-form testing uses ordinary Chrome and ends at review; the user performs the final
page action.
