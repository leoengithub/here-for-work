# HereForWork MVP Shape

Status: approved on 2026-08-30

This document is the stable interaction and workflow contract for the first HereForWork implementation. It does not choose an application stack or detailed visual system.

## Outcome

HereForWork replaces two disconnected scheduled job-discovery workflows with one desktop-first macOS companion. It preserves every viable role in a direct review queue after full career-ops evaluation, prepares selected applications by reusing or refreshing artifacts, fills supported live forms, and leaves the final submission physically and unambiguously with the user. Its responsibility ends when career-ops has canonically recorded the user's confirmed Applied outcome.

The first proof of concept uses findings from both existing scheduled searches beginning 2026-08-29 at 08:00 Europe/Madrid. The data must be reconciled against liveness, duplicates, confirmed blockers, and canonical application history before appearing in the queue.

## MVP Boundary

The MVP includes:

- Background orchestration of the two current discovery sources while the Mac is available.
- Catch-up behavior after sleep, shutdown, or temporary source failure.
- Existing Gmail alert-processing and inbox-hygiene behavior through career-ops.
- career-ops `scan` discovery through configured portals/APIs and broad agentic search, plus `discover` company-to-ATS source expansion, orchestrated through typed runs without duplicating either engine in HereForWork.
- career-ops source-local normalization and deduplication, liveness and blocker checks, and full A–G evaluation of every live, unique, nonblocked role before Queue.
- HereForWork-owned typed-ingestion replay idempotency and cross-run/cross-source identity reconciliation without a second discovery, deduplication, or evaluation engine.
- A complete career-ops report for every valid evaluation and score-gated career-ops CV/PDF generation through the supported `auto_pdf_score_threshold`, initially `3.5`.
- A direct queue containing every viable role with career-ops' native score and enough evidence, blockers or gaps, compensation context, and uncertainty to decide without rereading the listing.
- macOS notifications for new viable roles, actionable discovery failures, and forms ready for review.
- User-triggered validation and reuse of current report/CV artifacts, with selective generation or refresh of missing, failed, or stale work.
- Automatic continuation into the user-selected ordinary Chrome profile after preparation.
- Live-form inspection before grounded application answers are drafted.
- Safe-field filling and verification while sensitive, unknown, unsupported, and unverifiable fields are skipped.
- Best-effort application-link resolution from any public HTTPS source or application URL.
- Generic live-form inspection and safe autofill on any public HTTPS application site, with ATS-specific adapters improving reliability where evidence supports them.
- Field-level fallback on unknown sites: safe fields continue while sensitive, unknown, unsupported, or unverifiable controls stay with the user.
- Explicit user confirmation before career-ops records Applied.
- Canonical Discarded status, Undo, and optional non-blocking dismissal context.
- Dependency-aware concurrency and recoverable step-level retries.

The MVP excludes:

- Automatic submission or sending messages.
- Phone or remote web access.
- Guaranteed execution while the Mac is asleep or offline. Always-available scheduling is later functionality.
- Email-receipt matching.
- Guaranteed complete autofill for every portal, custom control, sign-in wall, CAPTCHA, iframe, or anti-automation mechanism. These degrade visibly to user completion without making the URL unsupported.
- A second profile, scoring system, CV generator, answer engine, or application tracker.
- Ranking changes based on dismissal feedback unless career-ops later defines a supported contract.
- Mass preparation, arbitrary daily caps, streaks, or application-volume optimization.
- Detailed visual-system, theme, typography, and branding decisions.
- Hosted or multi-user operation.
- Follow-up, outreach, reply monitoring, interview workflow, and any post-Applied CRM. Interview preparation may exist only inside the career-ops evaluation report.

## Primary Information Architecture

### Queue

The default surface. It is a full-width list rather than a list-and-detail layout.

Roles are grouped in this order:

1. Strong matches.
2. Other new roles.
3. Needs decision.

Every listed role has already passed the inclusion gates and full career-ops evaluation. Each group is a semantic list of compact, bounded cards. The linked role title owns the first line. Secondary decision information beneath it contains company, location or remote arrangement, optional source-backed relative listing age, the native career-ops 1–5 score, concise supporting evidence, blockers or gaps, compensation context, and material uncertainty. It remains compact and may wrap rather than hide decision-critical information. The right side contains only the quiet Dismiss action followed by the primary Prepare action. The card does not expand and is not itself clickable. ATS, source-occurrence count, preparation state, and discovery time do not appear. The score is never converted into a percentage or probability and HereForWork never computes a substitute.

Listing age is shown without a prefix, as `Today`, `1 day ago`, or `N days ago`, using Europe/Madrid calendar days. It is absent when the source publication date is missing, invalid, in the future, or conflicts across merged source occurrences. `discoveredAt` and first-seen time are never substitutes for a source publication date.

