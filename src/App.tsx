import { useEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Alert, AlertAction, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty";
import { Field, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Sheet, SheetContent, SheetHeader, SheetTitle } from "@/components/ui/sheet";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Textarea } from "@/components/ui/textarea";
import {
  checkIntegrations,
  configureBrowserBridge,
  createOperationalBackup,
  dismissRole,
  exportRedactedDiagnostics,
  exportOperationalSummary,
  getDashboard,
  getBrowserSetup,
  getBrowserSessions,
  importDataset,
  preflightLatestBackup,
  prepareRole,
  reconcileApplicationHistory,
  requestNotificationPermission,
  runProviderProbe,
  retryBrowserSession,
  continueInBrowser,
  confirmApplicationApplied,
  getPreparationDetail,
  openPreparationArtifact,
  quitApp,
  cancelPreparation,
  sendTestNotification,
  saveQueueFilters,
  setBackgroundEnabled,
  startBrowserConnectionCheck,
  undoPreparation,
  undoDismissal,
} from "./api";
import type {
  DashboardState,
  BrowserSetup,
  IntegrationHealth,
  ProviderProbeResult,
  MaintenanceResult,
  RestorePreflight,
  QueueGroup,
  RoleSummary,
  BrowserSessionSummary,
  PreparationDetail,
  QueueFilters,
} from "./types";

const groupOrder: QueueGroup[] = ["strong_match", "other_new", "needs_decision"];

const groupCopy: Record<QueueGroup, { title: string; description: string }> = {
  strong_match: {
    title: "Strong matches",
    description: "Verified fit with no known blocker.",
  },
  other_new: {
    title: "Other new roles",
    description: "Worth seeing, but not yet a strong match.",
  },
  needs_decision: {
    title: "Needs a decision",
    description: "A blocker or uncertainty needs your judgment.",
  },
};

const formatRelative = (iso: string): string => {
  const date = new Date(iso);
  if (Number.isNaN(date.valueOf())) return "Unknown time";
  const elapsedMinutes = Math.max(0, Math.round((Date.now() - date.valueOf()) / 60_000));
  if (elapsedMinutes < 1) return "Just now";
  if (elapsedMinutes < 60) return `${elapsedMinutes}m ago`;
  const elapsedHours = Math.round(elapsedMinutes / 60);
  if (elapsedHours < 24) return `${elapsedHours}h ago`;
  return date.toLocaleDateString(undefined, { month: "short", day: "numeric" });
};

const atsLabel = (applicationUrl: string | null): string => {
  if (!applicationUrl) return "Web form";
  try {
    const hostname = new URL(applicationUrl).hostname;
    if (hostname.includes("ashbyhq.com")) return "Ashby";
    if (hostname.includes("greenhouse.io")) return "Greenhouse";
    if (hostname.includes("lever.co")) return "Lever";
  } catch {
    return "Web form";
  }
  return "Web form";
};

function RoleRow({
  role,
  canPrepare,
  canDismiss,
  busy,
  onPrepare,
  onCancel,
  onDismiss,
}: {
  role: RoleSummary;
  canPrepare: boolean;
  canDismiss: boolean;
  busy: boolean;
  onPrepare: (roleId: string) => void;
  onCancel: (roleId: string) => void;
  onDismiss: (roleId: string) => void;
}) {
  const preparing = role.preparationState === "preparing";
  return (
    <article className="role-row">
      <span className="role-row__main">
        {role.applicationUrl ? (
          <a className="role-row__title" href={role.applicationUrl} target="_blank" rel="noreferrer">
            {role.title}
          </a>
        ) : (
          <span className="role-row__title">{role.title}</span>
        )}
        <span className="role-row__company">{role.company}</span>
        {role.uncertainty ? <span className="role-row__uncertainty">{role.uncertainty}</span> : null}
      </span>
      <span className="role-row__meta">
        <span>{role.location}</span>
        <span>{atsLabel(role.applicationUrl)}</span>
        {role.sourceCount > 1 ? <span>{role.sourceCount} source occurrences</span> : null}
      </span>
      <Badge variant="outline">{role.preparationState.replaceAll("_", " ")}</Badge>
      <span className="role-row__actions">
        <Button

          type="button"
          disabled={preparing ? false : !canPrepare || busy}
          aria-describedby="queue-action-status"
          onClick={() => preparing ? onCancel(role.id) : onPrepare(role.id)}
        >
          {preparing ? "Cancel preparation" : "Prepare"}
        </Button>
        <Button
          variant="outline"
          type="button"
          disabled={!canDismiss || busy}
          aria-describedby="queue-action-status"
          onClick={() => onDismiss(role.id)}
        >
          Dismiss
        </Button>
      </span>
    </article>
  );
}

function HandledQueue() {
  return (
    <Empty className="empty-state" role="region" aria-labelledby="handled-empty-title">
      <EmptyHeader>
        <EmptyMedia className="empty-state__mark" aria-hidden="true">H</EmptyMedia>
        <EmptyTitle><h2 id="handled-empty-title">No new roles are waiting.</h2></EmptyTitle>
        <EmptyDescription>Prepared and dismissed roles remain visible in Applications and canonical career-ops history.</EmptyDescription>
      </EmptyHeader>
    </Empty>
  );
}

function EmptyQueue({ onImport }: { onImport: () => void }) {
  return (
    <Empty className="empty-state" role="region" aria-labelledby="empty-title">
      <EmptyHeader>
        <EmptyMedia className="empty-state__mark" aria-hidden="true">H</EmptyMedia>
        <EmptyTitle><h2 id="empty-title">Your review queue is ready for its first run.</h2></EmptyTitle>
        <EmptyDescription>
          Import the two discovery snapshots to reconcile roles without changing career-ops, Gmail, or application history.
        </EmptyDescription>
      </EmptyHeader>
      <EmptyContent>
        <Button type="button" onClick={onImport}>
          Import discovery snapshot
        </Button>
        <p className="empty-state__hint">JSON stays on this Mac and is written only to HereForWork’s operational store.</p>
      </EmptyContent>
    </Empty>
  );
}

