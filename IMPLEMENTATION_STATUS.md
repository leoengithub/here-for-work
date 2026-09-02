# Implementation status

Date: 2026-09-03
Stage: approved product direction implemented in HereForWork where the current career-ops boundary allows it; browser fallback and executor cutover remain intentionally blocked

## Current product truth

The repository now reflects the approved direction:

- Queue publishes only roles that already have a canonical career-ops evaluation result.
- Queue shows the native 1–5 score together with concise evidence, blockers or gaps,
  compensation context, and material uncertainty.
- Prepare reuses the canonical report and only reuses a CV/PDF bundle when HereForWork can
  prove exact manifest identity, provenance, and compatibility. Everything else refreshes
  conservatively.
- The browser extension remains the primary review-only driver. It inspects, drafts,
  fills supported safe fields, verifies settled read-back, and releases the page for human
  review. It does not submit.
- Canonical Applied remains terminal. A tracking failure retries only the canonical
  career-ops write and never reopens or repeats a form.

These statements describe the implemented HereForWork behavior, not a plan.

## Deliberate limits

Some approved end-state behaviors are still blocked by the audited career-ops interface and
therefore remain unavailable by design:

- HereForWork does not yet own a public, versioned career-ops executor for `scan`,
  company-to-ATS expansion plus liveness plus full A-G evaluation plus atomic result
  receipts. Queue gating currently depends on canonical evaluation artifacts that already
  exist and pass the fail-closed reader, rather than on a newly exposed public evaluator.
- The review-only browser fallback remains unavailable. career-ops' internal Apply /
  Playwright surfaces do not expose a stable public lease-transfer and typed result
  contract, so HereForWork must not call them directly.
- Scheduling ownership has not moved. Existing scheduled tasks remain authoritative per
  source until the typed-result, shadow, canary, and explicit cutover gates in
  `SCHEDULING_MIGRATION.md` are satisfied.

Those are boundary constraints, not unfinished copy.

## Current capability reality

- `evaluation.result.read.v1` is implemented as a strict conditional read behind exact
  upstream revision and compatibility fingerprints.
- `artifacts.inspect.v1` is implemented as a strict conditional read that reuses the
  canonical report and only exact HereForWork-committed bundles.
- `browser.review_fallback.v1` remains unavailable.
- The canonical Applied writer remains post-verified and idempotent, but the capability
  manifest correctly reports it as degraded rather than pre-certified.
- The approved product target for `auto_pdf_score_threshold` is `3.5`, but the current
  effective career-ops value is the upstream default `3.0` because no explicit setting was
  found during the audit. HereForWork must show that mismatch and must not rewrite it.

## Verified implementation evidence

- Apple Silicon `.app` launches without a local HTTP server.
- Closing the window leaves one background core; reopening restores it.
- Login launch is opt-in and has no menu-bar item.
- Queue is gated on canonical evaluations and excludes suspicious results before Queue.
- Queue cards reserve the first line for the title, keep decision-critical metadata below,
  and reserve the right edge for Dismiss then Prepare.
- Queue filters are initialized from verified career-ops preferences and remain a local
  projection, not a second evaluation engine.
- Prepare binds work to the current evaluation receipt, reuses the canonical report in
  place, and reuses only an exact prior HereForWork CV/PDF manifest.
- A stale PDF can be repaired without a provider; an unproven or stale CV causes a full CV
  refresh. The report and canonical tracker row are never duplicated or overwritten.
- Applications Dismiss is limited to failed or ready-for-review work. It records
  Discarded before removing generated preparation artifacts and local browser state.
- Outcome notifications are durable and deduped. Visible windows use in-app toasts; hidden
  windows use informational macOS notifications.
- The selected ordinary Chrome profile and private extension reconnect through the signed
  native host without WebDriver, CDP, remote debugging, or automation flags.
- The extension keeps the final page action guarded until release for review. No submit or
  finalize command exists in the transport.
- Real Ashby and generic KoreLabs proofs exercised inspect, grounded draft, safe fill,
  settled read-back, release for review, and no-submit behavior. Greenhouse and Lever
  still need their own live no-finalization observations.
- A user-confirmed external application was recorded through the canonical Applied writer
  without reopening its form.

## Current validation

This status file records the latest known contract state. Re-run the repository checks
before claiming a new release:

- `corepack pnpm typecheck`
- `corepack pnpm typecheck:extension`
- `corepack pnpm check:contracts`
- `corepack pnpm test`
- `corepack pnpm test:adapter`
- `corepack pnpm test:discovery-contract`
- `cargo test --manifest-path src-tauri/Cargo.toml --all-targets`
- `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`
- `corepack pnpm build:extension`
- `corepack pnpm tauri build`

## Remaining closeout items

- Observe Greenhouse and Lever live variants through the extension-primary no-submit path.
- Keep browser fallback disabled until a public lease/result contract exists.
- Decide explicitly in career-ops whether to configure `auto_pdf_score_threshold: 3.5`.
- Complete scheduling migration evidence separately; no source is ready for cutover yet.
- Publish refreshed private notes in the Obsidian vault for the decisions above.
