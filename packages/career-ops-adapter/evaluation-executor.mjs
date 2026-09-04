/**
 * HFW-owned compatibility wrapper for career-ops full A–G evaluation.
 *
 * Invokes only fixed upstream surfaces (batch/batch-runner.sh + merge-tracker
 * side effects already owned by that runner). Never parses worker console prose
 * as the result; success requires evaluation.result.read.v1 post-conditions.
 */

import { access, constants, readFile, writeFile, stat } from "node:fs/promises";
import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { resolve } from "node:path";

const BATCH_INPUT_RELATIVE = "batch/batch-input.tsv";
const BATCH_STATE_RELATIVE = "batch/batch-state.tsv";
const BATCH_LOCK_RELATIVE = "batch/batch-runner.pid";
const BATCH_RUNNER_RELATIVE = "batch/batch-runner.sh";
const MERGE_TRACKER_RELATIVE = "merge-tracker.mjs";
const FULL_AG_PROBE_FILES = Object.freeze([
  BATCH_RUNNER_RELATIVE,
  "batch/batch-prompt.md",
  MERGE_TRACKER_RELATIVE,
  "tracker.mjs",
  "tracker-parse.mjs",
  "modes/oferta.md",
  "templates/states.yml",
]);
const FULL_AG_PROMPT_MARKERS = Object.freeze([
  "#### Machine Summary",
  "authorization_confidence:",
  "authorization_evidence:",
  "authorization_scope:",
  "engagement_mechanism:",
  "authorization_question:",
  "```json",
]);
const HTTPS_URL_RE = /^https:\/\/[^\s]+$/i;
const GIT_SHA_RE = /^[0-9a-f]{40}$/;
const BATCH_RUN_TIMEOUT_MS = 45 * 60 * 1000;

function sha256(value) {
  return createHash("sha256").update(value).digest("hex");
}

async function canRead(path) {
  try {
    await access(path, constants.R_OK);
    return true;
  } catch {
    return false;
  }
}

async function canExecute(path) {
  try {
    await access(path, constants.X_OK);
    return true;
  } catch {
    return false;
  }
}

function normalizeUrl(value) {
  return String(value ?? "")
    .trim()
    .replace(/\/+$/, "")
    .toLowerCase();
}

export function offerIdFromUrl(url) {
  const digest = sha256(normalizeUrl(url));
  const numeric = Number.parseInt(digest.slice(0, 8), 16);
  return 100_000 + (numeric % 800_000);
}

export async function commandExists(commandName, envPath = process.env.PATH) {
  if (!commandName || /[^a-zA-Z0-9._+-]/.test(commandName)) return false;
  return new Promise((resolvePromise) => {
    const child = spawn("/bin/sh", ["-c", `command -v ${commandName}`], {
      env: { PATH: envPath || "" },
      stdio: ["ignore", "pipe", "ignore"],
      shell: false,
    });
    let output = "";
    child.stdout.on("data", (chunk) => {
      output += chunk.toString("utf8");
    });
    child.on("error", () => resolvePromise(false));
    child.on("close", (code) => {
      resolvePromise(code === 0 && output.trim().length > 0);
    });
  });
}

export async function fullAgCompatibilityProbe(root) {
  if (!root) return { fingerprint: null, claudeAvailable: false };
  const claudeAvailable = await commandExists("claude");
  if (!claudeAvailable) return { fingerprint: null, claudeAvailable: false };
  const runner = resolve(root, BATCH_RUNNER_RELATIVE);
  if (!(await canRead(runner)) || !(await canExecute(runner))) {
    return { fingerprint: null, claudeAvailable };
  }
  if (!(await canRead(resolve(root, MERGE_TRACKER_RELATIVE)))) {
    return { fingerprint: null, claudeAvailable };
  }
  const hashes = [];
  for (const relativePath of FULL_AG_PROBE_FILES) {
    try {
      const contents = await readFile(resolve(root, relativePath), "utf8");
      if (Buffer.byteLength(contents, "utf8") > 2_000_000) {
        return { fingerprint: null, claudeAvailable };
      }
      if (relativePath === "batch/batch-prompt.md"
          && FULL_AG_PROMPT_MARKERS.some((marker) => !contents.includes(marker))) {
        return { fingerprint: null, claudeAvailable };
      }
      if (relativePath === "tracker.mjs"
          && !contents.includes("SELECT id, date, company, role, score, status, pdf, report, notes FROM applications")) {
        return { fingerprint: null, claudeAvailable };
      }
      hashes.push([relativePath, sha256(contents)]);
    } catch {
      return { fingerprint: null, claudeAvailable };
    }
  }
  return { fingerprint: sha256(JSON.stringify(hashes)), claudeAvailable };
}