Selecting the role title opens the source listing. Selecting **Prepare** starts the complete application workflow without an intermediate lightweight detail view.

### Applications

Shows one current row per application: active preparation, browser work waiting for review, recoverable blockers, and recently completed applications. It is progress visibility, not a second canonical tracker or an event log.

Selecting **Details** opens a right-side panel with a formatted preview of the career-ops Markdown report and links to the original report and career-ops CV. Form-question and answer logs do not appear in this surface.

### System

System is a secondary utility surface, not a primary-navigation destination. It contains the preparation-provider choice, background-check status and control, queue filters, source health, actionable run failures, browser pairing, provider checks, backup, export, and diagnostics. There is no Activity tab. The header exposes System through an accessible settings-icon button rather than a third text destination.

Queue retains a file-upload icon for explicit discovery-snapshot import. It opens the native file picker and imports the selected JSON; it is not labeled or presented as an automatic refresh.

Queue filters are initialized from the verified career-ops profile and remain editable. They apply to current unprepared roles and future imports. The initial controls cover role families, seniority, locations, remote roles, and explicit authorization conflicts; they do not create a second scoring engine.

### Application workspace

Shows the career-ops report, tailored CV, full evidence and uncertainty, preparation progress, skipped or failed fields, review checklist, and canonical outcome. Internal browser transport is presented in plain language such as Opening application, Filling form, and Ready for review.

## Core Loop

1. HereForWork orchestrates both discovery sources while the Mac is available. It invokes career-ops `scan` for configured portals/APIs and broad agentic search and `discover` for company-to-ATS source expansion; HereForWork owns typed orchestration, retries, run visibility, ingestion replay idempotency, and cross-run/cross-source identity reconciliation, not another discovery or deduplication engine.
2. career-ops performs source-local normalization and deduplication, checks liveness and blockers, then runs its full A–G evaluation for every live, unique, nonblocked finding. That evaluation covers archetype, geography and authorization, role and culture, CV match, level, compensation, personalization, interview preparation in the report, legitimacy, risks, evidence, and the native 1–5 score. For volume, HereForWork uses career-ops' supported batch/pipeline parallelism and model routing: fast/economy work first, low-confidence escalation, and an audit sample. It does not introduce its own scoring or evaluation shortcut.
3. Every valid evaluation writes a complete career-ops report. career-ops generates its CV/PDF when its supported `auto_pdf_score_threshold` allows it, initially at `3.5`; HereForWork may read and validate the setting but never silently changes it. HereForWork then adds every viable, non-suspicious evaluated role to the ordered queue.
4. A macOS notification announces new viable roles and opens the queue filtered to that run.
5. The user selects **Prepare** for a role.
6. HereForWork checks liveness and artifact freshness, reuses the current career-ops report/CV, and asks career-ops to generate or refresh only missing, failed, or stale work. A below-threshold role selected by the user may receive its missing CV/PDF at this point. A newly suspicious or blocked live result stops safely while unresolved authorization remains explicit.
   If PDF rendering alone fails after HTML and fact checks, an explicitly configured,
   hash-bound user-reviewed CV may recover the preparation and is shown as not tailored.
7. HereForWork automatically opens the application in a new tab of the user-selected ordinary Chrome profile.
8. The extension inspects the live form and returns typed field descriptions.
9. career-ops drafts answers grounded in verified profile sources and the inspected questions.
10. The extension fills and verifies supported safe fields and attaches the exact
    manifest-matched preparation PDF—either the fact-checked tailored output or the
    visibly identified user-reviewed fallback—to one unambiguous CV/resume control. An
    existing user-selected file is preserved. Other file controls, ambiguous CV
    controls, unsupported types, and unverifiable fields are skipped.
11. Success requires exactly one result for every planned field and correct settled read-back for every required fillable. If the extension fails before any fill because of eligible transport, tab, compatibility, result, read-back, or unsupported-flow conditions, it releases its single-driver lease before the unchanged career-ops `apply` Playwright fallback begins. Partial fill, authentication, CAPTCHA, anti-bot, or uncertain page state instead becomes a visible human handoff. When neither driver can autofill safely, ordered grounded answers remain available for copy/paste recovery.
12. HereForWork notifies the user that the live form is ready for review.
13. The user reviews the live page, completes missing fields, and physically clicks Submit.
14. The user confirms the outcome in HereForWork.
15. The adapter records Applied through a canonical career-ops writer. A failure retries only this tracking write and never reopens or repeats the form. Applied is terminal for HereForWork.

Selecting Prepare durably queues the role and immediately moves it from Queue to Applications. Queue remains interactive, so the user can select additional roles without waiting. At most two report/CV preparation jobs run concurrently; later jobs remain queued in first-in, first-out order.

