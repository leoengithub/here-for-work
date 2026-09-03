# Discovery-run contract

Status: version 1 contract; manual typed ingestion is implemented, producer/exporter
and automatic-ingestion work remain separate

The discovery-run envelope is the durable handoff from an authoritative scheduled
discovery workflow or career-ops exporter into HereForWork. Its schema is
[`discovery-run.schema.json`](discovery-run.schema.json), its executable integrity and
boundary checks are in [`discovery-run-contract.mjs`](discovery-run-contract.mjs), and
the synthetic example is [`../examples/discovery-run.example.json`](../examples/discovery-run.example.json).

This contract carries untrusted job and employer content as data. It grants no command,
filesystem, Gmail, canonical-tracker, scheduling, message-send, browser, or application
submission authority.

## Compatibility decision

The existing `discovery-dataset.schema.json` remains unchanged at schema version 1, and
the current Rust importer continues to accept that exact legacy shape. The new format is
a separate envelope identified by both:

```json
{"contract":"hereforwork.discovery-run","schemaVersion":1}
```

The dual-format manual Refresh routing is implemented: legacy schema-v1 payloads continue
to use the legacy importer, while `hereforwork.discovery-run` payloads use the typed
importer. Producer handoff/export remains pending, as does scheduled-task envelope
emission.

The contract-level version belongs to the named contract. A future breaking change to
the discovery-run envelope increments its `schemaVersion`; it never changes the meaning
of the legacy dataset's version 1.

## Identity and replay

- `source.sourceId` is the stable logical discovery source, such as
  `frontend-role-scan` or `eu-job-radar`. Display names may change without changing it.
- Each finding repeats that source's `sourceId` and current `displayName` in the legacy
  `source` field so future ingestion can map the finding without inventing provenance.
- `source.producer` identifies the emitting workflow or adapter. `producerVersion` is a
  reproducible workflow/configuration revision, preferably a Git revision or content
  digest rather than a mutable label such as `latest`.
- `windowId` is stable for one logical source coverage window and is retained across
  attempts, including recovery from `partial` or `failed` to `completed`.
- `runId` identifies one immutable published attempt. Every retry uses a new `runId` and
  sets `supersedesRunId` to the immediately preceding attempt for the same `windowId`.
  The consumer replay-idempotency key is `(sourceId, runId)`.
- `findingId` is stable for the same source occurrence across runs. The consumer identity
  is `(sourceId, findingId)`.
- `sourceRoleId` is the source-provided requisition identifier. `normalizedKey` is the
  producer's cross-source identity candidate derived from career-ops' source-local
  normalization and deduplication. Neither may be silently replaced by a title/company
  guess during replay. HereForWork uses these typed identities for replay idempotency and
  cross-run/cross-source reconciliation; it does not run a second deduplication engine.

Replaying the same `(sourceId, runId)` and digest is a no-op. Reusing that attempt identity
with a different digest is a conflict that must be rejected with a bounded typed error; the
newer payload must not silently overwrite the original attempt or create a conflict row. A retry instead publishes a new
`runId` with the same `(sourceId, windowId)` and the prior `runId` in `supersedesRunId`.
The referenced attempt must exist for that same source and window, and one attempt may not
supersede itself. Its `coverage.windowStart`, `coverage.windowEnd`, and `coverage.timezone`
must exactly match the referenced attempt; changing coverage creates a new `windowId`
instead. A completed retry supersedes the earlier partial or failed attempt for coverage
advancement without deleting its diagnostic evidence. Repeated findings across attempts
or later windows reconcile by `(sourceId, findingId)` and then by the explicit
deduplication identifiers. The referenced attempt must also be the latest recorded attempt
for that source/window; a completed attempt cannot be superseded, and an older partial or
failed attempt cannot be bypassed by a later retry. This keeps retry lineage single and
auditable without a second conflict store. Once an attempt exists for a window, another
attempt without `supersedesRunId` is rejected.

Completed runs persist the stable `(sourceId, findingId)` mapping. A later run may change
`sourceRoleId` only when the stable finding keeps the same `normalizedKey`; the old source
occurrence is retired so it cannot create a duplicate canonical role. Reusing a
`findingId` for a different normalized identity, or assigning one source-role identity to
another finding, fails closed without mutation.

