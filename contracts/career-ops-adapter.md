# career-ops adapter protocol

Version 1 is an NDJSON request/response protocol over a child process. HereForWork
starts the bundled adapter with a fixed executable and fixed script path; neither
operation names nor subprocess arguments come from job or form content.

The verified upstream capability inventory, compatibility classes, fail-closed gates,
and blocked target operations are recorded in
[`career-ops-capabilities.md`](career-ops-capabilities.md). A product requirement in this
document is not evidence that the installed career-ops revision exposes a callable
interface for it.

Request envelope:

```json
{"id":"uuid","protocolVersion":1,"operation":"history.snapshot","input":{"limit":2000}}
```

Response envelope:

```json
{"id":"uuid","ok":true,"result":{}}
```

The implemented read-only operations are:

- `capabilities.get`: protocol version, operation allowlist, source ownership,
  and explicitly forbidden operations.
- `health.check`: verifies the configured career-ops root, tracker, canonical
  application history, writable adapter staging path, and either the pinned PDF
  browser or a valid hash-bound user-reviewed PDF fallback.
- `history.snapshot`: asks career-ops' tracker to return structured canonical
  application records from its rebuildable SQLite index.
- `evaluation.result.read.v1`: reads one known report/tracker result through the
  conditional, fingerprint-bound contract in [`evaluation-result.md`](evaluation-result.md).
- `artifacts.inspect.v1`: proves the current canonical report and classifies exact
  HFW-committed CV/PDF reuse or the smallest safe refresh through
  [`preparation-artifacts.md`](preparation-artifacts.md). Generic career-ops output
  files are not treated as reusable provenance.
- `profile.queue_filters.get`: derives editable queue-filter defaults from the
  verified career-ops profile without exposing the profile contents to the renderer.

There is deliberately no arbitrary command operation and no operation for
submitting an application or sending a message.

The version-1 discovery dataset may include `postedAt` as a source-backed ISO date or
zoned timestamp, or omit it when the producer has no publication evidence. HereForWork
stores the nullable value per source occurrence and clears it when that occurrence is
re-imported without a date. Merged roles expose it only when non-null occurrences agree;
conflicts stay absent rather than selecting an earliest or latest value. Discovery time
is never used as publication time.

The legacy dataset and current importer remain unchanged. The separately named
`hereforwork.discovery-run` contract in [`discovery-run.md`](discovery-run.md) adds run,
source, evidence, integrity, completion, and native career-ops score provenance for the
gradual Refresh/shadow migration. Defining that file contract does not add an adapter
operation, exporter, scheduler authority, or automatic ingestion path.

## Approved target capability direction

The terminal product flow is Discover → Evaluate → Queue → Prepare → Fill/Review →
canonical Applied. The current operation list below is an implemented personal-proof
boundary, not the final sequencing contract.

HereForWork will orchestrate career-ops' existing discovery behaviors: `scan` finds roles
through configured portals/APIs and broad agentic search, while `discover` expands a
company into ATS portal sources. For every live, unique, nonblocked role, career-ops must
then perform its full A–G evaluation before Queue and return the native 1–5 score, report,
evidence, blockers or gaps, compensation context, and material uncertainty. Every valid
evaluation writes its full report. career-ops may also generate CV/PDF artifacts under
its supported `auto_pdf_score_threshold`. HereForWork may read and validate that setting
but never silently rewrites it. The approved product target is `3.5`, while the latest
audit found the effective upstream fallback still at `3.0` because no explicit threshold
key was configured.

At volume, orchestration reuses career-ops' supported batch/pipeline parallelism and model
routing: fast/economy processing first, escalation for low-confidence results, and an
audit sample. The adapter must preserve the authoritative evaluation and provenance; this
strategy never permits HereForWork to generate an interim, fallback, or audit score.

The audit found public batch `--parallel`, `spend_tier`, and explicit `--model` behavior,
but no public low-confidence escalation or audit-sample capability. Those two target
behaviors remain blocked rather than being inferred from the cheaper pre-screen gate.

Prepare now validates and reuses the current canonical report plus an exact
HFW-committed CV bundle, and generates or refreshes only the smallest artifact set that
can be proved necessary. A below-threshold role explicitly chosen by the user may receive
a missing CV/PDF. The conditional wire contract and its current limits are documented in
[`preparation-artifacts.md`](preparation-artifacts.md).

