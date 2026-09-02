# career-ops capability boundary

Status: version 1 design; discovery/evaluation/browser operations are not implemented
Audit date: 2026-09-02

This document records which existing career-ops interfaces HereForWork may safely wrap,
which interfaces require a guarded compatibility adapter, and which approved product
capabilities are currently blocked. It does not add an entry point to career-ops, change
career-ops configuration, or make a presentation-oriented command into an API.

## Audit baseline

The installed checkout was inspected read-only at commit
`3988ef93cfc2c89790e1cd9379ba242d7048ff5e`. Its local `main` was 11 commits ahead of and
1,556 commits behind its already-present `origin/main` ref, and contained unrelated
user-layer changes. The checkout was not changed, cleaned, fetched, installed, or run in
a write mode.

The existing `origin/main` ref was inspected without fetching or checking it out:

- commit: `2e33ea866d7681a779c6729c7aef76dc6280d0bc`
- commit date: `2026-09-02T12:25:40+02:00`
- declared core version: `1.31.0`

The installed checkout also declares `1.31.0`. The version string therefore cannot prove
interface compatibility. HereForWork must record the exact Git revision when available,
probe every required capability, and reject unexpected shapes before enabling a run.

No configured `auto_pdf_score_threshold` key was present during the audit. Upstream's
documented and implemented fallback is therefore `3.0`, not the product's approved
initial `3.5`. HereForWork may report that mismatch and block a threshold-dependent run;
it must not rewrite the setting.

## Evidence inspected

The audit inspected these public or user-facing surfaces:

- `package.json`, `VERSION`, `README.md`, `docs/SCRIPTS.md`, and
  `docs/APPLY_AUTOFILL.md`
- `scan.mjs`, `scan-ats-full.mjs`, `discover-ats.mjs`, `check-liveness.mjs`, and
  `liveness-core.mjs`
- `modes/scan.md`, `modes/discover.md`, `modes/pipeline.md`, `modes/batch.md`,
  `modes/oferta.md`, `modes/auto-pipeline.md`, and `modes/apply.md`
- `batch/batch-runner.sh`, `batch/batch-prompt.md`, `batch/README.md`,
  `openrouter-runner.mjs`, and `prepare-application.mjs`
- `tracker.mjs`, `set-status.mjs`, `application-answers.mjs`,
  `generate-pdf.mjs`, and the report `## Machine Summary` convention

The following non-mutating help commands were executed against the installed checkout:

```text
node scan.mjs --help
node discover-ats.mjs --help
node check-liveness.mjs --help
node openrouter-runner.mjs --help
node prepare-application.mjs --help
node generate-pdf.mjs --help
node application-answers.mjs --help
node set-status.mjs --help
node tracker.mjs --help
node batch-tailor.mjs --help
```

The audit also inspected, from the existing `origin/main` Git objects, the internal web
implementation under `web/src/lib/core/`, `web/src/lib/apply/`, and
`web/src/app/api/apply/`, plus `web/src/app/api/run/route.ts`. These files demonstrate
working implementation ideas; they are not a published external protocol.

## Compatibility classes

| Class | Meaning | HereForWork behavior |
|---|---|---|
| Contracted | Documented machine-readable input/output with a safe invocation mode | Enable only after exact shape and safety probes pass |
| Conditional | Machine-readable behavior exists, but has no independent protocol version or stable package export | Pin the upstream revision and validate the complete result; otherwise unavailable |
| Presentation only | Human prose, Markdown agent mode, or aggregate exit code | Never parse as an interface |
| Internal only | Source export or local web route without a published compatibility promise | Do not import or call across repository boundaries |
| Missing | Required semantics do not exist as a callable interface | Fail closed and surface the blocked capability |

## Verified capability matrix