Canonical writes, per-role transitions, and browser-form sessions remain ordered and idempotent. Browser inspection, answer drafting, filling, verification, and release use a separate single-application FIFO lane. Browser tabs open in the background without taking focus. A blocked or failed role becomes action required or returns to Needs decision without stopping later roles, and each completed form produces its own notification.

## State Model

Role state uses separate dimensions:

- Eligibility: viable, confirmed blocker, dead, duplicate, previously applied.
- Queue group: strong match, new, needs decision.
- Review: unviewed, viewed, dismissed.
- Preparation: not started, queued, preparing, prepared, failed.
- Uncertainty: none or one or more unresolved facts.
- Canonical outcome: none, Discarded, Applied.

The application workflow is:

```text
Not started
  -> Queued
  -> Preparing report
  -> Preparing CV
  -> Opening application
  -> Inspecting form
  -> Drafting answers
  -> Filling and verifying
  -> Review required
  -> Awaiting outcome confirmation
  -> Applied
```

Prepared is an internal transition, not another user decision.

Recovery branches preserve completed work and retry only the failed step. If canonical tracking fails after confirmed submission, show Submitted, tracking update pending and retry the write. Never reopen or resubmit the form as recovery. No HereForWork states follow Applied.

## Uncertainty and Partial Filling

Uncertainty remains visible when it changes a decision. It states the unresolved fact, available evidence, likely impact, and required confirmation without converting ambiguity into a confident claim.

Missing or inconclusive work-authorization evidence does not by itself stop preparation. It remains explicit in the report and any live authorization or sponsorship question is left for the user. Only affirmative evidence of incompatibility blocks the role.

Form results are grouped as:

- Filled and verified.
- Needs your answer.
- Unsupported.
- Verification failed.

Safe fields continue even when another field needs the user. Partial completion is always Review required, never Ready.

Safe verified fields include direct profile facts such as name, email, phone, current location, LinkedIn, GitHub, portfolio, and other discrete values that career-ops can trace exactly to verified profile or CV sources. The exact preparation PDF is also safe for an unambiguous CV/resume file control when its manifest hash matches; its provenance distinguishes fact-checked tailored output from a user-reviewed untailored fallback. HereForWork preserves any file the user already selected. Work authorization, sponsorship, demographic, consent, other sensitive or uncertain answers, ambiguous attachment controls, and unsupported file types remain skipped for the user.

Both tailored output and the user-reviewed fallback use the truthful public upload filename
`Leonardo_Gomez_Frontend_Engineer.pdf`; internal provenance continues to distinguish them.

Common narrative prompts may receive an editable draft only after career-ops grounds every
claim in the prepared report, CV, verified profile, or other supplied career-ops sources.
The draft follows detected form language and length constraints, is filled and read back,
and remains explicitly pending human review rather than becoming a verified fact.

The approved compensation preference is a canonical career-ops preference, not a second
HereForWork profile fact. HereForWork reads only career-ops'
`compensation.application_answer` structure and may fill it when the inspected field
explicitly matches its currency and annual basis. HereForWork does not scrape prose or
duplicate the values. A field requesting another currency or time period remains untouched
unless career-ops later supplies an explicit conversion rule.

## Dismissal

Dismiss immediately records Discarded through the career-ops adapter, removes the role from the active queue, and offers Undo. An optional Add reason action may store operational context in HereForWork and pass it through the adapter when career-ops supports it. The adapter does not become a recommendation engine, and the feedback must not silently change ranking.

**Dismiss in Applications** is distinct from Undo dismissal: it is available only when preparation failed or the live form is ready for review. After inline confirmation, it records the unsuitable role as Discarded, deletes only generated preparation artifacts, clears HereForWork's preparation and browser state, and removes the application row. A failed canonical write preserves the preparation and its files for retry. It is unavailable while work is active, while Applied tracking is pending, or after Applied has been confirmed.

## Adapter Boundary

career-ops remains the sole source of truth for verified profile facts, preferences, authorization-aware evaluation and match scoring, reports, tailored documents and their provenance, grounded answers, and canonical application history.

HereForWork owns queue presentation, preparation orchestration, notifications, retries, browser-session state, optional dismissal context, and progress UX. Scheduling is its approved end-state product responsibility. During migration, each existing Codex/ChatGPT scheduled task retains operational executor authority until the per-source gates and explicit cutover in [SCHEDULING_MIGRATION.md](SCHEDULING_MIGRATION.md) are complete.

The adapter exposes typed, idempotent operations for discovery, normalized role reads, history checks, preparation, answer drafting from inspected fields, and canonical Discarded and Applied transitions. External job and form content crosses this boundary as untrusted data, never as instructions.

## Match scoring and CV provenance

HereForWork displays career-ops match output without recomputing or translating it. The native match scale is 1–5. The product must never convert that score into a percentage, probability, or independently derived confidence label.