The adapter must invoke fixed upstream career-ops behavior without patching, forking,
vendoring, or adding HereForWork-specific entry points to career-ops. career-ops owns
source-local discovery normalization and deduplication. HereForWork owns typed
orchestration, schedules, retries, ingestion replay idempotency, cross-run/cross-source
identity reconciliation, and run visibility, but never a second discovery,
deduplication, evaluation, scoring, report, or CV engine.

## Writable personal-proof operations

Writable operations use only fixed, existing career-ops CLIs. HereForWork does not add
an entry point to career-ops and does not edit `applications.md` directly.

The extension keeps provider execution separate from canonical writes:

1. `preparation.context.get` validates any public HTTPS role/application URL,
   attempts bounded source-to-form resolution, and returns the resolved URL with a
   bounded context, source hashes, canonical evaluation identity, and an
   `artifacts.inspect.v1` plan. It returns a CV-only provider prompt only for a full CV
   refresh; exact reuse and PDF-only repair do not invoke a provider.
   Known ATS providers are optimizations; fetch, login, or parsing failures preserve
   a conservative generic context instead of rejecting the role.
2. HereForWork invokes the selected Codex or Claude subscription CLI in an
   ephemeral, tool-free working directory.
3. Discovery excludes typed `suspicious` findings before Queue. HereForWork then evaluates
   the typed score, legitimacy, and authorization result before artifact commit. A viable
   match continues when authorization is unknown, bounded research is inconclusive, or
   legitimacy is `Proceed with Caution`; the concrete warning remains in the report and
   live legal fields remain user-owned. A confirmed authorization conflict or newly
   detected `Suspicious` result is discarded, while a below-threshold verified match
   returns to Needs decision.
4. `preparation.result.commit` is the adapter transaction boundary. It revalidates the
   exact plan and either returns an existing HFW bundle, repairs only its PDF, or stages
   a full CV refresh. It validates HTML, facts, PDF structure, paths, and hashes before
   exclusive version publication and a compare-and-swap PDF-index update. The canonical
   report and tracker row are immutable inputs. It does not update application status.
5. `answers.context.get` binds a prepared application to the exact hash of a
   live, typed form snapshot. Job and form text remains marked as untrusted data.
6. HereForWork invokes the selected provider against that bounded context.
7. `answers.result.validate` returns only classified field instructions with
   provenance. Direct facts remain `safe_verified`; source-grounded narrative
   answers are separately marked `grounded_draft`, carry their form-language and
   detected length policy, and remain pending human review. Canonical compensation
   preferences are separately marked `canonical_preference` and come only from the
   strictly validated career-ops `compensation.application_answer` YAML structure.
   The adapter does not scrape prose or infer conversions. It rejects stale
   snapshots and never writes to a browser.
8. After classified fills are read back, `answers.result.commit` accepts only the
   bounded review items and verification results for that context hash. It
   invokes career-ops' fixed `application-answers.mjs` writer and returns the
   updated report hash.
9. HereForWork queues `release_for_review` only after that canonical answer
   write succeeds. A failure leaves the browser session recoverable and retries
   only the answer writer, not inspection or filling.

The implemented extension-only sequence follows the primary browser-driver contract in
[`browser-driver.md`](browser-driver.md). Success requires exactly one result for every planned
field and correct settled read-back for every required fillable. Hard pre-fill transport or
handshake failure, wrong or missing tab, zero compatible fields, and unsupported multi-page,
modal, iframe, or custom-widget behavior can release the extension lease for a future fallback.
Missing or invalid result sets fail closed. A read-back mismatch after mutation is instead a
human handoff because another driver cannot safely infer what the page retained.

Only one driver may own a preparation. The extension must release its lease before any
review-only fallback driver takes over. Automatic fallback is limited to a hard pre-fill
failure without authentication, CAPTCHA, anti-bot, partial-fill, or uncertain state;
those conditions require a visible human handoff. The approved target fallback returns a
grounded ordered answer plan and, if autofill cannot proceed, copy/paste recovery. No
fallback may add a submit or finalization operation.

