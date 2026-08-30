# Product

## Register

product

## Users

HereForWork initially serves an active job seeker who checks several job sources multiple times per day and spends substantial time copying job details, judging uncertain matches, preparing materials, and filling repetitive application forms.

The broader intended user is an active job seeker who wants significant automation but rejects mass application spam. They are working under time and attention pressure. They need plausible roles preserved, uncertainty explained honestly, prepared answers grounded in verified facts, and control over every application sent in their name.

## Product Purpose

HereForWork turns fragmented job discovery and application preparation into one reviewable workflow. Scheduled runs collect and classify jobs. The user reviews a unified queue and selects **Prepare application** when a role deserves effort. Only then does the system generate a full evaluation and tailored CV. HereForWork then opens the live application, inspects its actual questions, drafts grounded answers, and fills supported fields. The user reviews the live page and physically submits it.

Success means the user spends less time searching, copying, prompting, and repeating profile answers while applying to viable roles with greater consistency and no loss of authorship or control.

## Brand Personality

**Capable, encouraging.**

The product should feel like a competent partner that has already done the administrative work and presents the next decision clearly. Encouragement should come from reduced friction, honest progress, and useful preparation, not streaks, pressure, or artificial celebration.

## Anti-references

- Mass auto-apply products that treat application count as the primary success metric.
- Interfaces that hide blockers or express uncertain fit as confident fact.
- Gamification that adds pressure to an already stressful search.

Specific visual anti-references are not yet defined. Do not infer a full aesthetic from the job-search or AI-product category.

## Design Principles

1. **Do the operational work, preserve the decision.** Automate discovery and preparation while leaving application judgment and submission with the user.
2. **Preparation follows intent.** Do not generate expensive, personalized artifacts until the user selects a role.
3. **Uncertainty stays visible.** A plausible role with an unresolved question remains reviewable. Never convert ambiguity into confidence for the sake of a cleaner interface.
4. **Hygiene should be invisible.** Deduplication, normalization, and source merging happen before presentation. Surface them only when they change a decision.
5. **Progress should feel capable, not coercive.** Help an active seeker move through every viable opportunity without arbitrary caps, streaks, or application-volume pressure.
6. **The final action remains human.** HereForWork can prepare and fill, but it never submits or sends.

## Approved MVP Shape

The approved interaction and workflow contract is documented in [MVP_SHAPE.md](MVP_SHAPE.md).

The MVP is desktop-first for macOS. Its primary surface is a direct list of viable roles, ordered as strong matches, other new roles, and roles needing a decision. Selecting **Prepare application** starts one continuous workflow from career-ops report and CV generation through browser opening, live-form inspection, grounded answer drafting, safe-field filling, verification, and notification that the form is ready for human review.

The browser companion initially targets Greenhouse, Lever, and Ashby. Unsupported portals retain a manual browser workflow. Detailed visual-system decisions and phone access remain later work.

## Accessibility & Inclusion

- Target WCAG 2.2 AA.
- Make core workflows fully keyboard accessible.
- Never encode status or confidence through color alone.
- Respect reduced-motion preferences.
- Use plain explanations for uncertainty, blockers, failures, and recovery actions.
- Preserve user agency for sensitive, demographic, legal, salary, authorization, and self-identification fields.
