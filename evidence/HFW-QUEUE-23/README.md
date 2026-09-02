# HFW-QUEUE-23 visual QA

The Queue was exercised through the real Vite application at
`?queue-preview=decisions` with a development-only fixture containing three
canonical career-ops evaluations and two pre-Queue lifecycle records.

## Registry inspection

`shadcn view button badge card skeleton empty alert` was run against the
project's `base-maia` registry configuration before UI edits.

- `Button`, `Badge`, `Empty`, and `Alert` already match the registry and were
  left unchanged.
- The registry `Card` was not used. Its generic header, content, and footer
  regions add unnecessary structure to the existing semantic
  `ul > li > article` comparison list. The documented project-owned role row
  remains the appropriate pattern.
- No shadcn component was adapted, forked, or rewritten.

## Captures

- `queue-desktop-1440x1000.png`: desktop hierarchy, role ordering, native score,
  decision evidence, and right-edge actions.
- `queue-narrow-600x960.png`: narrow reflow with both actions below the decision
  information and no horizontal overflow.
- `queue-narrow-focus-prepare.png`: visible keyboard focus on the primary action.
- `queue-200-percent-1440x1000.png`: 200 percent zoom with decision information
  and actions still available.

The final narrow probe reported `innerWidth: 600`, `scrollWidth: 600`, all role
cards within the viewport, and reduced-motion preference enabled. The final
reload had no page errors; the console contained only Vite connection messages
and React's development-tools notice.
