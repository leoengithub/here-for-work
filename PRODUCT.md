# Product

## Register

product

## Users

HereForWork initially serves an active job seeker who checks several job sources multiple times per day and spends substantial time copying job details, judging uncertain matches, preparing materials, and filling repetitive application forms.

The broader intended user is an active job seeker who wants significant automation but rejects mass application spam. They are working under time and attention pressure. They need plausible roles preserved, uncertainty explained honestly, prepared answers grounded in verified facts, and control over every application sent in their name.

## Product Purpose

HereForWork turns fragmented job discovery and application preparation into one reviewable workflow. Scheduled runs use career-ops to discover, normalize, deduplicate within its discovery engine, check, and fully evaluate every live, unique, nonblocked role before Queue. HereForWork makes typed ingestion replay-idempotent and reconciles identity across runs and sources; it does not build a second deduplication or discovery engine. The user reviews career-ops' native score, evidence, blockers, compensation context, and uncertainty in one compact queue, then selects **Prepare application** when a role deserves effort. Prepare reuses the current career-ops report and CV or refreshes only missing, failed, or stale artifacts before opening the live application, inspecting its actual questions, drafting grounded answers, and filling supported fields. The user reviews the live page and physically submits it.

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
2. **Evaluation precedes presentation; preparation follows intent.** Fully evaluate viable roles before Queue. Reuse score-gated career-ops artifacts, and generate or refresh additional preparation work only after the user selects a role.
3. **Uncertainty stays visible.** A plausible role with an unresolved question remains reviewable. Never convert ambiguity into confidence for the sake of a cleaner interface.
4. **Hygiene should be invisible.** Deduplication, normalization, and source merging happen before presentation. Surface them only when they change a decision.
5. **Progress should feel capable, not coercive.** Help an active seeker move through every viable opportunity without arbitrary caps, streaks, or application-volume pressure.
6. **The final action remains human.** HereForWork can prepare and fill, but it never submits or sends.

## Approved MVP Shape

The approved interaction and workflow contract is documented in [MVP_SHAPE.md](MVP_SHAPE.md).

The MVP is desktop-first for macOS. Its primary surface is a direct list of viable roles, ordered as strong matches, other new roles, and roles needing a decision. Selecting **Prepare application** starts one continuous workflow from career-ops artifact validation and selective generation or refresh through browser opening, live-form inspection, grounded answer drafting, safe-field filling, verification, and notification that the form is ready for human review.

Prepare is asynchronous and role-scoped. Selecting it immediately moves that role into Applications without blocking Queue or other controls. HereForWork may run up to two report/CV preparations concurrently; additional selections wait durably in first-in, first-out order. Browser inspection, answer drafting, filling, verification, and release remain one application at a time in a separate FIFO lane. A blocked or failed role never prevents a later role from continuing, and completed tabs open in the background without taking focus.

The browser companion accepts any public HTTPS application URL. It first attempts to resolve source listings to their application form, then inspects every site through a conservative generic adapter. Greenhouse, Lever, Ashby, and future ATS-specific adapters improve reliability but never define which sites are supported. Unknown or unverifiable controls are skipped individually while safe fields continue. Detailed visual-system decisions and phone access remain later work.

Primary navigation contains Queue and Applications only. Queue filters live in the secondary System surface, begin with verified career-ops preferences, and apply to current unprepared roles and future imports. Queue presents the native career-ops 1–5 score together with concise evidence, blockers or gaps, compensation context, and material uncertainty; HereForWork never rescales or recomputes it. Applications presents one current state per role; a Details action opens the formatted career-ops report rather than a question-answer log.

Discovery must exclude roles classified as suspicious before they reach Queue; users are not expected to investigate scams or employer identity as queue work. Every live, unique, nonblocked role receives the full career-ops A–G evaluation, and each valid evaluation writes its complete report before Queue. At volume, HereForWork relies on career-ops' supported batch/pipeline parallelism and model routing: fast/economy processing first, escalation for low-confidence results, and an audit sample. This never authorizes a HereForWork score. career-ops may also generate its CV/PDF during evaluation according to its supported `auto_pdf_score_threshold`; HereForWork may read and validate that configuration but never silently rewrite it. The upstream fallback is `3.0` when the key is absent; this personal career-ops configuration is explicitly set to the approved `3.5`, and the capabilities check reports it as configured. Prepare retains a suspicious live result as a safety backstop, reuses current artifacts, and generates or refreshes only missing, failed, or stale work—including CV/PDF work for a below-threshold role the user explicitly chooses to prepare. Missing or inconclusive authorization evidence remains visible rather than being converted into confidence. Dismiss in Applications is available only after preparation fails or the form is ready for review. It records Discarded, removes only generated preparation artifacts, and clears the local preparation/browser state.

The product workflow ends at canonical **Applied**. After the user physically submits the form and confirms that outcome, HereForWork asks career-ops to record Applied and retries only that tracking write if it fails. Follow-up, outreach, reply monitoring, interviews, and post-application CRM are outside HereForWork; interview preparation may appear only as content in the career-ops evaluation report.

## Accessibility & Inclusion

- Target WCAG 2.2 AA.
- Make core workflows fully keyboard accessible.
- Never encode status or confidence through color alone.
- Respect reduced-motion preferences.
- Use plain explanations for uncertainty, blockers, failures, and recovery actions.
- Preserve user agency for sensitive, demographic, legal, salary, authorization, and self-identification fields.