async function readBatchInput(root) {
  const path = resolve(root, BATCH_INPUT_RELATIVE);
  if (!(await canRead(path))) return [];
  const text = await readFile(path, "utf8");
  return text
    .split(/\r?\n/)
    .map((line) => line.trimEnd())
    .filter((line) => line.length > 0)
    .slice(1)
    .map((line) => {
      const [id, url, source = "", notes = ""] = line.split("\t");
      return {
        id: Number.parseInt(id, 10),
        url: url ?? "",
        source,
        notes,
      };
    })
    .filter((row) => Number.isSafeInteger(row.id) && row.id > 0 && HTTPS_URL_RE.test(row.url));
}

async function readBatchStateMap(root) {
  const path = resolve(root, BATCH_STATE_RELATIVE);
  if (!(await canRead(path))) return new Map();
  const text = await readFile(path, "utf8");
  const map = new Map();
  for (const line of text.split(/\r?\n/)) {
    if (!line.trim() || line.startsWith("id\t")) continue;
    const [id, url, status, , , reportNum, score, error] = line.split("\t");
    const numericId = Number.parseInt(id, 10);
    if (!Number.isSafeInteger(numericId)) continue;
    map.set(numericId, {
      id: numericId,
      url: url ?? "",
      status: status ?? "",
      reportNum: reportNum && reportNum !== "-" ? reportNum : null,
      score: score && score !== "-" ? score : null,
      error: error || null,
    });
  }
  return map;
}

async function assertBatchNotLocked(root) {
  const lockPath = resolve(root, BATCH_LOCK_RELATIVE);
  if (!(await canRead(lockPath))) return;
  const raw = (await readFile(lockPath, "utf8")).trim();
  const pid = Number.parseInt(raw, 10);
  if (!Number.isSafeInteger(pid) || pid <= 0) {
    throw new Error("career-ops batch runner lock is unreadable; refuse to start evaluation.");
  }
  try {
    process.kill(pid, 0);
    throw new Error("career-ops batch runner is already active; refuse concurrent evaluation.");
  } catch (error) {
    if (error?.code === "ESRCH") return;
    throw error;
  }
}

async function writeExclusiveBatchInput(root, offer) {
  const existing = await readBatchInput(root);
  const state = await readBatchStateMap(root);
  const foreignBusy = existing.filter((row) => {
    if (normalizeUrl(row.url) === normalizeUrl(offer.url)) return false;
    const status = state.get(row.id)?.status ?? "pending";
    return ["pending", "processing", "paused_rate_limit", "rate_limited"].includes(status);
  });
  if (foreignBusy.length > 0) {
    throw new Error(
      "career-ops batch-input contains other unfinished offers; refuse to overwrite orchestration state.",
    );
  }
  const body = [
    "id\turl\tsource\tnotes",
    `${offer.id}\t${offer.url}\t${offer.source}\t${offer.notes}`,
    "",
  ].join("\n");
  await writeFile(resolve(root, BATCH_INPUT_RELATIVE), body, "utf8");
}

function runProcess(command, args, { cwd, env, timeoutMs }) {
  return new Promise((resolvePromise, reject) => {
    const child = spawn(command, args, {
      cwd,
      env,
      stdio: ["ignore", "pipe", "pipe"],
      shell: false,
    });
    const stdout = [];
    const stderr = [];
    let settled = false;
    const timer = setTimeout(() => {
      if (settled) return;
      child.kill("SIGTERM");
      reject(new Error(`evaluation process timed out after ${timeoutMs}ms`));
    }, timeoutMs);
    child.stdout.on("data", (chunk) => stdout.push(chunk));
    child.stderr.on("data", (chunk) => stderr.push(chunk));
    child.on("error", (error) => {
      clearTimeout(timer);
      if (settled) return;
      settled = true;
      reject(error);
    });
    child.on("close", (code) => {
      clearTimeout(timer);
      if (settled) return;
      settled = true;
      resolvePromise({
        code: code ?? 1,
        stdout: Buffer.concat(stdout).toString("utf8"),
        stderr: Buffer.concat(stderr).toString("utf8"),
      });
    });
  });
}

