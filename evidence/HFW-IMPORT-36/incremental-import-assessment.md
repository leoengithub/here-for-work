# HFW-IMPORT-36 incremental import assessment

Assessment date: 2026-09-03 (Europe/Madrid)

## Result

The minimum typed-ingestion repair is implemented in this worktree, but no
incremental import was executed against the installed personal database. The
available post-boundary outputs still cannot be wrapped deterministically into
valid envelopes without inventing source/window/finding identities.

The existing scheduled workflows remain authoritative. No Gmail, schedule,
career-ops profile/configuration, application, or message action was performed.

## Last observed boundary

The live HereForWork database was inspected read-only at the redacted app-data
location. Its operational schema is version 20. The strongest import marker is
the global setting `last_successful_discovery_at`:

```text
2026-09-01T11:53:14+02:00
```

The latest import activity was recorded at `2026-09-01T09:57:11.709808Z` and
reported `1 new, 0 updated, 42 unchanged`. Earlier activity recorded imports of
`32 new`, `7 new`, and `42 new` findings. This establishes the latest imported
snapshot boundary, but not a typed source cursor or immutable run identity.

The latest persisted source occurrences are:

| source | latest discovered_at | occurrences |
| --- | --- | ---: |
| `frontend-role-scan` | `2026-09-01T09:46:03+02:00` | 41 |
| `eu-job-radar` | `2026-09-01T09:02:00+02:00` | 40 |
| `application-status-email` | `2026-08-30T11:30:43+02:00` | 1 |

There is no `source_cursors` table. `source_schedules.last_successful_at`
remains the older staged baseline `2026-08-29T06:00:00Z` for both discovery
sources, so it cannot safely be promoted to the latest import boundary.

## Available post-boundary outputs

The following outputs exist after the boundary, but none is a typed,
digest-sealed `hereforwork.discovery-run` artifact:

* The Frontend Role Scan source output in career-ops reports one completed scan
  at `2026-09-01T21:32:21.694Z`: 5,662 found, 40 `new_added`, 10 duplicates,
  and 13 source-reported errors. Its 40 new rows are source-side scan history,
  not a HereForWork discovery-run envelope.
* EU Job Radar has six post-boundary Markdown report sections at
  `2026-09-01 13:02`, `2026-09-01 18:00`, `2026-09-02 09:02`,
  `2026-09-02 13:02`, `2026-09-02 18:00`, and `2026-09-03 09:01`
  Europe/Madrid. They describe 40 alert cards in total, with review,
  discarded, duplicate, and held-for-evidence outcomes. Markdown reports do
  not provide `windowId`, `runId`, `findingId`, digest, or a replay key.

The live database has 15 preserved `action_required` runs through the
2026-09-03 windows. Every discover step is `source_adapter_not_configured`;
these are migration-readiness records, not imported source results.

## Supported import and safety evidence

The legacy Queue selected-file path (`import_dataset`) remains unchanged for
schema-v1 `DiscoveryDataset` payloads. The new `import_discovery_run` path
accepts the public version-1 envelope, validates its cross-field invariants and
canonical SHA-256 digest, and transactionally records `(sourceId, runId,
digest)`. An exact replay is a no-op; reuse of a run ID with a different digest
fails closed. Completed runs reconcile through the existing source/source-role
and normalized identity path and advance a source-specific cursor only after
successful transaction commit. Partial/failed runs are recorded for diagnosis
without publishing findings or advancing a cursor. The legacy global setting is
not changed by typed ingestion.

The Queue file picker now discriminates only the top-level `contract` field:
`hereforwork.discovery-run` uses `import_discovery_run`; all other payloads keep
the legacy `import_dataset` route. The UI appearance and file-picker semantics
are unchanged.

Typed run status is included in the existing System recent-run list and in the
redacted diagnostics export. Issues remain bounded and actionable; evidence is
limited to stable IDs, kinds, timestamps, public URLs, and content hashes. The
read-only cursor API exposes each source's latest successful coverage. Completed
runs persist `(sourceId, findingId)` mappings, reconcile a stable finding across
source-role reissues, and reject normalized-identity conflicts. Older or equal
coverage runs are recorded as diagnostics without reconciling roles or rewinding
the cursor. Retry lineage accepts only the latest partial/failed attempt for the
same window and rejects bypassed or completed attempts.

The repository contract explicitly requires typed run identity, digest
validation, replay idempotency, and cursor advancement only after completed
coverage. No post-boundary artifact met those requirements. A raw TSV/Markdown
conversion would fabricate provenance and could re-import findings outside a
known window.

## Counts and integrity (read-only baseline)

| metric | count |
| --- | ---: |
| roles | 78 |
| source occurrences | 82 |
| visible Queue roles | 0 |
| pre-Queue roles | 63 |
| handled roles | 15 |
| active preparation rows | 5 |
| pending runs | 0 |
| action-required runs | 15 |

`PRAGMA integrity_check` returned `ok`.

No personal-database mutation was performed, therefore runtime after and replay
counts are intentionally `not applicable`; the baseline remains unchanged. The
new migration is schema v22 and is covered by temporary-store tests. Typed run
diagnostics and stable source/finding identities are readable through the
dashboard, redacted diagnostics export, and `get_discovery_cursors`. No backup,
source file, or generated runtime artifact was removed.

## Smallest safe repair

The producer/exporter must now emit those immutable artifacts. The smallest
scheduled-task change is an exporter at the end of each existing source run
that:

1. emits one `hereforwork.discovery-run` JSON object per source window, with a
   stable `sourceId` (`frontend-role-scan` or `eu-job-radar`), deterministic
   `windowId`, fresh attempt `runId`, stable `findingId`/`sourceRoleId`, exact
   coverage bounds, and an explicit `completed`, `partial`, or `failed` status;
2. carries the producer revision, typed evidence, and an existing canonical
   career-ops score or the explicit `not_scored` state; it must reject missing
   or ambiguous identities instead of deriving them from title/company text;
3. sets `supersedesRunId` only on a retry of the same window, computes the
   contract SHA-256 after all fields are final, and writes the final JSON by
   flushing a same-directory `.partial` file followed by an atomic rename;
4. leaves the existing scheduled executor, Gmail, and career-ops state alone.

Until those steps produce typed, digest-sealed files, the TSV/Markdown outputs
must not be fed to the importer. The existing scheduled tasks remain
authoritative and must not be replaced or edited by this ingestion path.

## Validation

* Rust store tests: 125 passed, including first import/replay, digest conflict,
  failed-run no-cursor, invalid-retry coverage rollback, and forced transaction
  rollback tests, stable finding identity changes/conflicts, out-of-order windows,
  retry-lineage constraints, diagnostics read/export, and input bounds.
* Contract tests: 15 passed, including strict timestamp, number-canonicalization,
  and UTF-8 byte-limit parity cases.
* Cross-runtime numeric parity covers the adversarial `3.6495739800251394`, boundary
  scores, exponent forms, signed zero, and 10,000 deterministic valid binary64 scores.
* `cargo fmt --check` passed after formatting.
* `cargo clippy --all-targets -- -D warnings` passed.
* `corepack pnpm install --offline --frozen-lockfile` could not open the shared
  pnpm store index. The configured store path was verified under
  `/Users/leo/Library/pnpm/store/v11`; no worktree-local store was used, and
  TypeScript/browser validation was not run.
