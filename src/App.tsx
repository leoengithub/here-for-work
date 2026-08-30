import { useEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
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
  setBackgroundEnabled,
  startBrowserConnectionCheck,
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
  if (!applicationUrl) return "Portal";
  try {
    const hostname = new URL(applicationUrl).hostname;
    if (hostname.includes("ashbyhq.com")) return "Ashby";
    if (hostname.includes("greenhouse.io")) return "Greenhouse";
    if (hostname.includes("lever.co")) return "Lever";
  } catch {
    return "Portal";
  }
  return "Portal";
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
      <span className="state-pill">{role.preparationState.replaceAll("_", " ")}</span>
      <span className="role-row__actions">
        <button
          className="button button--primary"
          type="button"
          disabled={preparing ? false : !canPrepare || busy}
          aria-describedby="queue-action-status"
          onClick={() => preparing ? onCancel(role.id) : onPrepare(role.id)}
        >
          {preparing ? "Cancel preparation" : "Prepare"}
        </button>
        <button
          className="button button--quiet"
          type="button"
          disabled={!canDismiss || busy}
          aria-describedby="queue-action-status"
          onClick={() => onDismiss(role.id)}
        >
          Dismiss
        </button>
      </span>
    </article>
  );
}

function HandledQueue() {
  return (
    <section className="empty-state" aria-labelledby="handled-empty-title">
      <div className="empty-state__mark" aria-hidden="true">H</div>
      <h2 id="handled-empty-title">No new roles are waiting.</h2>
      <p>Prepared and dismissed roles remain visible in Applications and canonical career-ops history.</p>
    </section>
  );
}

function EmptyQueue({ onImport }: { onImport: () => void }) {
  return (
    <section className="empty-state" aria-labelledby="empty-title">
      <div className="empty-state__mark" aria-hidden="true">H</div>
      <h2 id="empty-title">Your review queue is ready for its first run.</h2>
      <p>
        Import the two discovery snapshots to reconcile roles without changing career-ops, Gmail, or application history.
      </p>
      <button className="button button--primary" type="button" onClick={onImport}>
        Import discovery snapshot
      </button>
      <p className="empty-state__hint">JSON stays on this Mac and is written only to HereForWork’s operational store.</p>
    </section>
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
        No browser session yet. Pair the extension, open a supported ATS page, then run the connection check from System.
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
              <strong>{session.pageTitle ?? "Active ATS page"}</strong>
              <span>{session.ats ? session.ats.toUpperCase() : "Waiting for a supported page"}</span>
            </div>
            <span className="state-pill">{browserStatusLabel[session.status]}</span>
          </div>
          {session.status === "connection_verified" ? (
            <p>
              {session.fieldCount} fields inspected. {session.safeFieldCount} can accept verified facts; {session.needsUserCount} stay with you. Nothing was filled or finalized.
            </p>
          ) : null}
          {session.status === "review_required" ? (
            <div>
              <p>The page is released for your review. HereForWork cannot submit it.</p>
              {session.reviewItems?.length ? (
                <ul>
                  {session.reviewItems.map((item) => (
                    <li key={item.fieldId}>
                      <strong>{item.label}</strong>: {item.decision.replaceAll("_", " ")}
                      {item.answer ? <span> — {item.answer}</span> : null}
                      {item.reason ? <span> — {item.reason}</span> : null}
                      {item.provenance?.length ? <small>Sources: {item.provenance.join(", ")}</small> : null}
                    </li>
                  ))}
                </ul>
              ) : null}
              {session.purpose === "application" && onConfirmApplied && isLatestApplicationAttempt ? (
                <button className="button button--primary" type="button" onClick={() => onConfirmApplied(session.id)} disabled={busy}>
                  I submitted this application
                </button>
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
              <button className="button button--primary" type="button" onClick={() => onConfirmApplied(session.id)} disabled={busy}>
                Retry tracking update
              </button>
            </div>
          ) : null}
          {session.errorCode ? <p className="browser-session-list__error">{session.errorCode.replaceAll("_", " ")}</p> : null}
          {session.status === "action_required" ? (
            <button className="button button--quiet" type="button" onClick={() => onRetry(session.id)} disabled={busy}>
              Retry session
            </button>
          ) : null}
        </li>
        );
      })}
    </ol>
  );
}

