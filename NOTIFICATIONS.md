# Preparation outcome notifications

HereForWork treats preparation outcomes as durable local events. SQLite records the
outcome in the same transaction that changes the preparation or browser-session state.
The Applications screen remains the source of truth even when notification permission
is denied or the operating system cannot display a notification.

## Outcome contract

- `preparation_failed` is created only for a real report/CV, answer-persistence, or
  browser-preparation failure. Needs-decision and cancelled preparations do not create
  failure outcomes.
- `application_ready` is created only after the active browser driver has completed its
  verified release for human review. For the extension, that is
  `release_for_review`; the separately versioned career-ops `apply` fallback must expose
  an equivalent bounded outcome before it can create the same event. Completing the
  report, CV, or answer plan alone is not sufficient.
- Failure records retain a bounded, sanitized error code, exact stage, detail, and safe
  retry policy. URLs, local paths, email addresses, and long context identifiers are
  removed before persistence or display.
- Dedupe keys bind a failure to its preparation attempt and a ready outcome to its
  browser session. Each event can be claimed by only one delivery surface.

## Delivery behavior

When the window is visible, HereForWork shows its existing in-app toast. A failure toast
persists and **View details** opens Applications and the matching preparation details.
A ready toast remains for 30 seconds and **Review form** focuses the already-released
Chrome tab. That command only activates the existing tab and window; it does not inspect,
fill, finalize, or submit the form.

When the window is hidden, HereForWork attempts an informational macOS notification.
Native notifications intentionally have no action button or click-routing contract. A
permission denial or delivery error is recorded as a terminal delivery failure; the
Applications row remains available for recovery.

Fully quitting the app expires pending or in-flight outcome notifications during the
next startup. They are not replayed after a restart. Closing only the window leaves the
background app running, so a later outcome may still be delivered natively during that
same app session.

Retry is available in preparation Details when the stored policy permits recovery.
`retry_same_preparation` and `repair_runtime_then_retry` reuse the same preparation id.
`fresh_preparation_provider_run` and `fresh_preparation_id` expose Prepare again, which
cancels the failed job and starts a new preparation id so provider results are never
reused. `cv_fact_check_failed` also exposes Continue anyway: an explicit confirm that
completes the same preparation as user-accepted and unverified, then queues the browser
session. It never claims a fact-check pass. `manual_repair_required` stays diagnostic only.

The HereForWork adapter owns the durable compensating preparation transaction and uses
fixed upstream career-ops CLIs. Structured code, stage, retry policy, and bounded
diagnostics cross the NDJSON boundary. A PDF render failure recovered with a configured,
hash-bound user-reviewed CV is recorded as warning/provenance and does not emit a false
`preparation_failed` outcome; any later publication or tracker failure still does.
