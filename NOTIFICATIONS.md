# Preparation outcome notifications

HereForWork treats preparation outcomes as durable local events. SQLite records the
outcome in the same transaction that changes the preparation or browser-session state.
The Applications screen remains the source of truth even when notification permission
is denied or the operating system cannot display a notification.

## Outcome contract

- `preparation_failed` is created only for a real report/CV, answer-persistence, or
  browser-preparation failure. Needs-decision and cancelled preparations do not create
  failure outcomes.
- `application_ready` is created only after the extension completes
  `release_for_review`. Completing the report or CV is not sufficient.
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

Retry is available in preparation Details only when the stored policy permits reusing
the same preparation (`retry_same_preparation` or `repair_runtime_then_retry`). Policies
that require a fresh provider run, a fresh preparation ID, or manual repair never reuse
the failed preparation automatically.

The career-ops adapter prefers `hfw-preparation-commit.mjs` when it is present. It writes
the bounded request with mode `0600`, invokes the atomic entrypoint with that request and
the preparation effect directory, and preserves structured failure metadata. Once that
entrypoint is present, a failure never falls back to the legacy multi-command writer.