function SystemPanel({
  dashboard,
  health,
  busy,
  onRefresh,
  onReconcile,
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
        <button className="button button--quiet" type="button" onClick={onRefresh} disabled={busy}>
          {busy ? "Checking…" : "Check again"}
        </button>
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
        <button
          className="button button--primary"
          type="button"
          onClick={onReconcile}
          disabled={busy || health?.careerOps.ready !== true}
        >
          Reconcile history
        </button>
      </div>
      <section className="browser-setup" aria-labelledby="browser-setup-title">
        <div>
          <h3 id="browser-setup-title">Ordinary Chrome profile</h3>
          <p>
            Load the unpacked extension from <code>{browserSetup?.extensionDirectory ?? "Checking…"}</code>, then enter Chrome’s extension ID and the Installation ID shown in the extension popup. No WebDriver, debugging port, or automation flags are used.
          </p>
        </div>
        <div className="browser-setup__fields">
          <label>
            Chrome profile
            <select value={profileId} onChange={(event) => onProfileIdChange(event.target.value)} disabled={busy}>
              <option value="">Select a profile</option>
              {browserSetup?.profiles.map((profile) => (
                <option key={profile.id} value={profile.id}>{profile.name} · {profile.id}</option>
              ))}
            </select>
          </label>
          <label>
            Extension ID
            <input
              value={extensionId}
              onChange={(event) => onExtensionIdChange(event.target.value.trim().toLowerCase())}
              placeholder="32-letter Chrome extension ID"
              autoComplete="off"
              spellCheck={false}
            />
          </label>
          <label>
            Installation ID
            <input
              value={installationId}
              onChange={(event) => onInstallationIdChange(event.target.value.trim().toLowerCase())}
              placeholder="UUID shown in the extension popup"
              autoComplete="off"
              spellCheck={false}
            />
          </label>
          <button className="button button--primary" type="button" onClick={onConnectBrowser} disabled={busy || !profileId || extensionId.length !== 32 || installationId.length !== 36}>
            Connect selected profile
          </button>
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
            <p>In the selected ordinary Chrome profile, open an Ashby, Greenhouse, or Lever application page. This check inspects field definitions, then releases the page. It never fills or finalizes.</p>
          </div>
          <button className="button button--quiet" type="button" onClick={onCheckBrowser} disabled={busy || !browserSetup?.approvedExtensionId}>
            Inspect active ATS page
          </button>
        </div>
        <BrowserSessions sessions={browserSessions.slice(0, 1)} busy={busy} onRetry={onRetryBrowser} />
      </section>

      <div className="history-control history-control--sources">
        <div>
          <h3>Discovery schedules</h3>
          <p>
            Europe/Madrid windows are preserved and consolidated per source. The existing scheduled workflows still own execution during the parallel proof.
          </p>
        </div>
        <span className="state-pill">Staged</span>
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
        <details className="run-details">
          <summary>Recent run state</summary>
          <ol>
            {dashboard.recentRuns.map((run) => (
              <li key={run.id}>
                <span>{run.sourceId}</span>
                <span>{run.kind.replaceAll("_", " ")}</span>
                <strong>{run.status.replaceAll("_", " ")}</strong>
              </li>
            ))}
          </ol>
        </details>
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
          <button className="button button--quiet" type="button" onClick={() => onProbeProvider("codex")} disabled={busy || health?.codex.ready !== true}>
            Test Codex
          </button>
          <button className="button button--quiet" type="button" onClick={() => onProbeProvider("claude")} disabled={busy || health?.claude.ready !== true}>
            Test Claude
          </button>
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
        <button
          className="button button--quiet"
          type="button"
          onClick={notificationsReady ? onTestNotification : onEnableNotifications}
          disabled={busy}
        >
          {notificationsReady ? "Send test alert" : "Enable alerts"}
        </button>
      </div>

      <section className="activity-panel" aria-labelledby="activity-title">
        <h3 id="activity-title">Recent activity</h3>
        <ol>
          {dashboard.activity.map((entry) => (
            <li key={entry.id}>
              <span>{entry.message}</span>
              <time dateTime={entry.occurredAt}>{formatRelative(entry.occurredAt)}</time>
            </li>
          ))}
        </ol>
      </section>
      <div className="history-control">
        <div>
          <h3>Backup and export</h3>
          <p>{maintenanceCopy}</p>
        </div>
        <div className="button-cluster">
          <button className="button button--quiet" type="button" onClick={onBackup} disabled={busy}>Create backup</button>
          <button className="button button--quiet" type="button" onClick={onExport} disabled={busy}>Export summary</button>
          <button className="button button--quiet" type="button" onClick={onDiagnostics} disabled={busy}>Export diagnostics</button>
          <button className="button button--quiet" type="button" onClick={onPreflight} disabled={busy}>Check latest backup</button>
        </div>
      </div>
      <div className="history-control">
        <div>
          <h3>Application lifecycle</h3>
          <p>Closing the window keeps enabled background work available. Quit stops the local core completely.</p>
        </div>
        <button className="button button--quiet" type="button" onClick={onQuit} disabled={busy}>Quit HereForWork</button>
      </div>
    </section>
  );
}