| Product capability | Verified career-ops surface | Class | Compatibility decision |
|---|---|---|---|
| Configured portal/API scan | `scan.mjs`; writes `data/pipeline.md`, `data/scan-history.tsv`, and run counters; stdout is human prose | Presentation only | Do not parse stdout. A future adapter may consume a separate versioned export, not infer a run from prose or incidental file diffs. |
| Broad reverse-ATS scan | `scan-ats-full.mjs --dry-run --json` emits one JSON object with coverage/degradation fields and offers, but still creates or updates `data/cache/ats-companies/*.json` | Conditional, locally side-effecting | Do not run it in the career-ops checkout. It may back discovery only for an exact probed revision and strict result validator after HFW provides an isolated execution/copy/cache boundary, or upstream provides a verified zero-checkout-write mode. A capped, stale, stopped, or malformed run is not a completed empty run. |
| Agentic broad Web search | `modes/scan.md` instructions and the career-ops web Explore implementation | Internal/presentation only | No callable public contract exists. Keep this source blocked rather than claiming that `scan.mjs` provides it. |
| Company to ATS expansion | `discover-ats.mjs` preview emits documented JSON (`metadata`, `resolved`, `unresolved`, `pendingEntries`); `--write` is explicit | Contracted for preview | Wrap preview only. Any configuration write remains a separate visible user-approved action; HereForWork never adds `--write` silently. |
| Per-role liveness | `check-liveness.mjs` emits prose and an aggregate exit code; `liveness-core.mjs` exports internal objects | Presentation/internal only | No typed per-role public result exists. Pre-Queue publication remains blocked until the adapter has a versioned active/expired/uncertain result with evidence. |
| Full A-G evaluation | `modes/oferta.md` and `modes/pipeline.md` agent instructions; web `/api/run` orchestrates them internally | Internal/presentation only | No public versioned evaluate-one or evaluate-batch process contract exists. Do not parse agent prose or call the private web route. |
| Batch parallelism | `batch/batch-runner.sh --parallel N` with fixed in-repository input/state/log paths and canonical side effects | Conditional, side-effecting | Proven behavior exists, but it is not isolatable or idempotent enough for direct HFW orchestration. Require a supported invocation contract or an upstream-neutral wrapper boundary before use. |
| Model tier/routing | `config/profile.yml` `spend_tier`; batch `--model`; economy pre-screen for standard/premium | Conditional | HFW may read and report the supported setting. It must not select an uncontracted model or rewrite the preference. |
| Low-confidence escalation and audit sample | No matching behavior in the inspected batch/pipeline implementation | Missing | Approved target remains blocked; do not describe ordinary pre-screening as escalation or sampling. |
| Complete evaluated report and native score | Report `## Machine Summary` YAML plus tracker score; `batch/batch-prompt.md` defines the fields | Conditional | Accept only a strict complete-report parser tied to a known revision, with numeric score 1–5 and report/tracker identity agreement. The advisory pipeline `rank:` value is never canonical. |
| Evaluation result receipt | Batch worker's final fenced JSON, `batch-state.tsv`, report, tracker addition, and canonical tracker merge | Conditional | No single versioned atomic receipt exists. HFW must not mark evaluation complete from exit code or fenced JSON alone. |
| Score-gated CV/PDF | `auto_pdf_score_threshold`; report PDF header; `generate-pdf.mjs --batch` results file | Conditional | Read and validate the configured/effective threshold. Never substitute the approved value or infer freshness from file existence. |
| Artifact identity/provenance/freshness | Report path, PDF index rows, generated paths, and some hashes in HereForWork's current preparation journal | Missing for the approved boundary | Upstream exposes no structured source hashes, classified CV changes, stale reason, or review/block status. Artifact-aware Prepare must remain blocked or conservatively regenerate after explicit user intent. |
| Grounded application-answer read/write | `application-answers.mjs --read --strict` emits JSON; its writer is fixed and report-scoped | Contracted for the existing narrow use | Keep the current adapter validation and context-hash guards. This does not itself drive a browser. |
| Manual prefill summary | `prepare-application.mjs` validates three ATS families and prints suggested values | Presentation only | It does not open, inspect, or fill a browser and cannot satisfy the fallback contract. |
| Playwright Apply driver | career-ops web internals maintain Playwright sessions and expose local Next.js routes; `modes/apply.md` is an agent workflow | Internal only | The implementation is not a supported external capability, has no lease token compatible with HereForWork, and opens its own browser context. Do not import or call it as the approved fallback. |
| Canonical Applied write | `set-status.mjs ... Applied --json`, wrapped by current `application.applied.confirm` | Contracted through the HFW adapter | Retain HFW idempotency/effect verification. A retry touches tracking only and never starts a form driver. |

## Minimum HereForWork-owned contract

