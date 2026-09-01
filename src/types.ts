export type QueueGroup = "strong_match" | "other_new" | "needs_decision";
export type PreparationState =
  | "not_started"
  | "queued"
  | "preparing"
  | "prepared"
  | "review_required"
  | "applied_recorded"
  | "action_required"
  | "failed";

export interface RoleSummary {
  id: string;
  company: string;
  title: string;
  location: string;
  source: string;
  sourceCount: number;
  queueGroup: QueueGroup;
  eligibilitySummary: string;
  uncertainty: string | null;
  postedAt: string | null;
  discoveredAt: string;
  applicationUrl: string | null;
  preparationState: PreparationState;
  canonicalTrackerId: number | null;
  canonicalStatus: string | null;
}

export interface ActivityEntry {
  id: string;
  kind: string;
  message: string;
  occurredAt: string;
}

export interface DashboardState {
  roles: RoleSummary[];
  recentlyDismissed: RoleSummary[];
  preparations: PreparationSummary[];
  activity: ActivityEntry[];
  queueFilters: QueueFilters;
  lastSuccessfulDiscoveryAt: string | null;
  backgroundEnabled: boolean;
  adapterStatus: "not_configured" | "ready" | "action_required";
  handledCount: number;
  pendingRunCount: number;
  actionRequiredRunCount: number;
  sources: SourceScheduleSummary[];
  recentRuns: RunSummary[];
}

export interface QueueFilters {
  roleFamilies: string[];
  seniority: string[];
  locations: string[];
  remoteAllowed: boolean;
  requireAuthorizationPath: boolean;
}

export interface PrepareRoleOutcome {
  dashboard: DashboardState;
  disposition: "queued";
  message: string;
}

export interface PreparationSummary {
  id: string;
  roleId: string;
  company: string;
  title: string;
  provider: "codex" | "claude";
  status: "queued" | "preparing" | "completed" | "action_required" | "cancelled";
  step: string;
  attempt: number;
  reportPath: string | null;
  cvPdfPath: string | null;
  errorClass: string | null;
  errorStage: string | null;
  errorDetail: string | null;
  retryPolicy: string | null;
  updatedAt: string;
}

export interface PreparationDetail {
  preparationId: string;
  roleId: string;
  company: string;
  title: string;
  provider: "codex" | "claude";
  status: string;
  stage: string;
  errorClass: string | null;
  errorDetail: string | null;
  retryPolicy: string | null;
  reportMarkdown: string | null;
  reportPath: string | null;
  cvPdfPath: string | null;
}

export interface OutcomeNotification {
  id: string;
  eventKind: "preparation_failed" | "application_ready";
  title: string;
  body: string;
  actionKind: "view_details" | "review_form";
  actionLabel: "View details" | "Review form";
  roleId: string;
  preparationId: string;
  browserSessionId: string | null;
  createdAt: string;
}

export interface SourceScheduleSummary {
  sourceId: string;
  displayName: string;
  timezone: string;
  scheduleHours: string;
  executionMode: "staged" | "active";
  lastSuccessfulAt: string | null;
  actionRequiredCount: number;
}

export interface RunSummary {
  id: string;
  sourceId: string;
  kind: "catch_up" | "manual";
  coverageStart: string;
  coverageEnd: string;
  status: "queued" | "running" | "retryable" | "action_required" | "permanent" | "completed" | "cancelled";
  attempt: number;
  errorClass: string | null;
  updatedAt: string;
}

export interface CheckResult {
  ready: boolean;
  detail: string;
}

export interface IntegrationHealth {
  careerOps: CheckResult;
  codex: CheckResult;
  claude: CheckResult;
}

export interface ReconcileResult {
  matched: number;
  cleared: number;
  unmatched: number;
}

export interface ScheduledRun {
  id: string;
  sourceId: string;
  coverageStart: string;
  coverageEnd: string;
  status: string;
}

export type ProviderProbeResult = ProviderProbeSchema & {
  provider: "codex" | "claude";
};

export interface ChromeProfile {
  id: string;
  name: string;
}

export interface BrowserSetup {
  profiles: ChromeProfile[];
  selectedProfileId: string | null;
  approvedExtensionId: string | null;
  pendingExtensionId: string | null;
  approvedInstallationId: string | null;
  pendingInstallationId: string | null;
  extensionDirectory: string;
  nativeHostRegistered: boolean;
  lastConnectedAt: string | null;
}

export type BrowserSessionStatus =
  | "waiting_for_extension"
  | "inspecting"
  | "drafting_answers"
  | "answering"
  | "filling"
  | "persisting_answers"
  | "saving_answers"
  | "releasing"
  | "connection_verified"
  | "review_required"
  | "submitted_tracking_pending"
  | "applied_recorded"
  | "action_required";

export interface BrowserSessionSummary {
  id: string;
  purpose: "connection_check" | "application";
  roleId: string | null;
  preparationId: string | null;
  status: BrowserSessionStatus;
  ats: "ashby" | "greenhouse" | "lever" | "generic" | null;
  pageTitle: string | null;
  pageUrl: string | null;
  snapshotFingerprint: string | null;
  fieldCount: number;
  safeFieldCount: number;
  needsUserCount: number;
  errorCode: string | null;
  reviewItems: Array<{ fieldId: string; label: string; decision: string; answer?: string; provenance?: string[]; reason?: string }> | null;
  fillResults: Array<{ fieldId: string; status: string; reason: string | null }> | null;
  updatedAt: string;
}

export interface MaintenanceResult {
  path: string;
  integrity: string;
}

export interface RestorePreflight {
  path: string;
  integrity: string;
  schemaVersion: number;
  roleCount: number;
  runCount: number;
}

export interface ImportResult {
  imported: number;
  updated: number;
  unchanged: number;
}
import type { HereForWorkDiscoveryDataset } from "./generated/discovery-dataset";
import type { HereForWorkDiscoveryRun } from "./generated/discovery-run";
import type { ProviderProbeSchema } from "./generated/provider-probe";

export type DiscoveryDataset = HereForWorkDiscoveryDataset;
export type DiscoveryRun = HereForWorkDiscoveryRun;