async function runBatchRunner(root, offerId) {
  const runner = resolve(root, BATCH_RUNNER_RELATIVE);
  const result = await runProcess(
    "/bin/bash",
    [runner, "--parallel", "1", "--limit", "1", "--start-from", String(offerId)],
    {
      cwd: root,
      env: { ...process.env },
      timeoutMs: BATCH_RUN_TIMEOUT_MS,
    },
  );
  if (result.code !== 0) {
    const diagnostic = (result.stderr || result.stdout).trim().slice(0, 2_000);
    throw new Error(
      `career-ops batch-runner exited ${result.code}${diagnostic ? `: ${diagnostic}` : ""}`,
    );
  }
}

function trackerNotesContainUrl(notes, url) {
  const expected = normalizeUrl(url);
  return String(notes ?? "")
    .split(/\s+/)
    .map((token) => normalizeUrl(token.replace(/[.,;)\]]+$/g, "")))
    .some((candidate) => candidate === expected);
}

export function findEvaluatedTrackerByUrl(records, url) {
  const matches = (Array.isArray(records) ? records : [])
    .filter((record) => record && record.status === "Evaluated")
    .filter((record) => trackerNotesContainUrl(record.notes, url)
      || normalizeUrl(record.notes).includes(normalizeUrl(url)));
  if (matches.length === 1) return matches[0];
  return null;
}

export function findEvaluatedTrackerByReportNum(records, reportNum) {
  if (!reportNum) return null;
  const prefix = String(reportNum).replace(/^0+/, "") || "0";
  const padded = String(reportNum).padStart(3, "0");
  const matches = (Array.isArray(records) ? records : [])
    .filter((record) => record && record.status === "Evaluated")
    .filter((record) => {
      const report = String(record.report ?? "");
      return report.includes(`reports/${padded}-`)
        || report.includes(`[${padded}]`)
        || report.includes(`[${prefix}]`)
        || new RegExp(`\\b${padded}-`).test(report);
    });
  if (matches.length === 1) return matches[0];
  return null;
}

export function findEvaluatedTrackerByIdentity(records, company, title) {
  const normalizeIdentity = (value) => String(value ?? "")
    .toLowerCase()
    .normalize("NFKD")
    .replace(/[^\w\s]/g, " ")
    .replace(/\s+/g, " ")
    .trim();
  const companyNeedle = normalizeIdentity(company);
  const titleNeedle = normalizeIdentity(title);
  if (!companyNeedle || !titleNeedle) return null;
  const matches = (Array.isArray(records) ? records : [])
    .filter((record) => record && record.status === "Evaluated")
    .filter((record) => {
      const companyHay = normalizeIdentity(record.company);
      const titleHay = normalizeIdentity(record.role);
      return (companyHay === companyNeedle || companyHay.includes(companyNeedle) || companyNeedle.includes(companyHay))
        && (titleHay === titleNeedle || titleHay.includes(titleNeedle) || titleNeedle.includes(titleHay));
    });
  if (matches.length === 1) return matches[0];
  return null;
}

function resolveEvaluatedTracker(records, { url, company, title, offerState }) {
  return findEvaluatedTrackerByUrl(records, url)
    || findEvaluatedTrackerByReportNum(records, offerState?.reportNum)
    || findEvaluatedTrackerByIdentity(records, company, title);
}

async function findReportPathForOffer(root, offerState, tracker) {
  if (tracker?.report) {
    const linkMatch = String(tracker.report).match(/\(([^)]+\.md)\)/);
    if (linkMatch) {
      const relative = linkMatch[1].replace(/^\.\.\//, "").replace(/^\//, "");
      if (relative.startsWith("reports/")) return relative;
      if (!relative.includes("/")) return `reports/${relative}`;
      return relative.startsWith("reports/") ? relative : null;
    }
  }
  if (!offerState?.reportNum) return null;
  const reportsDir = resolve(root, "reports");
  const { readdir } = await import("node:fs/promises");
  const entries = await readdir(reportsDir).catch(() => []);
  const prefix = String(offerState.reportNum).padStart(3, "0");
  const matches = entries.filter((name) => name.startsWith(`${prefix}-`) && name.endsWith(".md"));
  if (matches.length !== 1) return null;
  return `reports/${matches[0]}`;
}

