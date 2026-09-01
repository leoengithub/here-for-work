# career-ops adapter protocol

Version 1 is an NDJSON request/response protocol over a child process. HereForWork
starts the bundled adapter with a fixed executable and fixed script path; neither
operation names nor subprocess arguments come from job or form content.

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

## Writable personal-proof operations

Writable operations use only fixed, existing career-ops CLIs. HereForWork does not add
an entry point to career-ops and does not edit `applications.md` directly.

The extension keeps provider execution separate from canonical writes:

1. `preparation.context.get` validates any public HTTPS role/application URL,
   attempts bounded source-to-form resolution, and returns the resolved URL with a
   bounded context, output schema, source hashes, and fixed career-ops instructions.
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
4. `preparation.result.commit` is the adapter transaction boundary. It rejects a stale
   context or invalid result, stages the HTML and fact checks privately, renders the PDF
   against a staged `CAREER_OPS_PDF_INDEX`, and validates HTML, facts, PDF structure,
   index rows, paths, and hashes before publication. It publishes exclusively and uses
   compare-and-swap checks while updating the report, artifact bundle, and canonical PDF
   index. The canonical tracker merge is the commit point and is post-verified by the
   exact HereForWork effect UUID, source URL, and report link. It does not update
   application status.
5. `answers.context.get` binds a prepared application to the exact hash of a
   live, typed form snapshot. Job and form text remains marked as untrusted data.
6. HereForWork invokes the selected provider against that bounded context.
7. `answers.result.validate` returns only classified field instructions with
   provenance. It rejects stale snapshots and never writes to a browser.
8. After safe fields are read back, `answers.result.commit` accepts only the
   bounded review items and verification results for that context hash. It
   invokes career-ops' fixed `application-answers.mjs` writer and returns the
   updated report hash.
9. HereForWork queues `release_for_review` only after that canonical answer
   write succeeds. A failure leaves the browser session recoverable and retries
   only the answer writer, not inspection or filling.

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
