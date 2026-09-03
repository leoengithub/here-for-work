import { useEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
import {
  Alert02Icon,
  CheckmarkCircle02Icon,
  Clock01Icon,
  FileUploadIcon,
  Settings01Icon,
} from "@hugeicons/core-free-icons";
import { HugeiconsIcon } from "@hugeicons/react";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Spinner } from "@/components/ui/spinner";
import { Progress, ProgressLabel, ProgressValue } from "@/components/ui/progress";
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
  Toaster,
  createDismissalNoticeController,
  createOutcomeNoticeController,
  createToastManager,
} from "@/components/ui/toast";
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
  importDiscoveryRun,
  preflightLatestBackup,
  prepareRole,
  reconcileApplicationHistory,
  requestNotificationPermission,
  runProviderProbe,
  retryBrowserSession,
  reopenApplicationForm,
  continueInBrowser,
  confirmApplicationApplied,
  getPreparationDetail,
  getCvFallbackSetting,
  focusReviewForm,
  openPreparationArtifact,
  quitApp,
  cancelPreparation,
  sendTestNotification,
  saveQueueFilters,
  setCvFallbackSetting,
  setBackgroundEnabled,
  startBrowserConnectionCheck,
  takeInAppOutcomeNotifications,
  dismissPreparation,
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
  PreparationSummary,
  QueueFilters,
  CvFallbackSetting,
} from "./types";
import { formatPublicationAge } from "./lib/publication-age";
import { isApplicationsPreview } from "./dev/applications-preview";

const groupOrder: QueueGroup[] = ["strong_match", "other_new", "needs_decision"];
const dismissalToast = createToastManager();
const dismissalNotices = createDismissalNoticeController(dismissalToast);
const outcomeNotices = createOutcomeNoticeController(dismissalToast);

const groupCopy: Record<QueueGroup, string> = {
  strong_match: "Strong matches",
  other_new: "Other new roles",
  needs_decision: "Needs a decision",
};

const unevaluatedRiskCopy: Record<string, string> = {
  classification: "Classification not evaluated",
  culture: "Culture not evaluated",
  interviewRedflags: "Interview risks not evaluated",
  aiInfra: "AI infrastructure not evaluated",
  aiScreeningDisclosure: "AI screening disclosure not evaluated",
};

const formatNativeScore = (score: number): string => `${Number.isInteger(score) ? score : score.toFixed(1)}/5`;

const uniqueText = (values: Array<string | null | undefined>): string[] => (
  [...new Set(values.map((value) => value?.trim()).filter((value): value is string => Boolean(value)))]
);

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

export function RoleRow({
  role,
  canPrepare,
  canDismiss,
  busy,
  enqueuing,
  onPrepare,
  onDismiss,
}: {
  role: RoleSummary;
  canPrepare: boolean;
  canDismiss: boolean;
  busy: boolean;
  enqueuing: boolean;
  onPrepare: (roleId: string) => void;
  onDismiss: (roleId: string) => void;
}) {
  const titleId = `queue-role-${role.id}-title`;
  const statusId = `queue-role-${role.id}-status`;
  const publicationAge = formatPublicationAge(role.postedAt);
  const evaluation = role.evaluation;
  const decisionChecks = evaluation ? [...evaluation.blockers, ...evaluation.gaps] : [];
  const uncertainty = evaluation
    ? uniqueText([
        role.uncertainty,
        evaluation.materialUncertainty.authorizationQuestion,
        ...evaluation.materialUncertainty.notEvaluatedRiskSignals.map(
          (signal) => unevaluatedRiskCopy[signal] ?? "A risk area was not evaluated",
        ),
      ])
    : uniqueText([role.uncertainty]);
  return (
    <li className="role-list__item">
      <article className="role-card" aria-labelledby={titleId} aria-busy={enqueuing || undefined}>
        <div className="role-card__heading">
          {role.applicationUrl ? (
            <a id={titleId} className="role-card__title" href={role.applicationUrl} target="_blank" rel="noreferrer">
              {role.title}
            </a>
          ) : (
            <span id={titleId} className="role-card__title">{role.title}</span>
          )}
        </div>
        <p className="role-card__meta">
          <span>{role.company}</span>
          <span aria-hidden="true">·</span>
          <span>{role.location}</span>
          {publicationAge ? (
            <>
              <span aria-hidden="true">·</span>
              <span className="role-card__age">{publicationAge}</span>
            </>
          ) : null}
        </p>
        {evaluation ? (
          <div className="role-card__decision" aria-label="career-ops evaluation">
            <div className="role-card__evaluation-summary">
              <span className="role-card__score" aria-label={`career-ops match score ${evaluation.nativeScore} out of 5`}>
                {formatNativeScore(evaluation.nativeScore)}
              </span>
              <span className="role-card__risk">
                Legitimacy: {evaluation.legitimacy} · Risk: {evaluation.riskLevel}
              </span>
            </div>
            {evaluation.strengths.length > 0 ? (
              <p className="role-card__decision-line">
                <span>Evidence:</span>
                {evaluation.strengths.join(" · ")}
              </p>
            ) : null}
            {decisionChecks.length > 0 ? (
              <p className="role-card__decision-line role-card__decision-line--attention">
                <span>Check:</span>
                {decisionChecks.join(" · ")}
              </p>
            ) : null}
            {evaluation.compensation ? (
              <p className="role-card__decision-line">
                <span>Compensation:</span>
                {evaluation.compensation}
              </p>
            ) : null}
            {uncertainty.length > 0 ? (
              <p className="role-card__decision-line role-card__decision-line--uncertainty">
                <span>Uncertain:</span>
                {uncertainty.join(" · ")}
              </p>
            ) : null}
          </div>
        ) : null}
        <div className="role-card__actions">
          <Button
            variant="ghost"
            type="button"
            disabled={!canDismiss || busy || enqueuing}
            aria-describedby={statusId}
            onClick={() => onDismiss(role.id)}
          >
            Dismiss
          </Button>
          <Button
            type="button"
            disabled={!canPrepare || busy || enqueuing}
            aria-describedby={statusId}
            onClick={() => onPrepare(role.id)}
          >
            {enqueuing ? "Queueing…" : "Prepare"}
          </Button>
        </div>
        <span id={statusId} className="visually-hidden" role="status" aria-live="polite">
          {enqueuing ? `${role.title} is being queued for preparation.` : ""}
        </span>
      </article>
    </li>
  );
}