const browserStatusLabel: Record<BrowserSessionSummary["status"], string> = {
  waiting_for_extension: "Waiting for extension",
  inspecting: "Inspecting active page",
  drafting_answers: "Drafting grounded answers",
  answering: "Drafting grounded answers",
  filling: "Filling verified fields",
  persisting_answers: "Saving grounded answers",
  saving_answers: "Saving grounded answers",
  releasing: "Releasing page",
  connection_verified: "Connection verified",
  review_required: "Review required",
  submitted_tracking_pending: "Submitted · tracking pending",
  applied_recorded: "Applied recorded",
  action_required: "Needs your attention",
};

export function BrowserSessions({
  sessions,
  busy,
  onRetry,
  onConfirmApplied,
}: {
  sessions: BrowserSessionSummary[];
  busy: boolean;
  onRetry: (sessionId: string) => void;
  onConfirmApplied?: (sessionId: string) => void;
}) {
  if (sessions.length === 0) {
    return (
      <p className="browser-session-empty">
        No browser session yet. Pair the extension, open an HTTPS application form, then run the connection check from System.
      </p>
    );
  }
  const latestApplicationSessionIds = new Set<string>();
  const latestByPreparation = new Map<string, BrowserSessionSummary>();
  for (const session of sessions) {
    if (session.purpose !== "application" || !session.preparationId) continue;
    const current = latestByPreparation.get(session.preparationId);
    if (!current || session.updatedAt > current.updatedAt) latestByPreparation.set(session.preparationId, session);
  }
  for (const session of latestByPreparation.values()) latestApplicationSessionIds.add(session.id);
  return (
    <ol className="browser-session-list" aria-label="Browser sessions">
      {sessions.map((session) => {
        const isLatestApplicationAttempt = session.purpose !== "application"
          || !session.preparationId
          || latestApplicationSessionIds.has(session.id);
        return (
        <li key={session.id}>
          <div className="browser-session-list__heading">
            <div>
              <strong>{session.pageTitle ?? "Active application page"}</strong>
              <span>{session.ats === "generic" ? "WEB FORM" : session.ats ? session.ats.toUpperCase() : "Waiting for an application page"}</span>
            </div>
            <Badge variant="outline">{browserStatusLabel[session.status]}</Badge>
          </div>
          {session.status === "connection_verified" ? (
            <p>
              {session.fieldCount} fields inspected. {session.safeFieldCount} can accept verified facts; {session.needsUserCount} stay with you. Nothing was filled or finalized.
            </p>
          ) : null}
          {session.status === "review_required" ? (
            <div>
              <p>The page is released for your review. HereForWork cannot submit it.</p>
              {session.fieldCount === 0 ? (
                <p>No compatible fields were found after the inspection window. Complete this form manually; the application itself remains available.</p>
              ) : null}
              {session.purpose === "application" && onConfirmApplied && isLatestApplicationAttempt ? (
                <Button  type="button" onClick={() => onConfirmApplied(session.id)} disabled={busy}>
                  I submitted this application
                </Button>
              ) : null}
              {session.purpose === "application" && !isLatestApplicationAttempt ? (
                <p>This is an earlier browser attempt. Confirm an outcome only from the newest review session.</p>
              ) : null}
            </div>
          ) : null}
          {session.status === "applied_recorded" ? <p>Your confirmed outcome is recorded in canonical career-ops history.</p> : null}
          {session.status === "submitted_tracking_pending" && onConfirmApplied && isLatestApplicationAttempt ? (
            <div>
              <p>The form is not touched again. Only the canonical career-ops tracking write will be retried.</p>
              <Button  type="button" onClick={() => onConfirmApplied(session.id)} disabled={busy}>
                Retry tracking update
              </Button>
            </div>
          ) : null}
          {session.errorCode ? <p className="browser-session-list__error">{session.errorCode.replaceAll("_", " ")}</p> : null}
          {session.status === "action_required" ? (
            <Button variant="outline" type="button" onClick={() => onRetry(session.id)} disabled={busy}>
              Retry session
            </Button>
          ) : null}
        </li>
        );
      })}
    </ol>
  );
}

