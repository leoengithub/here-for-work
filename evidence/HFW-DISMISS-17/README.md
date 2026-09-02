# HFW-DISMISS-17 visual evidence

Deterministic source: `?application-preview=states`; light theme, English, 100% zoom, no live database, browser profile, or external form.

## Pass history

- `baseline/`: the unchanged Applications list at 1440x900, 768x900, and 390x844. The baseline capture records the default state; the previous inline confirmation was not captured before implementation.
- `after-pass-1/`: default and confirmation-open states at all three viewports. Review found that `Keep role` inherited destructive text color and that confirmation copy had an unnecessarily long measure.
- `after-pass-2/`: final default and confirmation-open states at all three viewports after one coherent correction. No further visual discrepancy was found, so the loop stopped.

## Behavioral checks

- The accessibility tree exposes `Dismiss` only for the failed and ready-for-review rows in the seven-state fixture.
- Opening confirmation moves focus to `Keep role`; cancelling restores focus to the row's `Dismiss` trigger.
- Confirmation actions are 44px high. Direct row actions receive the same minimum target height under `pointer: coarse`.
- No new spinner or animation was introduced; existing active-state spinners retain their reduced-motion rule.

## Component decision

The shadcn registry was inspected for `Button` and `Alert`. The existing registry components were composed without modifying their implementation; only feature-scoped layout styles were added.