export function PreQueueStatus({ roles }: { roles: DashboardState["preQueueRoles"] }) {
  if (roles.length === 0) return null;
  const attentionCount = roles.filter((role) => role.state === "needs_attention").length;
  const evaluatingCount = roles.length - attentionCount;
  const parts = [];
  if (evaluatingCount > 0) {
    parts.push(`${evaluatingCount} ${evaluatingCount === 1 ? "role is" : "roles are"} being evaluated`);
  }
  if (attentionCount > 0) {
    parts.push(`${attentionCount} ${attentionCount === 1 ? "needs" : "need"} attention in System`);
  }
  return <p className="pre-queue-status" role="status">{parts.join(". ")}.</p>;
}

export type QueueOperationalState =
  | { kind: "evaluating"; count: number; startedAt: string | null }
  | { kind: "progress"; completed: number; total: number }
  | { kind: "waiting"; lastSuccessfulAt: string | null }
  | { kind: "blocked"; count: number; runCount: number }
  | { kind: "idle" };

/**
 * The queue deliberately derives its presentation from the durable dashboard
 * snapshot. A role held in pre-Queue is never treated as an empty queue.
 */
export function deriveQueueOperationalState(dashboard: DashboardState): QueueOperationalState {
  const attentionCount = dashboard.preQueueRoles.filter((role) => role.state === "needs_attention").length;
  if (attentionCount > 0 || dashboard.actionRequiredRunCount > 0) {
    return { kind: "blocked", count: attentionCount, runCount: dashboard.actionRequiredRunCount };
  }

  const awaitingCount = dashboard.preQueueRoles.filter((role) => role.state === "awaiting_evaluation").length;
  const syncingCount = dashboard.preQueueRoles.filter((role) => role.state === "syncing").length;
  if (syncingCount > 0 && awaitingCount === 0) {
    const completed = dashboard.roles.length;
    return { kind: "progress", completed, total: completed + syncingCount };
  }

  const evaluatingCount = awaitingCount + syncingCount;
  if (evaluatingCount > 0) {
    const timestamps = dashboard.preQueueRoles
      .filter((role) => role.state === "awaiting_evaluation" || role.state === "syncing")
      .map((role) => role.updatedAt)
      .filter(Boolean)
      .sort();
    return { kind: "evaluating", count: evaluatingCount, startedAt: timestamps[0] ?? null };
  }

  if (dashboard.pendingRunCount > 0 || dashboard.lastSuccessfulDiscoveryAt !== null) {
    return { kind: "waiting", lastSuccessfulAt: dashboard.lastSuccessfulDiscoveryAt };
  }

  return { kind: "idle" };
}

