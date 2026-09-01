# HFW-PREP-08 System visual QA

Date: 2026-09-01

Fixture boundary: Vite browser fallback with an empty dashboard. The Tauri store,
career-ops data, and configured personal PDF were not loaded or mutated.

## Captures

- `system-reviewed-cv-1440x900.png`
- `system-reviewed-cv-900x700-focused.png`

## Checks

- The control uses the existing registry-derived shadcn `Field`, `FieldLabel`,
  `Input`, and `Button` primitives. Fallback provenance uses the existing `Alert`,
  `AlertTitle`, and `AlertDescription` primitives; no new UI primitive was added.
- The textbox exposes the accessible label `Reviewed CV fallback` through the
  matching `for`/`id` pair.
- Keyboard focus is visible at 900x700. Pressing Tab from the textbox moves to
  the `Save reviewed CV` button.
- Horizontal document width exactly matched both viewports (1440 and 900 pixels),
  with no horizontal overflow.
- The fixture rendered `Not configured` and the generic placeholder only. No
  personal filesystem path appeared in visible page text.
- At 1440x900 the full fallback control and background setting remain visible in
  the first viewport. At 900x700 the responsive two-column layout remains aligned,
  and the focused control, supporting copy, save action, and following setting are
  visible without clipping.

Result: pass. No clear visual, accessibility, focus, or overflow defect found; no
source change was made during this QA pass.
