---
name: HereForWork
description: A quiet operations desk for deliberate job-search decisions.
colors:
  signal-lime: "oklch(0.841 0.238 128.85)"
  signal-ink: "oklch(0.405 0.101 131.063)"
  mist-canvas: "oklch(0.987 0.002 197.1)"
  clean-surface: "oklch(0.998 0.001 197.1)"
  working-mist: "oklch(0.963 0.002 197.1)"
  cool-border: "oklch(0.925 0.005 214.3)"
  quiet-ink: "oklch(0.148 0.004 228.8)"
  secondary-ink: "oklch(0.56 0.021 213.5)"
  destructive: "oklch(0.577 0.245 27.325)"
  verified: "oklch(0.47 0.09 145)"
typography:
  display:
    fontFamily: "Space Grotesk Variable, sans-serif"
    fontSize: "1.7rem"
    fontWeight: 600
    lineHeight: "1.15"
    letterSpacing: "-0.035em"
  title:
    fontFamily: "Space Grotesk Variable, sans-serif"
    fontSize: "1.35rem"
    fontWeight: 600
    lineHeight: "1.25"
    letterSpacing: "-0.025em"
  body:
    fontFamily: "Manrope Variable, sans-serif"
    fontSize: "0.875rem"
    fontWeight: 400
    lineHeight: "1.5"
  label:
    fontFamily: "Manrope Variable, sans-serif"
    fontSize: "0.75rem"
    fontWeight: 600
    lineHeight: "1.25"
rounded:
  sm: "0.375rem"
  md: "0.5rem"
  lg: "0.625rem"
  xl: "0.875rem"
  full: "9999px"
spacing:
  xs: "0.25rem"
  sm: "0.5rem"
  md: "0.75rem"
  lg: "1rem"
  xl: "1.5rem"
  2xl: "2rem"
components:
  button-primary:
    backgroundColor: "{colors.signal-lime}"
    textColor: "{colors.signal-ink}"
    typography: "{typography.label}"
    rounded: "{rounded.full}"
    padding: "0 0.75rem"
    height: "2.25rem"
  button-outline:
    backgroundColor: "{colors.working-mist}"
    textColor: "{colors.quiet-ink}"
    typography: "{typography.label}"
    rounded: "{rounded.full}"
    padding: "0 0.75rem"
    height: "2.25rem"
  button-destructive:
    backgroundColor: "{colors.working-mist}"
    textColor: "{colors.destructive}"
    typography: "{typography.label}"
    rounded: "{rounded.full}"
    padding: "0 0.75rem"
    height: "2.25rem"
---

# Design System: HereForWork

## Overview

**Creative North Star: "Quiet Operations Desk"**

HereForWork should feel like a well-prepared desk at the start of a focused work session. The interface is calm, capable, and direct. Operational complexity stays behind the surface; the next meaningful decision remains obvious without pressure, celebration, or false certainty.

The physical scene is a job seeker reviewing several viable roles on a Mac during a concentrated daytime session. A light Mist canvas supports long reading and comparison. Signal Lime appears only where the interface is ready for a deliberate action or where current selection must be unmistakable.

The system uses shadcn components as its first source, with Base UI semantics, Hugeicons, Manrope for working text, and Space Grotesk for concise hierarchy. HereForWork-specific components exist only when the shadcn registry has no suitable semantic pattern.

**Key Characteristics:**

- Restrained, light-first surfaces with a single Signal Lime accent.
- Direct lists and dividers instead of repeated card grids.
- Compact, tactile controls with explicit states and generous focus treatment.
- Visible uncertainty and progress expressed through text, not color alone.
- Responsive state motion only, never decorative choreography.

## Colors

The palette pairs cool Mist neutrals with one high-energy Signal Lime. The contrast is intentional: the workspace stays quiet so meaningful actions read immediately.

### Primary

- **Signal Lime:** Reserved for the primary action, current selection, and the smallest meaningful state indicators.
- **Signal Ink:** Used as readable content on Signal Lime and for restrained accent text when the bright color would not meet contrast requirements.

### Neutral

- **Mist Canvas:** The application background, subtly tinted away from pure white.
- **Clean Surface:** Menus, drawers, and bounded controls that must separate from the canvas.
- **Working Mist:** Hover, selected-secondary, and low-emphasis control surfaces.
- **Cool Border:** Dividers and control outlines, never decorative framing.
- **Quiet Ink:** Primary text and high-confidence labels.
- **Secondary Ink:** Supporting metadata that does not change the decision.

### Semantic

- **Destructive:** Irreversible or discard-oriented actions and their explanatory errors.
- **Verified:** Confirmed healthy or completed states, always paired with text.

**The One Signal Rule.** Signal Lime occupies no more than ten percent of a normal working surface. If several controls compete in Lime, the hierarchy is wrong.

**The No Pure Neutral Rule.** Canvas, surfaces, and ink remain subtly cool-tinted. Pure white and pure black are forbidden.

## Typography

**Display Font:** Space Grotesk Variable (sans-serif fallback)
**Body Font:** Manrope Variable (sans-serif fallback)