export function QueueOperationalStatus({
  dashboard,
  onOpenSystem,
}: {
  dashboard: DashboardState;
  onOpenSystem?: () => void;
}) {
  const state = deriveQueueOperationalState(dashboard);
  if (state.kind === "idle") return null;

  if (state.kind === "blocked") {
    const roleCopy = state.count === 1 ? "1 role" : `${state.count} roles`;
    const runCopy = state.runCount === 1 ? "1 discovery run" : `${state.runCount} discovery runs`;
    const explanation = state.count > 0 && state.runCount > 0
      ? `${roleCopy} and ${runCopy} need attention before new roles can appear.`
      : state.count > 0
        ? `${roleCopy} need attention before they can appear in Queue.`
        : `${runCopy} need attention before new roles can appear.`;
    const heading = state.count > 0
      ? `${state.count} ${state.count === 1 ? "role" : "roles"} need attention`
      : `${state.runCount} discovery ${state.runCount === 1 ? "run" : "runs"} need attention`;
    return (
      <div className="queue-operational-status queue-operational-status--blocked" role="status" aria-live="polite">
        <HugeiconsIcon icon={Alert02Icon} strokeWidth={2} aria-hidden="true" />
        <div className="queue-operational-status__body">
          <strong>{heading}</strong>
          <p>{explanation}</p>
        </div>
        {onOpenSystem ? (
          <Button variant="outline" type="button" onClick={onOpenSystem}>Open System</Button>
        ) : null}
      </div>
    );
  }

  if (state.kind === "progress") {
    const percentage = state.total > 0 ? (state.completed / state.total) * 100 : 0;
    return (
      <div className="queue-operational-status queue-operational-status--progress" role="status" aria-live="polite">
        <div className="queue-operational-status__body">
          <div className="queue-operational-status__line">
            <strong>Evaluation progress</strong>
            <span>{state.completed} of {state.total} complete</span>
          </div>
          <Progress value={percentage}>
            <ProgressLabel className="visually-hidden">
              Evaluation progress: {state.completed} of {state.total} complete
            </ProgressLabel>
            <ProgressValue className="visually-hidden" />
          </Progress>
        </div>
      </div>
    );
  }

  if (state.kind === "evaluating") {
    return (
      <div className="queue-operational-status queue-operational-status--evaluating" role="status" aria-live="polite">
        <Spinner className="motion-reduce:animate-none" aria-hidden="true" />
        <div className="queue-operational-status__body">
          <strong>{state.count} {state.count === 1 ? "role is" : "roles are"} being evaluated</strong>
          <p>{state.startedAt ? `Started ${formatRelative(state.startedAt)}.` : "Evaluation is in progress."}</p>
        </div>
      </div>
    );
  }

  return (
    <div className="queue-operational-status queue-operational-status--waiting" role="status" aria-live="polite">
      <HugeiconsIcon icon={Clock01Icon} strokeWidth={2} aria-hidden="true" />
      <div className="queue-operational-status__body">
        <strong>Queue is waiting for its next run</strong>
        <p>
          {state.lastSuccessfulAt
            ? <time dateTime={state.lastSuccessfulAt}>Last successful run {formatRelative(state.lastSuccessfulAt)}.</time>
            : "No successful run recorded yet."}
        </p>
      </div>
    </div>
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

type ApplicationState = {
  label: string;
  tone: "neutral" | "active" | "attention" | "success";
  active: boolean;
  explanation: string | null;
};

const activeBrowserStatuses = new Set<BrowserSessionSummary["status"]>([
  "waiting_for_extension",
  "inspecting",
  "drafting_answers",
  "answering",
  "filling",
  "persisting_answers",
  "saving_answers",
  "releasing",
]);

function browserFailureExplanation(errorCode: string | null): string {
  if (errorCode === "answer_persistence_failed" || errorCode === "answer_persistence_interrupted") {
    return "The filled answers could not be saved. Retry resumes local saving without touching the form.";
  }
  if (errorCode === "application_tab_recovery_failed") {
    return "The application tab could not be restored. Retry opens the application again.";
  }
  if (errorCode === "public_browser_fallback_unavailable") {
    return "The extension released its lease before filling. A second review-only driver is unavailable because career-ops does not expose the required public lease and result contract; complete the released page manually.";
  }
  return "HereForWork could not finish the browser step. Check the selected Chrome profile, then retry.";
}

function preparationFailureExplanation(item: PreparationSummary): string {
  if (item.errorClass === "app_interrupted") {
    return "HereForWork closed while this was running. Retry it, or cancel it and return the role to Queue.";
  }
  if (item.errorClass === "artifact_commit_failed" || item.errorStage?.includes("commit")) {
    return "The prepared files could not be saved. Retry uses the same preparation.";
  }
  return "Preparation stopped before the application was ready. Retry uses the saved preparation.";
}

export function deriveApplicationState(
  item: PreparationSummary,
  latestSession: BrowserSessionSummary | undefined,
  recordingApplication: boolean,
): ApplicationState {
  if (recordingApplication) {
    return { label: "Recording application…", tone: "active", active: true, explanation: null };
  }
  if (latestSession?.status === "applied_recorded") {
    return { label: "Application recorded", tone: "success", active: false, explanation: null };
  }
  if (latestSession?.status === "submitted_tracking_pending") {
    return {
      label: "Tracking update failed",
      tone: "attention",
      active: false,
      explanation: "The form is not touched again. Retry only the career-ops tracking update.",
    };
  }
  if (item.status === "queued") {
    return { label: "Waiting", tone: "neutral", active: false, explanation: null };
  }
  if (item.status === "preparing") {
    return { label: "Preparing CV", tone: "active", active: true, explanation: null };
  }
  if (!latestSession) {
    if (item.status === "action_required") {
      return {
        label: "Preparation failed",
        tone: "attention",
        active: false,
        explanation: preparationFailureExplanation(item),
      };
    }
    return { label: "Waiting", tone: "neutral", active: false, explanation: null };
  }
  if (latestSession.status === "action_required") {
    return {
      label: "Preparation failed",
      tone: "attention",
      active: false,
      explanation: browserFailureExplanation(latestSession.errorCode),
    };
  }
  if (latestSession.status === "review_required") {
    return { label: "Ready for review", tone: "success", active: false, explanation: null };
  }
  if (latestSession.status === "waiting_for_extension" || latestSession.status === "inspecting") {
    return { label: "Opening form", tone: "active", active: true, explanation: null };
  }
  if (activeBrowserStatuses.has(latestSession.status)) {
    return { label: "Filling form", tone: "active", active: true, explanation: null };
  }
  return { label: browserStatusLabel[latestSession.status], tone: "neutral", active: false, explanation: null };
}

export function canDismissApplicationPreparation(
  item: PreparationSummary,
  latestSession: BrowserSessionSummary | undefined,
  recordingApplication: boolean,
): boolean {
  if (recordingApplication) return false;
  const state = deriveApplicationState(item, latestSession, false);
  return state.label === "Preparation failed" || state.label === "Ready for review";
}

function ApplicationStatus({ state }: { state: ApplicationState }) {
  const icon = state.active ? (
    <Spinner className="motion-reduce:animate-none" />
  ) : state.tone === "attention" ? (
    <HugeiconsIcon icon={Alert02Icon} strokeWidth={2} aria-hidden="true" />
  ) : state.tone === "success" ? (
    <HugeiconsIcon icon={CheckmarkCircle02Icon} strokeWidth={2} aria-hidden="true" />
  ) : (
    <HugeiconsIcon icon={Clock01Icon} strokeWidth={2} aria-hidden="true" />
  );
  return (
    <Badge
      className="application-state"
      data-tone={state.tone}
      variant={state.tone === "attention" ? "destructive" : "outline"}
    >
      {icon}
      {state.label}
    </Badge>
  );
}

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
              {session.errorCode === "public_browser_fallback_unavailable" ? (
                <p>The extension stopped before filling. The separate review-only fallback is unavailable, so use the grounded answers and complete this page manually.</p>
              ) : null}
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
  selectedProvider,
  onSelectedProviderChange,
  cvFallbackSetting,
  cvFallbackPath,
  onCvFallbackPathChange,
  onSaveCvFallback,
  onToggleBackground,
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
  selectedProvider: "codex" | "claude";
  onSelectedProviderChange: (provider: "codex" | "claude") => void;
  cvFallbackSetting: CvFallbackSetting;
  cvFallbackPath: string;
  onCvFallbackPathChange: (path: string) => void;
  onSaveCvFallback: () => void;
  onToggleBackground: () => void;
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
      <section className="queue-runtime-settings" aria-labelledby="queue-runtime-settings-title">
        <div>
          <h3 id="queue-runtime-settings-title">Queue settings</h3>
          <p>Choose how preparation runs and whether HereForWork checks for work in the background.</p>
        </div>
        <div className="queue-runtime-settings__controls">
          <Field className="provider-select">
            <FieldLabel htmlFor="preparation-provider">Preparation provider</FieldLabel>
            <Select
              value={selectedProvider}
              onValueChange={(value) => {
                if (value === "codex" || value === "claude") onSelectedProviderChange(value);
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
          <Field>
            <FieldLabel htmlFor="reviewed-cv-fallback">Reviewed CV fallback</FieldLabel>
            <Input
              id="reviewed-cv-fallback"
              value={cvFallbackPath}
              onChange={(event) => onCvFallbackPathChange(event.target.value)}
              placeholder="/absolute/path/to/reviewed-cv.pdf"
              disabled={busy}
            />
            <span className="background-setting__state">
              {cvFallbackSetting.sha256
                ? "Available · hash-bound to this file"
                : "Not configured"}
            </span>
            <span className="background-setting__state">
              Used only if tailored PDF rendering fails after HTML and fact checks.
            </span>
            <Button variant="outline" type="button" onClick={onSaveCvFallback} disabled={busy}>
              Save reviewed CV
            </Button>
          </Field>
          <div className="background-setting">
            <div>
              <span className="background-setting__label">Background checks</span>
              <span className="background-setting__state">
                {dashboard.backgroundEnabled ? "On" : "Off"}
              </span>
            </div>
            <Button
              variant="outline"
              type="button"
              aria-pressed={dashboard.backgroundEnabled}
              onClick={onToggleBackground}
              disabled={busy}
            >
              {dashboard.backgroundEnabled ? "Turn off" : "Turn on"}
            </Button>
          </div>
        </div>
      </section>
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
      {dashboard.recentRuns.length > 0 || dashboard.discoveryRuns.length > 0 ? (
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
              {dashboard.discoveryRuns.map((run) => (
                <li key={`discovery-${run.runId}`} data-discovery-run-id={run.runId}>
                  <span>{run.sourceDisplayName}</span>
                  <span>discovery run</span>
                  <strong>{run.status.replaceAll("_", " ")}</strong>
                  {run.issues[0] ? <span>{run.issues[0].code}: {run.issues[0].message}</span> : null}
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
  const [cvFallbackSetting, setCvFallbackSettingState] = useState<CvFallbackSetting>({ path: null, sha256: null });
  const [cvFallbackPath, setCvFallbackPath] = useState("");
  const [browserSetup, setBrowserSetup] = useState<BrowserSetup | null>(null);
  const [browserSessions, setBrowserSessions] = useState<BrowserSessionSummary[]>([]);
  const [extensionId, setExtensionId] = useState("");
  const [installationId, setInstallationId] = useState("");
  const [profileId, setProfileId] = useState("");
  const [maintenanceResult, setMaintenanceResult] = useState<MaintenanceResult | null>(null);
  const [restorePreflight, setRestorePreflight] = useState<RestorePreflight | null>(null);
  const [preparationDetail, setPreparationDetail] = useState<PreparationDetail | null>(null);
  const [selectedPreparationId, setSelectedPreparationId] = useState<string | null>(null);
  const [queueFiltersDraft, setQueueFiltersDraft] = useState<QueueFilters | null>(null);
  const [dismissPreparationId, setDismissPreparationId] = useState<string | null>(null);
  const [dismissingPreparationId, setDismissingPreparationId] = useState<string | null>(null);
  const [dismissPreparationError, setDismissPreparationError] = useState<string | null>(null);
  const [enqueuingRoleIds, setEnqueuingRoleIds] = useState<Set<string>>(() => new Set());
  const [cancellationRequestedRoleIds, setCancellationRequestedRoleIds] = useState<Set<string>>(() => new Set());
  const [reopeningPreparationIds, setReopeningPreparationIds] = useState<Set<string>>(() => new Set());
  const [recordingSessionIds, setRecordingSessionIds] = useState<Set<string>>(() => (
    isApplicationsPreview() ? new Set(["session-recording"]) : new Set()
  ));
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
    let active = true;
    getCvFallbackSetting()
      .then((setting) => {
        if (!active) return;
        setCvFallbackSettingState(setting);
        setCvFallbackPath(setting.path ?? "");
      })
      .catch((reason: unknown) => {
        if (active) setError(reason instanceof Error ? reason.message : String(reason));
      });
    return () => {
      active = false;
    };
  }, []);

  useEffect(() => {
    if (!import.meta.env.DEV) return;
    const preview = new URLSearchParams(window.location.search).get("notification-preview");
    const showPreview = () => {
      if (preview === "failure") {
        outcomeNotices.show({
          id: "visual-failure",
          eventKind: "preparation_failed",
          title: "Preparation failed",
          body: "Frontend Engineer at Northstar Tools. CV fact check: One tailored statement needs review.",
          actionKind: "view_details",
          actionLabel: "View details",
          roleId: "visual-role",
          preparationId: "visual-preparation",
          browserSessionId: null,
          createdAt: new Date().toISOString(),
        }, () => undefined, () => undefined);
      } else if (preview === "ready") {
        outcomeNotices.show({
          id: "visual-ready",
          eventKind: "application_ready",
          title: "Application ready for review",
          body: "Frontend Engineer at Northstar Tools. The live form is released in Chrome. Only you can submit it.",
          actionKind: "review_form",
          actionLabel: "Review form",
          roleId: "visual-role",
          preparationId: "visual-preparation",
          browserSessionId: "visual-session",
          createdAt: new Date().toISOString(),
        }, () => undefined, () => undefined);
      }
    };
    const previewWindow = window as typeof window & { __showHfwNotificationPreview?: () => void };
    previewWindow.__showHfwNotificationPreview = showPreview;
    const previewTimer = window.setTimeout(showPreview, 100);
    return () => {
      window.clearTimeout(previewTimer);
      delete previewWindow.__showHfwNotificationPreview;
    };
  }, []);

  useEffect(() => {
    let active = true;
    const takeNotifications = () => {
      void takeInAppOutcomeNotifications()
        .then((notifications) => {
          if (!active) return;
          for (const notification of notifications) {
            outcomeNotices.show(
              notification,
              (preparationId) => {
                setSelectedPreparationId(preparationId);
                setView("applications");
                void getPreparationDetail(preparationId)
                  .then((detail) => {
                    if (active) setPreparationDetail(detail);
                  })
                  .catch((reason: unknown) => {
                    if (active) setError(reason instanceof Error ? reason.message : String(reason));
                  });
              },
              (sessionId) => {
                void focusReviewForm(sessionId).catch((reason: unknown) => {
                  if (active) setError(reason instanceof Error ? reason.message : String(reason));
                });
              },
            );
          }
        })
        .catch(() => {
          // Applications remains authoritative if notification delivery is unavailable.
        });
    };
    takeNotifications();
    const interval = window.setInterval(takeNotifications, 750);
    return () => {
      active = false;
      window.clearInterval(interval);
    };
  }, []);

  useEffect(() => {
    if (view !== "applications" || !selectedPreparationId) return;
    window.requestAnimationFrame(() => {
      document
        .getElementById(`preparation-${selectedPreparationId}`)
        ?.scrollIntoView?.({ block: "nearest" });
    });
  }, [selectedPreparationId, view]);

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
      if (view === "applications") {
        void getDashboard().then((state) => {
          if (!active) return;
          setDashboard(state);
          setQueueFiltersDraft(state.queueFilters);
        }).catch(() => {
          // Per-role failures remain visible in the last successful dashboard state.
        });
      }
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
      let contract: unknown = null;
      try {
        const candidate: unknown = JSON.parse(payload);
        if (candidate && typeof candidate === "object" && "contract" in candidate) {
          contract = candidate.contract;
        }
      } catch {
        // The legacy importer remains the source of the detailed parse error.
      }
      if (contract === "hereforwork.discovery-run") {
        await importDiscoveryRun(payload);
      } else {
        await importDataset(payload);
      }
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

  const persistCvFallback = async () => {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const setting = await setCvFallbackSetting(cvFallbackPath);
      setCvFallbackSettingState(setting);
      setCvFallbackPath(setting.path ?? "");
      setNotice(setting.path
        ? "Reviewed CV fallback saved and hash-bound to the selected PDF."
        : "Reviewed CV fallback cleared.");
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

  const reopenPreparedRole = async (preparationId: string) => {
    if (reopeningPreparationIds.has(preparationId)) return;
    setReopeningPreparationIds((current) => new Set(current).add(preparationId));
    setError(null);
    try {
      await reopenApplicationForm(preparationId);
      setBrowserSessions(await getBrowserSessions());
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
      try {
        setBrowserSessions(await getBrowserSessions());
      } catch {
        // Preserve the recovery error when refreshing browser state also fails.
      }
    } finally {
      setReopeningPreparationIds((current) => {
        const next = new Set(current);
        next.delete(preparationId);
        return next;
      });
    }
  };

  const confirmApplied = async (sessionId: string) => {
    if (recordingSessionIds.has(sessionId)) return;
    setRecordingSessionIds((current) => new Set(current).add(sessionId));
    setError(null);
    try {
      await confirmApplicationApplied(sessionId);
      const [nextDashboard, nextSessions] = await Promise.all([getDashboard(), getBrowserSessions()]);
      setDashboard(nextDashboard);
      setBrowserSessions(nextSessions);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
      try {
        const [nextDashboard, nextSessions] = await Promise.all([getDashboard(), getBrowserSessions()]);
        setDashboard(nextDashboard);
        setBrowserSessions(nextSessions);
      } catch {
        // Preserve the canonical tracking error if the follow-up refresh also fails.
      }
    } finally {
      setRecordingSessionIds((current) => {
        const next = new Set(current);
        next.delete(sessionId);
        return next;
      });
    }
  };

  const dismissQueueRole = async (roleId: string) => {
    const role = dashboard?.roles.find((item) => item.id === roleId);
    if (!role) return;
    setBusy(true);
    setError(null);
    try {
      setDashboard(await dismissRole(roleId));
      dismissalNotices.show(
        role.id,
        role.title,
        () => void restoreDismissedRole(role.id, role.title),
      );
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setBusy(false);
    }
  };

  const prepareQueueRole = async (roleId: string, provider = selectedProvider) => {
    setEnqueuingRoleIds((current) => new Set(current).add(roleId));
    setError(null);
    setNotice(null);
    try {
      const outcome = await prepareRole(roleId, provider);
      setDashboard(outcome.dashboard);
      setQueueFiltersDraft(outcome.dashboard.queueFilters);
      setNotice(outcome.message);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
      try {
        setDashboard(await getDashboard());
      } catch {
        // Preserve the actionable preparation error when refreshing state also fails.
      }
    } finally {
      setEnqueuingRoleIds((current) => {
        const next = new Set(current);
        next.delete(roleId);
        return next;
      });
    }
  };

  const cancelQueuePreparation = async (roleId: string) => {
    if (cancellationRequestedRoleIds.has(roleId)) return;
    setCancellationRequestedRoleIds((current) => new Set(current).add(roleId));
    try {
      const requested = await cancelPreparation(roleId);
      if (!requested) setError("The preparation had already finished or stopped.");
      setDashboard(await getDashboard());
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setCancellationRequestedRoleIds((current) => {
        const next = new Set(current);
        next.delete(roleId);
        return next;
      });
    }
  };

  const restoreDismissedRole = async (roleId: string, roleTitle: string) => {
    setBusy(true);
    dismissalNotices.undoing(roleId, roleTitle);
    try {
      setDashboard(await undoDismissal(roleId));
      dismissalNotices.completed(roleId, roleTitle);
    } catch (reason) {
      const detail = reason instanceof Error ? reason.message : String(reason);
      dismissalNotices.failed(
        roleId,
        roleTitle,
        detail,
        () => void restoreDismissedRole(roleId, roleTitle),
      );
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
    setSelectedPreparationId(preparationId);
    try {
      setPreparationDetail(await getPreparationDetail(preparationId));
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setBusy(false);
    }
  };

  const openDismissPreparation = (preparationId: string) => {
    setDismissPreparationId(preparationId);
    setDismissPreparationError(null);
    window.requestAnimationFrame(() => {
      document.getElementById(`keep-preparation-${preparationId}`)?.focus();
    });
  };

  const keepPreparation = (preparationId: string) => {
    setDismissPreparationId(null);
    setDismissPreparationError(null);
    window.requestAnimationFrame(() => {
      document.getElementById(`dismiss-preparation-${preparationId}`)?.focus();
    });
  };

  const discardPreparation = async (preparationId: string) => {
    if (dismissingPreparationId) return;
    setDismissingPreparationId(preparationId);
    setError(null);
    setNotice(null);
    setDismissPreparationError(null);
    try {
      const next = await dismissPreparation(preparationId);
      setDashboard(next);
      setBrowserSessions(await getBrowserSessions());
      setPreparationDetail((current) => current?.preparationId === preparationId ? null : current);
      setDismissPreparationId(null);
      setNotice("Role dismissed. career-ops was updated and generated preparation files were deleted.");
    } catch {
      setDismissPreparationError(
        "Dismiss did not finish. The preparation remains in Applications so you can try again.",
      );
      try {
        const [nextDashboard, nextSessions] = await Promise.all([getDashboard(), getBrowserSessions()]);
        setDashboard(nextDashboard);
        setBrowserSessions(nextSessions);
      } catch {
        // Keep the row and its recovery action visible from the last known state.
      }
    } finally {
      setDismissingPreparationId(null);
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

  const queueOperationalState = deriveQueueOperationalState(dashboard);
  const emptyQueueContent = queueOperationalState.kind !== "idle"
    ? <QueueOperationalStatus dashboard={dashboard} onOpenSystem={() => setView("system")} />
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
          selectedProvider={selectedProvider}
          onSelectedProviderChange={setSelectedProvider}
          cvFallbackSetting={cvFallbackSetting}
          cvFallbackPath={cvFallbackPath}
          onCvFallbackPathChange={setCvFallbackPath}
          onSaveCvFallback={() => void persistCvFallback()}
          onToggleBackground={() => void toggleBackground()}
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
    const detailBrowserSession = preparationDetail
      ? browserSessions
          .filter((session) => session.purpose === "application" && session.preparationId === preparationDetail.preparationId)
          .sort((left, right) => right.updatedAt.localeCompare(left.updatedAt))[0]
      : undefined;
    mainContent = (
      <main className="status-workspace" aria-labelledby="applications-title">
        <p className="eyebrow">Application workflow</p>
        <h2 id="applications-title">Applications</h2>
        <p>One current status per application. Open Details when you want the full career-ops report.</p>
        {dashboard.preparations.length > 0 ? (
          <ol
            className="preparation-list"
            aria-label="Application preparations"
            aria-live="polite"
            aria-relevant="additions text"
          >
            {dashboard.preparations.map((item) => {
              const latestSession = browserSessions
                .filter((session) => session.purpose === "application" && session.preparationId === item.id)
                .sort((left, right) => right.updatedAt.localeCompare(left.updatedAt))[0];
              const recordingApplication = Boolean(latestSession && recordingSessionIds.has(latestSession.id));
              const applicationState = deriveApplicationState(item, latestSession, recordingApplication);
              const browserActive = Boolean(latestSession && activeBrowserStatuses.has(latestSession.status));
              const canDismissPreparation = canDismissApplicationPreparation(item, latestSession, recordingApplication);
              const dismissalOpen = dismissPreparationId === item.id;
              const dismissalInProgress = dismissingPreparationId === item.id;
              const canRetryPreparation = item.status === "action_required"
                && item.step !== "undo_cleanup"
                && ["retry_same_preparation", "repair_runtime_then_retry"].includes(item.retryPolicy ?? "retry_same_preparation");
              const canCancelPreparation = item.status === "queued"
                || item.status === "preparing"
                || (item.status === "action_required" && item.errorClass === "app_interrupted");
              return (
              <li
                key={item.id}
                id={`preparation-${item.id}`}
                data-preparation-id={item.id}
                data-selected={selectedPreparationId === item.id || undefined}
                aria-busy={applicationState.active || undefined}
                tabIndex={-1}
              >
                <div className="preparation-list__identity">
                  <strong>{item.title}</strong>
                  <span>
                    {item.company} · {item.provider}
                    {item.cvSource === "user_reviewed_fallback" ? " · User-reviewed CV" : ""}
                  </span>
                </div>
                <ApplicationStatus state={applicationState} />
                {applicationState.explanation ? (
                  <p className="preparation-list__explanation">{applicationState.explanation}</p>
                ) : null}
                {canCancelPreparation ? (
                  <div className="preparation-list__actions">
                    <Button
                      variant="outline"
                      type="button"
                      onClick={() => void cancelQueuePreparation(item.roleId)}
                      disabled={cancellationRequestedRoleIds.has(item.roleId)}
                    >
                      {cancellationRequestedRoleIds.has(item.roleId) ? "Cancelling…" : "Cancel and return to Queue"}
                    </Button>
                  </div>
                ) : null}
                {item.status === "action_required" ? (
                  <div className="preparation-list__actions">
                    <Button variant="outline" type="button" onClick={() => void showPreparationDetail(item.id)} disabled={busy || dismissalInProgress}>
                      Details
                    </Button>
                    {canRetryPreparation ? (
                      <Button
                        type="button"
                        onClick={() => void prepareQueueRole(item.roleId, item.provider)}
                        disabled={enqueuingRoleIds.has(item.roleId) || dismissalInProgress}
                      >
                        {enqueuingRoleIds.has(item.roleId) ? "Queueing…" : "Retry preparation"}
                      </Button>
                    ) : null}
                    {canDismissPreparation ? (
                      <Button
                        id={`dismiss-preparation-${item.id}`}
                        variant="destructive"
                        type="button"
                        aria-expanded={dismissalOpen}
                        aria-controls={`dismiss-preparation-confirmation-${item.id}`}
                        onClick={() => openDismissPreparation(item.id)}
                        disabled={dismissalInProgress}
                      >
                        Dismiss
                      </Button>
                    ) : null}
                  </div>
                ) : null}
                {item.status === "completed" ? (
                  <div className="preparation-list__actions">
                    <Button variant="outline" type="button" onClick={() => void showPreparationDetail(item.id)} disabled={busy || dismissalInProgress}>
                      Details
                    </Button>
                    <Button variant="outline" type="button" onClick={() => void openArtifact(item.id, "cv")} disabled={busy || dismissalInProgress}>
                      {item.cvSource === "user_reviewed_fallback" ? "Open user-reviewed CV" : "Open tailored CV"}
                    </Button>
                    {!latestSession ? (
                    <Button

                      type="button"
                      onClick={() => void continuePreparedRole(item.id)}
                      disabled={busy || dismissalInProgress || !browserSetup?.approvedInstallationId}
                    >
                      Open in browser
                    </Button>
                    ) : null}
                    {latestSession?.status === "action_required" ? (
                      <Button type="button" onClick={() => void retryBrowser(latestSession.id)} disabled={busy || dismissalInProgress}>
                        Retry browser step
                      </Button>
                    ) : null}
                    {latestSession?.status === "review_required" ? (
                      <>
                        <Button
                          variant="outline"
                          type="button"
                          onClick={() => void reopenPreparedRole(item.id)}
                          disabled={busy || dismissalInProgress || reopeningPreparationIds.has(item.id) || recordingApplication}
                        >
                          {reopeningPreparationIds.has(item.id) ? "Reopening…" : "Reopen and refill"}
                        </Button>
                        <Button
                          type="button"
                          onClick={() => void confirmApplied(latestSession.id)}
                          disabled={busy || dismissalInProgress || recordingApplication}
                        >
                          {recordingApplication ? "Recording…" : "I submitted this application"}
                        </Button>
                      </>
                    ) : null}
                    {latestSession?.status === "submitted_tracking_pending" ? (
                      <Button type="button" onClick={() => void confirmApplied(latestSession.id)} disabled={busy || dismissalInProgress || recordingApplication}>
                        {recordingApplication ? "Recording…" : "Retry tracking update"}
                      </Button>
                    ) : null}
                    {canDismissPreparation ? (
                      <Button
                        id={`dismiss-preparation-${item.id}`}
                        variant="destructive"
                        type="button"
                        aria-expanded={dismissalOpen}
                        aria-controls={`dismiss-preparation-confirmation-${item.id}`}
                        onClick={() => openDismissPreparation(item.id)}
                        disabled={busy || dismissalInProgress || Boolean(browserActive)}
                      >
                        Dismiss
                      </Button>
                    ) : null}
                  </div>
                ) : null}
                {dismissalOpen ? (
                  <Alert
                    id={`dismiss-preparation-confirmation-${item.id}`}
                    className="preparation-list__confirmation"
                    variant="destructive"
                  >
                    <HugeiconsIcon icon={Alert02Icon} strokeWidth={2} aria-hidden="true" />
                    <AlertTitle>Dismiss this role?</AlertTitle>
                    <AlertDescription>
                      <p>
                        This marks {item.title} as Discarded in career-ops, permanently deletes its generated preparation files, and removes it from Applications.
                      </p>
                      {dismissPreparationError ? (
                        <p className="preparation-list__dismiss-error" aria-live="assertive">
                          {dismissPreparationError}
                        </p>
                      ) : null}
                      <div className="button-cluster">
                        <Button
                          id={`keep-preparation-${item.id}`}
                          className="text-foreground"
                          variant="outline"
                          type="button"
                          onClick={() => keepPreparation(item.id)}
                          disabled={dismissalInProgress}
                        >
                          Keep role
                        </Button>
                        <Button
                          variant="destructive"
                          type="button"
                          onClick={() => void discardPreparation(item.id)}
                          disabled={dismissalInProgress}
                        >
                          {dismissalInProgress ? "Dismissing…" : "Dismiss role"}
                        </Button>
                      </div>
                    </AlertDescription>
                  </Alert>
                ) : null}
              </li>
              );
            })}
          </ol>
        ) : (
          <Empty className="empty-state empty-state--compact" role="region" aria-labelledby="applications-empty-title">
            <EmptyHeader>
              <EmptyTitle><h3 id="applications-empty-title">No application preparations yet.</h3></EmptyTitle>
              <EmptyDescription>Select Prepare in Queue. The role appears here immediately while HereForWork works in the background.</EmptyDescription>
            </EmptyHeader>
          </Empty>
        )}
        {preparationDetail ? (
          <Sheet open onOpenChange={(open) => {
            if (!open) {
              const preparationId = preparationDetail.preparationId;
              setPreparationDetail(null);
              window.requestAnimationFrame(() => {
                document.getElementById(`preparation-${preparationId}`)?.focus();
              });
            }
          }}>
            <SheetContent className="preparation-detail" aria-labelledby="preparation-detail-title">
              <SheetHeader className="preparation-detail__heading">
              <div>
                <p className="eyebrow">
                  {preparationDetail.status === "action_required" ? "Preparation failure" : "career-ops report"}
                </p>
                  <SheetTitle id="preparation-detail-title">{preparationDetail.title}</SheetTitle>
                  <p>{preparationDetail.company} · {preparationDetail.provider}</p>
              </div>
              </SheetHeader>
              <div className="preparation-detail__actions">
                {preparationDetail.reportPath ? (
                  <Button variant="outline" type="button" onClick={() => void openArtifact(preparationDetail.preparationId, "report")}>Open original report</Button>
                ) : null}
                {preparationDetail.cvPdfPath ? (
                  <Button variant="outline" type="button" onClick={() => void openArtifact(preparationDetail.preparationId, "cv")}>
                    {preparationDetail.cvSource === "user_reviewed_fallback" ? "Open user-reviewed CV" : "Open fact-checked tailored CV"}
                  </Button>
                ) : null}
                {preparationDetail.status === "action_required"
                  && ["retry_same_preparation", "repair_runtime_then_retry"].includes(preparationDetail.retryPolicy ?? "retry_same_preparation") ? (
                  <Button
                    type="button"
                    onClick={() => void prepareQueueRole(preparationDetail.roleId, preparationDetail.provider)}
                    disabled={enqueuingRoleIds.has(preparationDetail.roleId)}
                  >
                    {enqueuingRoleIds.has(preparationDetail.roleId) ? "Queueing…" : "Retry preparation"}
                  </Button>
                ) : null}
                {detailBrowserSession?.status === "action_required" ? (
                  <Button type="button" onClick={() => void retryBrowser(detailBrowserSession.id)} disabled={busy}>
                    Retry browser step
                  </Button>
                ) : null}
              </div>
              {preparationDetail.cvSource === "user_reviewed_fallback" ? (
                <Alert>
                  <AlertTitle>User-reviewed CV fallback</AlertTitle>
                  <AlertDescription>
                    PDF rendering failed, so this preparation uses your configured reviewed CV. It was not tailored for this role.
                  </AlertDescription>
                </Alert>
              ) : null}
              {preparationDetail.errorDetail ? (
                <Alert variant="destructive">
                  <AlertTitle>{preparationDetail.stage.replaceAll("_", " ").replaceAll(".", " ")}</AlertTitle>
                  <AlertDescription>{preparationDetail.errorDetail}</AlertDescription>
                </Alert>
              ) : null}
              {preparationDetail.status === "action_required"
                && ["fresh_preparation_provider_run", "fresh_preparation_id", "manual_repair_required"].includes(preparationDetail.retryPolicy ?? "") ? (
                <p className="preparation-detail__empty">
                  This failure needs a fresh preparation or manual repair. HereForWork will not reuse this preparation automatically.
                </p>
              ) : null}
              {detailBrowserSession?.errorCode ? (
                <Alert variant="destructive">
                  <AlertTitle>Browser preparation stopped</AlertTitle>
                  <AlertDescription>{detailBrowserSession.errorCode.replaceAll("_", " ")}</AlertDescription>
                </Alert>
              ) : null}
              {preparationDetail.reportMarkdown ? <MarkdownPreview markdown={preparationDetail.reportMarkdown} /> : (
                <p className="preparation-detail__empty">No report was committed. Applications keeps this failure available until you decide whether to retry.</p>
              )}
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
              <Button
                className="icon-action"
                variant="outline"
                size="icon"
                type="button"
                aria-label="Import discovery snapshot"
                title="Import discovery snapshot"
                onClick={() => fileInputRef.current?.click()}
                disabled={busy}
              >
                <HugeiconsIcon icon={FileUploadIcon} strokeWidth={2} />
              </Button>
            </div>
            <Input
              ref={fileInputRef}
              className="visually-hidden"
              type="file"
              accept="application/json,.json"
              tabIndex={-1}
              aria-hidden="true"
              onChange={(event) => {
                const file = event.target.files?.[0];
                if (file) void handleImport(file);
              }}
            />
          </div>

          {dashboard.roles.length > 0 ? (
            <QueueOperationalStatus dashboard={dashboard} onOpenSystem={() => setView("system")} />
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
                      <h3 id={`${group}-heading`}>{groupCopy[group]}</h3>
                      <span aria-label={`${roles.length} roles`}>{roles.length}</span>
                    </div>
                    <ul className="role-list">
                      {roles.map((role) => (
                        <RoleRow
                          key={role.id}
                          role={role}
                          canPrepare={dashboard.adapterStatus === "ready" && role.applicationUrl !== null && role.evaluation != null}
                          canDismiss={dashboard.adapterStatus === "ready"}
                          busy={busy}
                          enqueuing={enqueuingRoleIds.has(role.id)}
                          onPrepare={(roleId) => void prepareQueueRole(roleId)}
                          onDismiss={(roleId) => void dismissQueueRole(roleId)}
                        />
                      ))}
                    </ul>
                  </section>
                );
              })}
            </div>
          )}
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
          <Button
            className="icon-action"
            variant="outline"
            size="icon"
            type="button"
            aria-label="System settings"
            title="System settings"
            aria-pressed={view === "system"}
            onClick={() => {
              setView("system");
              if (!health) void refreshHealth();
            }}
          >
            <HugeiconsIcon icon={Settings01Icon} strokeWidth={2} />
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
      <Toaster toastManager={dismissalToast} timeout={30_000} limit={3} />
    </Tabs>
  );
}
