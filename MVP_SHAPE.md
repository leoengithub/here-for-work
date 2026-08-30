# HereForWork MVP Shape

Status: approved on 2026-08-30

This document is the stable interaction and workflow contract for the first HereForWork implementation. It does not choose an application stack or detailed visual system.

## Outcome

HereForWork replaces two disconnected scheduled job-discovery workflows with one desktop-first macOS companion. It preserves every viable role in a direct review queue, prepares applications only after user intent, fills supported live forms, and leaves the final submission physically and unambiguously with the user.

The first proof of concept uses findings from both existing scheduled searches beginning 2026-08-29 at 08:00 Europe/Madrid. The data must be reconciled against liveness, duplicates, confirmed blockers, and canonical application history before appearing in the queue.

## MVP Boundary

The MVP includes:

- Background orchestration of the two current discovery sources while the Mac is available.
- Catch-up behavior after sleep, shutdown, or temporary source failure.
- Existing Gmail alert-processing and inbox-hygiene behavior through career-ops.
- Normalization, liveness checks, deduplication, and lightweight classification.
- A direct queue containing every viable role, including roles with explained uncertainty.
- macOS notifications for new viable roles, actionable discovery failures, and forms ready for review.
- User-triggered career-ops report and tailored CV generation.
- Automatic continuation into the user-selected ordinary Chrome profile after preparation.
- Live-form inspection before grounded application answers are drafted.
- Safe-field filling and verification while sensitive, unknown, unsupported, and unverifiable fields are skipped.
- Extension coverage for Greenhouse, Lever, and Ashby, subject to evidence from the real proof-of-concept dataset.
- Manual browser completion for unsupported portals.
- Explicit user confirmation before career-ops records Applied.
- Canonical Discarded status, Undo, and optional non-blocking dismissal context.
- Dependency-aware concurrency and recoverable step-level retries.

The MVP excludes:

- Automatic submission or sending messages.
- Phone or remote web access.
- Guaranteed execution while the Mac is asleep or offline. Always-available scheduling is later functionality.
- Email-receipt matching.
- Universal ATS support, with Workday and custom portals initially using manual completion unless real usage changes the priority.
- A second profile, scoring system, CV generator, answer engine, or application tracker.
- Ranking changes based on dismissal feedback unless career-ops later defines a supported contract.
- Mass preparation, arbitrary daily caps, streaks, or application-volume optimization.
- Detailed visual-system, theme, typography, and branding decisions.
- Hosted or multi-user operation.

## Primary Information Architecture

### Queue

The default surface. It is a full-width list rather than a list-and-detail layout.

Roles are grouped in this order:

1. Strong matches.
2. Other new roles.
3. Needs decision.

Every listed role has already passed the inclusion gates. A row contains only the role, company, location or remote arrangement, ATS when known, concise uncertainty when it changes the decision, preparation state, Prepare action, and secondary dismiss action. Listing age and discovery time do not appear.

Selecting the role title opens the source listing. Selecting **Prepare** starts the complete application workflow without an intermediate lightweight detail view.

### Applications

Shows active preparation, browser work waiting for review, recoverable blockers, and recently completed applications. It is progress visibility, not a second canonical tracker.

### Activity

Shows source health, run progress, actionable failures, retries, and summary counts for duplicates, dead listings, prior applications, and confirmed blockers. Exclusion diagnostics remain secondary to the daily queue.

### Application workspace

Shows the career-ops report, tailored CV, full evidence and uncertainty, preparation progress, inspected questions, answer provenance, skipped or failed fields, review checklist, and canonical outcome. Internal browser transport is presented in plain language such as Opening application, Filling form, and Ready for review.

## Core Loop

1. HereForWork runs both discovery sources while the Mac is available.
2. career-ops normalizes, deduplicates, checks liveness, and classifies the findings.
3. HereForWork adds every viable role to the ordered queue.
4. A macOS notification announces new viable roles and opens the queue filtered to that run.
5. The user selects **Prepare** for a role.
6. career-ops generates the full report and tailored CV.
7. HereForWork automatically opens the application in the user-selected ordinary Chrome profile.
8. The extension inspects the live form and returns typed field descriptions.
9. career-ops drafts answers grounded in verified profile sources and the inspected questions.
10. The extension fills and verifies supported safe fields while skipping everything requiring the user.
11. HereForWork notifies the user that the live form is ready for review.
12. The user reviews the live page, completes missing fields, and physically clicks Submit.
13. The user confirms the outcome in HereForWork.
14. The adapter records Applied through a canonical career-ops writer.

Preparation and document tasks may overlap where their dependencies permit. Canonical writes, per-role transitions, and browser-form sessions remain ordered and idempotent. If several preparations finish together, browser sessions open one at a time.

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

Recovery branches preserve completed work and retry only the failed step. If canonical tracking fails after confirmed submission, show Submitted, tracking update pending and retry the write. Never reopen or resubmit the form as recovery.

## Uncertainty and Partial Filling

Uncertainty remains visible when it changes a decision. It states the unresolved fact, available evidence, likely impact, and required confirmation without converting ambiguity into a confident claim.

Form results are grouped as:

- Filled and verified.
- Needs your answer.
- Unsupported.
- Verification failed.

Safe fields continue even when another field needs the user. Partial completion is always Review required, never Ready.

## Dismissal

Dismiss immediately records Discarded through the career-ops adapter, removes the role from the active queue, and offers Undo. An optional Add reason action may store operational context in HereForWork and pass it through the adapter when career-ops supports it. The adapter does not become a recommendation engine, and the feedback must not silently change ranking.

## Adapter Boundary

career-ops remains the source of truth for verified profile facts, preferences, authorization-aware evaluation, reports, tailored documents, grounded answers, and canonical application history.

HereForWork owns scheduling, queue presentation, preparation orchestration, notifications, retries, browser-session state, optional dismissal context, and progress UX.

The adapter exposes typed, idempotent operations for discovery, normalized role reads, history checks, preparation, answer drafting from inspected fields, and canonical Discarded and Applied transitions. External job and form content crosses this boundary as untrusted data, never as instructions.

## Extension Boundary

The extension may inspect visible fields on approved ATS hosts, return structured field definitions, fill an explicit prepared payload, verify values, and report skipped or failed fields.

It may not generate answers independently, infer sensitive facts, update canonical tracking, click Submit, call `form.submit()`, simulate submission, or expand permissions without a separate decision.

The main app flow is built before extension coverage, but the MVP is not accepted until at least one real review-before-submit application completes through the integrated browser workflow. Initial planned coverage is Greenhouse, Lever, and Ashby. The proof-of-concept dataset may change the order or replace a low-value family with a more frequent one.

## Notifications

- New viable roles: open the queue filtered to the originating run.
- Actionable discovery failure: open Activity at the failed source.
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
| HereForWork          Queue   Applications   Activity           Updated 10:42|
+------------------------------------------------------------------------------+
| TODAY'S QUEUE                                                   12 READY     |
|                                                                              |
| STRONG MATCHES                                                               |
| [L] Senior Frontend Engineer   Linear - Remote Spain       ASHBY   [Prepare]|
| [R] Product Engineer           Ramp - Remote Europe   GREENHOUSE   [Prepare]|
| [N] Frontend Platform Engineer Notion - Madrid             LEVER   [Prepare]|
|                                                                              |
| NEW                                                                          |
| [C] Senior UI Engineer         Company - Barcelona         ASHBY   [Prepare]|
|                                                                              |
| NEEDS DECISION                                                               |
| [?] Frontend Engineer          Company - Remote EU                 [Prepare]|
|     Authorization route unclear                                             |
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
