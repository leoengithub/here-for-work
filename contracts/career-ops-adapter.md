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
  application history, and HereForWork-owned mirror-index path.
- `history.snapshot`: asks career-ops' tracker to parse canonical history into a
  HereForWork-owned derived index, then returns structured application records.

There is deliberately no arbitrary command operation and no operation for
submitting an application or sending a message.

## Writable personal-proof operations

Writable operations require a matching, explicitly approved entry point in the
career-ops repository. They are not emulated by editing its Markdown files from
HereForWork.

The extension keeps provider execution separate from canonical writes:

1. `preparation.context.get` validates the role and returns a bounded context,
   output schema, source hashes, and fixed career-ops instructions.
2. HereForWork invokes the selected Codex or Claude subscription CLI in an
   ephemeral, tool-free working directory.
3. `preparation.result.commit` rejects a stale context or invalid result, runs
   career-ops fact and artifact checks, and atomically publishes the report and
   tailored CV references. It does not update application status.
4. `answers.context.get` binds a prepared application to the exact hash of a
   live, typed form snapshot. Job and form text remains marked as untrusted data.
5. HereForWork invokes the selected provider against that bounded context.
6. `answers.result.validate` returns only classified field instructions with
   provenance. It rejects stale snapshots and never writes to a browser.
7. After safe fields are read back, `answers.result.commit` accepts only the
   bounded review items and verification results for that context hash. It
   invokes career-ops' fixed `application-answers.mjs` writer and returns the
   updated report hash.
8. HereForWork queues `release_for_review` only after that canonical answer
   write succeeds. A failure leaves the browser session recoverable and retries
   only the answer writer, not inspection or filling.

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
