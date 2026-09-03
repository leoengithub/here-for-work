import type { BrowserSessionSummary, BrowserSetup, DashboardState, PreQueueRecoveryDescriptor, PreparationSummary, QueueEvaluationSummary, RoleSummary } from "../types";

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
  discoveryRuns: [],
  discoveryCursors: [],
};

function queueRole(
  id: string,
  title: string,
  company: string,
  queueGroup: RoleSummary["queueGroup"],
  score: number,
  overrides: Partial<QueueEvaluationSummary> = {},
): RoleSummary {
  return {
    id,
    company,
    title,
    location: "Remote, Spain",
    source: "Fixture",
    sourceCount: 1,
    queueGroup,
    eligibilitySummary: "",
    uncertainty: null,
    postedAt: "2026-08-30",
    discoveredAt: now,
    applicationUrl: `https://example.test/${id}/apply`,
    preparationState: "not_started",
    canonicalTrackerId: Number(id.replace(/\D/g, "")) || 1,
    canonicalStatus: "Evaluated",
    evaluation: {
      nativeScore: score,
      legitimacy: "High Confidence",
      riskLevel: "Low",
      strengths: ["React platform work is directly supported by the CV."],
      blockers: [],
      gaps: [],
      compensation: "€52k–€58k gross annually",
      authorizationConfidence: "interesting",
      authorizationQuestion: "Confirm the employing entity for Spain.",
      materialUncertainty: {
        confidence: "High",
        authorizationQuestion: "",
        notEvaluatedRiskSignals: [],
      },
      ...overrides,
    },
  };
}

export const queuePreviewDashboard: DashboardState = {
  ...applicationsPreviewDashboard,
  roles: [
    queueRole("queue-1", "Senior Frontend Platform Engineer", "Northstar Tools", "strong_match", 4.6),
    queueRole("queue-2", "Product Engineer", "Copperline", "other_new", 4.1, {
      strengths: ["Product-facing frontend ownership matches recent experience."],
      compensation: null,
      riskLevel: "Medium",
      legitimacy: "Proceed with Caution",
      gaps: ["The role asks for production GraphQL ownership not explicit in the listing."],
      materialUncertainty: {
        confidence: "Medium",
        authorizationQuestion: "Confirm whether the role can employ someone working from Spain.",
        notEvaluatedRiskSignals: ["culture"],
      },
    }),
    queueRole("queue-3", "Frontend Engineer, Design Systems and Accessibility", "Meridian Labs", "needs_decision", 3.7, {
      strengths: ["Design-system and accessibility work is supported by recent projects."],
      blockers: ["The advertised compensation is below the preferred range."],
      compensation: "€45k–€49k gross annually",
      riskLevel: "Medium",
      materialUncertainty: {
        confidence: "Medium",
        authorizationQuestion: "Confirm the contract route from Spain.",
        notEvaluatedRiskSignals: ["aiScreeningDisclosure"],
      },
    }),
  ],
  preQueueRoles: [
    {
      roleId: "pending-1",
      company: "Example Labs",
      title: "Frontend Engineer",
      state: "syncing",
      reason: "evaluation_result_read_pending",
      recovery: { scope: "none", action: null },
      attempt: 1,
      updatedAt: now,
    },
    {
      roleId: "attention-1",
      company: "Example Studio",
      title: "UI Engineer",
      state: "needs_attention",
      reason: "evaluation_result_invalid_or_stale",
      recovery: { scope: "global_reconcile", action: "reconcile_application_history" },
      attempt: 2,
      updatedAt: now,
    },
  ],
  preparations: [],
};

export type QueuePreviewMode = "decisions" | "evaluating" | "progress" | "waiting" | "blocked" | "idle";

const preQueueFixture = (count: number, state: "awaiting_evaluation" | "syncing" | "needs_attention", reason: string) => (
  Array.from({ length: count }, (_, index) => ({
    roleId: `preview-${state}-${index + 1}`,
    company: state === "needs_attention" ? "Northstar Tools" : "Example Labs",
    title: state === "needs_attention" ? "Frontend Engineer" : "Platform Engineer",
    state,
    reason,
    recovery: (state === "needs_attention" && reason === "canonical_history_unavailable"
      ? { scope: "global_reconcile", action: "reconcile_application_history" }
      : { scope: "none", action: null }) satisfies PreQueueRecoveryDescriptor,
    attempt: state === "needs_attention" ? 2 : 1,
    updatedAt: "2026-09-03T08:00:00Z",
  }))
);

export function getQueuePreviewDashboard(mode: QueuePreviewMode): DashboardState {
  if (mode === "decisions") return queuePreviewDashboard;
  if (mode === "evaluating") {
    return {
      ...queuePreviewDashboard,
      roles: [],
      preQueueRoles: preQueueFixture(4, "awaiting_evaluation", "evaluation_pending"),
      lastSuccessfulDiscoveryAt: "2026-09-03T07:30:00Z",
      pendingRunCount: 1,
    };
  }
  if (mode === "progress") {
    return {
      ...queuePreviewDashboard,
      preQueueRoles: preQueueFixture(2, "syncing", "evaluation_result_read_pending"),
      queueEvaluationProgress: { completed: 3, total: 5 },
      pendingRunCount: 0,
      actionRequiredRunCount: 0,
    };
  }
  if (mode === "waiting") {
    return {
      ...queuePreviewDashboard,
      roles: [],
      preQueueRoles: [],
      preparations: [],
      recentlyDismissed: [],
      lastSuccessfulDiscoveryAt: "2026-09-03T06:45:00Z",
      pendingRunCount: 0,
    };
  }
  if (mode === "blocked") {
    return {
      ...queuePreviewDashboard,
      roles: [],
      preQueueRoles: preQueueFixture(63, "needs_attention", "canonical_history_unavailable"),
      preparations: [],
      recentlyDismissed: [],
      lastSuccessfulDiscoveryAt: "2026-09-03T06:45:00Z",
      pendingRunCount: 0,
      actionRequiredRunCount: 0,
    };
  }
  return {
    ...queuePreviewDashboard,
    roles: [],
    preQueueRoles: [],
    preparations: [],
    recentlyDismissed: [],
    lastSuccessfulDiscoveryAt: null,
    pendingRunCount: 0,
    actionRequiredRunCount: 0,
  };
}

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

export function isQueuePreview(): boolean {
  return import.meta.env.DEV
    && new URLSearchParams(window.location.search).has("queue-preview");
}

export function getQueuePreviewMode(): QueuePreviewMode {
  const mode = new URLSearchParams(window.location.search).get("queue-preview");
  return mode === "evaluating" || mode === "progress" || mode === "waiting" || mode === "blocked" || mode === "idle"
    ? mode
    : "decisions";
}
