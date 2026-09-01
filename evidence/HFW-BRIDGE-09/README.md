# HFW-BRIDGE-09 browser handoff evidence

Date: 2026-09-01 (Europe/Madrid)

## Root cause

The app launched Chrome through macOS `open -a ... --args`. When Chrome was
already running, LaunchServices could reuse an existing Chrome instance without
honoring `--profile-directory`, so the application URL was absent from the
approved `Profile 1` extension context. The extension then exhausted its exact
URL lookup and returned an unclassified error. Reloading the extension restarted
the worker, but could not move the already-open page into its profile.

The live retry also exposed two recovery defects on the same session:

- answer-persistence Retry selected an older failed inspect command instead of
  the current persistence stage;
- a manifest-bound CV field produced one text-fill result and one upload result,
  so the fixed career-ops writer rejected the duplicate field id.

## Repaired flow

HereForWork validates the persisted Chrome profile against Chrome Local State,
launches the Chrome executable directly with the selected profile and exact
public HTTPS URL, and waits for the already-approved extension. The extension
polls exact matching tabs, then creates one inactive fallback tab in its own
approved profile when the native launch was missed. It retries the content
script, inspects the form, fills only safe verified fields, emits one result per
field (the manifest-bound upload owns the CV field), persists grounded answers,
releases the page, records `review_required`, and emits one ready notification.
No command or recovery path can submit or send the form.

## Bounds and timings

- Native extension poll interval: 750 ms.
- Exact-tab wait: 20 attempts, 250 ms apart (4.75 s delay / about 5 s wall
  ceiling), followed by one inactive profile-local fallback tab.
- Content-script retry: 20 attempts, 250 ms apart (4.75 s delay ceiling).
- Dynamic-form readiness: 20 attempts, 250 ms apart (4.75 s delay ceiling).
- Pending extension handshake timeout: 15 s.
- Command lease: 30 s, at most 3 leases. A disconnected expired lease has a
  further 15 s reconnect window; continuously reclaimed commands have a 90 s
  absolute lease ceiling.
- Live repaired inspect command: 2.070 s.
- Live safe fill/read-back command: 0.625 s.
- Live release command: 0.400 s.
- Ready-notification delivery after release: 0.044 s.

## Live result

The existing failed Symbiotic preparation was retried in the configured
`HereForWork` Chrome profile with the installed, approved extension. The form
contained 9 inspected fields: 5 safe fields and 4 user-owned fields. The
manifest-matched CV was present, four identity/contact/link values read back as
verified, the existing CV selection was preserved on the recovery fill, and the
four sensitive/unknown fields remained for the user. The session finished as
`review_required`; the Submit control was observed and was not clicked.

The unredacted Chrome capture is preserved only at
`/private/tmp/HFW-BRIDGE-09-safe-filled-form-unredacted.jpeg` and is not tracked.
The repository copy redacts the verified name and email values.

## Artifacts

- `before-extension-command-failed.jpeg`: original action-required state and
  the three pre-fix duplicate failure notices.
- `safe-filled-form-no-submit-redacted.png`: configured Chrome profile, attached
  CV, and redacted safe-filled controls.
- `review-required-ready-notification.jpeg`: released session after the ready
  notification was delivered; the transient toast had already cleared.
