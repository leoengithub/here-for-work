# HFW-LOAD-44 Queue operational states

Visual QA was run against the local Vite preview at `http://localhost:1420/` with
the deterministic `queue-preview` fixtures, light theme, English, 100% zoom, and no
authentication.

## Pass history

- `pass-1/current/` captures the paused implementation before the critique fix.
- `pass-1/baseline/` preserves those pre-fix captures for comparison.
- `pass-2/current/` captures the corrected implementation after moving the operational
  banner to the full Queue width.
- The critique fix addressed a material mobile/tablet defect where progress was constrained
  to the title column instead of the Queue content width.

Each state was checked at 390x844, 1024x768, and 1440x900:

- `evaluating`: shadcn Spinner, count, and elapsed start time only.
- `progress`: shadcn Progress with completed/total count.
- `waiting`: clock, last successful run.
- `blocked`: alert icon, 63-role count, explanation, and Open System action.
- `idle`: ordinary first-run empty state with Import discovery snapshot action.

Accessibility assertions confirmed polite live regions, no ETA copy, an accessible progress
label/value, Open System action availability, and an import action only for idle mode.
Reduced-motion media was enabled during the accessibility pass. No final visual approval is
claimed.