The existing NDJSON adapter remains the only HereForWork-owned process boundary. Its
capability response must become a strict, versioned manifest before new orchestration is
enabled. The minimum manifest needs:

```text
contract: hereforwork.career-ops-capabilities
schemaVersion: 1
adapterProtocolVersion
upstreamRevision: exact Git SHA or explicit unavailable
upstreamDeclaredVersion
capabilities[]:
  id
  status: supported | degraded | unavailable
  interfaceClass: contracted | conditional
  sourceRevision
  probeRevision
  constraints[]
```

Unknown fields, unknown capability IDs, a missing revision, a failed probe, or a changed
result shape disable only the affected capability and produce an actionable diagnostic.
They never downgrade to prose parsing. A declared version match cannot replace a probe.

The smallest semantic capability set required by the approved workflow is:

1. `discovery.reverse_ats.run.v1` — typed scan with coverage and degradation, executed
   outside the career-ops checkout through an isolated HFW-owned copy/cache boundary or
   a verified zero-checkout-write upstream mode.
2. `discovery.company_ats.preview.v1` — typed preview; no implicit configuration write.
3. `liveness.role.read.v1` — active/expired/uncertain with source evidence.
4. `evaluation.full_ag.run.v1` — fixed full evaluation producing an idempotent result
   receipt, complete report, canonical score, and optional threshold-gated artifacts.
5. `evaluation.result.read.v1` — strict report/tracker reconciliation and evidence
   projection.
6. `artifacts.inspect.v1` — source/output identity, validation outcome, provenance,
   freshness, and a reason for missing/failed/stale.
7. `browser.review_fallback.v1` — explicit single-driver lease transfer, typed planned
   and actual field results, settled read-back, human-handoff reasons, and no submit
   action.
8. `canonical.applied.write.v1` — existing idempotent tracking-only transition.

Capabilities 3, 4, 6, and 7 are unavailable against both audited revisions. Capability 1
is conditional; capability 2 and the narrow read/write pieces behind capability 8 have
usable machine surfaces. The pre-Queue pipeline and Playwright fallback must not be
enabled until their required capabilities are supported.

## Fail-closed invariants

- No adapter behavior parses human CLI prose, ANSI output, Markdown recommendations, or
  arbitrary JSON found in agent logs.
- A full evaluation succeeds only when the report exists, all required A-G sections and
  strict Machine Summary exist, the native score is 1–5, and the canonical tracker row
  agrees with the role URL/report identity.
- A report/PDF file timestamp is not freshness or provenance. Missing structured evidence
  yields `unavailable`, never `fresh`.
- `auto_pdf_score_threshold` is read and validated from career-ops. An absent key is
  reported with upstream's effective default. A mismatch with product configuration is
  visible and never repaired silently.
- A discovery run that is capped, stale, stopped by outage, malformed, partial, or missing
  coverage is not a completed empty run.
- A discovery command that writes caches, counters, history, or other local state never
  runs inside the career-ops checkout. HFW must prove an isolated execution/copy/cache
  boundary or a verified zero-checkout-write mode before enabling it.
- Browser fallback cannot start until the extension's lease is durably released. No
  internal web session, browser context, or process is assumed transferable.
- Neither a capability manifest nor any future operation may contain submit, finalize,
  message-send, arbitrary shell, or page-selected command authority.

## Owner decisions and upstream dependencies

1. Choose the supported career-ops baseline: the divergent installed checkout, the
   inspected `origin/main`, or a later explicitly updated checkout. The declared version
   cannot make this decision.
2. Decide whether to set `auto_pdf_score_threshold: 3.5` in career-ops. Until that user
   configuration change is made explicitly, HereForWork must show effective `3.0` and
   must not claim that the approved threshold is active.
3. Wait for and consume upstream-neutral public contracts for typed liveness, full A-G
   evaluation/result receipts, artifact freshness/provenance, and review-only Playwright
   driving. HereForWork-specific entry points in career-ops remain forbidden. Contributing
   such contracts upstream is work in a separate external project and requires explicit
   authorization; this plan does not authorize it.
4. Decide whether the reverse-ATS `--json` surface is sufficiently supported to pin after
   an isolated execution/copy/cache boundary exists, or whether discovery must wait for an
   explicitly documented schema version and verified zero-checkout-write mode.
