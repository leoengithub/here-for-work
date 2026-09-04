# External Applied from Applications Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Let idle Applications rows record canonical career-ops **Applied** via **I applied elsewhere** when the user applied outside HereForWork (e.g. LinkedIn Easy Apply), without a browser session.

**Architecture:** New role-scoped Applied path reuses the existing career-ops Applied writer and `application.applied.confirm` adapter effect. Store gates idle prep + no active browser; on success retires local work and cancels preparation jobs so the row leaves Applications while **keeping** on-disk artifacts. Session-bound **I submitted this application** stays unchanged and hides the external CTA when outcome-confirmable.

**Tech Stack:** Rust/SQLite store (`src-tauri`), Tauri commands, React/`App.tsx` Applications UI, Vitest, career-ops adapter Applied writer (no career-ops source edits).

**Spec:** `docs/superpowers/specs/2026-09-04-external-applied-from-applications-design.md`

## Global Constraints

- Never submit or send an application.
- career-ops remains sole canonical Applied tracker; no second Applied store.
- Keep generated preparation artifact files on success (do not run Discard cleanup delete).
- Block while prep is `queued`/`preparing` or browser application work is active; user Cancels first.
- Hide **I applied elsewhere** when latest application session is `review_required` or `submitted_tracking_pending`.
- CTA copy: **I applied elsewhere**; confirm buttons **Keep role** / **Record Applied**.
- Reject command unless `user_confirmed == true`.
- Branch: `hfw-mark-applied-elsewhere`; draft PR to `main`; no merge without user OK.
- Do not weaken fact checks or invent Queue CTA.

## File map

| File | Responsibility |
| --- | --- |
| `src-tauri/src/store.rs` | `begin_applied_effect_for_role`, `complete_applied_effect_for_role`, eligibility checks, cancel prep jobs without file delete, `applied_tracking_pending` on dashboard prep rows |
| `src-tauri/src/domain.rs` | `PreparationSummary.applied_tracking_pending` |
| `src-tauri/src/lib.rs` | Tauri `mark_application_applied` |
| `src/types.ts` | `appliedTrackingPending` on `PreparationSummary` |
| `src/api.ts` | `markApplicationApplied(roleId)` |
| `src/App.tsx` | `canMarkAppliedElsewhere`, inline confirm UI (list + Details) |
| `src/App.test.tsx` | Gate + CTA + confirm invoke tests |
| `src/dev/applications-preview.ts` | Fixture row for external Applied CTA |
| `MVP_SHAPE.md` | Distill Applications external Applied vs Dismiss vs browser confirm |
| `evidence/HFW-MARK-APPLIED-01/README.md` | Sanitized ship evidence |

---

### Task 1: Store role-scoped Applied effect (TDD)

**Files:**
- Modify: `src-tauri/src/domain.rs` (`PreparationSummary`)
- Modify: `src-tauri/src/store.rs` (methods + dashboard query + tests near existing Applied tests ~10978)
- Test: `cargo test --manifest-path src-tauri/Cargo.toml --lib external_applied_`

**Interfaces:**
- Produces:
  - `Store::begin_applied_effect_for_role(&mut self, role_id: &str) -> Result<AdapterEffectContext, StoreError>`
  - `Store::complete_applied_effect_for_role(&mut self, role_id: &str, idempotency_key: &str, tracker_id: i64, canonical_status: &str) -> Result<(), StoreError>`
  - `PreparationSummary.applied_tracking_pending: bool` (serde `appliedTrackingPending`)

- [x] **Step 1: Add failing store tests**

Append near other Applied tests in `store.rs`:

