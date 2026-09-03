# Discovery-run producer

`scripts/emit-discovery-run.mjs` is the supported producer/sealer for the version-1
`hereforwork.discovery-run` handoff. It accepts one structured JSON draft, validates the
complete contract, computes the existing `hfw-discovery-run-v1` SHA-256 digest, and
publishes one immutable JSON file. It does not parse Markdown or TSV and it does not run
discovery, evaluation, Gmail, career-ops tracking, scheduling, or application actions.

## Command

```sh
corepack pnpm produce:discovery-run -- \
  --input /path/to/source-generated-draft.json \
  --output-dir /path/to/here-for-work/inbox/discovery-runs
```

Input may be `-` (the default) for a JSON object on stdin. The default output directory
is `inbox/discovery-runs` relative to the command's working directory. The command prints
a small JSON result containing `status`, `path`, `replay`, and `digest`.

The draft contains every envelope field except the producer-owned `contract`,
`schemaVersion`, and `integrity` fields:

```json
{
  "windowId": "<stable-window-id>",
  "runId": "<unique-attempt-id>",
  "supersedesRunId": "<immediately-previous-attempt-id>",
  "source": {
    "sourceId": "eu-job-radar",
    "displayName": "EU Job Radar",
    "producer": "career-ops-exporter",
    "producerVersion": "<reproducible-revision>"
  },
  "coverage": {
    "windowStart": "2026-09-03T09:00:00+02:00",
    "windowEnd": "2026-09-03T13:00:00+02:00",
    "timezone": "Europe/Madrid"
  },
  "generatedAt": "2026-09-03T13:00:02+02:00",
  "status": "completed",
  "findings": [],
  "issues": []
}
```

`supersedesRunId` must be omitted for the first attempt, not set to `null`. Every
finding must retain the source-generated source role identity, normalized identity,
evidence, and exactly one career-ops score state. A scored finding must use the native
`career_ops_1_to_5` value and source-backed `sourceVersion`; an unavailable evaluation
must use the explicit `not_scored` state. The producer never chooses a score or fills
missing provenance. Partial runs require an issue and may include observed findings;
failed runs require an issue and contain no findings.

The producer bounds the input at 8 MiB, findings at 10,000, and issues at 1,000. The
contract's UTF-8 field limits and all cross-field/date/identity/evidence checks remain
authoritative. The final file is named
`discovery-run--SOURCE_ID--RUN_ID.json`. It is staged as a `.partial` file, flushed,
and published atomically without replacing a concurrently published file. Consumers
ignore `.partial` files.

## Identity rules

- `sourceId` is exactly `frontend-role-scan` or `eu-job-radar` and never changes when a
  display name changes.
- `windowId` identifies one logical coverage interval. Keep it unchanged for retries;
  changing the interval creates a new window.
- `runId` identifies one immutable attempt. Every retry gets a new `runId` and points to
  the immediately preceding attempt with `supersedesRunId`.
- `findingId` identifies the same source occurrence across attempts and later windows;
  derive it from the source's stable requisition/listing identity, never from a title or
  company guess. `sourceRoleId` and `normalizedKey` remain the source/career-ops values.
- A same `(sourceId, runId)` and digest is an idempotent replay. A same identity with a
  different digest is rejected and cannot overwrite the first file.

These rules deliberately mirror the consumer contract in
[`discovery-run.md`](discovery-run.md); they do not transfer scheduler or career-ops
authority to HereForWork.

## Source templates

The minimal structured templates are also checked into
[`../examples/discovery-run-draft.eu-job-radar.json`](../examples/discovery-run-draft.eu-job-radar.json)
and [`../examples/discovery-run-draft.frontend-role-scan.json`](../examples/discovery-run-draft.frontend-role-scan.json).
They are completed zero-finding examples for the two source identities. Before publishing
a real window, the source must replace the window/attempt metadata and populate findings
from the authoritative source-generated records.

### EU Job Radar prompt

Append this producer instruction to the existing authoritative EU Job Radar task only
after its executor has a verified way to write a local JSON file and run the command:

> Keep the existing EU Job Radar discovery, evaluation, Gmail, and career-ops behavior
> unchanged. At the end of each covered window, construct one structured JSON draft for
> HereForWork using sourceId `eu-job-radar` and displayName `EU Job Radar`. Reuse the
> same windowId for retries of the exact coverage window; create a new runId for every
> attempt and set supersedesRunId to the immediately previous attempt. Keep findingId
> stable for each source occurrence, and copy sourceRoleId, normalizedKey, evidence,
> legitimacy, queueGroup, and the complete canonical career-ops matchScore exactly from
> authoritative structured data. Do not infer identity, score, authorization, or
> provenance from title/company text. Include an issue for every partial or failed
> condition; failed runs have no findings. Do not write Markdown or TSV for this handoff.
> Pass the JSON draft unchanged to
> `corepack pnpm produce:discovery-run -- --input <draft-path> --output-dir
> <here-for-work>/inbox/discovery-runs`. Confirm the command reports `published` or an
> exact-digest `replayed` result. This emits an observation artifact only; do not pause
> this task or change its Gmail, tracker, schedule, or authority.

### Frontend Role Scan prompt

Append this producer instruction to the existing authoritative Frontend Role Scan task
only after its executor has a verified way to write a local JSON file and run the command:

> Keep the existing Frontend Role Scan discovery and evaluation behavior unchanged. At
> the end of each covered window, construct one structured JSON draft for HereForWork
> using sourceId `frontend-role-scan` and displayName `Frontend Role Scan`. Reuse the
> same windowId for retries of the exact coverage window; create a new runId for every
> attempt and set supersedesRunId to the immediately previous attempt. Keep findingId
> stable for each source occurrence, and copy sourceRoleId, normalizedKey, evidence,
> legitimacy, queueGroup, and the complete canonical career-ops matchScore exactly from
> authoritative structured data. Do not infer identity, score, authorization, or
> provenance from title/company text. Include an issue for every partial or failed
> condition; failed runs have no findings. Do not write Markdown or TSV for this handoff.
> Pass the JSON draft unchanged to
> `corepack pnpm produce:discovery-run -- --input <draft-path> --output-dir
> <here-for-work>/inbox/discovery-runs`. Confirm the command reports `published` or an
> exact-digest `replayed` result. This emits an observation artifact only; do not pause
> this task or change its schedule, authority, or any career-ops state.

## Migration boundary

The current HereForWork surface supports selecting a final `.json` file through manual
Refresh/import. It does not automatically watch `inbox/discovery-runs`, and this command
does not replace either scheduled task. The remaining external step is to update each
authoritative task with the applicable prompt above and a verified executor-side file/
command handoff. After that, shadow ingestion and the per-source gates in
[`SCHEDULING_MIGRATION.md`](../SCHEDULING_MIGRATION.md) are still required before any
authority cutover.
