# HFW-NOTIFY-06 visual QA

Captured from the Vite browser fallback with development-only deterministic outcome
events. The rendered component is the same Base UI/shadcn Toast used by the Tauri
renderer; no production data or native notification was used.

Environment: English (`html[lang=en]`), light color scheme, browser scale 1, device
pixel ratio 1, and visual viewport scale 1.

Pass 1:

- `pass-1/failure-1440x900.png`
- `pass-1/failure-900x700.png`
- `pass-1/ready-1440x900.png`
- `pass-1/ready-900x700.png`

Inspection result: both states remain fully visible and readable without overlap or
clipping at both approved desktop viewports. Titles, safety copy, and one bounded action
retain a clear hierarchy. Failure remains persistent; ready expires after 30 seconds as
verified by the component test. User visual/product approval remains outstanding.