```rust
#[test]
fn external_applied_rejects_active_preparation() {
    let directory = tempfile::tempdir().unwrap();
    let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
    import_evaluated(&mut store, DATASET);
    let role_id = store.dashboard().unwrap().roles[0].id.clone();
    store.begin_preparation(&role_id, "codex").unwrap();
    let err = store.begin_applied_effect_for_role(&role_id).unwrap_err();
    assert!(err.to_string().contains("Cancel active work first"));
}

#[test]
fn external_applied_rejects_outcome_confirmable_browser_session() {
    let directory = tempfile::tempdir().unwrap();
    let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
    import_evaluated(&mut store, DATASET);
    let role_id = store.dashboard().unwrap().roles[0].id.clone();
    let work = queue_and_claim(&mut store, &role_id, "codex");
    store
        .complete_preparation(
            &work.id,
            &PreparationCompletion {
                tracker_id: 42,
                report_path: "reports/042-example.md",
                report_hash: &"b".repeat(64),
                cv_pdf_path: "output/042-example/cv.pdf",
                cv_pdf_hash: &"c".repeat(64),
                cv_source: "tailored_generated",
            },
        )
        .unwrap();
    let session = store.queue_application_session(&work.id).unwrap();
    store
        .connection
        .execute(
            "UPDATE browser_sessions SET status = 'review_required' WHERE id = ?1",
            [&session.id],
        )
        .unwrap();
    let err = store.begin_applied_effect_for_role(&role_id).unwrap_err();
    assert!(err.to_string().contains("confirm the open application session"));
}

#[test]
fn external_applied_on_failed_prep_records_applied_and_leaves_applications() {
    let directory = tempfile::tempdir().unwrap();
    let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
    import_evaluated(&mut store, DATASET);
    let role_id = store.dashboard().unwrap().roles[0].id.clone();
    let first = queue_and_claim(&mut store, &role_id, "codex");
    store
        .fail_preparation_with_policy(
            &first.id,
            "cv_fact_check_failed",
            "stage.fact_verification",
            "CV fact check failed — unsupported metric-like claims: 8 years",
            "fresh_preparation_provider_run",
        )
        .unwrap();
    // Simulate canonical tracker present (eval already imported with tracker in DATASET path —
    // set explicitly if needed):
    store
        .connection
        .execute(
            "UPDATE roles SET canonical_tracker_id = 42, canonical_status = 'Evaluated' WHERE id = ?1",
            [&role_id],
        )
        .unwrap();

    let effect = store.begin_applied_effect_for_role(&role_id).unwrap();
    store
        .complete_applied_effect_for_role(&role_id, &effect.idempotency_key, 42, "Applied")
        .unwrap();

    let dashboard = store.dashboard().unwrap();
    assert!(dashboard.preparations.iter().all(|p| p.role_id != role_id));
    let status: String = store
        .connection
        .query_row(
            "SELECT canonical_status FROM roles WHERE id = ?1",
            [&role_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(status, "Applied");
    let prep_status: String = store
        .connection
        .query_row(
            "SELECT status FROM preparation_jobs WHERE id = ?1",
            [&first.id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(prep_status, "cancelled");
}

#[test]
fn external_applied_tracking_failure_sets_pending_flag_for_retry() {
    let directory = tempfile::tempdir().unwrap();
    let mut store = Store::open(directory.path().join("test.sqlite3")).unwrap();
    import_evaluated(&mut store, DATASET);
    let role_id = store.dashboard().unwrap().roles[0].id.clone();
    let first = queue_and_claim(&mut store, &role_id, "codex");
    store
        .fail_preparation(&first.id, "provider_failed", "provider.invoke", "Provider failed.")
        .unwrap();
    store
        .connection
        .execute(
            "UPDATE roles SET canonical_tracker_id = 42, canonical_status = 'Evaluated' WHERE id = ?1",
            [&role_id],
        )
        .unwrap();
    let effect = store.begin_applied_effect_for_role(&role_id).unwrap();
    store
        .fail_adapter_effect(&effect.idempotency_key, "canonical_write_failed")
        .unwrap();
    let row = store
        .dashboard()
        .unwrap()
        .preparations
        .into_iter()
        .find(|p| p.role_id == role_id)
        .unwrap();
    assert!(row.applied_tracking_pending);
    let retry = store.begin_applied_effect_for_role(&role_id).unwrap();
    assert_eq!(retry.idempotency_key, effect.idempotency_key);
}
```

Adjust helpers (`queue_and_claim`, `DATASET` tracker) to match existing test fixtures in the same module — if `canonical_tracker_id` is already set by `import_evaluated`, drop the explicit UPDATE.

- [x] **Step 2: Run tests — expect FAIL**

Run: `corepack pnpm build:adapter && corepack pnpm build:extension && cargo test --manifest-path src-tauri/Cargo.toml --lib external_applied_`

Expected: compile error / FAIL — methods missing.

- [x] **Step 3: Implement store methods**

Add to `PreparationSummary` in `domain.rs`:

```rust
pub applied_tracking_pending: bool,
```