function markdownInline(value: string): ReactNode[] {
  return value.split(/(\*\*[^*]+\*\*|`[^`]+`)/g).filter(Boolean).map((part, index) => {
    if (part.startsWith("**") && part.endsWith("**")) return <strong key={index}>{part.slice(2, -2)}</strong>;
    if (part.startsWith("`") && part.endsWith("`")) return <code key={index}>{part.slice(1, -1)}</code>;
    return <span key={index}>{part}</span>;
  });
}

function MarkdownPreview({ markdown }: { markdown: string }) {
  const lines = markdown.replaceAll("\r\n", "\n").split("\n");
  const blocks: ReactNode[] = [];
  let index = 0;
  while (index < lines.length) {
    const line = lines[index].trimEnd();
    if (!line.trim()) {
      index += 1;
      continue;
    }
    if (line.startsWith("```")) {
      const code: string[] = [];
      index += 1;
      while (index < lines.length && !lines[index].trim().startsWith("```")) {
        code.push(lines[index]);
        index += 1;
      }
      index += 1;
      blocks.push(<pre key={`code-${index}`} tabIndex={0}><code>{code.join("\n")}</code></pre>);
      continue;
    }
    const heading = line.match(/^(#{1,4})\s+(.+)$/);
    if (heading) {
      const level = Math.min(4, heading[1].length + 1);
      const Heading = `h${level}` as "h2" | "h3" | "h4";
      blocks.push(<Heading key={`heading-${index}`}>{markdownInline(heading[2])}</Heading>);
      index += 1;
      continue;
    }
    if (/^[-*]\s+/.test(line)) {
      const items: string[] = [];
      while (index < lines.length && /^[-*]\s+/.test(lines[index].trim())) {
        items.push(lines[index].trim().replace(/^[-*]\s+/, ""));
        index += 1;
      }
      blocks.push(<ul key={`list-${index}`}>{items.map((item, itemIndex) => <li key={itemIndex}>{markdownInline(item)}</li>)}</ul>);
      continue;
    }
    if (/^\d+\.\s+/.test(line)) {
      const items: string[] = [];
      while (index < lines.length && /^\d+\.\s+/.test(lines[index].trim())) {
        items.push(lines[index].trim().replace(/^\d+\.\s+/, ""));
        index += 1;
      }
      blocks.push(<ol key={`ordered-${index}`}>{items.map((item, itemIndex) => <li key={itemIndex}>{markdownInline(item)}</li>)}</ol>);
      continue;
    }
    if (/^---+$/.test(line.trim())) {
      blocks.push(<hr key={`rule-${index}`} />);
      index += 1;
      continue;
    }
    const paragraph = [line.trim()];
    index += 1;
    while (index < lines.length && lines[index].trim() && !/^(#{1,4})\s+|^```|^[-*]\s+|^\d+\.\s+|^---+$/.test(lines[index].trim())) {
      paragraph.push(lines[index].trim());
      index += 1;
    }
    blocks.push(<p key={`paragraph-${index}`}>{markdownInline(paragraph.join(" "))}</p>);
  }
  return <div className="markdown-preview">{blocks}</div>;
}

const lineSeparated = (values: string[]): string => values.join("\n");
const parseLineSeparated = (value: string): string[] => [...new Set(value.split("\n").map((item) => item.trim()).filter(Boolean))];

function SystemPanel({
  dashboard,
  health,
  busy,
  onRefresh,
  onReconcile,
  queueFilters,
  onQueueFiltersChange,
  onSaveQueueFilters,
  notificationsReady,
  onEnableNotifications,
  onTestNotification,
  providerProbe,
  onProbeProvider,
  browserSetup,
  extensionId,
  installationId,
  profileId,
  onExtensionIdChange,
  onInstallationIdChange,
  onProfileIdChange,
  onConnectBrowser,
  browserSessions,
  onCheckBrowser,
  onRetryBrowser,
  maintenanceResult,
  restorePreflight,
  onBackup,
  onExport,
  onDiagnostics,
  onPreflight,
  onQuit,
}: {
  dashboard: DashboardState;
  health: IntegrationHealth | null;
  busy: boolean;
  onRefresh: () => void;
  onReconcile: () => void;
  queueFilters: QueueFilters;
  onQueueFiltersChange: (filters: QueueFilters) => void;
  onSaveQueueFilters: () => void;
  notificationsReady: boolean;
  onEnableNotifications: () => void;
  onTestNotification: () => void;
  providerProbe: ProviderProbeResult | null;
  onProbeProvider: (provider: "codex" | "claude") => void;
  browserSetup: BrowserSetup | null;
  extensionId: string;
  installationId: string;
  profileId: string;
  onExtensionIdChange: (value: string) => void;
  onInstallationIdChange: (value: string) => void;
  onProfileIdChange: (value: string) => void;
  onConnectBrowser: () => void;
  browserSessions: BrowserSessionSummary[];
  onCheckBrowser: () => void;
  onRetryBrowser: (sessionId: string) => void;
  maintenanceResult: MaintenanceResult | null;
  restorePreflight: RestorePreflight | null;
  onBackup: () => void;
  onExport: () => void;
  onDiagnostics: () => void;
  onPreflight: () => void;
  onQuit: () => void;
}) {
  const checks = health
    ? [
        ["career-ops", health.careerOps],
        ["Codex CLI", health.codex],
        ["Claude CLI", health.claude],
      ] as const
    : [];
  let maintenanceCopy = "Create a backup, export your local summary, or produce diagnostics with role details and personal browser state omitted.";
  if (maintenanceResult) {
    maintenanceCopy = `Integrity ${maintenanceResult.integrity}; wrote ${maintenanceResult.path}`;
  }
  if (restorePreflight) {
    maintenanceCopy = `Latest backup is ${restorePreflight.integrity}, schema ${restorePreflight.schemaVersion}, with ${restorePreflight.roleCount} roles and ${restorePreflight.runCount} runs.`;
  }
  return (
    <section className="system-panel" aria-labelledby="system-title">
      <p className="eyebrow">Local integrations</p>
      <div className="system-panel__heading">
        <div>
          <h2 id="system-title">System status</h2>
          <p>HereForWork checks local tools without storing subscription credentials.</p>
        </div>
        <Button variant="outline" type="button" onClick={onRefresh} disabled={busy}>
          {busy ? "Checking…" : "Check again"}
        </Button>
      </div>

      {health ? (
        <dl className="integration-list">
          {checks.map(([label, check]) => (
            <div key={label}>
              <dt>
                <span className="status-dot" data-active={check.ready} aria-hidden="true" />
                {label}
              </dt>
              <dd>{check.detail}</dd>
            </div>
          ))}
        </dl>
      ) : (
        <p className="system-panel__empty">Run a check to verify the local adapter and provider CLIs.</p>
      )}

      <div className="history-control">
        <div>
          <h3>Canonical application history</h3>
          <p>
            {dashboard.handledCount > 0
              ? `${dashboard.handledCount} discovered roles are already represented in career-ops and hidden from the new queue.`
              : "Reconcile before preparing a role so prior applications never reappear as new."}
          </p>
        </div>
        <Button

          type="button"
          onClick={onReconcile}
          disabled={busy || health?.careerOps.ready !== true}
        >
          Reconcile history
        </Button>
      </div>
      <section className="queue-filter-settings" aria-labelledby="queue-filter-settings-title">
        <div className="queue-filter-settings__heading">
          <div>
            <h3 id="queue-filter-settings-title">Queue filters</h3>
            <p>Initialized from your verified career-ops profile. Changes apply to current unprepared roles and future imports.</p>
          </div>
          <Button  type="button" onClick={onSaveQueueFilters} disabled={busy}>
            Save filters
          </Button>
        </div>
        <div className="queue-filter-settings__fields">
          <Field>
            <FieldLabel htmlFor="queue-role-families">Role families</FieldLabel>
            <Textarea
              id="queue-role-families"
              key={lineSeparated(queueFilters.roleFamilies)}
              defaultValue={lineSeparated(queueFilters.roleFamilies)}
              onBlur={(event) => onQueueFiltersChange({ ...queueFilters, roleFamilies: parseLineSeparated(event.target.value) })}
              rows={4}
            />
          </Field>
          <Field>
            <FieldLabel htmlFor="queue-seniority">Seniority</FieldLabel>
            <Textarea
              id="queue-seniority"
              key={lineSeparated(queueFilters.seniority)}
              defaultValue={lineSeparated(queueFilters.seniority)}
              onBlur={(event) => onQueueFiltersChange({ ...queueFilters, seniority: parseLineSeparated(event.target.value) })}
              rows={4}
            />
          </Field>
          <Field>
            <FieldLabel htmlFor="queue-locations">Locations</FieldLabel>
            <Textarea
              id="queue-locations"
              key={lineSeparated(queueFilters.locations)}
              defaultValue={lineSeparated(queueFilters.locations)}
              onBlur={(event) => onQueueFiltersChange({ ...queueFilters, locations: parseLineSeparated(event.target.value) })}
              rows={4}
            />
          </Field>
        </div>
        <div className="queue-filter-settings__checks">
          <Field orientation="horizontal">
            <Checkbox
              id="queue-remote-allowed"
              checked={queueFilters.remoteAllowed}
              onCheckedChange={(checked) => onQueueFiltersChange({ ...queueFilters, remoteAllowed: checked })}
            />
            <FieldLabel htmlFor="queue-remote-allowed">Include remote roles</FieldLabel>
          </Field>
          <Field orientation="horizontal">
            <Checkbox
              id="queue-authorization-path"
              checked={queueFilters.requireAuthorizationPath}
              onCheckedChange={(checked) => onQueueFiltersChange({ ...queueFilters, requireAuthorizationPath: checked })}
            />
            <FieldLabel htmlFor="queue-authorization-path">Hide explicit authorization conflicts</FieldLabel>
          </Field>
        </div>
      </section>
      <section className="browser-setup" aria-labelledby="browser-setup-title">
        <div>
          <h3 id="browser-setup-title">Ordinary Chrome profile</h3>
          <p>
            Load the unpacked extension from <code>{browserSetup?.extensionDirectory ?? "Checking…"}</code>, then enter Chrome’s extension ID and the Installation ID shown in the extension popup. No WebDriver, debugging port, or automation flags are used.
          </p>
        </div>
        <div className="browser-setup__fields">
          <Field>
            <FieldLabel htmlFor="browser-profile">Chrome profile</FieldLabel>
            <Select value={profileId || null} onValueChange={(value) => onProfileIdChange(value ?? "")} disabled={busy}>
              <SelectTrigger id="browser-profile" className="w-full" aria-label="Chrome profile">
                <SelectValue placeholder="Select a profile" />
              </SelectTrigger>
              <SelectContent alignItemWithTrigger={false}>
              {browserSetup?.profiles.map((profile) => (
                  <SelectItem key={profile.id} value={profile.id}>{profile.name} · {profile.id}</SelectItem>
              ))}
              </SelectContent>
            </Select>
          </Field>
          <Field>
            <FieldLabel htmlFor="browser-extension-id">Extension ID</FieldLabel>
            <Input
              id="browser-extension-id"
              value={extensionId}
              onChange={(event) => onExtensionIdChange(event.target.value.trim().toLowerCase())}
              placeholder="32-letter Chrome extension ID"
              autoComplete="off"
              spellCheck={false}
            />
          </Field>
          <Field>
            <FieldLabel htmlFor="browser-installation-id">Installation ID</FieldLabel>
            <Input
              id="browser-installation-id"
              value={installationId}
              onChange={(event) => onInstallationIdChange(event.target.value.trim().toLowerCase())}
              placeholder="UUID shown in the extension popup"
              autoComplete="off"
              spellCheck={false}
            />
          </Field>
          <Button  type="button" onClick={onConnectBrowser} disabled={busy || !profileId || extensionId.length !== 32 || installationId.length !== 36}>
            Connect selected profile
          </Button>
        </div>
        {browserSetup?.approvedExtensionId ? (
          <p className="browser-setup__status">
            Connected to {browserSetup.profiles.find((profile) => profile.id === browserSetup.selectedProfileId)?.name ?? browserSetup.selectedProfileId}. Native host {browserSetup.nativeHostRegistered ? "registered" : "not registered"}.
            {browserSetup.lastConnectedAt ? ` Extension checked in ${formatRelative(browserSetup.lastConnectedAt)}.` : " Waiting for the extension to check in."}
          </p>
        ) : null}
        <div className="browser-check">
          <div>
            <h4>Live boundary check</h4>
            <p>In the selected ordinary Chrome profile, open any HTTPS application form. This check inspects field definitions, then releases the page. It never fills or finalizes.</p>
          </div>
          <Button variant="outline" type="button" onClick={onCheckBrowser} disabled={busy || !browserSetup?.approvedExtensionId}>
            Inspect active application page
          </Button>
        </div>
        <BrowserSessions sessions={browserSessions.filter((session) => session.purpose === "connection_check").slice(0, 1)} busy={busy} onRetry={onRetryBrowser} />
      </section>

      <div className="history-control history-control--sources">
        <div>
          <h3>Discovery schedules</h3>
          <p>
            Europe/Madrid windows are preserved and consolidated per source. The existing scheduled workflows still own execution during the parallel proof.
          </p>
        </div>
        <Badge variant="outline">Staged</Badge>
      </div>
      <dl className="source-list">
        {dashboard.sources.map((source) => (
          <div key={source.sourceId}>
            <dt>{source.displayName}</dt>
            <dd>
              {source.executionMode === "staged"
                ? `${source.actionRequiredCount} preserved catch-up ${source.actionRequiredCount === 1 ? "window" : "windows"}; source adapter not enabled.`
                : `${dashboard.pendingRunCount} operational ${dashboard.pendingRunCount === 1 ? "run" : "runs"} pending.`}
            </dd>
          </div>
        ))}
      </dl>
      {dashboard.recentRuns.length > 0 ? (
        <Collapsible className="run-details">
          <CollapsibleTrigger className="run-details__trigger">Recent run state</CollapsibleTrigger>
          <CollapsibleContent>
            <ol>
              {dashboard.recentRuns.map((run) => (
                <li key={run.id}>
                  <span>{run.sourceId}</span>
                  <span>{run.kind.replaceAll("_", " ")}</span>
                  <strong>{run.status.replaceAll("_", " ")}</strong>
                </li>
              ))}
            </ol>
          </CollapsibleContent>
        </Collapsible>
      ) : null}
      <div className="history-control">
        <div>
          <h3>Provider conformance</h3>
          <p>
            {providerProbe
              ? `${providerProbe.provider} returned the versioned structured contract successfully.`
              : "A live probe uses an ephemeral, tool-free session and sends no career data."}
          </p>
        </div>
        <div className="button-cluster">
          <Button variant="outline" type="button" onClick={() => onProbeProvider("codex")} disabled={busy || health?.codex.ready !== true}>
            Test Codex
          </Button>
          <Button variant="outline" type="button" onClick={() => onProbeProvider("claude")} disabled={busy || health?.claude.ready !== true}>
            Test Claude
          </Button>
        </div>
      </div>

      <div className="history-control">
        <div>
          <h3>macOS notifications</h3>
          <p>
            {notificationsReady
              ? "Permission is available for new-role, failure, and review-ready alerts."
              : "Permission is requested only when you choose to enable alerts."}
          </p>
        </div>
        <Button
          variant="outline"
          type="button"
          onClick={notificationsReady ? onTestNotification : onEnableNotifications}
          disabled={busy}
        >
          {notificationsReady ? "Send test alert" : "Enable alerts"}
        </Button>
      </div>

      <div className="history-control">
        <div>
          <h3>Backup and export</h3>
          <p>{maintenanceCopy}</p>
        </div>
        <div className="button-cluster">
          <Button variant="outline" type="button" onClick={onBackup} disabled={busy}>Create backup</Button>
          <Button variant="outline" type="button" onClick={onExport} disabled={busy}>Export summary</Button>
          <Button variant="outline" type="button" onClick={onDiagnostics} disabled={busy}>Export diagnostics</Button>
          <Button variant="outline" type="button" onClick={onPreflight} disabled={busy}>Check latest backup</Button>
        </div>
      </div>
      <div className="history-control">
        <div>
          <h3>Application lifecycle</h3>
          <p>Closing the window keeps enabled background work available. Quit stops the local core completely.</p>
        </div>
        <Button variant="outline" type="button" onClick={onQuit} disabled={busy}>Quit HereForWork</Button>
      </div>
    </section>
  );
}

export function App() {
  const [dashboard, setDashboard] = useState<DashboardState | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [view, setView] = useState<"queue" | "applications" | "system">("queue");
  const [health, setHealth] = useState<IntegrationHealth | null>(null);
  const [notificationsReady, setNotificationsReady] = useState(false);
  const [providerProbe, setProviderProbe] = useState<ProviderProbeResult | null>(null);
  const [selectedProvider, setSelectedProvider] = useState<"codex" | "claude">("codex");
  const [browserSetup, setBrowserSetup] = useState<BrowserSetup | null>(null);
  const [browserSessions, setBrowserSessions] = useState<BrowserSessionSummary[]>([]);
  const [extensionId, setExtensionId] = useState("");
  const [installationId, setInstallationId] = useState("");
  const [profileId, setProfileId] = useState("");
  const [maintenanceResult, setMaintenanceResult] = useState<MaintenanceResult | null>(null);
  const [restorePreflight, setRestorePreflight] = useState<RestorePreflight | null>(null);
  const [preparationDetail, setPreparationDetail] = useState<PreparationDetail | null>(null);
  const [queueFiltersDraft, setQueueFiltersDraft] = useState<QueueFilters | null>(null);
  const [undoPreparationId, setUndoPreparationId] = useState<string | null>(null);
  const [cancellationRequestedRoleId, setCancellationRequestedRoleId] = useState<string | null>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    let active = true;
    getDashboard()
      .then((state) => {
        if (!active) return;
        setDashboard(state);
        setQueueFiltersDraft(state.queueFilters);
      })
      .catch((reason: unknown) => {
        if (active) setError(reason instanceof Error ? reason.message : String(reason));
      });
    return () => {
      active = false;
    };
  }, []);

  useEffect(() => {
    if (view !== "applications" && view !== "system") return;
    let active = true;
    const refresh = () => {
      void Promise.all([getBrowserSessions(), getBrowserSetup()]).then(([sessions, setup]) => {
        if (!active) return;
        setBrowserSessions(sessions);
        setBrowserSetup(setup);
      }).catch(() => {
        // The primary action handler reports errors. Background refresh remains quiet.
      });
    };
    refresh();
    const interval = window.setInterval(refresh, 1_000);
    return () => {
      active = false;
      window.clearInterval(interval);
    };
  }, [view]);

  const handleImport = async (file: File) => {
    setBusy(true);
    setError(null);
    try {
      const payload = await file.text();
      await importDataset(payload);
      const next = await getDashboard();
      setDashboard(next);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setBusy(false);
      if (fileInputRef.current) fileInputRef.current.value = "";
    }
  };

  const toggleBackground = async () => {
    if (!dashboard) return;
    setBusy(true);
    try {
      setDashboard(await setBackgroundEnabled(!dashboard.backgroundEnabled));
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setBusy(false);
    }
  };

  const persistQueueFilters = async () => {
    if (!queueFiltersDraft) return;
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const next = await saveQueueFilters(queueFiltersDraft);
      setDashboard(next);
      setQueueFiltersDraft(next.queueFilters);
      setNotice("Queue filters saved. Current unprepared roles and future imports now use them.");
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setBusy(false);
    }
  };

  const refreshHealth = async () => {
    setBusy(true);
    setError(null);
    try {
      const [nextHealth, nextBrowserSetup] = await Promise.all([checkIntegrations(), getBrowserSetup()]);
      setHealth(nextHealth);
      setBrowserSetup(nextBrowserSetup);
      setExtensionId(nextBrowserSetup.approvedExtensionId ?? nextBrowserSetup.pendingExtensionId ?? "");
      setInstallationId(nextBrowserSetup.approvedInstallationId ?? nextBrowserSetup.pendingInstallationId ?? "");
      setProfileId(nextBrowserSetup.selectedProfileId ?? nextBrowserSetup.profiles[0]?.id ?? "");
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setBusy(false);
    }
  };

  const reconcileHistory = async () => {
    setBusy(true);
    setError(null);
    try {
      await reconcileApplicationHistory();
      const next = await getDashboard();
      setDashboard(next);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setBusy(false);
    }
  };

  const enableNotifications = async () => {
    setBusy(true);
    setError(null);
    try {
      setNotificationsReady(await requestNotificationPermission());
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setBusy(false);
    }
  };

  const testNotification = async () => {
    try {
      await sendTestNotification();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  };

  const probeProvider = async (provider: "codex" | "claude") => {
    setBusy(true);
    setError(null);
    try {
      setProviderProbe(await runProviderProbe(provider));
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setBusy(false);
    }
  };

  const connectBrowser = async () => {
    setBusy(true);
    setError(null);
    try {
      setBrowserSetup(await configureBrowserBridge(extensionId, installationId, profileId));
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setBusy(false);
    }
  };

  const checkBrowser = async () => {
    setBusy(true);
    setError(null);
    try {
      await startBrowserConnectionCheck();
      setBrowserSessions(await getBrowserSessions());
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setBusy(false);
    }
  };

  const retryBrowser = async (sessionId: string) => {
    setBusy(true);
    setError(null);
    try {
      await retryBrowserSession(sessionId);
      setBrowserSessions(await getBrowserSessions());
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setBusy(false);
    }
  };

  const continuePreparedRole = async (preparationId: string) => {
    setBusy(true);
    setError(null);
    try {
      await continueInBrowser(preparationId);
      setBrowserSessions(await getBrowserSessions());
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setBusy(false);
    }
  };

  const confirmApplied = async (sessionId: string) => {
    const session = browserSessions.find((item) => item.id === sessionId);
    if (session?.status !== "submitted_tracking_pending") {
      const confirmed = window.confirm("Confirm that you physically clicked Submit and the application was accepted by the site. HereForWork will record Applied in canonical career-ops history.");
      if (!confirmed) return;
    }
    setBusy(true);
    setError(null);
    try {
      await confirmApplicationApplied(sessionId);
      const [nextDashboard, nextSessions] = await Promise.all([getDashboard(), getBrowserSessions()]);
      setDashboard(nextDashboard);
      setBrowserSessions(nextSessions);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setBusy(false);
    }
  };

  const dismissQueueRole = async (roleId: string) => {
    setBusy(true);
    setError(null);
    try {
      setDashboard(await dismissRole(roleId));
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setBusy(false);
    }
  };

  const prepareQueueRole = async (roleId: string) => {
    setBusy(true);
    setError(null);
    setNotice(null);
    setDashboard((current) => current ? {
      ...current,
      roles: current.roles.map((role) => role.id === roleId ? { ...role, preparationState: "preparing" } : role),
    } : current);
    try {
      const outcome = await prepareRole(roleId, selectedProvider);
      setDashboard(outcome.dashboard);
      setQueueFiltersDraft(outcome.dashboard.queueFilters);
      setNotice(outcome.message);
      if (outcome.disposition === "browser_started" || outcome.disposition === "prepared_browser_action_required") {
        setView("applications");
      } else {
        setView("queue");
      }
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
      try {
        setDashboard(await getDashboard());
      } catch {
        // Preserve the actionable preparation error when refreshing state also fails.
      }
    } finally {
      setCancellationRequestedRoleId(null);
      setBusy(false);
    }
  };

  const cancelQueuePreparation = async (roleId: string) => {
    if (cancellationRequestedRoleId === roleId) return;
    setCancellationRequestedRoleId(roleId);
    try {
      const requested = await cancelPreparation(roleId);
      if (!requested) setError("The preparation had already finished or stopped.");
    } catch (reason) {
      setCancellationRequestedRoleId(null);
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  };

  const restoreDismissedRole = async (roleId: string) => {
    setBusy(true);
    setError(null);
    try {
      setDashboard(await undoDismissal(roleId));
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setBusy(false);
    }
  };

  const maintain = async (kind: "backup" | "export" | "diagnostics") => {
    setBusy(true);
    setError(null);
    try {
      let result: MaintenanceResult;
      if (kind === "backup") {
        result = await createOperationalBackup();
      } else if (kind === "export") {
        result = await exportOperationalSummary();
      } else {
        result = await exportRedactedDiagnostics();
      }
      setMaintenanceResult(result);
      setRestorePreflight(null);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setBusy(false);
    }
  };

  const preflightBackup = async () => {
    setBusy(true);
    setError(null);
    try {
      setRestorePreflight(await preflightLatestBackup());
      setMaintenanceResult(null);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setBusy(false);
    }
  };

  const showPreparationDetail = async (preparationId: string) => {
    setBusy(true);
    setError(null);
    try {
      setPreparationDetail(await getPreparationDetail(preparationId));
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setBusy(false);
    }
  };

  const discardPreparation = async (preparationId: string) => {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const next = await undoPreparation(preparationId);
      setDashboard(next);
      setBrowserSessions(await getBrowserSessions());
      setPreparationDetail((current) => current?.preparationId === preparationId ? null : current);
      setUndoPreparationId(null);
      setNotice("Preparation discarded. career-ops history was updated and generated files were deleted.");
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setBusy(false);
    }
  };

  const openArtifact = async (preparationId: string, artifact: "report" | "cv") => {
    try {
      await openPreparationArtifact(preparationId, artifact);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  };

  if (!dashboard) {
    return <main className="loading-shell" aria-live="polite">Opening your queue…</main>;
  }

  const emptyQueueContent = dashboard.preparations.length > 0 || dashboard.recentlyDismissed.length > 0
    ? <HandledQueue />
    : <EmptyQueue onImport={() => fileInputRef.current?.click()} />;

  let mainContent: ReactNode;
  if (view === "system") {
    mainContent = (
      <main className="system-workspace">
        <SystemPanel
          dashboard={dashboard}
          health={health}
          busy={busy}
          onRefresh={() => void refreshHealth()}
          onReconcile={() => void reconcileHistory()}
          queueFilters={queueFiltersDraft ?? dashboard.queueFilters}
          onQueueFiltersChange={setQueueFiltersDraft}
          onSaveQueueFilters={() => void persistQueueFilters()}
          notificationsReady={notificationsReady}
          onEnableNotifications={() => void enableNotifications()}
          onTestNotification={() => void testNotification()}
          providerProbe={providerProbe}
          onProbeProvider={(provider) => void probeProvider(provider)}
          browserSetup={browserSetup}
          extensionId={extensionId}
          installationId={installationId}
          profileId={profileId}
          onExtensionIdChange={setExtensionId}
          onInstallationIdChange={setInstallationId}
          onProfileIdChange={setProfileId}
          onConnectBrowser={() => void connectBrowser()}
          browserSessions={browserSessions}
          onCheckBrowser={() => void checkBrowser()}
          onRetryBrowser={(sessionId) => void retryBrowser(sessionId)}
          maintenanceResult={maintenanceResult}
          restorePreflight={restorePreflight}
          onBackup={() => void maintain("backup")}
          onExport={() => void maintain("export")}
          onDiagnostics={() => void maintain("diagnostics")}
          onPreflight={() => void preflightBackup()}
          onQuit={() => void quitApp()}
        />
      </main>
    );
  } else if (view === "applications") {
    mainContent = (
      <main className="status-workspace" aria-labelledby="applications-title">
        <p className="eyebrow">Application workflow</p>
        <h2 id="applications-title">Applications</h2>
        <p>One current status per application. Open Details when you want the full career-ops report.</p>
        {dashboard.preparations.length > 0 ? (
          <ol className="preparation-list" aria-label="Application preparations">
            {dashboard.preparations.map((item) => {
              const latestSession = browserSessions
                .filter((session) => session.purpose === "application" && session.preparationId === item.id)
                .sort((left, right) => right.updatedAt.localeCompare(left.updatedAt))[0];
              const status = latestSession ? browserStatusLabel[latestSession.status] : item.step.replaceAll("_", " ");
              const browserActive = latestSession && !["review_required", "action_required", "submitted_tracking_pending", "applied_recorded"].includes(latestSession.status);
              return (
              <li key={item.id}>
                <div className="preparation-list__identity">
                  <strong>{item.title}</strong>
                  <span>{item.company} · {item.provider}</span>
                </div>
                <Badge variant="outline">{status}</Badge>
                {item.status === "completed" ? (
                  <div className="preparation-list__actions">
                    <Button variant="outline" type="button" onClick={() => void showPreparationDetail(item.id)} disabled={busy}>
                      Details
                    </Button>
                    <Button variant="outline" type="button" onClick={() => void openArtifact(item.id, "cv")} disabled={busy}>
                      Open CV
                    </Button>
                    {!latestSession ? (
                    <Button

                      type="button"
                      onClick={() => void continuePreparedRole(item.id)}
                      disabled={busy || !browserSetup?.approvedInstallationId}
                    >
                      Open in browser
                    </Button>
                    ) : null}
                    {latestSession?.status === "action_required" ? (
                      <Button  type="button" onClick={() => void retryBrowser(latestSession.id)} disabled={busy}>
                        Retry browser
                      </Button>
                    ) : null}
                    {latestSession?.status === "review_required" ? (
                      <Button  type="button" onClick={() => void confirmApplied(latestSession.id)} disabled={busy}>
                        I submitted this application
                      </Button>
                    ) : null}
                    {latestSession?.status === "submitted_tracking_pending" ? (
                      <Button  type="button" onClick={() => void confirmApplied(latestSession.id)} disabled={busy}>
                        Retry tracking update
                      </Button>
                    ) : null}
                    {latestSession?.status !== "applied_recorded" ? (
                      <Button variant="destructive" type="button" onClick={() => setUndoPreparationId(item.id)} disabled={busy || Boolean(browserActive)}>
                        Undo preparation
                      </Button>
                    ) : null}
                  </div>
                ) : null}
                {item.errorClass ? <p className="browser-session-list__error">{item.errorClass.replaceAll("_", " ")}</p> : null}
                {undoPreparationId === item.id ? (
                  <div className="preparation-list__confirmation" role="alert">
                    <p>Discard this role and permanently delete its generated report and tailored CV files?</p>
                    <div className="button-cluster">
                      <Button variant="destructive" type="button" onClick={() => void discardPreparation(item.id)} disabled={busy}>
                        Discard preparation
                      </Button>
                      <Button variant="outline" type="button" onClick={() => setUndoPreparationId(null)} disabled={busy}>
                        Keep preparation
                      </Button>
                    </div>
                  </div>
                ) : null}
              </li>
              );
            })}
          </ol>
        ) : (
          <Empty className="empty-state empty-state--compact" role="region" aria-labelledby="applications-empty-title">
            <EmptyHeader>
              <EmptyTitle><h3 id="applications-empty-title">No prepared applications yet.</h3></EmptyTitle>
              <EmptyDescription>Prepare a suitable role from Queue. HereForWork will add it here only after career-ops creates the verified materials.</EmptyDescription>
            </EmptyHeader>
          </Empty>
        )}
        {preparationDetail ? (
          <Sheet open onOpenChange={(open) => {
            if (!open) setPreparationDetail(null);
          }}>
            <SheetContent className="preparation-detail" aria-labelledby="preparation-detail-title">
              <SheetHeader className="preparation-detail__heading">
              <div>
                <p className="eyebrow">career-ops report</p>
                  <SheetTitle id="preparation-detail-title">Preparation details</SheetTitle>
              </div>
              </SheetHeader>
              <div className="preparation-detail__actions">
                <Button variant="outline" type="button" onClick={() => void openArtifact(preparationDetail.preparationId, "report")}>Open original report</Button>
                <Button variant="outline" type="button" onClick={() => void openArtifact(preparationDetail.preparationId, "cv")}>Open verified CV</Button>
              </div>
              <MarkdownPreview markdown={preparationDetail.reportMarkdown} />
            </SheetContent>
          </Sheet>
        ) : null}
      </main>
    );
  } else {
    mainContent = (
      <main className="queue-workspace">
        <section className="queue-pane" aria-labelledby="queue-title">
          <div className="queue-heading">
            <div>
              <p className="eyebrow">Today’s queue</p>
              <h2 id="queue-title">Review queue</h2>
            </div>
            <div className="queue-heading__actions">
              <Field className="provider-select">
                <FieldLabel htmlFor="preparation-provider">Preparation provider</FieldLabel>
                <Select
                  value={selectedProvider}
                  onValueChange={(value) => {
                    if (value === "codex" || value === "claude") setSelectedProvider(value);
                  }}
                  disabled={busy}
                >
                  <SelectTrigger id="preparation-provider" size="sm" aria-label="Preparation provider">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="codex">Codex</SelectItem>
                    <SelectItem value="claude">Claude</SelectItem>
                  </SelectContent>
                </Select>
              </Field>
              <Button variant="outline" type="button" onClick={() => fileInputRef.current?.click()} disabled={busy}>
                Import
              </Button>
            </div>
            <Input
              ref={fileInputRef}
              className="visually-hidden"
              type="file"
              accept="application/json,.json"
              onChange={(event) => {
                const file = event.target.files?.[0];
                if (file) void handleImport(file);
              }}
            />
          </div>

          {dashboard.recentlyDismissed[0] ? (
            <Alert className="undo-strip" role="status" aria-live="polite">
              <AlertDescription><strong>{dashboard.recentlyDismissed[0].title}</strong> was recorded as Discarded in career-ops.</AlertDescription>
              <AlertAction>
                <Button variant="outline" type="button" onClick={() => void restoreDismissedRole(dashboard.recentlyDismissed[0].id)} disabled={busy}>
                  Undo
                </Button>
              </AlertAction>
            </Alert>
          ) : null}
          {dashboard.roles.length === 0 ? (
            emptyQueueContent
          ) : (
            <div className="queue-groups">
              {groupOrder.map((group) => {
                const roles = dashboard.roles.filter((role) => role.queueGroup === group);
                if (roles.length === 0) return null;
                return (
                  <section className="queue-group" key={group} aria-labelledby={`${group}-heading`}>
                    <div className="queue-group__heading">
                      <div>
                        <h3 id={`${group}-heading`}>{groupCopy[group].title}</h3>
                        <p>{groupCopy[group].description}</p>
                      </div>
                      <span aria-label={`${roles.length} roles`}>{roles.length}</span>
                    </div>
                    <div className="role-list">
                      {roles.map((role) => (
                        <RoleRow
                          key={role.id}
                          role={role}
                          canPrepare={dashboard.adapterStatus === "ready" && role.applicationUrl !== null}
                          canDismiss={dashboard.adapterStatus === "ready"}
                          busy={busy}
                          onPrepare={(roleId) => void prepareQueueRole(roleId)}
                          onCancel={(roleId) => void cancelQueuePreparation(roleId)}
                          onDismiss={(roleId) => void dismissQueueRole(roleId)}
                        />
                      ))}
                    </div>
                  </section>
                );
              })}
            </div>
          )}
          {dashboard.roles.length > 0 ? (
            <p className="queue-action-status" id="queue-action-status">
              Prepare sends bounded career-ops context to the selected tool-free subscription CLI, then career-ops verifies and owns the report and CV. Dismiss records canonical history immediately and can be undone. Neither action submits an application.
            </p>
          ) : null}
        </section>
      </main>
    );
  }

  return (
    <Tabs
      className="app-shell gap-0"
      value={view === "system" ? null : view}
      onValueChange={(value) => {
        if (value === "queue" || value === "applications") setView(value);
      }}
    >
      <header className="topbar">
        <div className="brand-lockup">
          <span className="brand-mark" aria-hidden="true">H</span>
          <div>
            <h1>HereForWork</h1>
            <p>Review what deserves your attention.</p>
          </div>
        </div>
        <nav className="primary-nav" aria-label="Primary">
          <TabsList className="primary-nav__list" variant="line">
            {(["queue", "applications"] as const).map((destination) => (
              <TabsTrigger className="primary-nav__item" value={destination} key={destination}>
                {destination}
              </TabsTrigger>
            ))}
          </TabsList>
        </nav>
        <div className="topbar__actions">
          <span className="status-line">
            <span className="status-dot" data-active={dashboard.backgroundEnabled} aria-hidden="true" />
            {dashboard.backgroundEnabled ? "Background checks on" : "Background checks off"}
          </span>
          <Button variant="outline" type="button" onClick={toggleBackground} disabled={busy}>
            {dashboard.backgroundEnabled ? "Turn off" : "Turn on"}
          </Button>
          <Button
            variant="outline"
            type="button"
            aria-pressed={view === "system"}
            onClick={() => {
              setView("system");
              if (!health) void refreshHealth();
            }}
          >
            System
          </Button>
        </div>
      </header>

      {error ? (
        <Alert className="error-banner" variant="destructive">
          <AlertTitle>That didn’t work.</AlertTitle>
          <AlertDescription>{error}</AlertDescription>
          <AlertAction>
            <Button variant="ghost" size="icon-xs" type="button" onClick={() => setError(null)} aria-label="Dismiss error">×</Button>
          </AlertAction>
        </Alert>
      ) : null}

      {notice ? (
        <Alert className="notice-banner" role="status">
          <AlertDescription>{notice}</AlertDescription>
          <AlertAction>
            <Button variant="ghost" size="icon-xs" type="button" onClick={() => setNotice(null)} aria-label="Dismiss notice">×</Button>
          </AlertAction>
        </Alert>
      ) : null}

      {view === "system" ? mainContent : (
        <TabsContent className="contents" value={view}>
          {mainContent}
        </TabsContent>
      )}
    </Tabs>
  );
}