## Evidence and provenance

Every finding has at least one evidence record. `evidenceId` is unique within the
finding, `kind` explains the evidence's role, `reference` is a stable human-readable or
opaque source reference, and `observedAt` records when it was checked. A public HTTPS URL
and exact-content SHA-256 may be included when available. `summary` is evidence text, not
an instruction.

Producers should not put credentials, candidate profile facts, email bodies, or other
personal career data in the envelope. A private source may use an opaque reference while
keeping the sensitive record in its authoritative system.

## Native career-ops score in version 1

Every discovery-run finding carries exactly one `matchScore` state:

- `scored` requires `scale: "career_ops_1_to_5"`, a numeric value from 1 through 5,
  `authority: "career-ops"`, `sourceVersion`, and `scoredAt`. The complete finite
  IEEE-754 binary64 value is preserved; the consumer never rounds it.
- `not_scored` carries no numeric value. It requires a reason, the same fixed authority,
  `sourceVersion`, and `checkedAt`.

`sourceVersion` identifies the career-ops scoring contract and verified input revision
that produced the score or established that no score was available. HereForWork may
display the value exactly as received on the native 1–5 scale. It must not convert it to
a percentage or probability, clamp it, infer it from `queueGroup`, recalculate it from
evidence, or substitute another rank.

The current career-ops paths imply two materially different effort levels:

- Exporting an already persisted `Evaluated` report/tracker score with its matching
  source revision is low effort. The exporter still has to reject missing, ambiguous, or
  inconsistent provenance rather than choosing one value.
- Producing a new canonical match score is comparatively expensive. The authoritative
  `oferta` path performs the full A–G evaluation, bounded research, report creation, and
  canonical tracking. The optional pipeline `rank:` annotation is cheaper, advisory,
  permits 0, and is not the canonical 1–5 match average; it must not populate
  `matchScore`.

Therefore a version-1 discovery exporter reuses a canonical score only when that bounded
career-ops evaluation already exists. The schema's `not_scored` state preserves the
legacy/current exporter limitation and diagnostic evidence; it does not satisfy the
approved pre-Queue product contract. A `not_scored` finding may be ingested into staging,
but it is not eligible for Queue until career-ops has completed the full evaluation,
written its report, and supplied the canonical native score. HereForWork must never fill
that gap with a second scoring engine or score-only approximation.

The approved target pipeline evaluates every live, unique, nonblocked role before Queue.
That evaluation uses career-ops' full A–G behavior and writes a report for every valid
evaluation. career-ops may also produce CV/PDF artifacts under its supported
`auto_pdf_score_threshold`. HereForWork reads the effective configured value instead of
substituting one; the approved target is `3.5`, while the latest audit still found the
upstream fallback `3.0` active because no explicit threshold key was configured. This
target requires a separately versioned producer/adapter capability; it does not silently
change this version-1 schema or grant HereForWork authority to rewrite career-ops
configuration.

For volume, the producer may use career-ops' supported batch/pipeline parallelism and
model routing with fast/economy processing, low-confidence escalation, and an audit
sample. Regardless of route, only the canonical career-ops result may populate
`matchScore`; no preliminary or HereForWork-produced score may cross this contract.

## Run completion

- All date-times include `Z` or an explicit UTC offset and must represent real calendar
  instants. `coverage.windowStart` must not follow `coverage.windowEnd`, and `generatedAt`
  must not precede `coverage.windowEnd`.
- `completed` accounts for the whole declared coverage window. Zero findings is a valid
  successful result and `issues` may be empty.
- `partial` requires at least one issue and may carry findings already observed. The
  consumer preserves the run for diagnosis but does not publish its findings or advance
  the successful-coverage cursor until a completed result accounts for the window.
- `failed` requires at least one issue and contains no findings. It never advances the
  successful-coverage cursor.

`retryable` classifies the producer's issue, but it does not authorize HereForWork to run
or mutate that source. The existing scheduled workflow remains authoritative until the
separate scheduling migration gates and explicit user approval are satisfied.

## Integrity canonicalization