In `store.rs` dashboard preparation query, select:

```sql
EXISTS (
  SELECT 1 FROM adapter_effects e
   WHERE e.role_id = p.role_id
     AND e.operation = 'application.applied.confirm'
     AND e.status IN ('pending', 'action_required')
) AS applied_tracking_pending
```

Map into `PreparationSummary.applied_tracking_pending`.

Implement `begin_applied_effect_for_role`:

1. Load role `canonical_tracker_id`, `canonical_status`.
2. Error if missing tracker or status is Applied/Discarded/Rejected.
3. Error if discard effect pending (same as session path).
4. Error if latest non-cancelled prep status ∈ (`queued`, `preparing`) → `"Cancel active work first"`.
5. Error if no idle prep (`action_required` or `completed`) for role.
6. Error if any application browser session for role has status in active set OR `review_required` / `submitted_tracking_pending`:
   - active: `waiting_for_extension`, `inspecting`, `drafting_answers`, `answering`, `filling`, `persisting_answers`, `saving_answers`, `releasing`, `connection_verified`
   - if `review_required` / `submitted_tracking_pending` → `"confirm the open application session instead"`
7. If completed Applied effect already exists for tracker → return that context (idempotent read like session path).
8. If `action_required` effect exists for `application.applied.confirm` on role → reopen as pending (or return existing key after resetting to pending) for retry — match how session retry reuses effect; simplest: `UPDATE ... SET status = 'pending'` on the action_required row and return it, or `begin_adapter_effect` only when none pending/action_required.
9. Else `begin_adapter_effect(role_id, "application.applied.confirm", None, Some(tracker_id))`.

Implement `complete_applied_effect_for_role`:

1. Require `canonical_status == "Applied"`.
2. Complete pending adapter effect (same SQL pattern as `complete_applied_effect`).
3. Set role `canonical_status = Applied`, date, tracker id.
4. Set `evaluation_sync` terminal `canonical_terminal`.
5. Retire application browser sessions for role to `applied_recorded` (reuse SQL from `retire_role_work_for_applied` / lines ~3407–3414).
6. Cancel **all** non-cancelled `preparation_jobs` for role (`status = 'cancelled'`) — **do not** delete artifact files or call discard cleanup.
7. Set `roles.preparation_state` to `prepared` if any job was previously `completed`, else `not_started`.
8. Insert activity: `"The user confirmed an external application; career-ops recorded the canonical Applied outcome."`

- [x] **Step 4: Run tests — expect PASS**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib external_applied_`

Expected: PASS

- [x] **Step 5: Commit**

```bash
git add src-tauri/src/domain.rs src-tauri/src/store.rs
git commit -m "$(cat <<'EOF'
feat: add role-scoped external Applied store path