async function ensureTrackerMerged(root, runTracker) {
  const mergeScript = resolve(root, MERGE_TRACKER_RELATIVE);
  if (!(await canRead(mergeScript))) {
    throw new Error("career-ops merge-tracker.mjs is unavailable.");
  }
  const result = await runProcess(
    process.execPath,
    [mergeScript],
    {
      cwd: root,
      env: { ...process.env },
      timeoutMs: 120_000,
    },
  );
  if (result.code !== 0) {
    const diagnostic = (result.stderr || result.stdout).trim().slice(0, 1_500);
    throw new Error(`career-ops merge-tracker exited ${result.code}${diagnostic ? `: ${diagnostic}` : ""}`);
  }
  await runTracker(["query", "--limit", "1", "--json"]).catch(() => ({ output: "[]" }));
}

async function releaseFailedReportCollisions(root, offerId, reportNum) {
  if (!reportNum) return;
  const statePath = resolve(root, BATCH_STATE_RELATIVE);
  if (!(await canRead(statePath))) return;
  const text = await readFile(statePath, "utf8");
  const lines = text.split(/\r?\n/);
  if (lines.length === 0) return;
  const header = lines[0];
  const next = [header];
  let changed = false;
  for (const line of lines.slice(1)) {
    if (!line.trim()) {
      next.push(line);
      continue;
    }
    const parts = line.split("\t");
    const id = Number.parseInt(parts[0], 10);
    const status = parts[2] ?? "";
    const rowReport = parts[5] ?? "";
    if (
      Number.isSafeInteger(id)
      && id !== offerId
      && status === "failed"
      && String(rowReport) === String(reportNum)
    ) {
      parts[5] = "-";
      changed = true;
      next.push(parts.join("\t"));
      continue;
    }
    next.push(line);
  }
  if (changed) {
    await writeFile(statePath, `${next.join("\n").replace(/\n+$/, "\n")}`, "utf8");
  }
}

async function recoverMergedTrackerAddition(root, offerId) {
  const { rename, readdir } = await import("node:fs/promises");
  const mergedDir = resolve(root, "batch/tracker-additions/merged");
  const pendingDir = resolve(root, "batch/tracker-additions");
  const entries = await readdir(mergedDir).catch(() => []);
  const matches = entries.filter((name) => (
    name === `${offerId}.tsv`
    || name.includes(`-${offerId}.tsv`)
    || name.startsWith(`${offerId}-`)
  ));
  for (const name of matches) {
    await rename(resolve(mergedDir, name), resolve(pendingDir, name)).catch(() => {});
  }
  return matches.length;
}

/**
 * @param {object} deps
 * @param {string} deps.root
 * @param {object} deps.input
 * @param {() => Promise<object>} deps.capabilityManifest
 * @param {(input: object) => Promise<object>} deps.readEvaluationResult
 * @param {(args: string[]) => Promise<{ output: string }>} deps.runTracker
 */
