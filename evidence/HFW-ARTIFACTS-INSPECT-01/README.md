# HFW-ARTIFACTS-INSPECT-01 — artifact probe fingerprint live proof

Date: 2026-09-04

## Cause

`artifacts.inspect.v1` stayed `unavailable` because the HFW adapter's
`artifactInspectionCompatibilityProbe` markers no longer matched the installed
career-ops revision (`3988ef93cfc2c89790e1cd9379ba242d7048ff5e`):

- `build-cv-html.mjs` no longer contains the string `cv-payload` (CLI is
  `<input.json> <output.html>`; HFW still stages `cv-payload.json` as the input file).
- `CAREER_OPS_PDF_INDEX` moved from `generate-pdf.mjs` into `tracker-utils.mjs`
  behind `resolvePdfIndexPath`.

career-ops source was not modified (D-052).

## Live proof (adapter CLI against installed career-ops)

`capabilities.get`:

| field | value |
|---|---|
| `artifacts.inspect.v1.status` | `degraded` |
| `artifacts.inspect.v1.compatibilityFingerprint` | `8450919d1ca90261afe99461cdbb73caa38ad91d99832f05e269af7695695ba1` |
| diagnostic | `safe_shape_probe_required` |

`artifacts.inspect.v1` for tracker `102` (canonical report
`reports/102-coches-net-front-end-engineer-2026-08-29.md`):

| field | value |
|---|---|
| ok | true |
| report.action | `reuse` / `canonical_evaluation_current` |
| cv.action | `refresh` / `no_hfw_bundle` / `full_cv` |

That clears the Prepare gate failure class `artifact_inspection_unavailable`. The
existing failed `preparation_jobs` row for role
`8acfb1ba-79a1-4e22-beb0-43c9c5e813e4` was left unchanged (store cleanup is T4).

## Validation

- `pnpm test:adapter` — 65 pass / 0 fail
- `git -C /Users/leo/Work/career-ops status` — no modifications from this task
  (pre-existing unrelated dirty files only)