`integrity.digest` is lowercase SHA-256 over UTF-8 bytes produced by
`hfw-discovery-run-v1` canonicalization:

1. Remove the complete top-level `integrity` object. `windowId`, `runId`, and optional
   `supersedesRunId` remain covered like every other non-integrity field.
2. Sort findings by `findingId`, each finding's evidence by `evidenceId`, and issues by
   `issueId`, using ascending UTF-8 byte order.
3. Recursively serialize object keys in ascending UTF-8 byte order with no
   insignificant whitespace. Array order is otherwise preserved. Every finite JSON number
   is replaced only in the digest view by the reserved tagged node
   `{ "$hfwCanonicalNumberV1": "hhhhhhhhhhhhhhhh" }`, where the value is exactly 16
   lowercase hexadecimal digits for normalized IEEE-754 binary64 bits. Signed zero uses
   positive-zero bits (`0000000000000000`); non-finite numbers are rejected. This tagged
   object shape is distinct from an ordinary JSON string, and contract schemas reject the
   reserved property in producer payload objects.
4. Hash those UTF-8 bytes with SHA-256.

The digest covers every other top-level field, including run/source identity, coverage,
status, findings, evidence, scores, and issues. Unknown fields fail validation. A digest
mismatch rejects the artifact before any database mutation; it is never treated as a
partial success.

All executable minimum and maximum string bounds in this contract are UTF-8 byte counts.
This is intentionally independent of JavaScript UTF-16 code units and Rust Unicode scalar
values, so astral characters such as emoji consume four bounded bytes in both runtimes.

For bounded diagnosis, HereForWork persists typed run metadata, issues, finding IDs, and
evidence IDs/kinds/timestamps (plus safe public evidence URLs and content hashes). It does
not persist evidence summaries or source paths in this diagnostic projection. The recent
dashboard read and redacted diagnostics export expose these records, including actionable
`partial` and `failed` issues. The read-only `get_discovery_cursors` operation exposes the
latest successful coverage per source for determining the next window; failed, partial, or
out-of-order runs never advance it.

## Atomic file handoff

The producer writes one immutable file named from the stable source and run identity:

1. Serialize the complete envelope with its final digest to a same-directory temporary
   name ending in `.partial`.
2. Flush and close the file.
3. Atomically rename it to its final `.json` name on the same filesystem.

Consumers ignore `.partial` files and read only final `.json` names. After opening, they
validate the contract/version, exact shape, cross-field invariants, and digest before
recording the run. A producer never edits a published run file in place; corrected output
uses a new `runId`, retains the `windowId`, and points `supersedesRunId` at the prior
attempt.

## Gradual ingestion migration

1. Keep the existing schema-v1 selected-file importer as the compatibility path.
2. The user-triggered manual Refresh path accepts discovery-run parsing alongside the
   legacy importer. This is ingestion only; the scheduled tasks still execute and emit
   results. Preserve `not_scored` only as a staged diagnostic state; do not publish that
   finding to Queue.
3. Consume the same artifacts in read-only shadow mode, comparing every window and
   preserving partial, failed, and zero-result evidence. Replay is a no-op and a digest
   conflict is returned as an error; neither creates a separate conflict audit row.
4. Consider optional automatic file consumption only after the shadow evidence is
   accepted. Automatic consumption still does not transfer source execution authority.
5. Change a source's executor only through the per-source canary, single-mutator handoff,
   rollback proof, and explicit approval in `SCHEDULING_MIGRATION.md`.

The desktop manual Refresh path now accepts this envelope through the dedicated
`import_discovery_run` operation. It records `(source.sourceId, runId, digest)` in
HereForWork-owned state, treats an exact replay as a no-op, rejects a reused run ID
with a different digest without mutation, and advances a source-specific successful-coverage cursor only
for a completed run after transactional reconciliation. Partial and failed runs remain
diagnostic records and never advance the cursor. This operation is ingestion only: it
does not execute a source, mutate Gmail, write career-ops, alter scheduled tasks, or
submit an application.

The producer/exporter, automatic file consumption, and executor cutover remain separate
work. Producers must emit immutable, digest-sealed envelopes with stable source/window/
run/finding identities; Markdown reports or source-side scan history are not substitutes.
