import { invoke } from "@tauri-apps/api/core";
import type {
  DashboardState,
  DiscoveryCursor,
  DiscoveryRunImportResult,
  ImportResult,
  IntegrationHealth,
  ReconcileResult,
  ScheduledRun,
  ProviderProbeResult,
  BrowserSetup,
  MaintenanceResult,
  RestorePreflight,
  BrowserSessionSummary,
  PreparationDetail,
  PrepareRoleOutcome,
  QueueFilters,
  OutcomeNotification,
  CvFallbackSetting,
} from "./types";
import {
  applicationsPreviewBrowserSetup,
  applicationsPreviewDashboard,
  applicationsPreviewSessions,
  isApplicationsPreview,
  isQueuePreview,
  getQueuePreviewDashboard,
  getQueuePreviewMode,
} from "./dev/applications-preview";

const isTauri = (): boolean => "__TAURI_INTERNALS__" in window;

const browserFallback: DashboardState = {
  roles: [],
  preQueueRoles: [],
  recentlyDismissed: [],
  preparations: [],
  activity: [
    {
      id: "browser-preview",
      kind: "system",
      message: "Browser preview is using an empty local state. Open the desktop app to use the operational store.",
      occurredAt: new Date().toISOString(),
    },
  ],
  queueFilters: {
    roleFamilies: [],
    seniority: [],
    locations: [],
    remoteAllowed: true,
    requireAuthorizationPath: true,
  },
  lastSuccessfulDiscoveryAt: null,
  backgroundEnabled: false,
  adapterStatus: "not_configured",
  handledCount: 0,
  pendingRunCount: 0,
  actionRequiredRunCount: 0,
  sources: [],
  recentRuns: [],
  discoveryRuns: [],
  discoveryCursors: [],
};

export async function getDashboard(): Promise<DashboardState> {
  if (isApplicationsPreview()) return applicationsPreviewDashboard;
  if (isQueuePreview()) return getQueuePreviewDashboard(getQueuePreviewMode());
  if (!isTauri()) return browserFallback;
  return invoke<DashboardState>("get_dashboard");
}

export async function checkIntegrations(): Promise<IntegrationHealth> {
  if (!isTauri()) {
    return {
      careerOps: { ready: false, detail: "Available in the desktop app.", capabilities: null },
      codex: { ready: false, detail: "Available in the desktop app." },
      claude: { ready: false, detail: "Available in the desktop app." },
    };
  }
  return invoke<IntegrationHealth>("check_integrations");
}

export async function reconcileApplicationHistory(): Promise<ReconcileResult> {
  if (!isTauri()) throw new Error("History reconciliation is available in the desktop app.");
  return invoke<ReconcileResult>("reconcile_application_history");
}

export async function requestNotificationPermission(): Promise<boolean> {
  if (!isTauri()) return false;
  return invoke<boolean>("request_notification_permission");
}

export async function sendTestNotification(): Promise<void> {
  if (!isTauri()) throw new Error("Native notifications are available in the desktop app.");
  return invoke<void>("send_test_notification");
}

export async function takeInAppOutcomeNotifications(): Promise<OutcomeNotification[]> {
  if (!isTauri()) return [];
  return invoke<OutcomeNotification[]>("take_in_app_outcome_notifications");
}

export async function queueManualDiscovery(sourceId: "frontend-role-scan" | "eu-job-radar"): Promise<ScheduledRun> {
  if (!isTauri()) throw new Error("Discovery scheduling is available in the desktop app.");
  return invoke<ScheduledRun>("queue_manual_discovery", { sourceId });
}

export async function runProviderProbe(provider: "codex" | "claude"): Promise<ProviderProbeResult> {
  if (!isTauri()) throw new Error("Provider execution is available in the desktop app.");
  return invoke<ProviderProbeResult>("run_provider_probe", { provider });
}

