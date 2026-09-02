import type { BrowserSessionSummary, BrowserSetup, DashboardState, PreparationSummary } from "../types";

const now = "2026-09-01T12:00:00Z";

function preparation(
  id: string,
  title: string,
  company: string,
  status: PreparationSummary["status"],
  step: string,
  overrides: Partial<PreparationSummary> = {},
): PreparationSummary {
  return {
    id,
    roleId: `role-${id}`,
    company,
    title,
    provider: "codex",
    status,
    step,
    attempt: 1,
    reportPath: status === "completed" ? `reports/${id}.md` : null,
    cvPdfPath: status === "completed" ? `output/${id}/cv.pdf` : null,
    cvSource: status === "completed" ? "tailored_generated" : null,
    errorClass: null,
    errorStage: null,
    errorDetail: null,
    retryPolicy: null,
    updatedAt: now,
    ...overrides,
  };
}

export const applicationsPreviewDashboard: DashboardState = {
  roles: [],
  preQueueRoles: [],
  recentlyDismissed: [],
  preparations: [
    preparation("waiting", "Frontend Engineer", "Northstar", "queued", "queued"),
    preparation("preparing", "Senior Frontend Platform Engineer", "Atlas", "preparing", "preparing_cv"),
    preparation("failed", "Senior React Frontend Developer", "BCNC Group", "action_required", "preparing_cv", {
      errorClass: "artifact_commit_failed",
      errorStage: "preparation.result.commit",
      errorDetail: "The preparation artifacts could not be committed.",
      retryPolicy: null,
    }),
    preparation("review", "Product Engineer", "Linear", "completed", "completed"),
    preparation("recording", "Frontend Software Engineer, React/TypeScript/Next.js", "Trivelta", "completed", "completed"),
    preparation("recorded", "Member of Technical Staff", "Meridian Labs", "completed", "completed"),
    preparation("tracking", "UI Engineer", "Copperline", "completed", "completed"),
  ],
  activity: [],
  queueFilters: {
    roleFamilies: [],
    seniority: [],
    locations: [],
    remoteAllowed: true,
    requireAuthorizationPath: true,
  },
  lastSuccessfulDiscoveryAt: now,
  backgroundEnabled: false,
  adapterStatus: "ready",
  handledCount: 0,
  pendingRunCount: 0,
  actionRequiredRunCount: 0,
  sources: [],
  recentRuns: [],
};

function browserSession(
  id: string,
  preparationId: string,
  status: BrowserSessionSummary["status"],
): BrowserSessionSummary {
  return {
    id,
    purpose: "application",
    roleId: `role-${preparationId}`,
    preparationId,
    status,
    ats: "ashby",
    pageTitle: "Application",
    pageUrl: `https://example.test/${preparationId}/application`,
    snapshotFingerprint: "a".repeat(64),
    fieldCount: 4,
    safeFieldCount: 3,
    needsUserCount: 1,
    errorCode: status === "submitted_tracking_pending" ? "canonical_write_failed" : null,
    reviewItems: null,
    fillResults: null,
    updatedAt: now,
  };
}

export const applicationsPreviewSessions: BrowserSessionSummary[] = [
  browserSession("session-review", "review", "review_required"),
  browserSession("session-recording", "recording", "review_required"),
  browserSession("session-recorded", "recorded", "applied_recorded"),
  browserSession("session-tracking", "tracking", "submitted_tracking_pending"),
];

export const applicationsPreviewBrowserSetup: BrowserSetup = {
  profiles: [{ id: "Profile 1", name: "Work" }],
  selectedProfileId: "Profile 1",
  approvedExtensionId: "a".repeat(32),
  pendingExtensionId: null,
  approvedInstallationId: "019d0000-0000-7000-8000-000000000001",
  pendingInstallationId: null,
  extensionDirectory: "/fixture/extension",
  nativeHostRegistered: true,
  lastConnectedAt: now,
};

export function isApplicationsPreview(): boolean {
  return import.meta.env.DEV
    && new URLSearchParams(window.location.search).get("application-preview") === "states";
}
