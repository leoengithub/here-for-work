import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  App,
  BrowserSessions,
  PreQueueStatus,
  QueueOperationalStatus,
  RoleRow,
  deriveApplicationState,
  deriveQueueOperationalState,
  preparationRecoveryAction,
} from "./App";
import type { BrowserSessionSummary, PreparationSummary } from "./types";
import {
  applicationsPreviewBrowserSetup,
  applicationsPreviewDashboard,
  applicationsPreviewSessions,
  getQueuePreviewDashboard,
} from "./dev/applications-preview";

afterEach(() => {
  cleanup();
  window.history.replaceState({}, "", "/");
  delete (window as typeof window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
  vi.restoreAllMocks();
});

const preparationFixture: PreparationSummary = {
  id: "preparation-1",
  roleId: "role-1",
  company: "Acme",
  title: "Frontend Engineer",
  provider: "codex",
  status: "completed",
  step: "completed",
  attempt: 1,
  reportPath: "reports/1.md",
  cvPdfPath: "output/1/cv.pdf",
  cvSource: "tailored_generated",
  errorClass: null,
  errorStage: null,
  errorDetail: null,
  retryPolicy: null,
  updatedAt: "2026-09-01T12:00:00Z",
  appliedTrackingPending: false,
};

const browserSessionFixture: BrowserSessionSummary = {
  id: "session-1",
  purpose: "application",
  roleId: "role-1",
  preparationId: "preparation-1",
  status: "review_required",
  ats: "ashby",
  pageTitle: "Frontend Engineer",
  pageUrl: "https://example.test/application",
  snapshotFingerprint: "a".repeat(64),
  fieldCount: 3,
  safeFieldCount: 2,
  needsUserCount: 1,
  errorCode: null,
  reviewItems: null,
  fillResults: null,
  updatedAt: "2026-09-01T12:00:00Z",
};

describe("App", () => {
  it("derives terminal and attention states before stale preparation steps", () => {
    expect(deriveApplicationState({
      ...preparationFixture,
      status: "action_required",
      step: "preparing_cv",
      errorClass: "artifact_commit_failed",
    }, undefined, false)).toMatchObject({
      label: "Preparation failed",
      tone: "attention",
    });
    expect(deriveApplicationState(preparationFixture, browserSessionFixture, false)).toMatchObject({
      label: "Ready for review",
      tone: "success",
    });
    expect(deriveApplicationState(preparationFixture, {
      ...browserSessionFixture,
      status: "applied_recorded",
    }, false)).toMatchObject({
      label: "Application recorded",
      tone: "success",
    });
    expect(deriveApplicationState({
      ...preparationFixture,
      status: "action_required",
      step: "preparing_cv",
      errorClass: "app_interrupted",
    }, {
      ...browserSessionFixture,
      status: "applied_recorded",
    }, false)).toMatchObject({
      label: "Application recorded",
      tone: "success",
    });
  });

  it("uses explicit waiting, progress, and tracking recovery states", () => {
    expect(deriveApplicationState({ ...preparationFixture, status: "queued", step: "queued" }, undefined, false).label)
      .toBe("Waiting");
    expect(deriveApplicationState({ ...preparationFixture, status: "preparing", step: "preparing_cv" }, undefined, false))
      .toMatchObject({ label: "Preparing CV", active: true });
    expect(deriveApplicationState(preparationFixture, {
      ...browserSessionFixture,
      status: "waiting_for_extension",
    }, false)).toMatchObject({ label: "Opening form", active: true });
    expect(deriveApplicationState(preparationFixture, {
      ...browserSessionFixture,
      status: "filling",
    }, false)).toMatchObject({ label: "Filling form", active: true });
    expect(deriveApplicationState(preparationFixture, browserSessionFixture, true))
      .toMatchObject({ label: "Recording application…", active: true });
    expect(deriveApplicationState(preparationFixture, {
      ...browserSessionFixture,
      status: "submitted_tracking_pending",
      errorCode: "canonical_write_failed",
    }, false)).toMatchObject({ label: "Tracking update failed", tone: "attention" });
  });

  it("exposes Prepare again for fresh preparation policies and keeps same-id retry separate", () => {
    expect(preparationRecoveryAction("action_required", "retry_same_preparation")).toBe("retry");
    expect(preparationRecoveryAction("action_required", "repair_runtime_then_retry")).toBe("retry");
    expect(preparationRecoveryAction("action_required", "fresh_preparation_provider_run")).toBe("prepare_again");
    expect(preparationRecoveryAction("action_required", "fresh_preparation_id")).toBe("prepare_again");
    expect(preparationRecoveryAction("action_required", "manual_repair_required")).toBeNull();
    expect(preparationRecoveryAction("action_required", "fresh_preparation_provider_run", "undo_cleanup")).toBeNull();
    expect(preparationRecoveryAction("preparing", "fresh_preparation_provider_run")).toBeNull();
  });

  it("renders Prepare again for fact-check recovery in Applications", async () => {
    const dashboard = {
      ...applicationsPreviewDashboard,
      preparations: applicationsPreviewDashboard.preparations.map((item) => (
        item.id === "failed"
          ? {
            ...item,
            errorClass: "cv_fact_check_failed",
            errorStage: "stage.fact_verification",
            errorDetail: "CV fact check failed — unsupported metric-like claims: 8 years",
            retryPolicy: "fresh_preparation_provider_run",
          }
          : item
      )),
    };
    const invoke = vi.fn(async (command: string) => {
      if (command === "get_dashboard") return dashboard;
      if (command === "get_cv_fallback_setting") return { path: null, sha256: null };
      if (command === "take_in_app_outcome_notifications") return [];
      if (command === "get_browser_setup") return applicationsPreviewBrowserSetup;
      if (command === "get_browser_sessions") return applicationsPreviewSessions;
      throw new Error(`Unexpected command: ${command}`);
    });
    (window as typeof window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = { invoke };
    const { container } = render(<App />);
    await screen.findByRole("heading", { name: "Review queue" });
    fireEvent.click(screen.getByRole("tab", { name: "applications" }));
    const failedRow = container.querySelector<HTMLElement>('[data-preparation-id="failed"]')!;
    expect(within(failedRow).getByRole("button", { name: "Prepare again" })).toBeEnabled();
    expect(within(failedRow).queryByRole("button", { name: "Retry preparation" })).not.toBeInTheDocument();
    expect(failedRow).toHaveTextContent("Prepare again starts a fresh provider run");
  });

  it("renders truthful fixture states and direct recovery actions", async () => {
    window.history.replaceState({}, "", "/?application-preview=states");
    const { container } = render(<App />);
    await screen.findByRole("heading", { name: "Review queue" });
    fireEvent.click(screen.getByRole("tab", { name: "applications" }));

    expect(await screen.findAllByText("Preparation failed")).toHaveLength(2);
    expect(screen.getByText("Waiting")).toBeInTheDocument();
    expect(screen.getByText("Preparing CV")).toBeInTheDocument();
    expect(screen.getByText("Ready for review")).toBeInTheDocument();
    expect(screen.getByText("Recording application…")).toBeInTheDocument();
    expect(screen.getByText("Application recorded")).toBeInTheDocument();
    expect(screen.getByText("Tracking update failed")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Retry preparation" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Prepare again" })).toBeEnabled();
    expect(screen.getAllByRole("button", { name: "Reopen and refill" })).toHaveLength(2);
    expect(screen.getAllByRole("button", { name: "Reopen and refill" })[0]).toBeEnabled();
    expect(screen.getAllByRole("button", { name: "Cancel and return to Queue" })).toHaveLength(2);
    expect(container.querySelector('[data-preparation-id="failed"]')).not.toHaveTextContent("preparing cv");
    expect(container.querySelector('[data-preparation-id="recording"]')).not.toHaveTextContent("Undo preparation");
    expect(container.querySelector('[data-preparation-id="tracking"]')).not.toHaveTextContent("Undo preparation");
    expect(within(container.querySelector<HTMLElement>('[data-preparation-id="failed"]')!).getByRole("button", { name: "Dismiss" })).toBeEnabled();
    expect(within(container.querySelector<HTMLElement>('[data-preparation-id="fact-check-failed"]')!).getByRole("button", { name: "Prepare again" })).toBeEnabled();
    expect(within(container.querySelector<HTMLElement>('[data-preparation-id="review"]')!).getByRole("button", { name: "Dismiss" })).toBeEnabled();
    for (const preparationId of ["waiting", "preparing", "recording", "tracking", "recorded"]) {
      expect(within(container.querySelector<HTMLElement>(`[data-preparation-id="${preparationId}"]`)!).queryByRole("button", { name: "Dismiss" }))
        .not.toBeInTheDocument();
    }
  });

  it("opens a row-scoped inline dismissal confirmation and returns focus safely", async () => {
    window.history.replaceState({}, "", "/?application-preview=states");
    const { container } = render(<App />);
    await screen.findByRole("heading", { name: "Review queue" });
    fireEvent.click(screen.getByRole("tab", { name: "applications" }));
    const failedRow = container.querySelector<HTMLElement>('[data-preparation-id="failed"]')!;

    fireEvent.click(within(failedRow).getByRole("button", { name: "Dismiss" }));

    const confirmation = within(failedRow).getByRole("alert");
    expect(confirmation).toHaveTextContent("Dismiss this role?");
    expect(confirmation).toHaveTextContent(
      "This marks Senior React Frontend Developer as Discarded in career-ops, permanently deletes its generated preparation files, and removes it from Applications.",
    );
    expect(within(confirmation).getByRole("button", { name: "Keep role" })).toBeEnabled();
    expect(within(confirmation).getByRole("button", { name: "Dismiss role" })).toBeEnabled();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    await waitFor(() => expect(within(confirmation).getByRole("button", { name: "Keep role" })).toHaveFocus());

    fireEvent.click(within(confirmation).getByRole("button", { name: "Keep role" }));
    expect(within(failedRow).queryByRole("alert")).not.toBeInTheDocument();
    await waitFor(() => expect(within(failedRow).getByRole("button", { name: "Dismiss" })).toHaveFocus());
  });

  it("shows dismissal progress, disables conflicting actions, and removes the successful row", async () => {
    let finishDismissal: (() => void) | undefined;
    let dismissed = false;
    const dashboardAfterDismissal = {
      ...applicationsPreviewDashboard,
      preparations: applicationsPreviewDashboard.preparations.filter((item) => item.id !== "failed"),
    };
    const invoke = vi.fn(async (command: string) => {
      if (command === "get_dashboard") return dismissed ? dashboardAfterDismissal : applicationsPreviewDashboard;
      if (command === "get_cv_fallback_setting") return { path: null, sha256: null };
      if (command === "take_in_app_outcome_notifications") return [];
      if (command === "get_browser_setup") return applicationsPreviewBrowserSetup;
      if (command === "get_browser_sessions") return applicationsPreviewSessions;
      if (command === "dismiss_preparation") {
        return new Promise((resolve) => {
          finishDismissal = () => {
            dismissed = true;
            resolve(dashboardAfterDismissal);
          };
        });
      }
      throw new Error(`Unexpected command: ${command}`);
    });
    (window as typeof window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = { invoke };
    const { container } = render(<App />);
    await screen.findByRole("heading", { name: "Review queue" });
    fireEvent.click(screen.getByRole("tab", { name: "applications" }));
    const failedRow = container.querySelector<HTMLElement>('[data-preparation-id="failed"]')!;
    fireEvent.click(within(failedRow).getByRole("button", { name: "Dismiss" }));
    const dismissRole = within(failedRow).getByRole("button", { name: "Dismiss role" });
    dismissRole.focus();
    fireEvent.click(dismissRole);

    expect(within(failedRow).getByRole("button", { name: "Dismissing…" })).toBeDisabled();
    expect(within(failedRow).getByRole("button", { name: "Retry preparation" })).toBeDisabled();
    expect(invoke).toHaveBeenCalledWith("dismiss_preparation", { preparationId: "failed" }, undefined);

    await act(async () => finishDismissal?.());
    await waitFor(() => expect(container.querySelector('[data-preparation-id="failed"]')).not.toBeInTheDocument());
    expect(await screen.findByText(/Role dismissed\. career-ops was updated/)).toBeInTheDocument();
  });

  it("preserves a failed dismissal inline and retries the same visible preparation", async () => {
    let attempts = 0;
    const dashboardAfterDismissal = {
      ...applicationsPreviewDashboard,
      preparations: applicationsPreviewDashboard.preparations.filter((item) => item.id !== "failed"),
    };
    const invoke = vi.fn(async (command: string) => {
      if (command === "get_dashboard") return attempts > 1 ? dashboardAfterDismissal : applicationsPreviewDashboard;
      if (command === "get_cv_fallback_setting") return { path: null, sha256: null };
      if (command === "take_in_app_outcome_notifications") return [];
      if (command === "get_browser_setup") return applicationsPreviewBrowserSetup;
      if (command === "get_browser_sessions") return applicationsPreviewSessions;
      if (command === "dismiss_preparation") {
        attempts += 1;
        if (attempts === 1) throw new Error("canonical writer unavailable");
        return dashboardAfterDismissal;
      }
      throw new Error(`Unexpected command: ${command}`);
    });
    (window as typeof window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = { invoke };
    const { container } = render(<App />);
    await screen.findByRole("heading", { name: "Review queue" });
    fireEvent.click(screen.getByRole("tab", { name: "applications" }));
    const failedRow = container.querySelector<HTMLElement>('[data-preparation-id="failed"]')!;
    fireEvent.click(within(failedRow).getByRole("button", { name: "Dismiss" }));
    const firstDismissal = within(failedRow).getByRole("button", { name: "Dismiss role" });
    await waitFor(() => expect(within(failedRow).getByRole("button", { name: "Keep role" })).toHaveFocus());
    firstDismissal.focus();
    fireEvent.click(firstDismissal);

    expect(await within(failedRow).findByText(/Dismiss did not finish/)).toBeInTheDocument();
    expect(container.querySelector('[data-preparation-id="failed"]')).toBeInTheDocument();
    const retryDismissal = within(failedRow).getByRole("button", { name: "Dismiss role" });
    expect(retryDismissal).toHaveFocus();
    fireEvent.click(retryDismissal);

    await waitFor(() => expect(container.querySelector('[data-preparation-id="failed"]')).not.toBeInTheDocument());
    expect(attempts).toBe(2);
  });

  it("records an already-submitted application with one click and no browser confirmation gate", async () => {
    let applied = false;
    const invoke = vi.fn(async (command: string) => {
      if (command === "get_dashboard") return applicationsPreviewDashboard;
      if (command === "get_cv_fallback_setting") return { path: null, sha256: null };
      if (command === "take_in_app_outcome_notifications") return [];
      if (command === "get_browser_setup") return applicationsPreviewBrowserSetup;
      if (command === "get_browser_sessions") {
        return applicationsPreviewSessions
          .filter((session) => session.id === "session-review")
          .map((session) => applied ? { ...session, status: "applied_recorded" } : session);
      }
      if (command === "confirm_application_applied") {
        applied = true;
        return { ...applicationsPreviewSessions[0], status: "applied_recorded" };
      }
      throw new Error(`Unexpected command: ${command}`);
    });
    (window as typeof window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = { invoke };
    const confirm = vi.spyOn(window, "confirm");

    render(<App />);
    await screen.findByRole("heading", { name: "Review queue" });
    fireEvent.click(screen.getByRole("tab", { name: "applications" }));
    fireEvent.click(await screen.findByRole("button", { name: "I submitted this application" }));

    expect(await screen.findByText("Application recorded")).toBeInTheDocument();
    expect(confirm).not.toHaveBeenCalled();
    expect(invoke).toHaveBeenCalledWith(
      "confirm_application_applied",
      { sessionId: "session-review", userConfirmed: true },
      undefined,
    );
  });

  it("keeps post-submit recovery tracking-only and leaves recorded applications terminal", async () => {
    window.history.replaceState({}, "", "/?application-preview=states");
    const { container } = render(<App />);
    await screen.findByRole("heading", { name: "Review queue" });
    fireEvent.click(screen.getByRole("tab", { name: "applications" }));
    await screen.findByText("Tracking update failed");
    await screen.findByText("Application recorded");

    const trackingRow = container.querySelector<HTMLElement>('[data-preparation-id="tracking"]')!;
    expect(within(trackingRow).getByText("Tracking update failed")).toBeInTheDocument();
    expect(within(trackingRow).getByText("The form is not touched again. Retry only the career-ops tracking update.")).toBeInTheDocument();
    expect(within(trackingRow).getByRole("button", { name: "Retry tracking update" })).toBeEnabled();
    expect(within(trackingRow).queryByRole("button", { name: "Reopen and refill" })).not.toBeInTheDocument();
    expect(within(trackingRow).queryByRole("button", { name: "Dismiss" })).not.toBeInTheDocument();
    expect(within(trackingRow).queryByRole("button", { name: "I submitted this application" })).not.toBeInTheDocument();

    const recordedRow = container.querySelector<HTMLElement>('[data-preparation-id="recorded"]')!;
    expect(within(recordedRow).getByText("Application recorded")).toBeInTheDocument();
    expect(within(recordedRow).getByRole("button", { name: "Details" })).toBeEnabled();
    expect(within(recordedRow).getByRole("button", { name: "Open tailored CV" })).toBeEnabled();
    expect(within(recordedRow).queryByRole("button", { name: "Retry tracking update" })).not.toBeInTheDocument();
    expect(within(recordedRow).queryByRole("button", { name: "Reopen and refill" })).not.toBeInTheDocument();
    expect(within(recordedRow).queryByRole("button", { name: "Dismiss" })).not.toBeInTheDocument();
    expect(within(recordedRow).queryByRole("button", { name: "I submitted this application" })).not.toBeInTheDocument();
  });

  it("starts an explicit refill attempt from a ready application", async () => {
    const invoke = vi.fn(async (command: string) => {
      if (command === "get_dashboard") return applicationsPreviewDashboard;
      if (command === "get_cv_fallback_setting") return { path: null, sha256: null };
      if (command === "take_in_app_outcome_notifications") return [];
      if (command === "get_browser_setup") return applicationsPreviewBrowserSetup;
      if (command === "get_browser_sessions") {
        return applicationsPreviewSessions.filter((session) => session.id === "session-review");
      }
      if (command === "reopen_application_form") {
        return { ...browserSessionFixture, id: "session-refill", status: "waiting_for_extension" };
      }
      throw new Error(`Unexpected command: ${command}`);
    });
    (window as typeof window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = { invoke };

    render(<App />);
    await screen.findByRole("heading", { name: "Review queue" });
    fireEvent.click(screen.getByRole("tab", { name: "applications" }));
    fireEvent.click(await screen.findByRole("button", { name: "Reopen and refill" }));

    expect(invoke).toHaveBeenCalledWith(
      "reopen_application_form",
      { preparationId: "review" },
      undefined,
    );
  });
  it("shows the operational empty state in browser preview", async () => {
    render(<App />);
    expect(await screen.findByRole("heading", { name: "Review queue" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Your review queue is ready for its first run." })).toBeInTheDocument();
    expect(screen.queryByRole("combobox", { name: "Preparation provider" })).not.toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: "Import discovery snapshot" })).toHaveLength(2);
    expect(screen.getByRole("button", { name: "Settings" })).toBeEnabled();
  });

  it("routes typed discovery envelopes while preserving the legacy importer", async () => {
    const invoke = vi.fn(async (command: string) => {
      if (command === "get_dashboard") return applicationsPreviewDashboard;
      if (command === "get_cv_fallback_setting") return { path: null, sha256: null };
      if (command === "take_in_app_outcome_notifications") return [];
      if (command === "get_browser_setup") return applicationsPreviewBrowserSetup;
      if (command === "get_browser_sessions") return applicationsPreviewSessions;
      if (command === "import_discovery_run") return {
        imported: 0, updated: 0, unchanged: 0, replayed: false, recorded: true, cursorAdvanced: false,
      };
      if (command === "import_dataset") return { imported: 0, updated: 0, unchanged: 0 };
      throw new Error(`Unexpected command: ${command}`);
    });
    (window as typeof window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = { invoke };
    render(<App />);
    await screen.findByRole("heading", { name: "Review queue" });
    const input = document.querySelector<HTMLInputElement>('input[type="file"]')!;

    fireEvent.change(input, {
      target: { files: [new File(['{"contract":"hereforwork.discovery-run"}'], "run.json", { type: "application/json" })] },
    });
    await waitFor(() => expect(invoke).toHaveBeenCalledWith(
      "import_discovery_run",
      { payload: '{"contract":"hereforwork.discovery-run"}' },
      undefined,
    ));

    fireEvent.change(input, {
      target: { files: [new File(['{"schemaVersion":1}'], "snapshot.json", { type: "application/json" })] },
    });
    await waitFor(() => expect(invoke).toHaveBeenCalledWith(
      "import_dataset",
      { payload: '{"schemaVersion":1}' },
      undefined,
    ));
  });

  it("keeps primary navigation focused on queue and applications", async () => {
    render(<App />);
    await screen.findByRole("heading", { name: "Review queue" });

    expect(screen.getByRole("tablist")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("tab", { name: "applications" }));
    expect(screen.getByRole("heading", { name: "Applications" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "No application preparations yet." })).toBeInTheDocument();
    expect(screen.queryByRole("tab", { name: "activity" })).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("tab", { name: "queue" }));
    expect(screen.getByRole("heading", { name: "Review queue" })).toBeInTheDocument();
  });

  it("renders canonical Queue decision information without translating the score", async () => {
    window.history.replaceState({}, "", "/?queue-preview=decisions");
    const { container } = render(<App />);

    await screen.findByRole("heading", { name: "Review queue" });
    expect(screen.getByRole("heading", { name: "Strong matches" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Other new roles" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Needs a decision" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Needs attention" })).toBeInTheDocument();
    expect(screen.getByText("Example Studio")).toBeInTheDocument();
    expect(screen.getByText("The evaluation result is invalid or stale.")).toBeInTheDocument();
    expect(screen.getByText("Included in the group retry above.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Retry history sync" })).toBeEnabled();
    expect(screen.queryByText("1 role needs attention")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Open System" })).not.toBeInTheDocument();
    expect(screen.getByLabelText("career-ops match score 4.6 out of 5")).toHaveTextContent("4.6/5");
    expect(screen.getByText("Legitimacy: Proceed with Caution · Risk: Medium")).toBeInTheDocument();
    expect(screen.getByText(/The role asks for production GraphQL ownership/)).toBeInTheDocument();
    expect(screen.getByText("€52k–€58k gross annually")).toBeInTheDocument();
    expect(screen.getByText(/Culture not evaluated/)).toBeInTheDocument();
    expect(screen.queryByText(/%/)).not.toBeInTheDocument();
    expect(screen.queryByText(/Ashby|Greenhouse|Lever|Web form/)).not.toBeInTheDocument();
    expect(container.querySelectorAll(".role-card")).toHaveLength(3);
    expect(screen.getAllByRole("button", { name: "Dismiss" })).toHaveLength(3);
    expect(screen.getAllByRole("button", { name: "Prepare" })).toHaveLength(3);
  });

  it("keeps other roles actionable while one preparation is being queued", () => {
    const role = {
      id: "role-1",
      company: "Acme",
      title: "Frontend Engineer",
      location: "Remote",
      source: "Fixture",
      sourceCount: 1,
      queueGroup: "strong_match" as const,
      eligibilitySummary: "Strong match",
      uncertainty: null,
      postedAt: "2026-08-30",
      discoveredAt: "2026-08-31T08:00:00Z",
      applicationUrl: "https://example.com/apply",
      preparationState: "not_started" as const,
      canonicalTrackerId: null,
      canonicalStatus: null,
      evaluation: {
        nativeScore: 4.2,
        legitimacy: "Proceed with Caution" as const,
        riskLevel: "Medium" as const,
        strengths: ["React ownership is supported by recent work."],
        blockers: [],
        gaps: ["GraphQL ownership is not confirmed."],
        compensation: "€50k–€55k gross annually",
        authorizationConfidence: "investigate",
        authorizationQuestion: "Confirm the employing entity.",
        materialUncertainty: {
          confidence: "Medium" as const,
          authorizationQuestion: "Confirm whether employment from Spain is available.",
          notEvaluatedRiskSignals: ["culture"],
        },
      },
    };
    render(
      <ul>
        <RoleRow
          role={role}
          canPrepare
          canDismiss
          busy={false}
          enqueuing
          onPrepare={() => undefined}
          onDismiss={() => undefined}
        />
        <RoleRow
          role={{ ...role, id: "role-2", title: "Product Engineer" }}
          canPrepare
          canDismiss
          busy={false}
          enqueuing={false}
          onPrepare={() => undefined}
          onDismiss={() => undefined}
        />
      </ul>,
    );

    expect(screen.getByRole("button", { name: "Queueing…" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Prepare" })).toBeEnabled();
    expect(screen.getAllByRole("button").map((button) => button.textContent)).toEqual([
      "Dismiss",
      "Queueing…",
      "Dismiss",
      "Prepare",
    ]);
    expect(screen.getAllByRole("listitem")).toHaveLength(2);
    expect(screen.getAllByRole("article")).toHaveLength(2);
    expect(screen.getByText("Frontend Engineer is being queued for preparation.")).toBeInTheDocument();
    expect(screen.getAllByLabelText("career-ops evaluation")).toHaveLength(2);
    expect(screen.getAllByLabelText("career-ops match score 4.2 out of 5")).toHaveLength(2);
    expect(screen.getAllByText("4.2/5")).toHaveLength(2);
    expect(screen.getAllByText(/Legitimacy: Proceed with Caution · Risk: Medium/)).toHaveLength(2);
    expect(screen.getAllByText(/React ownership is supported by recent work/)).toHaveLength(2);
    expect(screen.getAllByText(/GraphQL ownership is not confirmed/)).toHaveLength(2);
    expect(screen.getAllByText(/€50k–€55k gross annually/)).toHaveLength(2);
    expect(screen.getAllByText(/Confirm whether employment from Spain is available/)).toHaveLength(2);
    expect(screen.getAllByText(/Culture not evaluated/)).toHaveLength(2);
    expect(screen.queryByText(/%/)).not.toBeInTheDocument();
    expect(screen.queryByText(/Ashby|Greenhouse|Lever|Web form/)).not.toBeInTheDocument();
    expect(screen.queryByText(/source occurrences|not started/i)).not.toBeInTheDocument();
  });

  it("summarizes pre-Queue evaluation work without rendering role cards or actions", () => {
    render(
      <PreQueueStatus roles={[
        {
          roleId: "pending",
          company: "Acme",
          title: "Frontend Engineer",
          state: "syncing",
          reason: "evaluation_result_read_pending",
          recovery: { scope: "none", action: null },
          attempt: 1,
          updatedAt: "2026-09-01T12:00:00Z",
        },
        {
          roleId: "attention",
          company: "Beta",
          title: "Product Engineer",
          state: "needs_attention",
          reason: "evaluation_result_invalid_or_stale",
          recovery: { scope: "none", action: null },
          attempt: 2,
          updatedAt: "2026-09-01T12:00:00Z",
        },
      ]} />,
    );

    expect(screen.getByRole("status")).toHaveTextContent(
      "1 role is being evaluated. 1 needs attention.",
    );
    expect(screen.queryByRole("article")).not.toBeInTheDocument();
    expect(screen.queryByRole("button")).not.toBeInTheDocument();
  });

  it("offers one typed group retry for globally recoverable attention roles", async () => {
    window.history.replaceState({}, "", "/?queue-preview=blocked");
    render(<App />);

    await screen.findByRole("heading", { name: "Review queue" });
    expect(screen.getByRole("heading", { name: "Needs attention" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Retry history sync" })).toBeEnabled();
    expect(screen.getAllByText("Included in the group retry above.")).toHaveLength(63);
    expect(screen.queryByText("63 roles need attention before they can appear in Queue.")).not.toBeInTheDocument();
  });

  it("maps Queue operational snapshots to truthful status affordances", () => {
    expect(deriveQueueOperationalState(getQueuePreviewDashboard("evaluating"))).toMatchObject({
      kind: "evaluating",
      count: 4,
    });
    expect(deriveQueueOperationalState(getQueuePreviewDashboard("progress"))).toMatchObject({
      kind: "progress",
      completed: 3,
      total: 5,
    });
    expect(deriveQueueOperationalState(getQueuePreviewDashboard("waiting"))).toMatchObject({ kind: "waiting" });
    expect(deriveQueueOperationalState(getQueuePreviewDashboard("blocked"))).toMatchObject({
      kind: "blocked",
      count: 63,
    });
    expect(deriveQueueOperationalState(getQueuePreviewDashboard("idle"))).toMatchObject({ kind: "idle" });

    render(<QueueOperationalStatus dashboard={getQueuePreviewDashboard("blocked")} />);
    expect(screen.getByRole("status")).toHaveTextContent("63 roles need attention");
    expect(screen.queryByRole("button", { name: "Open System" })).not.toBeInTheDocument();
  });

  it("keeps live syncing on the spinner until exact progress is supplied", () => {
    const syncingDashboard = getQueuePreviewDashboard("progress");
    const withoutExactProgress = { ...syncingDashboard, queueEvaluationProgress: undefined };

    expect(deriveQueueOperationalState(withoutExactProgress)).toMatchObject({
      kind: "evaluating",
      count: 2,
    });
    expect(deriveQueueOperationalState(syncingDashboard)).toMatchObject({
      kind: "progress",
      completed: 3,
      total: 5,
    });
  });

  it("renders each Queue operational state with its truthful affordance", () => {
    const { rerender } = render(
      <QueueOperationalStatus dashboard={getQueuePreviewDashboard("evaluating")} />,
    );
    expect(screen.getByRole("status")).toHaveTextContent("4 roles are being evaluated");
    expect(screen.getByRole("status")).toHaveTextContent("Started");
    expect(screen.queryByText(/ETA/i)).not.toBeInTheDocument();

    rerender(<QueueOperationalStatus dashboard={getQueuePreviewDashboard("progress")} />);
    expect(screen.getByRole("progressbar", { name: "Evaluation progress: 3 of 5 complete" })).toHaveAttribute("aria-valuenow", "60");
    expect(screen.getByRole("status")).toHaveTextContent("3 of 5 complete");

    rerender(<QueueOperationalStatus dashboard={getQueuePreviewDashboard("waiting")} />);
    expect(screen.getByRole("status")).toHaveTextContent("Last successful run");

    rerender(<QueueOperationalStatus dashboard={getQueuePreviewDashboard("idle")} />);
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });

  it("keeps queue filters in Settings instead of primary navigation", async () => {
    render(<App />);
    await screen.findByRole("heading", { name: "Review queue" });

    expect(screen.queryByText("Background checks")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Settings" }));

    expect(screen.getByRole("heading", { name: "Queue filters" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Reconcile history" })).not.toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "queue" })).toHaveAttribute("aria-selected", "false");
    expect(screen.getByRole("tab", { name: "applications" })).toHaveAttribute("aria-selected", "false");
    expect(screen.getByRole("checkbox", { name: "Include remote roles" })).toBeInTheDocument();
    expect(screen.getByRole("checkbox", { name: "Hide explicit authorization conflicts" })).toBeInTheDocument();
    expect(screen.getByRole("combobox", { name: "Chrome profile" })).toBeInTheDocument();
    expect(screen.getByRole("combobox", { name: "Preparation provider" })).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "Reviewed CV fallback" })).toBeInTheDocument();
    expect(screen.getByText("Not configured")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Save reviewed CV" })).toBeInTheDocument();
    expect(screen.getByText("Background checks")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Turn on" })).toBeInTheDocument();
  });

  it("explains a verified inspection without implying a completed application", () => {
    render(
      <BrowserSessions
        busy={false}
        onRetry={() => undefined}
        sessions={[{
          id: "session-1",
          purpose: "connection_check",
    roleId: null,
    preparationId: null,
          status: "connection_verified",
          ats: "ashby",
          pageTitle: "Frontend Engineer",
          pageUrl: "https://jobs.ashbyhq.com/acme/role",
          snapshotFingerprint: "a".repeat(64),
          fieldCount: 4,
          safeFieldCount: 2,
          needsUserCount: 2,
    errorCode: null,
    reviewItems: null,
    fillResults: null,
          updatedAt: "2026-08-30T18:00:00Z",
        }]}
      />,
    );

    expect(screen.getByText("Connection verified")).toBeInTheDocument();
    expect(screen.getByText(/Nothing was filled or finalized/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /submit/i })).not.toBeInTheDocument();
  });

  it("offers outcome confirmation only on the newest review attempt", () => {
    const session = {
      purpose: "application" as const,
      roleId: "role-1",
      preparationId: "preparation-1",
      status: "review_required" as const,
      ats: "ashby" as const,
      pageTitle: "Frontend Engineer",
      pageUrl: "https://jobs.ashbyhq.com/acme/role/application",
      snapshotFingerprint: "a".repeat(64),
      fieldCount: 2,
      safeFieldCount: 2,
      needsUserCount: 0,
      errorCode: null,
      reviewItems: null,
      fillResults: null,
    };
    render(
      <BrowserSessions
        busy={false}
        onRetry={() => undefined}
        onConfirmApplied={() => undefined}
        sessions={[
          { ...session, id: "older", updatedAt: "2026-08-30T18:00:00Z" },
          { ...session, id: "newest", updatedAt: "2026-08-30T18:01:00Z" },
        ]}
      />,
    );

    expect(screen.getAllByRole("button", { name: "I submitted this application" })).toHaveLength(1);
    expect(screen.getByText(/earlier browser attempt/i)).toBeInTheDocument();
  });
});
