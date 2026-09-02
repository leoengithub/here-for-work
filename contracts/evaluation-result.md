# career-ops evaluation result read contract

Status: version 1, conditional and read-only

This contract projects one already-written career-ops A–G report and its canonical
tracker row into typed data. It does not execute evaluation, discover roles, write the
tracker, generate artifacts, open a browser, or submit/send anything.

## Upstream format decision

The installed career-ops checkout was inspected at `3988ef93cfc2c89790e1cd9379ba242d7048ff5e`.
The already-present `origin/main` object was inspected at
`2e33ea866d7681a779c6729c7aef76dc6280d0bc`.

The installed checkout documents the richer Machine Summary shape with
`authorization_confidence`, `authorization_evidence`, `authorization_scope`,
`engagement_mechanism`, and `authorization_question`. The inspected `origin/main`
shape has the legacy `work_auth` and `requirement_importance` fields instead, and does
not provide the canonical authorization result required by this read model. HereForWork
therefore accepts only the richer shape; it never derives canonical authorization from
legacy `work_auth` or advisory pipeline `rank`.

The adapter computes a SHA-256 compatibility fingerprint over the exact contents of:

- `batch/batch-prompt.md`
- `tracker.mjs`
- `tracker-parse.mjs`
- `modes/oferta.md`
- `templates/states.yml`

The probe also requires the documented authorization markers and the tracker’s exact
JSON projection. A caller must pass the fingerprint returned by `capabilities.get` to
`evaluation.result.read.v1`; the adapter recomputes it before and after the read. The
read is usable only while the manifest has an exact 40-character upstream Git revision,
matching capability source revision, and `degraded` status for this conditional
capability. Any missing source, changed source, missing Git revision, unavailable
status, or changed fingerprint fails closed.

## Operation

Request input:

```json
{
  "reportPath": "reports/007-example.md",
  "reportSha256": "<sha256>",
  "trackerId": 7,
  "compatibilityFingerprint": "<sha256 from capabilities.get>"
}
```

The report path must be relative, resolve inside the career-ops root after symlink
resolution, and match the tracker’s documented Markdown report link. The report hash
must match the bytes read. The tracker row must be the unique row for the requested ID,
must have the closed nine-field JSON projection, and must carry a native score in the
form `N/5`.

The report must contain exactly one `## Machine Summary` followed immediately by one
`yaml` fence, exactly one heading for each `## A)` through `## G)` in order, and exactly
one `## Risk Summary`. Only the documented Machine Summary YAML is parsed. Terminal
prose, arbitrary fenced JSON, agent logs, and human-facing report prose are never parsed
for result values. Unknown YAML fields, duplicate keys, malformed YAML, missing fields,
invalid enums, unbounded text, and non-native/out-of-range scores are rejected.

The returned model includes the report identity, canonical tracker identity and score,
native evaluation score, final decision, concise strengths, blockers/gaps, advertised
compensation context, legitimacy/risk, canonical authorization evidence and question,
and explicit material uncertainty. The canonical score is the tracker score after exact
agreement with the report score; no score is computed or rescaled by HereForWork.
