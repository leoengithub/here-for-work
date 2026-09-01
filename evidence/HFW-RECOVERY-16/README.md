# HFW-RECOVERY-16 visual and interaction QA

All captures use the deterministic `?application-preview=states` fixture. No local HereForWork database or career-ops canonical data was read or written.

## Capture matrix

- Light theme, English, 100% zoom.
- Desktop: 1440 × 900.
- Small laptop/tablet: 768 × 900.
- Narrow: 390 × 844.
- States: Waiting, Preparing CV, Preparation failed, Ready for review, Recording application, Application recorded, and Tracking update failed.

## Pass history

1. `baseline/`: raw preparation steps could appear as state, failures exposed only Details, and review rows lacked reopen/refill.
2. `after-pass-1/`: truthful state badges, recovery actions, and progress appeared. Review found a long role colliding with its state at 390 px, plus Undo remaining available during recording and after a tracking failure.
3. `after-pass-2/`: canonical-tracking protection was corrected and the long title no longer collided, but the narrow two-column row produced an awkward emergency word break.
4. `after-pass-3/`: at 460 px and below, identity and state each span the row while the state remains right-aligned. The long role reads naturally and actions retain clear separation.

## Accessibility and motion

- Accessibility snapshot exposes Applications as a selected tab and labelled panel, each action as a named button, both active rows with `aria-busy`, and each spinner as `role=status` with `aria-label=Loading`.
- Keyboard Tab reached System settings and then Cancel and return to Queue with a visible 3 px focus ring.
- With reduced motion emulated, both spinners reported `animation-name: none` and `animation-duration: 0s`.
- State does not rely on color: every state combines an icon or spinner with text.

## Component policy

The shadcn registry Spinner was installed. With explicit user approval, its prop type was narrowed so `strokeWidth` is numeric, matching Hugeicons. Its design and runtime behavior were not changed.

## Protocol audit

The browser bridge schema and extension command union remain exactly `inspect_request`, `fill_plan`, `release_for_review`, and `focus_review`. There is no Submit/finalize/send command. Existing extension tests also assert that submit controls are excluded and guarded until review release.
