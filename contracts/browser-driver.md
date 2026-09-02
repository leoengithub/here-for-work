# Browser driver contract

Status: extension driver implemented; review fallback unavailable

The ordinary Chrome extension is the only enabled form driver. Each application browser
session owns one durable, opaque, role-scoped driver lease. Every command and result carries
the exact session and lease identity; stale, mismatched, expired, or replayed identities are
rejected. Reconnect may redeliver a command, but the extension caches a completed fill result
for that command so the same fill is not executed twice.

## Inspection

Inspection returns a fingerprinted snapshot and a typed flow disposition. Native controls
may be fillable. Custom widgets, relevant embedded frames, modal forms, hidden later steps,
and a form with no compatible controls are `fallback_eligible` before any fill. Authentication,
CAPTCHA, and anti-bot structure are `human_handoff`. Page text and markup are untrusted data:
they can contribute evidence to a disposition, but they cannot select a bridge command,
driver, fallback, or terminal action.

The application driver never treats an empty readiness timeout as a successful inspection.
The inspected control fingerprint must still match immediately before a fill starts.

## Fill result

The planned field set is the union of fill instructions and the optional verified CV upload.
The result must contain that exact set once: no omissions, additions, or duplicate field IDs.
Each result contains a bounded status and reason code, whether the page was mutated, and a
SHA-256 of the normalized settled read-back (never the clear-text value). Verified results
must match the hash of the planned value or the manifest-bound PDF.

The extension dispatches `input` and `change`, waits through a bounded settling window, then
re-resolves the inspected control. Replacement, detachment, ambiguity, new invisibility, an
asynchronous value rewrite, or a missing select option produces a deterministic failed result.
Every required planned control must verify before the session may continue. Optional skipped
or failed controls remain visible in the review record.

## Lease release and handoff

`release_for_review` is accepted only after a verified fill (except the explicit connection
check, which never fills). It removes the extension's no-finalization guard and durably releases
the driver lease. `focus_review` only focuses the already released tab; it cannot inspect, fill,
release, or submit.

A hard inspection failure or a fill-plan rejection proven to occur before mutation may release
the extension lease and mark a future fallback eligible. A partial fill, read-back uncertainty,
authentication, CAPTCHA, anti-bot signal, or any potentially mutated state becomes a human
handoff and cannot be retried with another driver automatically. The guard is not silently
removed for these failure paths.

`browser.review_fallback.v1` remains `unavailable`. There is no fallback command, simulated
fallback, private career-ops import, or Playwright route. A future fallback must expose a public
review-only lease/result boundary and acquire only a durably released extension lease.

## Terminal safety

The bridge allowlist contains inspection, fill, release-for-review, and focus-for-review only.
It contains no submit, send, apply, finalize, arbitrary browser command, or arbitrary shell
operation. HereForWork never clicks the site's Submit control.