**Character:** Space Grotesk gives headings a precise operational voice. Manrope stays neutral and highly readable across dense role metadata, system diagnostics, and action labels.

### Hierarchy

- **Display** (600, 1.7rem, 1.15): Workspace titles and major panel headings only.
- **Title** (600, 1.35rem, 1.25): Drawer titles, empty-state headings, and meaningful subviews.
- **Body** (400, 0.875rem, 1.5): Decisions, explanations, and report previews, capped at 72 characters where prose is continuous.
- **Label** (600, 0.75rem, 1.25): Buttons, filters, metadata headings, and compact status labels.

**The Working Voice Rule.** Interface labels are short and literal. Typography must never turn an operational screen into a marketing page.

## Elevation

The system is flat by default. Depth comes from tonal separation, dividers, and spatial placement. Shadows appear only when one surface must physically sit above another, such as a report drawer or anchored popover.

### Shadow Vocabulary

- **Raised workspace:** A broad, low-contrast shadow used only by drawers and popovers that overlap active content.

**The Flat Desk Rule.** Rows, groups, and sections remain unshadowed. If every section looks lifted, nothing has priority.

## Components

Every new component starts with a shadcn registry search. Registry code is adapted to HereForWork tokens and safety constraints; it is not wrapped in a second generic component system.

### Buttons

- **Shape:** Fully rounded and compact, with heights from 2.25rem to 2.5rem for ordinary desktop actions.
- **Primary:** Signal Lime with Signal Ink; one primary action per action group.
- **Hover / Focus:** Hover changes tone without adding shadow. Focus uses the shared ring and remains visible against every surface. Active state may shift by one physical pixel.
- **Outline / Ghost:** Outline identifies a bounded secondary action. Ghost is reserved for low-emphasis actions in already structured areas.
- **Destructive:** Uses Destructive text on a quiet tinted surface. It never resembles the primary path.

### Chips

- **Style:** Display-only status chips use Working Mist, Quiet Ink, and full rounding.
- **State:** Color is always paired with a readable status label. Interactive filters use a shadcn toggle pattern rather than clickable badges.

### Cards / Containers

- **Corner Style:** Gently curved when a bounded surface is necessary.
- **Background:** Clean Surface against Mist Canvas.
- **Shadow Strategy:** Flat by default, as defined by the Flat Desk Rule.
- **Border:** Cool Border at one pixel where separation cannot be achieved through spacing alone.
- **Internal Padding:** Uses the 0.75rem, 1rem, and 1.5rem spacing steps according to information density.

### Inputs / Fields

- **Style:** Native inputs enhanced through the corresponding shadcn field component, with Cool Border, Clean Surface, and a 0.5rem to 0.625rem radius.
- **Focus:** Shared three-pixel ring with a visible border shift.
- **Error / Disabled:** Error text is explicit and adjacent. Disabled fields remain legible and never imply successful completion.

### Navigation

Primary navigation remains compact and familiar. Active state combines Quiet Ink, weight, and a Signal Lime marker. System stays a secondary utility action rather than a third primary destination and uses the established Hugeicons settings icon with an accessible label and 44-pixel target.

### Queue Role Card

The compact queue role card is the signature working component. Cards remain a single vertical list, with a flat Clean Surface, one-pixel Cool Border, gentle radius, and no shadow, expansion, whole-card click, or nested card. The linked role title occupies the top line alone. Company, location, optional source-backed age, and one decision-changing uncertainty share the secondary line below and wrap when required. Only actions occupy the right edge: quiet ghost Dismiss first, then primary Prepare. ATS, score, internal source counts, and preparation state stay out of Queue. Narrow layouts reflow actions beneath the content without hiding age, location, or uncertainty.

The shadcn Card registry primitive was inspected before this pattern was implemented. Its generic header/content/footer regions added unnecessary nested structure for a semantic `ul > li > article` comparison list, so the existing project-owned role-row component was refined in place instead. Interactive controls continue to use the shared shadcn Button primitive and design tokens.

## Do's and Don'ts

### Do:

- **Do** search the shadcn registry before implementing every new component.
- **Do** keep one obvious primary action in each action group.
- **Do** keep uncertainty, blockers, and skipped work visible in plain language.
- **Do** use semantic lists, headings, buttons, fields, and progress elements with complete keyboard behavior.
- **Do** use Signal Lime sparingly against Mist surfaces and verify WCAG 2.2 AA contrast for every text pairing.
- **Do** preserve the user's physical Submit decision and every other HereForWork safety boundary.

### Don't:

- **Don't** copy mass auto-apply products that treat application count as the primary success metric.
- **Don't** create interfaces that hide blockers or express uncertain fit as confident fact.
- **Don't** add gamification that adds pressure to an already stressful search.
- **Don't** use gradient text, decorative glassmorphism, hero-metric layouts, or identical card grids.
- **Don't** use colored side-stripe borders or shadows on ordinary rows and sections.
- **Don't** create a component from scratch when the shadcn registry already provides the required semantics.
- **Don't** use a modal as the first solution when inline disclosure, a drawer, or progressive reveal can preserve context.