The inspected career-ops `modes/apply.md` is an agent instruction, and
`prepare-application.mjs` prints a manual prefill summary. The newer career-ops web tree
contains an internal Playwright implementation, but it is not a versioned public process
contract, does not expose a transferable HereForWork lease, and opens its own browser
context. Therefore the fallback above is an approved requirement, not currently callable
adapter behavior. HereForWork may adapt around a future review-only fallback only through
an HFW-owned strict contract or a supported upstream-neutral boundary; it must not import
internal modules, call private local web routes, or add product-specific entry points to
career-ops to obtain it.

`preparation.artifacts.delete` is a narrowly scoped cleanup operation used by Undo
preparation. It accepts only the committed preparation UUID plus the exact report and
CV PDF paths recorded in the matching staging manifest. It deletes that report and the
manifest's generated job/CV files; path validation prevents access outside the fixed
HereForWork preparation layouts.

Safe-fill values must either map to a known verified profile fact or appear exactly in
the career-ops source named by the provider's provenance. This covers direct CV/profile
facts such as phone, location, LinkedIn, GitHub, portfolio, education, and employment
details without turning model output into an unverified fact source.

The CV upload is not provider-authored. HereForWork may create an internal upload
descriptor only for one inspected, unambiguous CV/resume file control and the exact PDF
path and SHA-256 recorded by the committed career-ops preparation manifest. The bridge
rechecks the root boundary, size, and hash and materializes bytes only in the transient
message to the approved extension; HereForWork does not store those PDF bytes. The
extension preserves any file already selected by the user and skips ambiguous controls,
non-PDF controls, unsupported attachment types, or unverifiable inputs.
The public upload filename is `Leonardo_Gomez_Frontend_Engineer.pdf` for both
tailored and reviewed-fallback artifacts; internal provenance continues to distinguish
`tailored_generated` from `user_reviewed_fallback`.

### Compensating preparation transaction

Preparation publication is durable and idempotent across process restarts. The adapter
stores a private identity-bound journal, revalidates source/context hashes immediately
before publication, rejects an effect UUID replay whose URL or report link differs, and
rolls back only files that still match the hashes it wrote. If the report, candidate
bundle, PDF index, or tracker changes concurrently—or a completed tracker merge cannot
be proven exactly—the state becomes `manual_repair_required` rather than overwriting or
claiming success.

This is compensating atomicity, not a global filesystem/database transaction. Writers
outside HereForWork do not participate in its lock or journal. Compare-and-swap checks
detect observed drift and refuse unsafe rollback, but cannot make unrelated external
writers transactional.

The optional local `CvFallbackSetting` stores an absolute PDF path and the SHA-256
computed when the user saves it in System. The path is not compiled into the app or
returned in dashboard data. It is eligible only after request, context, HTML, and fact
checks passed and PDF generation/rendering then failed. The adapter re-resolves the real
path and verifies PDF structure and the exact saved hash. Missing, changed, or invalid
fallback files fail explicitly. A successful recovery preserves the render diagnosis as
a warning, records `cvSource=user_reviewed_fallback`, and states that the CV is reviewed
by the user and was not tailored for that role.

Canonical decision operations are:

- `role.discard`: record Discarded immediately and return an opaque undo token.
- `role.discard.undo`: restore the exact prior state only when the canonical row
  still matches the effect represented by the undo token. A changed row returns
  a conflict instead of overwriting newer history.
- `application.applied.confirm`: after the user's separate outcome confirmation,
  record Applied through the canonical writer. Retrying this operation cannot
  reopen a browser session or repeat any page action.

Canonical Applied is terminal for HereForWork. Follow-up, outreach, reply monitoring,
interviews, and post-application CRM remain outside this protocol. Interview preparation
may be report content, but it does not create post-Applied workflow state.

Canonical decisions carry a UUID idempotency key. HereForWork retains that key across
retries, and the adapter verifies the canonical row plus the effect marker before
replaying or advancing a write. Reusing a decision after canonical state has diverged
returns a conflict instead of overwriting newer history.

Paths returned for artifacts are references inside career-ops-owned output roots; the
renderer never receives an arbitrary filesystem API.

## Permanent safety exclusions

These names are reserved and must always be rejected, including future protocol
versions:

- `application.submit`
- `application.finalize`
- `message.send`
- `shell.run`
- `browser.command`

Provider output cannot directly update canonical history. Job descriptions,
company text, recruiter messages, and form labels or values are never allowed to
select an operation, executable, script, path, argument, or tool permission.