Gate idle Applications roles, complete career-ops Applied without deleting prep files, and cancel jobs so the row leaves Applications.
EOF
)"
```

---

### Task 2: Tauri command + frontend API

**Files:**
- Modify: `src-tauri/src/lib.rs`
- Modify: `src/api.ts`
- Consumes: Task 1 store methods
- Produces: `mark_application_applied` command; `markApplicationApplied(roleId): Promise<DashboardState>`

- [x] **Step 1: Add Tauri command**

Mirror `confirm_application_applied` but role-scoped and return dashboard:

```rust
#[tauri::command]
fn mark_application_applied(
    role_id: String,
    user_confirmed: bool,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<DashboardState, String> {
    if !user_confirmed {
        return Err("Applied requires your explicit outcome confirmation.".to_string());
    }
    let context = {
        let mut store = state.store.lock().map_err(|_| "Operational store lock was poisoned".to_string())?;
        store.begin_applied_effect_for_role(&role_id).map_err(|e| e.to_string())?
    };
    let tracker_id = context.tracker_id.ok_or("Canonical tracker row is missing")?;
    let input = CanonicalRoleInput {
        idempotency_key: context.idempotency_key.clone(),
        event_date: madrid_today(),
        company: context.role.company,
        title: context.role.title,
        location: context.role.location,
        url: context.role.application_url,
    };
    let effect = {
        let _canonical_write = state.canonical_write_lock.lock().map_err(|_| "Canonical writer lock was poisoned".to_string())?;
        match state.adapter.confirm_applied(&input, tracker_id) {
            Ok(effect) => effect,
            Err(error) => {
                if let Ok(mut store) = state.store.lock() {
                    let _ = store.fail_adapter_effect(&context.idempotency_key, "canonical_write_failed");
                }
                let _ = app.notification().builder()
                    .title("Tracking update pending")
                    .body("Your external application is not repeated. Open HereForWork to retry only the career-ops update.")
                    .show();
                return Err(error.to_string());
            }
        }
    };
    let mut store = state.store.lock().map_err(|_| "Operational store lock was poisoned".to_string())?;
    store
        .complete_applied_effect_for_role(&role_id, &context.idempotency_key, effect.tracker_id, &effect.status)
        .map_err(|e| e.to_string())?;
    store.dashboard().map_err(|e| e.to_string())
}
```

Register in `invoke_handler![..., mark_application_applied, ...]`.

- [x] **Step 2: Add `src/api.ts` wrapper**

```ts
export async function markApplicationApplied(roleId: string): Promise<DashboardState> {
  if (!isTauri()) throw new Error("External Applied confirmation is available in the desktop app.");
  return invoke<DashboardState>("mark_application_applied", { roleId, userConfirmed: true });
}
```

- [x] **Step 3: Type field on frontend**

In `src/types.ts` `PreparationSummary`:

```ts
appliedTrackingPending: boolean;
```

Update preview fixtures / any object literals constructing `PreparationSummary` to include `appliedTrackingPending: false` (or true for tracking-pending fixture).

- [x] **Step 4: Commit**

```bash
git add src-tauri/src/lib.rs src/api.ts src/types.ts src/dev/applications-preview.ts
git commit -m "$(cat <<'EOF'
feat: expose mark_application_applied command

Wire Tauri + API for role-scoped external Applied with userConfirmed gate.
EOF
)"
```

---

### Task 3: Frontend eligibility helper (TDD)

**Files:**
- Modify: `src/App.tsx` (export helper)
- Modify: `src/App.test.tsx`
- Consumes: `PreparationSummary.appliedTrackingPending`, browser session statuses
- Produces: `canMarkAppliedElsewhere(item, latestSession, opts) -> boolean`

- [x] **Step 1: Write failing Vitest cases**

```ts
import { canMarkAppliedElsewhere } from "./App";

it("allows I applied elsewhere only for idle Applications rows without outcome session", () => {
  expect(canMarkAppliedElsewhere(
    { ...preparationFixture, status: "action_required" },
    undefined,
    { recordingApplication: false, dismissing: false },
  )).toBe(true);
  expect(canMarkAppliedElsewhere(
    { ...preparationFixture, status: "completed" },
    undefined,
    { recordingApplication: false, dismissing: false },
  )).toBe(true);
  expect(canMarkAppliedElsewhere(
    { ...preparationFixture, status: "queued" },
    undefined,
    { recordingApplication: false, dismissing: false },
  )).toBe(false);
  expect(canMarkAppliedElsewhere(
    { ...preparationFixture, status: "completed" },
    { ...browserSessionFixture, status: "review_required" },
    { recordingApplication: false, dismissing: false },
  )).toBe(false);
  expect(canMarkAppliedElsewhere(
    { ...preparationFixture, status: "completed" },
    { ...browserSessionFixture, status: "filling" },
    { recordingApplication: false, dismissing: false },
  )).toBe(false);
  expect(canMarkAppliedElsewhere(
    { ...preparationFixture, status: "action_required", appliedTrackingPending: true },
    undefined,
    { recordingApplication: false, dismissing: false },
  )).toBe(true); // still eligible; label becomes Retry tracking update in UI
});
```

Ensure `preparationFixture` includes `appliedTrackingPending: false`.

- [x] **Step 2: Run — expect FAIL**

Run: `corepack pnpm exec vitest run src/App.test.tsx -t "I applied elsewhere"`

Expected: FAIL — export missing.

- [x] **Step 3: Implement helper**

```ts
const ACTIVE_APPLICATION_BROWSER = new Set<BrowserSessionSummary["status"]>([
  "waiting_for_extension",
  "inspecting",
  "drafting_answers",
  "answering",
  "filling",
  "persisting_answers",
  "saving_answers",
  "releasing",
  "connection_verified",
]);

