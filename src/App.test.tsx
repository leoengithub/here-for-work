import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { App, BrowserSessions, RoleRow } from "./App";

afterEach(cleanup);

describe("App", () => {
  it("shows the operational empty state in browser preview", async () => {
    render(<App />);
    expect(await screen.findByRole("heading", { name: "Review queue" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Your review queue is ready for its first run." })).toBeInTheDocument();
    expect(screen.getByRole("combobox", { name: "Preparation provider" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Import discovery snapshot" })).toBeEnabled();
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
      discoveredAt: "2026-08-31T08:00:00Z",
      applicationUrl: "https://example.com/apply",
      preparationState: "not_started" as const,
      canonicalTrackerId: null,
      canonicalStatus: null,
    };
    render(
      <>
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
      </>,
    );

    expect(screen.getByRole("button", { name: "Queueing…" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Prepare" })).toBeEnabled();
  });

  it("keeps queue filters in System instead of primary navigation", async () => {
    render(<App />);
    await screen.findByRole("heading", { name: "Review queue" });

    fireEvent.click(screen.getByRole("button", { name: "System" }));

    expect(screen.getByRole("heading", { name: "Queue filters" })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "queue" })).toHaveAttribute("aria-selected", "false");
    expect(screen.getByRole("tab", { name: "applications" })).toHaveAttribute("aria-selected", "false");
    expect(screen.getByRole("checkbox", { name: "Include remote roles" })).toBeInTheDocument();
    expect(screen.getByRole("checkbox", { name: "Hide explicit authorization conflicts" })).toBeInTheDocument();
    expect(screen.getByRole("combobox", { name: "Chrome profile" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "settings" })).not.toBeInTheDocument();
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
