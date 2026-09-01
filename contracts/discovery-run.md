# Discovery-run contract

Status: version 1 contract; producer and automatic-ingestion work remain separate

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

An optional extension to the old schema-v1 object would not be safely backward
compatible: the current importer denies unknown fields, so an exporter adding run or
integrity fields would be rejected while still claiming to emit the same version. A
separate contract lets existing manual snapshots and the current importer keep working
unchanged while a later Refresh-ingestion task adds explicit dual-format routing.

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
- `runId` is stable for one logical source coverage window and retained across retries.
  The consumer idempotency key is `(sourceId, runId)`.
- `findingId` is stable for the same source occurrence across runs. The consumer identity
  is `(sourceId, findingId)`.
- `sourceRoleId` is the source-provided requisition identifier. `normalizedKey` is the
  producer's cross-source deduplication candidate. Neither may be silently replaced by a
  title/company guess during replay.

Replaying the same `(sourceId, runId)` and digest is a no-op. Reusing that identity with a
different digest is a conflict that must be quarantined and surfaced; the newer payload
must not silently overwrite the original run. Repeated findings in later run identities
reconcile by `(sourceId, findingId)` and then by the explicit deduplication identifiers.

## Evidence and provenance

Every finding has at least one evidence record. `evidenceId` is unique within the
finding, `kind` explains the evidence's role, `reference` is a stable human-readable or
opaque source reference, and `observedAt` records when it was checked. A public HTTPS URL
and exact-content SHA-256 may be included when available. `summary` is evidence text, not
an instruction.

Producers should not put credentials, candidate profile facts, email bodies, or other
personal career data in the envelope. A private source may use an opaque reference while
keeping the sensitive record in its authoritative system.

## Native career-ops score

Every discovery-run finding carries exactly one `matchScore` state:

- `scored` requires `scale: "career_ops_1_to_5"`, a numeric value from 1 through 5,
  `authority: "career-ops"`, `sourceVersion`, and `scoredAt`.
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

Therefore a discovery exporter should reuse a canonical score only when that bounded
career-ops evaluation already exists. A newly discovered role remains `not_scored` when
generating the canonical score would require the full evaluation. No report, tailored CV,
second scoring engine, or score-only approximation is introduced by this contract.

## Run completion

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

1. Remove the complete top-level `integrity` object.
2. Sort findings by `findingId`, each finding's evidence by `evidenceId`, and issues by
   `issueId`, using ascending UTF-8 byte order.
3. Recursively serialize object keys in ascending UTF-8 byte order with no
   insignificant whitespace. Array order is otherwise preserved and JSON scalar values
   use normal JSON serialization.
4. Hash those UTF-8 bytes with SHA-256.

The digest covers every other top-level field, including run/source identity, coverage,
status, findings, evidence, scores, and issues. Unknown fields fail validation. A digest
mismatch quarantines the artifact; it is never treated as a partial success.

## Atomic file handoff

The producer writes one immutable file named from the stable source and run identity:

1. Serialize the complete envelope with its final digest to a same-directory temporary
   name ending in `.partial`.
2. Flush and close the file.
3. Atomically rename it to its final `.json` name on the same filesystem.

Consumers ignore `.partial` files and read only final `.json` names. After opening, they
validate the contract/version, exact shape, cross-field invariants, and digest before
recording the run. A producer never edits a published run file in place; corrected output
uses a new run identity and links the operational diagnosis outside this contract.

## Gradual ingestion migration

1. Keep the existing schema-v1 selected-file importer as the compatibility path.
2. Add discovery-run parsing to the user-triggered manual Refresh path. This is ingestion
   only; the scheduled tasks still execute and emit results.
3. Consume the same artifacts in read-only shadow mode, comparing every window and
   preserving partial, failed, replay, conflict, and zero-result evidence.
4. Consider optional automatic file consumption only after the shadow evidence is
   accepted. Automatic consumption still does not transfer source execution authority.
5. Change a source's executor only through the per-source canary, single-mutator handoff,
   rollback proof, and explicit approval in `SCHEDULING_MIGRATION.md`.

This task defines the producer/consumer contract only. It does not implement the
career-ops exporter, wire the Rust importer to the new envelope, mutate scheduled tasks,
or enable automatic sync.