export function canMarkAppliedElsewhere(
  item: PreparationSummary,
  latestSession: BrowserSessionSummary | undefined,
  options: { recordingApplication: boolean; dismissing: boolean },
): boolean {
  if (options.recordingApplication || options.dismissing) return false;
  if (item.status !== "action_required" && item.status !== "completed") return false;
  if (!latestSession) return true;
  if (latestSession.status === "review_required" || latestSession.status === "submitted_tracking_pending") {
    return false;
  }
  if (ACTIVE_APPLICATION_BROWSER.has(latestSession.status)) return false;
  if (latestSession.status === "applied_recorded") return false;
  return true;
}
```

- [x] **Step 4: Run — expect PASS**

Run: `corepack pnpm exec vitest run src/App.test.tsx -t "I applied elsewhere"`

- [x] **Step 5: Commit**

```bash
git add src/App.tsx src/App.test.tsx
git commit -m "$(cat <<'EOF'
feat: gate I applied elsewhere on idle Applications rows

Hide external Applied when prep/browser work is active or session confirm applies.
EOF
)"
```

---

### Task 4: Applications list + Details UI

**Files:**
- Modify: `src/App.tsx`
- Modify: `src/App.test.tsx`
- Modify: `src/dev/applications-preview.ts`
- Consumes: `canMarkAppliedElsewhere`, `markApplicationApplied`

- [x] **Step 1: Fixture + failing UI test**

In `applications-preview.ts`, ensure failed idle row exists; add `appliedTrackingPending: false` on all prep fixtures.

Add test:

```ts
it("records external Applied from Applications inline confirm", async () => {
  const after = {
    ...applicationsPreviewDashboard,
    preparations: applicationsPreviewDashboard.preparations.filter((p) => p.id !== "failed"),
  };
  const invoke = vi.fn(async (command: string, args?: { roleId?: string }) => {
    if (command === "get_dashboard") return applicationsPreviewDashboard;
    if (command === "get_cv_fallback_setting") return { path: null, sha256: null };
    if (command === "take_in_app_outcome_notifications") return [];
    if (command === "get_browser_setup") return applicationsPreviewBrowserSetup;
    if (command === "get_browser_sessions") return applicationsPreviewSessions;
    if (command === "mark_application_applied") {
      expect(args?.roleId).toBeTruthy();
      return after;
    }
    throw new Error(`Unexpected command: ${command}`);
  });
  (window as typeof window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = { invoke };
  window.history.replaceState({}, "", "/?application-preview=states");
  const { container } = render(<App />);
  await screen.findByRole("heading", { name: "Review queue" });
  fireEvent.click(screen.getByRole("tab", { name: "applications" }));
  const failedRow = container.querySelector<HTMLElement>('[data-preparation-id="failed"]')!;
  fireEvent.click(within(failedRow).getByRole("button", { name: "I applied elsewhere" }));
  expect(within(failedRow).getByRole("alert")).toHaveTextContent(/Record Applied in career-ops/i);
  fireEvent.click(within(failedRow).getByRole("button", { name: "Record Applied" }));
  await waitFor(() => expect(container.querySelector('[data-preparation-id="failed"]')).not.toBeInTheDocument());
  expect(invoke).toHaveBeenCalledWith(
    "mark_application_applied",
    { roleId: expect.any(String), userConfirmed: true },
    undefined,
  );
});
```

Note: preview `failed` row `roleId` must match invoke expectation — read fixture helper for role id pattern.

- [x] **Step 2: Run — expect FAIL** (CTA missing)

- [x] **Step 3: Implement UI**

State (mirror dismiss):

```ts
const [markAppliedPreparationId, setMarkAppliedPreparationId] = useState<string | null>(null);
const [markingAppliedRoleId, setMarkingAppliedRoleId] = useState<string | null>(null);
const [markAppliedError, setMarkAppliedError] = useState<string | null>(null);
```

In Applications row actions, when `canMarkAppliedElsewhere(...)`:

- Button label: `item.appliedTrackingPending ? "Retry tracking update" : "I applied elsewhere"`
- Opens inline `Alert` (non-destructive variant OK; not Discarded):
  - Title: `Record Applied?`
  - Body: `This records {title} as Applied in career-ops. Preparation files stay on disk. HereForWork retires local work and removes the role from Applications.`
  - **Keep role** / **Record Applied** (or **Retry tracking update** when pending)

On confirm:

```ts
const dashboard = await markApplicationApplied(item.roleId);
setDashboard(dashboard);
// refresh browser sessions if needed
```

Also add same CTA + confirm block in preparation Details when eligible (hide when session confirm buttons show).

Toast on success optional: reuse existing outcome notice pattern if one exists for Applied; otherwise rely on row removal.

- [x] **Step 4: Run Vitest App suite**

Run: `corepack pnpm exec vitest run src/App.test.tsx`

Expected: PASS

- [x] **Step 5: Commit**

```bash
git add src/App.tsx src/App.test.tsx src/dev/applications-preview.ts
git commit -m "$(cat <<'EOF'
feat: add I applied elsewhere to Applications

Inline confirm records external Applied and clears the Applications row.
EOF
)"
```

---

### Task 5: Product docs + evidence stub

**Files:**
- Modify: `MVP_SHAPE.md` (Dismissal / Applications section)
- Create: `evidence/HFW-MARK-APPLIED-01/README.md`
- Modify: `docs/superpowers/specs/2026-09-04-external-applied-from-applications-design.md` (status → implemented-in-progress)

- [x] **Step 1: Update MVP_SHAPE.md**

After the Applications Dismiss paragraph, add:

```markdown
**I applied elsewhere in Applications** records Applied through the career-ops adapter when the user submitted outside HereForWork (for example LinkedIn Easy Apply). It is available on idle Applications rows (`action_required` or `completed`) when no application browser session is active or awaiting outcome confirmation. After inline confirmation it keeps generated preparation files, retires HereForWork preparation and browser work, cancels the Applications row, and never submits a form. A failed canonical write preserves the row for tracking retry only. Prefer **I submitted this application** when a live HereForWork browser session is waiting for outcome confirmation.
```

- [x] **Step 2: Evidence README**

```markdown
# HFW-MARK-APPLIED-01 — External Applied from Applications