export async function prepareRole(roleId: string, provider: "codex" | "claude"): Promise<PrepareRoleOutcome> {
  if (!isTauri()) throw new Error("Application preparation is available in the desktop app.");
  return invoke<PrepareRoleOutcome>("prepare_role", { roleId, provider });
}

export async function saveQueueFilters(filters: QueueFilters): Promise<DashboardState> {
  if (!isTauri()) throw new Error("Queue filters are available in the desktop app.");
  return invoke<DashboardState>("save_queue_filters", { filters });
}

export async function getCvFallbackSetting(): Promise<CvFallbackSetting> {
  if (!isTauri()) return { path: null, sha256: null };
  return invoke<CvFallbackSetting>("get_cv_fallback_setting");
}

export async function setCvFallbackSetting(path: string): Promise<CvFallbackSetting> {
  if (!isTauri()) throw new Error("Reviewed CV fallback settings are available in the desktop app.");
  return invoke<CvFallbackSetting>("set_cv_fallback_setting", { path });
}

export async function dismissPreparation(preparationId: string): Promise<DashboardState> {
  if (!isTauri()) throw new Error("Preparation cleanup is available in the desktop app.");
  return invoke<DashboardState>("dismiss_preparation", { preparationId });
}

export async function cancelPreparation(roleId: string): Promise<boolean> {
  if (!isTauri()) return false;
  return invoke<boolean>("cancel_preparation", { roleId });
}

export async function getPreparationDetail(preparationId: string): Promise<PreparationDetail> {
  if (!isTauri()) throw new Error("Preparation details are available in the desktop app.");
  return invoke<PreparationDetail>("get_preparation_detail", { preparationId });
}

export async function openPreparationArtifact(preparationId: string, artifact: "report" | "cv"): Promise<void> {
  if (!isTauri()) throw new Error("Preparation artifacts are available in the desktop app.");
  return invoke<void>("open_preparation_artifact", { preparationId, artifact });
}

export async function openExternalUrl(url: string): Promise<void> {
  if (!isTauri()) throw new Error("Opening listing URLs is available in the desktop app.");
  return invoke<void>("open_external_url", { url });
}

export async function continueUnverifiedPreparation(preparationId: string): Promise<DashboardState> {
  if (!isTauri()) throw new Error("Continue anyway is available in the desktop app.");
  return invoke<DashboardState>("continue_unverified_preparation", { preparationId });
}

export async function quitApp(): Promise<void> {
  if (!isTauri()) return;
  return invoke<void>("quit_app");
}

export async function getBrowserSetup(): Promise<BrowserSetup> {
  if (isApplicationsPreview()) return applicationsPreviewBrowserSetup;
  if (!isTauri()) {
    return {
      profiles: [],
      selectedProfileId: null,
      approvedExtensionId: null,
      pendingExtensionId: null,
      approvedInstallationId: null,
      pendingInstallationId: null,
      extensionDirectory: "Available in the desktop app.",
      nativeHostRegistered: false,
      lastConnectedAt: null,
    };
  }
  return invoke<BrowserSetup>("get_browser_setup");
}

export async function getBrowserSessions(): Promise<BrowserSessionSummary[]> {
  if (isApplicationsPreview()) return applicationsPreviewSessions;
  if (!isTauri()) return [];
  return invoke<BrowserSessionSummary[]>("get_browser_sessions");
}

export async function startBrowserConnectionCheck(): Promise<BrowserSessionSummary> {
  if (!isTauri()) throw new Error("Browser checks are available in the desktop app.");
  return invoke<BrowserSessionSummary>("start_browser_connection_check");
}

export async function retryBrowserSession(sessionId: string): Promise<BrowserSessionSummary> {
  if (!isTauri()) throw new Error("Browser session recovery is available in the desktop app.");
  return invoke<BrowserSessionSummary>("retry_browser_session", { sessionId });
}

export async function focusReviewForm(sessionId: string): Promise<BrowserSessionSummary> {
  if (!isTauri()) throw new Error("Review-form focus is available in the desktop app.");
  return invoke<BrowserSessionSummary>("focus_review_form", { sessionId });
}