export async function runFullAgEvaluation(deps) {
  const { root, input, capabilityManifest, readEvaluationResult, runTracker } = deps;
  if (!root) throw new Error("Adapter paths are not configured.");

  const url = String(input?.url ?? "").trim();
  if (!HTTPS_URL_RE.test(url) || url.length > 2_000) {
    throw new Error("evaluation.full_ag.run.v1 url must be a bounded HTTPS URL.");
  }
  const company = String(input?.company ?? "").trim();
  const title = String(input?.title ?? "").trim();
  if (!company || company.length > 240) throw new Error("company must be a non-empty string of at most 240 characters.");
  if (!title || title.length > 500) throw new Error("title must be a non-empty string of at most 500 characters.");
  const expectedFingerprint = String(input?.compatibilityFingerprint ?? "").trim().toLowerCase();
  if (!/^[a-f0-9]{64}$/.test(expectedFingerprint)) {
    throw new Error("compatibilityFingerprint must be a SHA-256 hash.");
  }
  const source = String(input?.source ?? "HereForWork").trim() || "HereForWork";
  if (source.length > 120) throw new Error("source must be at most 120 characters.");

  const before = await capabilityManifest();
  const fullAg = before.capabilities.find(({ id }) => id === "evaluation.full_ag.run.v1");
  const resultRead = before.capabilities.find(({ id }) => id === "evaluation.result.read.v1");
  if (!GIT_SHA_RE.test(before.upstreamRevision ?? "")
      || fullAg?.status !== "degraded"
      || fullAg?.interfaceClass !== "conditional"
      || fullAg?.sourceRevision !== before.upstreamRevision
      || fullAg?.compatibilityFingerprint !== expectedFingerprint
      || resultRead?.status !== "degraded"
      || !resultRead?.compatibilityFingerprint) {
    throw new Error("The career-ops full A-G evaluation capability is unavailable or has drifted.");
  }

  const history = JSON.parse((await runTracker(["query", "--limit", "5000", "--json"])).output);
  if (!Array.isArray(history)) throw new Error("career-ops returned invalid tracker JSON.");
  let offerId = offerIdFromUrl(url);
  let offerState = (await readBatchStateMap(root)).get(offerId) ?? null;
  let tracker = resolveEvaluatedTracker(history, { url, company, title, offerState });
  let executed = false;

  if (!tracker) {
    await assertBatchNotLocked(root);
    await writeExclusiveBatchInput(root, {
      id: offerId,
      url,
      source,
      notes: `${company} — ${title}; Source URL: ${url}`,
    });
    const priorState = (await readBatchStateMap(root)).get(offerId);
    if (!priorState || !["completed", "skipped"].includes(priorState.status)) {
      await runBatchRunner(root, offerId);
      executed = true;
    }
    offerState = (await readBatchStateMap(root)).get(offerId) ?? null;
    const refreshed = JSON.parse((await runTracker(["query", "--limit", "5000", "--json"])).output);
    if (!Array.isArray(refreshed)) throw new Error("career-ops returned invalid tracker JSON after evaluation.");
    tracker = resolveEvaluatedTracker(refreshed, { url, company, title, offerState });
    if (!tracker && offerState?.status === "completed") {
      await releaseFailedReportCollisions(root, offerId, offerState.reportNum);
      await recoverMergedTrackerAddition(root, offerId);
      await ensureTrackerMerged(root, runTracker);
      const rematerialized = JSON.parse((await runTracker(["query", "--limit", "5000", "--json"])).output);
      if (!Array.isArray(rematerialized)) {
        throw new Error("career-ops returned invalid tracker JSON after merge recovery.");
      }
      tracker = resolveEvaluatedTracker(rematerialized, { url, company, title, offerState });
    }
  }

  offerState = (await readBatchStateMap(root)).get(offerId) ?? offerState;
  if (!tracker) {
    const detail = offerState?.error || offerState?.status || "canonical tracker row missing";
    throw new Error(`Full A-G evaluation did not produce a canonical Evaluated tracker row (${detail}).`);
  }

  const reportPath = await findReportPathForOffer(root, offerState, tracker);
  if (!reportPath) {
    throw new Error("Full A-G evaluation did not produce a readable report path.");
  }
  const reportAbsolute = resolve(root, reportPath);
  const reportStat = await stat(reportAbsolute).catch(() => null);
  if (!reportStat?.isFile() || reportStat.size < 1 || reportStat.size > 500_000) {
    throw new Error("Full A-G evaluation report is missing or unbounded.");
  }
  const reportBytes = await readFile(reportAbsolute);
  const evaluation = await readEvaluationResult({
    reportPath,
    reportSha256: sha256(reportBytes),
    trackerId: tracker.id,
    compatibilityFingerprint: resultRead.compatibilityFingerprint,
  });

  const after = await capabilityManifest();
  const afterFullAg = after.capabilities.find(({ id }) => id === "evaluation.full_ag.run.v1");
  if (after.upstreamRevision !== before.upstreamRevision
      || afterFullAg?.compatibilityFingerprint !== expectedFingerprint
      || afterFullAg?.status !== "degraded") {
    throw new Error("career-ops full A-G evaluation capability changed during execution.");
  }

  return {
    contract: "hereforwork.career-ops-evaluation-receipt",
    schemaVersion: 1,
    upstreamRevision: evaluation.upstreamRevision,
    compatibilityFingerprint: expectedFingerprint,
    report: evaluation.report,
    role: evaluation.role,
    canonical: evaluation.canonical,
    evaluation: evaluation.evaluation,
    execution: {
      surface: "batch/batch-runner.sh",
      offerId,
      url,
      executed,
      completedAt: new Date().toISOString(),
    },
  };
}