## Intent
Idle Applications → **I applied elsewhere** → career-ops Applied; artifacts kept; row leaves Applications.

## Verification
- Store `external_applied_*` tests
- Vitest Applications CTA + confirm
- Manual: idle failed prep role → Record Applied → tracker Applied; files remain under career-ops output/

Do not paste personal CV content or full tracker notes here.
```

- [x] **Step 3: Commit**

```bash
git add MVP_SHAPE.md evidence/HFW-MARK-APPLIED-01/README.md docs/superpowers/specs/2026-09-04-external-applied-from-applications-design.md
git commit -m "$(cat <<'EOF'
docs: distill external Applied Applications behavior

Document I applied elsewhere vs Dismiss vs browser confirm and add evidence stub.
EOF
)"
```

---

### Task 6: Draft PR (no merge)

**Files:** none beyond branch push

- [x] **Step 1: Push + draft PR**

```bash
git push -u origin HEAD
gh pr create --draft --title "feat: mark Applied from Applications (I applied elsewhere)" --body "$(cat <<'EOF'
## Summary
- Role-scoped `mark_application_applied` for idle Applications rows after external submit (e.g. LinkedIn Easy Apply).
- Keeps prep artifacts; cancels Applications row; reuses career-ops Applied writer.
- Hides CTA when browser **I submitted this application** applies; blocks while prep/browser active.

## Test plan
- [x] Rust `external_applied_*`
- [x] Vitest Applications CTA
- [x] Manual: failed/idle prep → I applied elsewhere → Applied in career-ops; files remain

EOF
)"
```

- [x] **Step 2: Stop for user acceptance** — no Desktop ship unless user asks; no merge without explicit OK.

---

## Spec coverage self-review

| Spec requirement | Task |
| --- | --- |
| Applications-only CTA | 4 |
| Idle gates + block active | 1, 3 |
| Keep artifacts | 1 (`complete_applied_effect_for_role` no delete) |
| Leave Applications | 1 (cancel prep jobs) |
| Inline confirm + copy | 4 |
| Hide when session confirm shown | 3, 4 |
| Reuse career-ops Applied writer | 2 |
| `userConfirmed` | 2 |
| Tracking failure retry | 1 (`applied_tracking_pending`), 4 |
| MVP_SHAPE distill | 5 |
| Evidence | 5 |
| No Queue CTA / no submit | Global Constraints |

## Placeholder scan

None intentional. Test fixtures may need small adjustment to `DATASET` tracker fields — implementer must align with existing `import_evaluated` behavior rather than inventing a second tracker.