export async function continueInBrowser(preparationId: string): Promise<BrowserSessionSummary> {
  if (!isTauri()) throw new Error("Browser continuation is available in the desktop app.");
  return invoke<BrowserSessionSummary>("continue_in_browser", { preparationId });
}

export async function reopenApplicationForm(preparationId: string): Promise<BrowserSessionSummary> {
  if (!isTauri()) throw new Error("Application recovery is available in the desktop app.");
  return invoke<BrowserSessionSummary>("reopen_application_form", { preparationId });
}

export async function confirmApplicationApplied(sessionId: string): Promise<BrowserSessionSummary> {
  if (!isTauri()) throw new Error("Application confirmation is available in the desktop app.");
  return invoke<BrowserSessionSummary>("confirm_application_applied", { sessionId, userConfirmed: true });
}

export async function markApplicationApplied(roleId: string): Promise<DashboardState> {
  if (!isTauri()) throw new Error("External Applied confirmation is available in the desktop app.");
  return invoke<DashboardState>("mark_application_applied", { roleId, userConfirmed: true });
}

export async function configureBrowserBridge(extensionId: string, installationId: string, profileId: string): Promise<BrowserSetup> {
  if (!isTauri()) throw new Error("Browser setup is available in the desktop app.");
  return invoke<BrowserSetup>("configure_browser_bridge", { extensionId, installationId, profileId });
}

export async function createOperationalBackup(): Promise<MaintenanceResult> {
  if (!isTauri()) throw new Error("Operational backups are available in the desktop app.");
  return invoke<MaintenanceResult>("create_operational_backup");
}

export async function exportOperationalSummary(): Promise<MaintenanceResult> {
  if (!isTauri()) throw new Error("Operational exports are available in the desktop app.");
  return invoke<MaintenanceResult>("export_operational_summary");
}

export async function exportRedactedDiagnostics(): Promise<MaintenanceResult> {
  if (!isTauri()) throw new Error("Diagnostics export is available in the desktop app.");
  return invoke<MaintenanceResult>("export_redacted_diagnostics");
}

export async function preflightLatestBackup(): Promise<RestorePreflight> {
  if (!isTauri()) throw new Error("Restore preflight is available in the desktop app.");
  return invoke<RestorePreflight>("preflight_latest_backup");
}

export async function importDataset(payload: string): Promise<ImportResult> {
  if (!isTauri()) throw new Error("Dataset import is available in the desktop app.");
  return invoke<ImportResult>("import_dataset", { payload });
}

export async function importDiscoveryRun(payload: string): Promise<DiscoveryRunImportResult> {
  if (!isTauri()) throw new Error("Discovery-run import is available in the desktop app.");
  return invoke<DiscoveryRunImportResult>("import_discovery_run", { payload });
}

export async function getDiscoveryCursors(): Promise<DiscoveryCursor[]> {
  if (!isTauri()) throw new Error("Discovery cursors are available in the desktop app.");
  return invoke<DiscoveryCursor[]>("get_discovery_cursors");
}

export async function setBackgroundEnabled(enabled: boolean): Promise<DashboardState> {
  if (!isTauri()) throw new Error("Background settings are available in the desktop app.");
  return invoke<DashboardState>("set_background_enabled", { enabled });
}

export async function retryEvaluation(roleId: string): Promise<DashboardState> {
  if (!isTauri()) throw new Error("Evaluation retry is available in the desktop app.");
  return invoke<DashboardState>("retry_evaluation", { roleId });
}

export async function dismissRole(roleId: string): Promise<DashboardState> {
  if (!isTauri()) throw new Error("Canonical dismissal is available in the desktop app.");
  return invoke<DashboardState>("dismiss_role", { roleId });
}

export async function undoDismissal(roleId: string): Promise<DashboardState> {
  if (!isTauri()) throw new Error("Dismissal recovery is available in the desktop app.");
  return invoke<DashboardState>("undo_dismissal", { roleId });
}