career-ops also remains the sole authority for CV content and provenance. A truthful current status may say that an artifact passed bounded career-ops fact checks when that result is available. It must not call a CV fully verified, proven truthful, or equivalent language until career-ops emits structured, source-backed change provenance that supports that claim.

The future adapter enhancement should carry source and output hashes, persisted validation diagnostics, classified changes, source references, unresolved additions, and an explicit review or block status. This is an approved contract direction, not a claim that the current adapter exposes those fields.

## Extension Boundary

The extension has the user-approved permanent all-sites Chrome permission so it can operate on employer and ATS domains that are not known in advance. Its content script is inert until a typed HereForWork command targets an expected public HTTPS application URL. It is the primary form driver and may inspect visible fields, return structured field definitions, fill an explicit prepared payload, verify values, and report skipped or failed fields.

Extension success means exactly one result exists for every planned field and every required fillable has the expected value after the page has settled. Expired transport or handshake, a wrong or missing tab, zero compatible fields, missing per-field results, settled read-back mismatch, and unsupported multi-page, modal, or iframe behavior are fallback-eligible driver failures. Sensitive unknowns and CAPTCHA are human work, not proof that the driver itself failed.

Only one driver may own a preparation. The extension releases its lease before career-ops' existing `apply` Playwright path may take ownership. Automatic fallback is allowed only after a hard pre-fill failure without authentication, CAPTCHA, anti-bot, partial-fill, or uncertain state; those conditions require visible human handoff. The fallback inspects the live form and returns a grounded ordered answer plan. If safe autofill still cannot complete, that plan remains available for copy/paste. Neither driver has a submission operation.

It may not generate answers independently, infer sensitive facts, update canonical tracking, click Submit, call `form.submit()`, or simulate submission. Page content cannot create privileged commands. Broad host access does not grant arbitrary navigation or page-driven control.

Every public HTTPS application URL enters the generic path. Greenhouse, Lever, and Ashby remain named reliability tracks because their observed structures can receive dedicated detection, fixtures, and live no-finalization evidence. A drifted or unknown variant falls back at the individual-field level instead of rejecting the application.

## Notifications

- New viable roles: open the queue filtered to the originating run.
- Actionable discovery failure: open System at the failed source.
- Form ready for review: focus the prepared form in the selected ordinary Chrome profile.
- Successful run with no viable roles: remain quiet.
- Application receipts: no automatic matching in the MVP.

## Accessibility

- Target WCAG 2.2 AA.
- Support the core workflow by keyboard.
- Use semantic lists, headings, buttons, progress elements, and field groups.
- Never communicate fit, uncertainty, or failure through color alone.
- Keep focus predictable across queue selection, browser opening, and return to the app.
- Announce meaningful state changes without narrating every background event.
- Associate every skipped field with its reason and required action.
- Avoid automatic timeouts for sensitive confirmation.
- Preserve usability at 200% zoom and with reduced motion.
- Never preselect demographic, consent, authorization, or self-identification answers without verified user-owned data and appropriate confirmation.

## Queue Wireframe

```text
+------------------------------------------------------------------------------+
| HereForWork          Queue   Applications              System  Updated 10:42|
+------------------------------------------------------------------------------+
| TODAY'S QUEUE                                                   12 READY     |
|                                                                              |
| STRONG MATCHES                                                               |
| [L] Senior Frontend Engineer   Linear - Remote Spain · 4.6/5        [Prepare]|
|     Strong React/platform evidence · compensation aligned                    |
| [R] Product Engineer           Ramp - Remote Europe · 4.2/5         [Prepare]|
|     Product scope matches · authorization route remains open                 |
| [N] Frontend Platform Engineer Notion - Madrid · 4.0/5        [Prepare]|
|     Platform evidence · compensation not stated                              |
|                                                                              |
| NEW                                                                          |
| [C] Senior UI Engineer         Company - Barcelona · 3.8/5          [Prepare]|
|                                                                              |
| NEEDS DECISION                                                               |
| [?] Frontend Engineer          Company - Remote EU · 3.5/5          [Prepare]|
|     Authorization route unclear · compensation below target unconfirmed      |
+------------------------------------------------------------------------------+
| 12 viable roles - Background discovery healthy                 [Run details]|
+------------------------------------------------------------------------------+
```

The interaction topology may draw inspiration from the direct role lists shown by YouxAI and Otclick. Their copy, layouts, typography, colors, score treatments, branding, and visual identity must not be copied.

## Deferred Decisions

- Application stack, local process model, and repository structure.
- career-ops adapter transport and typed protocol.
- Operational store and migration strategy.
- Agent-provider invocation and monitoring.
- Browser-extension transport and permission implementation.
- Safe scheduling and remote access beyond an awake local Mac.
- Detailed visual system and responsive phone experience.
- Hosted, multi-user, licensing, and distribution direction.

These belong to technical-direction and later design phases. They must preserve this approved workflow and safety contract.