export function App() {
  const [dashboard, setDashboard] = useState<DashboardState | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [view, setView] = useState<"queue" | "applications" | "activity" | "system">("queue");
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
  const [cancellationRequestedRoleId, setCancellationRequestedRoleId] = useState<string | null>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    let active = true;
    getDashboard()
      .then((state) => {
        if (!active) return;
        setDashboard(state);
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
    setDashboard((current) => current ? {
      ...current,
      roles: current.roles.map((role) => role.id === roleId ? { ...role, preparationState: "preparing" } : role),
    } : current);
    try {
      setDashboard(await prepareRole(roleId, selectedProvider));
      setView("applications");
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
        <p>Prepared materials and browser work stay here through human review and outcome confirmation.</p>
        <dl className="decision-facts">
          <div><dt>Prepared materials</dt><dd>{dashboard.preparations.filter((item) => item.status === "completed").length} complete</dd></div>
          <div><dt>Recoverable blockers</dt><dd>{dashboard.preparations.filter((item) => item.status === "action_required").length + browserSessions.filter((session) => session.status === "action_required").length} active</dd></div>
          <div><dt>Canonical writes</dt><dd>Discard and Undo are available. Applied remains gated behind a completed form and your confirmation.</dd></div>
        </dl>
        {dashboard.preparations.length > 0 ? (
          <ol className="preparation-list" aria-label="Application preparations">
            {dashboard.preparations.map((item) => {
              const itemSessions = browserSessions.filter((session) => session.preparationId === item.id);
              const canRefillForReview = itemSessions.length > 0 && itemSessions.every((session) => session.status === "review_required");
              const browserFlowUnavailable = itemSessions.length > 0 && !canRefillForReview;
              return (
              <li key={item.id}>
                <div>
                  <strong>{item.title}</strong>
                  <span>{item.company} · {item.provider}</span>
                </div>
                <span className="state-pill">{item.step.replaceAll("_", " ")}</span>
                {item.status === "completed" ? (
                  <div>
                    <p>Report and verified CV are stored in career-ops. Continue in your selected ordinary Chrome profile.</p>
                    <div className="button-cluster">
                      <button className="button button--quiet" type="button" onClick={() => void showPreparationDetail(item.id)} disabled={busy}>
                        View report
                      </button>
                      <button className="button button--quiet" type="button" onClick={() => void openArtifact(item.id, "cv")} disabled={busy}>
                        Open verified CV
                      </button>
                    </div>
                    <button
                      className="button button--primary"
                      type="button"
                      onClick={() => void continuePreparedRole(item.id)}
                      disabled={busy || !browserSetup?.approvedInstallationId || browserFlowUnavailable}
                    >
                      {canRefillForReview ? "Refill for review" : browserFlowUnavailable ? "Browser flow started" : "Continue in browser"}
                    </button>
                  </div>
                ) : null}
                {item.errorClass ? <p className="browser-session-list__error">{item.errorClass.replaceAll("_", " ")}</p> : null}
              </li>
              );
            })}
          </ol>
        ) : null}
        {preparationDetail ? (
          <section className="preparation-detail" aria-labelledby="preparation-detail-title">
            <div className="preparation-detail__heading">
              <h3 id="preparation-detail-title">career-ops report</h3>
              <button className="button button--quiet" type="button" onClick={() => void openArtifact(preparationDetail.preparationId, "report")}>Open original</button>
            </div>
            <p><code>{preparationDetail.reportPath}</code></p>
            <pre tabIndex={0}>{preparationDetail.reportMarkdown}</pre>
          </section>
        ) : null}
        <BrowserSessions
          sessions={browserSessions}
          busy={busy}
          onRetry={(sessionId) => void retryBrowser(sessionId)}
          onConfirmApplied={(sessionId) => void confirmApplied(sessionId)}
        />
      </main>
    );
  } else if (view === "activity") {
    mainContent = (
      <main className="activity-workspace">
        <section className="activity-panel" aria-labelledby="activity-title">
          <p className="eyebrow">Operational history</p>
          <h2 id="activity-title">Activity</h2>
          <p className="activity-summary">
            {dashboard.actionRequiredRunCount} preserved discovery {dashboard.actionRequiredRunCount === 1 ? "window needs" : "windows need"} source-adapter cutover; {dashboard.pendingRunCount} executable runs are pending.
          </p>
          <ol>
            {dashboard.activity.map((entry) => (
              <li key={entry.id}>
                <span>{entry.message}</span>
                <time dateTime={entry.occurredAt}>{formatRelative(entry.occurredAt)}</time>
              </li>
            ))}
          </ol>
        </section>
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
              <label className="provider-select">
                Preparation provider
                <select value={selectedProvider} onChange={(event) => setSelectedProvider(event.target.value as "codex" | "claude")} disabled={busy}>
                  <option value="codex">Codex</option>
                  <option value="claude">Claude</option>
                </select>
              </label>
              <button className="button button--quiet" type="button" onClick={() => fileInputRef.current?.click()} disabled={busy}>
                Import
              </button>
            </div>
            <input
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
            <aside className="undo-strip" aria-live="polite">
              <span><strong>{dashboard.recentlyDismissed[0].title}</strong> was recorded as Discarded in career-ops.</span>
              <button className="button button--quiet" type="button" onClick={() => void restoreDismissedRole(dashboard.recentlyDismissed[0].id)} disabled={busy}>
                Undo
              </button>
            </aside>
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
    <div className="app-shell">
      <header className="topbar">
        <div className="brand-lockup">
          <span className="brand-mark" aria-hidden="true">H</span>
          <div>
            <h1>HereForWork</h1>
            <p>Review what deserves your attention.</p>
          </div>
        </div>
        <nav className="primary-nav" aria-label="Primary">
          {(["queue", "applications", "activity"] as const).map((destination) => (
            <button
              className="primary-nav__item"
              type="button"
              key={destination}
              data-active={view === destination}
              aria-current={view === destination ? "page" : undefined}
              onClick={() => setView(destination)}
            >
              {destination}
            </button>
          ))}
        </nav>
        <div className="topbar__actions">
          <span className="status-line">
            <span className="status-dot" data-active={dashboard.backgroundEnabled} aria-hidden="true" />
            {dashboard.backgroundEnabled ? "Background checks on" : "Background checks off"}
          </span>
          <button className="button button--quiet" type="button" onClick={toggleBackground} disabled={busy}>
            {dashboard.backgroundEnabled ? "Turn off" : "Turn on"}
          </button>
          <button
            className="button button--quiet"
            type="button"
            aria-pressed={view === "system"}
            onClick={() => {
              setView("system");
              if (!health) void refreshHealth();
            }}
          >
            System
          </button>
        </div>
      </header>

      {error ? (
        <div className="error-banner" role="alert">
          <strong>That didn’t work.</strong>
          <span>{error}</span>
          <button type="button" onClick={() => setError(null)} aria-label="Dismiss error">×</button>
        </div>
      ) : null}

      {mainContent}
    </div>
  );
}
