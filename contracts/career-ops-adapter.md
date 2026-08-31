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
  application history, pinned PDF browser, and writable adapter staging path.
- `history.snapshot`: asks career-ops' tracker to return structured canonical
  application records from its rebuildable SQLite index.
- `profile.queue_filters.get`: derives editable queue-filter defaults from the
  verified career-ops profile without exposing the profile contents to the renderer.

There is deliberately no arbitrary command operation and no operation for
submitting an application or sending a message.

## Writable personal-proof operations

Writable operations require a matching, explicitly approved entry point in the
career-ops repository. They are not emulated by editing its Markdown files from
HereForWork.

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
4. `preparation.result.commit` rejects a stale context or invalid result, runs
   career-ops fact and artifact checks, and atomically publishes the report and
   tailored CV references. It does not update application status.
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
