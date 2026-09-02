# Selective preparation artifacts

Status: implemented as a conditional, fail-closed HereForWork adapter contract

`artifacts.inspect.v1` decides which career-ops-owned preparation artifacts may be
reused. It is a read-only operation. The decision is valid only for the exact
career-ops Git revision and compatibility fingerprints supplied by the current
capability manifest.

## Canonical evaluation report

The report is reusable only when `evaluation.result.read.v1` proves all of the
following at inspection time:

- the tracker contains exactly one row for the supplied tracker id;
- the tracker report link resolves to a bounded regular file inside career-ops;
- the link's report number and path are parsed from the canonical tracker value;
- the supplied report path and SHA-256 match the file;
- company, role, native 1–5 score, and the structured Machine Summary agree; and
- both the evaluation and artifact compatibility fingerprints still match the
  same exact upstream revision.

Prepare reuses this report in place. It never duplicates, renumbers, or overwrites
the report and does not create another tracker row.

## CV and PDF

Existing generic career-ops CV/PDF files, `data/pdf-index.tsv` rows, file names,
and modification times do not prove freshness or provenance. They are not reused.

A CV bundle is reusable only from a committed HereForWork transaction manifest
whose preparation context, canonical report identity, exact upstream revision,
both compatibility fingerprints, bounded regular-file paths, SHA-256 hashes, and
explicit CV provenance still match. The allowed outcomes are:

- `none`: reuse the exact HTML, PDF, and changes bundle without invoking a provider
  or writer;
- `pdf_only`: reuse exact HTML and changes, rerun bounded fact checks, and render a
  new versioned PDF when only the PDF is missing or stale; or
- `full_cv`: request a new provider CV payload and changes, then build, fact-check,
  and render a new versioned bundle.

An explicitly selected below-threshold role may take the `full_cv` path. The
career-ops `auto_pdf_score_threshold` is read and reported by capabilities but is
never changed by HereForWork.

## Failure and recovery

Context, report, upstream revision, compatibility fingerprint, path, symlink, or
hash drift invalidates reuse. Ambiguous state refreshes the smallest safely known
unit; when that unit cannot be proved, the contract refreshes the complete CV
bundle.

Publishing uses a private restart journal, an exclusive version directory, and a
compare-and-swap update of `data/pdf-index.tsv`. An exact committed replay returns
the same identities without duplicate work. A failed publication rolls back only
unchanged transaction-owned files and preserves evidence when safe rollback cannot
be proved.

The configured user-reviewed PDF fallback is copied only after PDF rendering
fails. Its source must remain a regular PDF with the saved SHA-256 before and after
copying; HereForWork never overwrites or deletes the source. Its provenance remains
`user_reviewed_fallback`, `tailored: false`.

No report, CV, profile content, or form answer is persisted in HereForWork. Only
bounded paths, hashes, compatibility identities, provenance labels, and sanitized
failure data are retained. This contract has no browser or submission operation.
