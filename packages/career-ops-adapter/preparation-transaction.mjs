import { createHash, randomUUID } from "node:crypto";
import {
  chmod,
  copyFile,
  mkdir,
  readFile,
  realpath,
  rename,
  rm,
  stat,
  unlink,
  writeFile,
} from "node:fs/promises";
import { basename, dirname, isAbsolute, relative, resolve, sep } from "node:path";

const HASH_RE = /^[a-f0-9]{64}$/;
const UUID_V4_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
const STATE_SCHEMA_VERSION = 2;
const PDF_INDEX_HEADER = "# report\tpdf\thtml\tformat\tdate — written by generate-pdf.mjs, do not edit\n";

const FAILURE_POLICIES = Object.freeze({
  invalid_request: ["preflight.input", "fresh_preparation_id"],
  context_changed: ["preflight.context", "fresh_preparation_provider_run"],
  staging_conflict: ["preflight.staging", "fresh_preparation_id"],
  staging_corrupt: ["preflight.staging", "manual_repair_required"],
  preparation_in_progress: ["preflight.staging", "retry_same_preparation"],
  report_number_unavailable: ["stage.reserve", "retry_same_preparation"],
  cv_build_failed: ["stage.cv_html", "retry_same_preparation"],
  cv_fact_check_failed: ["stage.fact_verification", "fresh_preparation_provider_run"],
  pdf_generation_failed: ["stage.pdf", "repair_runtime_then_retry"],
  pdf_fallback_not_configured: ["stage.pdf_fallback", "repair_runtime_then_retry"],
  pdf_fallback_unavailable: ["stage.pdf_fallback", "repair_runtime_then_retry"],
  pdf_fallback_changed: ["stage.pdf_fallback", "manual_repair_required"],
  pdf_fallback_invalid: ["stage.pdf_fallback", "manual_repair_required"],
  staged_pdf_index_invalid: ["stage.pdf_index", "repair_runtime_then_retry"],
  publication_conflict: ["publish.artifacts", "fresh_preparation_id"],
  publication_drift: ["publish.rollback", "manual_repair_required"],
  tracker_drift: ["publish.tracker", "retry_same_preparation"],
  canonical_write_failed: ["publish.tracker", "retry_same_preparation"],
  canonical_identity_mismatch: ["publish.tracker_verify", "manual_repair_required"],
  rollback_failed: ["publish.rollback", "manual_repair_required"],
  artifact_commit_failed: ["preparation.result.commit", "retry_same_preparation"],
});

export class PreparationTransactionError extends Error {
  constructor(code, options = {}) {
    const [defaultStage, defaultPolicy] = FAILURE_POLICIES[code] ?? FAILURE_POLICIES.artifact_commit_failed;
    super(`Preparation commit failed with ${code}.`);
    this.name = "PreparationTransactionError";
    this.code = code;
    this.stage = options.stage ?? defaultStage;
    this.retryPolicy = options.retryPolicy ?? defaultPolicy;
    this.diagnosticId = options.diagnosticId ?? null;
    this.exitCode = Number.isInteger(options.exitCode) ? options.exitCode : null;
  }
}

function failure(code, options) {
  throw new PreparationTransactionError(code, options);
}

function sha256(value) {
  return createHash("sha256").update(value).digest("hex");
}

function canonicalJson(value) {
  if (Array.isArray(value)) return value.map(canonicalJson);
  if (!value || typeof value !== "object") return value;
  return Object.fromEntries(
    Object.keys(value).sort().filter((key) => value[key] !== undefined)
      .map((key) => [key, canonicalJson(value[key])]),
  );
}

async function existsAsFile(path) {
  try {
    return (await stat(path)).isFile();
  } catch {
    return false;
  }
}

async function existsAsDirectory(path) {
  try {
    return (await stat(path)).isDirectory();
  } catch {
    return false;
  }
}

async function fileHash(path) {
  return sha256(await readFile(path));
}

async function writePrivate(path, value, options = {}) {
  await mkdir(dirname(path), { recursive: true, mode: 0o700 });
  await writeFile(path, value, { ...options, mode: 0o600 });
  await chmod(path, 0o600).catch(() => {});
}

async function writeJsonAtomic(path, value) {
  const temporary = `${path}.${randomUUID()}.tmp`;
  await writePrivate(temporary, `${JSON.stringify(value, null, 2)}\n`);
  await rename(temporary, path);
}

async function writeAtomic(path, value) {
  const temporary = `${path}.${randomUUID()}.tmp`;
  await mkdir(dirname(path), { recursive: true });
  await writeFile(temporary, value, { flag: "wx" });
  await rename(temporary, path);
}

async function readJson(path, code = "staging_corrupt") {
  try {
    return JSON.parse(await readFile(path, "utf8"));
  } catch (error) {
    if (error?.code === "ENOENT") return null;
    failure(code);
  }
}

function relativeInside(root, path) {
  const value = relative(root, path);
  if (!value || value === ".." || value.startsWith(`..${sep}`) || isAbsolute(value)) {
    failure("invalid_request", { stage: "preflight.paths" });
  }
  return value.split(sep).join("/");
}

function resolvePersistedPath(root, value) {
  if (typeof value !== "string" || !value || isAbsolute(value)) {
    failure("staging_corrupt", { stage: "preflight.paths" });
  }
  const path = resolve(root, value);
  const inside = relative(root, path);
  if (!inside || inside === ".." || inside.startsWith(`..${sep}`) || isAbsolute(inside)) {
    failure("staging_corrupt", { stage: "preflight.paths" });
  }
  return path;
}

function normalizedUrl(value) {
  const parsed = new URL(value);
  parsed.hash = "";
  parsed.search = "";
  return parsed.href.replace(/\/$/, "");
}

function safeSlug(value, fallback = "role") {
  const slug = String(value ?? "")
    .normalize("NFKD")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 80);
  return slug || fallback;
}

function boundedWarning(value) {
  return String(value ?? "")
    .replace(/(?:https?:\/\/|file:\/\/)[^\s]+/gi, "[redacted]")
    .replace(/\/[\w.@%+~=-]+(?:\/[\w.@%+~=-]+){2,}/g, "[redacted-path]")
    .replace(/[\w.+-]+@[\w.-]+\.[a-z]{2,}/gi, "[redacted-email]")
    .replace(/[a-f0-9]{32,}/gi, "[redacted-id]")
    .replace(/\s+/g, " ")
    .trim()
    .slice(0, 240);
}

function renderFailureMetadata(error) {
  const exitCode = Number.isInteger(error?.exitCode) ? error.exitCode : null;
  return {
    code: "pdf_generation_failed",
    stage: "stage.pdf",
    exitCode,
    detail: boundedWarning(error?.message || "career-ops PDF rendering failed"),
  };
}

function parseFallbackConfiguration(raw) {
  if (!raw) return null;
  let value;
  try {
    value = typeof raw === "string" ? JSON.parse(raw) : raw;
  } catch {
    failure("pdf_fallback_invalid");
  }
  if (!value || typeof value !== "object" || Array.isArray(value)
      || Object.keys(value).some((key) => !["path", "sha256"].includes(key))
      || typeof value.path !== "string" || !isAbsolute(value.path)
      || typeof value.sha256 !== "string" || !HASH_RE.test(value.sha256)) {
    failure("pdf_fallback_invalid");
  }
  return { path: resolve(value.path), sha256: value.sha256 };
}

async function validatePdf(path, { expectedHash = null } = {}) {
  let bytes;
  try {
    bytes = await readFile(path);
  } catch {
    return null;
  }
  if (bytes.length < 64 || !bytes.subarray(0, 5).equals(Buffer.from("%PDF-"))) return null;
  if (!bytes.subarray(Math.max(0, bytes.length - 4096)).includes(Buffer.from("%%EOF"))) return null;
  const pageObjects = bytes.toString("latin1").match(/\/Type\s*\/Page(?!s)\b/g)?.length ?? 0;
  if (pageObjects < 1 || pageObjects > 20) return null;
  const digest = sha256(bytes);
  if (expectedHash && digest !== expectedHash) return null;
  return { bytes, sha256: digest, pageCount: pageObjects };
}

async function validateHtml(path) {
  let bytes;
  try {
    bytes = await readFile(path);
  } catch {
    return false;
  }
  if (bytes.length < 32 || bytes.length > 2_000_000) return false;
  const html = bytes.toString("utf8");
  return /<html(?:\s|>)/i.test(html)
    && /<body(?:\s|>)/i.test(html)
    && !/<script\b|javascript:|form\.submit|application\.submit|application\.finalize/i.test(html);
}

function validFactCheck(output) {
  try {
    const result = JSON.parse(String(output ?? ""));
    return result && typeof result === "object" && ["pass", "warn"].includes(result.verdict);
  } catch {
    return false;
  }
}

export async function reviewedCvFallbackReady(raw) {
  try {
    const fallback = parseFallbackConfiguration(raw);
    if (!fallback) return false;
    const fallbackPath = await realpath(fallback.path);
    return Boolean(await validatePdf(fallbackPath, { expectedHash: fallback.sha256 }));
  } catch {
    return false;
  }
}

function indexContent(existing, reportNum, pdfRelative, htmlRelative, format, eventDate) {
  const normalizedReport = String(Number(reportNum));
  const lines = String(existing ?? "").split(/\r?\n/).filter((line) => {
    if (!line.trim() || line.startsWith("#")) return false;
    const fields = line.split("\t");
    return String(Number(fields[0])) !== normalizedReport && fields[1] !== pdfRelative;
  });
  lines.push([reportNum, pdfRelative, htmlRelative, format, eventDate].join("\t"));
  return `${PDF_INDEX_HEADER}${lines.join("\n")}\n`;
}

function validateStagedIndex(content, reportNum, stagedPdfRelative, stagedHtmlRelative, format) {
  const rows = String(content ?? "").split(/\r?\n/).filter((line) => line.trim() && !line.startsWith("#"));
  if (rows.length !== 1) return false;
  const fields = rows[0].split("\t");
  return String(Number(fields[0])) === String(Number(reportNum))
    && fields[1] === stagedPdfRelative
    && fields[2] === stagedHtmlRelative
    && fields[3] === format;
}

function exactCanonicalRecord(records, state, role, canonicalTracker, root) {
  if (!Array.isArray(records) || !state?.artifacts?.report?.path || !state.reportNum) return null;
  const effectMarker = `HereForWork effect ${state.preparationId}`;
  const urlMarker = `Source URL=${normalizedUrl(role.url)}`;
  const candidates = records.filter((record) => typeof record?.notes === "string"
    && record.notes.includes(effectMarker));
  if (candidates.length !== 1) return null;
  const record = candidates[0];
  const link = String(record.report ?? "").match(/^\[(\d+)\]\(([^)]+)\)$/);
  if (!link || String(Number(link[1])) !== String(Number(state.reportNum))) return null;
  const expectedReport = resolve(root, state.artifacts.report.path);
  const linkedFromTracker = resolve(dirname(canonicalTracker), link[2]);
  const linkedFromRoot = resolve(root, link[2]);
  if (record.status !== "Evaluated"
      || (linkedFromTracker !== expectedReport && linkedFromRoot !== expectedReport)
      || !record.notes.includes(urlMarker)) return null;
  return record;
}

async function verifyPublishedArtifacts(state, root) {
  const entries = Object.values(state?.artifacts ?? {});
  if (entries.length !== 4) return false;
  for (const artifact of entries) {
    if (!artifact || typeof artifact.path !== "string" || !HASH_RE.test(artifact.sha256)) return false;
    const path = resolve(root, artifact.path);
    if (relative(root, path).startsWith("..") || !await existsAsFile(path)) return false;
    if (await fileHash(path) !== artifact.sha256) return false;
  }
  return true;
}

async function committedReplay(state, root, role, canonicalTracker, historyRecords) {
  if (!state || !await verifyPublishedArtifacts(state, root)) return null;
  const record = exactCanonicalRecord(await historyRecords(), state, role, canonicalTracker, root);
  if (!record) return null;
  return {
    outcome: "completed",
    preparationId: state.preparationId,
    contextHash: state.contextHash,
    trackerId: Number(record.id),
    artifacts: state.artifacts,
    cvProvenance: state.cvProvenance,
    warnings: state.warnings ?? [],
  };
}

async function publishExclusive(path, bytes) {
  await mkdir(dirname(path), { recursive: true });
  try {
    await writeFile(path, bytes, { flag: "wx" });
    return true;
  } catch (error) {
    if (error?.code !== "EEXIST") throw error;
    failure("publication_conflict");
  }
}

async function rollbackPublication(publication, state) {
  let incomplete = false;
  if (!publication) return false;
  try {
    if (publication.indexWriteIntent || publication.indexChanged) {
      const current = await readFile(publication.pdfIndexPath, "utf8").catch(() => null);
      if (current === publication.oldIndex) {
        // The process stopped before the intended compare-and-swap write.
      } else if (current !== publication.newIndex) {
        incomplete = true;
      } else if (publication.oldIndex === null) {
        await unlink(publication.pdfIndexPath).catch((error) => {
          if (error?.code !== "ENOENT") throw error;
        });
      } else {
        await writeAtomic(publication.pdfIndexPath, publication.oldIndex);
      }
    }
    if (publication.reportPublishIntent || publication.reportCreated) {
      if (!await existsAsFile(publication.finalReport)) {
        // The process stopped before the exclusive report write.
      } else if (publication.reportCreated
          && await fileHash(publication.finalReport) === state.artifacts.report.sha256) {
        await unlink(publication.finalReport);
      } else {
        // An intent without the persisted created flag is ambiguous after a
        // crash or concurrent exclusive-create failure. Never delete it.
        incomplete = true;
      }
    }
    if (publication.bundleMoveIntent || publication.bundleMoved) {
      const checks = publication.bundleChecks ?? [];
      const finalExists = await existsAsDirectory(publication.finalRoot);
      const candidateExists = await existsAsDirectory(publication.candidateRoot);
      const unchanged = finalExists
        && (await Promise.all(checks.map(async ([path, digest]) => await existsAsFile(path) && await fileHash(path) === digest))).every(Boolean);
      if (!finalExists && candidateExists) {
        // The process stopped before the intended rename.
      } else if (!unchanged || candidateExists) {
        incomplete = true;
      } else {
        await mkdir(dirname(publication.candidateRoot), { recursive: true, mode: 0o700 });
        await rename(publication.finalRoot, publication.candidateRoot);
      }
    }
  } catch {
    incomplete = true;
  }
  return incomplete;
}

function statePublication(state, root, candidateRoot, canonicalIndexPath) {
  if (!state?.publication) return null;
  const value = state.publication;
  return {
    ...value,
    finalRoot: resolvePersistedPath(root, value.finalRootRelative),
    finalReport: resolvePersistedPath(root, value.finalReportRelative),
    candidateRoot,
    pdfIndexPath: canonicalIndexPath,
    bundleChecks: (value.bundleChecks ?? []).map(([path, digest]) => [resolvePersistedPath(root, path), digest]),
  };
}

async function saveState(statePath, state, updates = {}) {
  Object.assign(state, updates, { updatedAt: new Date().toISOString() });
  await writeJsonAtomic(statePath, state);
}

function reportWithProvenance(reportHeader, role, result, eventDate, reportNum, pdfRelative, provenance) {
  const base = reportHeader(role, result, eventDate, reportNum, pdfRelative);
  if (provenance.source !== "user_reviewed_fallback") return base;
  const notice = "**CV source:** User-reviewed fallback PDF. The attached CV was not tailored for this role, and the proposed CV changes below were not applied to it.\n";
  return base.replace("\n---\n", `\n${notice}\n---\n`);
}

function changesWithProvenance(changes, provenance) {
  if (provenance.source !== "user_reviewed_fallback") return `${changes.trim()}\n`;
  return `> The attached PDF is the user's reviewed fallback CV. It is not tailored for this role. The items below are proposed changes only and were not applied to that PDF.\n\n${changes.trim()}\n`;
}

/**
 * Durable compensating transaction for preparation publication.
 *
 * career-ops remains the canonical writer. The adapter stages artifacts, invokes only
 * fixed upstream CLIs, and compensates its own file/index writes until merge-tracker
 * commits the tracker effect. External writers are not part of a global transaction;
 * drift is detected with compare-and-swap checks and becomes manual repair.
 */
export async function commitPreparationTransaction(options) {
  const {
    input,
    root: configuredRoot,
    trackerDb,
    stagingRoot: configuredStagingRoot,
    fallbackConfiguration,
    preparationSources,
    contextHash,
    assertPreparationResult,
    runCareerOpsScript,
    historyRecords,
    reportHeader,
  } = options;
  const preparationId = String(input?.preparationId ?? "").toLowerCase();
  if (!UUID_V4_RE.test(preparationId)) failure("invalid_request", { diagnosticId: null });
  const diagnostic = { diagnosticId: preparationId };
  if (!configuredRoot || !configuredStagingRoot || !trackerDb) failure("invalid_request", diagnostic);

  await mkdir(configuredStagingRoot, { recursive: true, mode: 0o700 });
  const root = await realpath(resolve(configuredRoot));
  const stagingRoot = await realpath(resolve(configuredStagingRoot));
  const effectDirectory = resolve(stagingRoot, preparationId);
  if (basename(effectDirectory) !== preparationId) failure("invalid_request", { ...diagnostic, stage: "preflight.paths" });
  await mkdir(effectDirectory, { recursive: true, mode: 0o700 });
  await chmod(effectDirectory, 0o700).catch(() => {});
  const lockPath = resolve(effectDirectory, "transaction.lock");
  try {
    await mkdir(lockPath);
  } catch (error) {
    if (error?.code === "EEXIST") failure("preparation_in_progress", diagnostic);
    throw error;
  }

  const statePath = resolve(effectDirectory, "commit-state.json");
  const providerResultPath = resolve(effectDirectory, "provider-result.json");
  const candidateRoot = resolve(root, "output", ".hfw-preparation-staging", preparationId, "candidate");
  const stagedRoot = dirname(candidateRoot);
  const stagedPayload = resolve(candidateRoot, "cv", "tailored", "v001", "cv-payload.json");
  const stagedHtml = resolve(candidateRoot, "cv", "tailored", "v001", "cv.html");
  const stagedPdf = resolve(candidateRoot, "cv", "tailored", "v001", "cv.pdf");
  const stagedChanges = resolve(candidateRoot, "cv", "tailored", "v001", "changes.md");
  const stagedJob = resolve(candidateRoot, "jd", "current.md");
  const stagedReport = resolve(stagedRoot, "report.md");
  const stagedIndex = resolve(effectDirectory, "pdf-index.staged.tsv");
  const canonicalTracker = await existsAsFile(resolve(root, "data", "applications.md"))
    ? resolve(root, "data", "applications.md")
    : resolve(root, "applications.md");
  const canonicalIndex = resolve(root, "data", "pdf-index.tsv");
  let state;
  let publication = null;
  let mergeSucceeded = false;
  let reservedReportNum = null;
  let stateOwned = false;

  const command = async (script, args, env, code, stage, retryPolicy) => {
    try {
      return await runCareerOpsScript(script, args, env);
    } catch (error) {
      throw new PreparationTransactionError(code, {
        ...diagnostic,
        stage,
        retryPolicy,
        exitCode: error?.exitCode,
      });
    }
  };
  const releaseReservation = async (reportNum) => {
    if (!reportNum) return;
    await runCareerOpsScript("reserve-report-num.mjs", ["--release", reportNum], {
      CAREER_OPS_TRACKER: canonicalTracker,
      CAREER_OPS_TRACKER_DB: trackerDb,
    }).catch(() => null);
  };

  try {
    const role = options.role;
    const job = input.job;
    const sources = await preparationSources();
    const currentContextHash = contextHash(role, job, sources);
    assertPreparationResult(input.result, currentContextHash);
    if (input.result.contextHash !== currentContextHash) failure("context_changed", diagnostic);
    const identityHash = sha256(JSON.stringify(canonicalJson({
      preparationId,
      eventDate: input.eventDate,
      role,
      job,
      result: input.result,
    })));
    state = await readJson(statePath);
    if (state) {
      if (state.schemaVersion !== STATE_SCHEMA_VERSION || state.preparationId !== preparationId
          || state.identityHash !== identityHash || state.contextHash !== currentContextHash) {
        failure("staging_conflict", diagnostic);
      }
      const replay = await committedReplay(state, root, role, canonicalTracker, historyRecords);
      if (replay) {
        await saveState(statePath, state, { status: "committed", stage: "complete" });
        return replay;
      }
      if (state.trackerCommitted || state.status === "committed") {
        failure("canonical_identity_mismatch", diagnostic);
      }
      publication = statePublication(state, root, candidateRoot, canonicalIndex);
      if (publication) {
        const rollbackIncomplete = await rollbackPublication(publication, state);
        if (rollbackIncomplete) failure("rollback_failed", diagnostic);
        await releaseReservation(state.reportNum);
        state.reportNum = null;
        state.artifacts = null;
        state.publication = null;
      }
    } else {
      state = {
        schemaVersion: STATE_SCHEMA_VERSION,
        preparationId,
        contextHash: currentContextHash,
        identityHash,
        eventDate: input.eventDate,
        status: "initialized",
        stage: "preflight.complete",
        reportNum: null,
        artifacts: null,
        cvProvenance: null,
        warnings: [],
        publication: null,
        trackerCommitted: false,
      };
    }
    stateOwned = true;
    await saveState(statePath, state);
    const providerResultBytes = Buffer.from(`${JSON.stringify(input.result, null, 2)}\n`);
    if (await existsAsFile(providerResultPath)) {
      if (sha256(await readFile(providerResultPath)) !== sha256(providerResultBytes)) {
        failure("staging_conflict", diagnostic);
      }
    } else {
      await writePrivate(providerResultPath, providerResultBytes, { flag: "wx" });
    }

    await rm(stagedRoot, { recursive: true, force: true });
    await mkdir(candidateRoot, { recursive: true, mode: 0o700 });
    await saveState(statePath, state, { status: "staging", stage: "stage.sources" });
    await writePrivate(stagedPayload, `${JSON.stringify(input.result.cvPayload, null, 2)}\n`);
    await writePrivate(stagedJob, `${job.description.trim()}\n`);

    let reservation;
    try {
      reservation = await runCareerOpsScript("reserve-report-num.mjs", [], {
        CAREER_OPS_TRACKER: canonicalTracker,
        CAREER_OPS_TRACKER_DB: trackerDb,
      });
    } catch (error) {
      throw new PreparationTransactionError("report_number_unavailable", { ...diagnostic, exitCode: error?.exitCode });
    }
    const reportNum = reservation.output.trim();
    if (!/^\d{3,}$/.test(reportNum)) failure("report_number_unavailable", diagnostic);
    reservedReportNum = reportNum;
    state.reportNum = reportNum;
    await saveState(statePath, state, { stage: "stage.reserve" });

    await saveState(statePath, state, { stage: "stage.cv_html" });
    await command("build-cv-html.mjs", [stagedPayload, stagedHtml], {}, "cv_build_failed", "stage.cv_html", "retry_same_preparation");
    if (!await validateHtml(stagedHtml)) failure("cv_build_failed", diagnostic);

    await saveState(statePath, state, { stage: "stage.fact_verification" });
    const factArgs = [stagedHtml, "--source", resolve(root, "cv.md")];
    if (await existsAsFile(resolve(root, "article-digest.md"))) factArgs.push("--source", resolve(root, "article-digest.md"));
    if (await existsAsFile(resolve(root, "config", "cv-facts.json"))) factArgs.push("--config", resolve(root, "config", "cv-facts.json"));
    factArgs.push("--json");
    const factCheck = await command("verify-cv-facts.mjs", factArgs, {}, "cv_fact_check_failed", "stage.fact_verification", "fresh_preparation_provider_run");
    if (!validFactCheck(factCheck.output)) failure("cv_fact_check_failed", diagnostic);

    let cvProvenance = { source: "tailored_generated", tailored: true, sourceSha256: null, renderRecovery: null };
    const warnings = [];
    await saveState(statePath, state, { stage: "stage.pdf" });
    let renderError = null;
    try {
      await command("generate-pdf.mjs", [
        stagedHtml,
        stagedPdf,
        `--format=${input.result.cvPayload.page_format}`,
        `--report=${reportNum}`,
        "--allow-reorder",
      ], {
        CAREER_OPS_TRACKER: canonicalTracker,
        CAREER_OPS_TRACKER_DB: trackerDb,
        CAREER_OPS_PDF_INDEX: stagedIndex,
      }, "pdf_generation_failed", "stage.pdf", "repair_runtime_then_retry");
      if (!await validatePdf(stagedPdf)) failure("pdf_generation_failed", diagnostic);
    } catch (error) {
      renderError = error instanceof PreparationTransactionError
        ? error
        : new PreparationTransactionError("pdf_generation_failed", diagnostic);
    }
    if (renderError) {
      const fallback = parseFallbackConfiguration(fallbackConfiguration);
      if (!fallback) failure("pdf_fallback_not_configured", { ...diagnostic, stage: "stage.pdf_fallback" });
      let fallbackPath;
      try {
        fallbackPath = await realpath(fallback.path);
      } catch {
        failure("pdf_fallback_unavailable", diagnostic);
      }
      const actualFallbackHash = await fileHash(fallbackPath).catch(() => null);
      if (!actualFallbackHash) failure("pdf_fallback_unavailable", diagnostic);
      if (actualFallbackHash !== fallback.sha256) failure("pdf_fallback_changed", diagnostic);
      const validFallback = await validatePdf(fallbackPath, { expectedHash: fallback.sha256 });
      if (!validFallback) failure("pdf_fallback_invalid", diagnostic);
      await copyFile(fallbackPath, stagedPdf);
      const recovery = renderFailureMetadata(renderError);
      cvProvenance = {
        source: "user_reviewed_fallback",
        tailored: false,
        sourceSha256: fallback.sha256,
        renderRecovery: recovery,
      };
      warnings.push({
        code: recovery.code,
        stage: recovery.stage,
        recoveredBy: "user_reviewed_fallback",
        detail: recovery.detail,
      });
      await writePrivate(stagedIndex, [
        PDF_INDEX_HEADER.trimEnd(),
        [reportNum, relativeInside(root, stagedPdf), relativeInside(root, stagedHtml), input.result.cvPayload.page_format, input.eventDate].join("\t"),
        "",
      ].join("\n"));
    }
    const validPdf = await validatePdf(stagedPdf);
    if (!validPdf) failure("pdf_generation_failed", diagnostic);
    const stagedIndexContent = await readFile(stagedIndex, "utf8").catch(() => "");
    if (!validateStagedIndex(
      stagedIndexContent,
      reportNum,
      relativeInside(root, stagedPdf),
      relativeInside(root, stagedHtml),
      input.result.cvPayload.page_format,
    )) failure("staged_pdf_index_invalid", diagnostic);
    await writePrivate(stagedChanges, changesWithProvenance(input.result.cvChangesMarkdown, cvProvenance));

    const stagedHashes = {
      payload: await fileHash(stagedPayload),
      html: await fileHash(stagedHtml),
      pdf: validPdf.sha256,
      changes: await fileHash(stagedChanges),
      job: await fileHash(stagedJob),
    };
    const revalidatedSources = await preparationSources();
    if (contextHash(role, job, revalidatedSources) !== currentContextHash) {
      failure("context_changed", { ...diagnostic, stage: "stage.context_revalidation" });
    }

    const key = `${reportNum}-${safeSlug(role.company)}-${safeSlug(role.title)}`;
    const finalRoot = resolve(root, "output", key);
    const finalPayload = resolve(finalRoot, "cv", "tailored", "v001", "cv-payload.json");
    const finalHtml = resolve(finalRoot, "cv", "tailored", "v001", "cv.html");
    const finalPdf = resolve(finalRoot, "cv", "tailored", "v001", "cv.pdf");
    const finalChanges = resolve(finalRoot, "cv", "tailored", "v001", "changes.md");
    const finalJob = resolve(finalRoot, "jd", "current.md");
    const finalReport = resolve(root, "reports", `${reportNum}-${safeSlug(role.company)}-${input.eventDate}.md`);
    const reportRelative = relativeInside(root, finalReport);
    const htmlRelative = relativeInside(root, finalHtml);
    const pdfRelative = relativeInside(root, finalPdf);
    const changesRelative = relativeInside(root, finalChanges);
    const reportBytes = Buffer.from(reportWithProvenance(
      reportHeader,
      role,
      input.result,
      input.eventDate,
      reportNum,
      pdfRelative,
      cvProvenance,
    ));
    await writePrivate(stagedReport, reportBytes);
    const artifacts = {
      report: { path: reportRelative, sha256: sha256(reportBytes) },
      cvHtml: { path: htmlRelative, sha256: stagedHashes.html },
      cvPdf: { path: pdfRelative, sha256: stagedHashes.pdf },
      cvChanges: { path: changesRelative, sha256: stagedHashes.changes },
    };
    state.artifacts = artifacts;
    state.cleanupPaths = [
      relativeInside(root, finalPayload),
      htmlRelative,
      pdfRelative,
      changesRelative,
      relativeInside(root, finalJob),
    ];
    state.cvProvenance = cvProvenance;
    state.warnings = warnings;

    if (await existsAsDirectory(finalRoot) || await existsAsFile(finalReport)) {
      failure("publication_conflict", diagnostic);
    }
    const oldIndex = await readFile(canonicalIndex, "utf8").catch((error) => error?.code === "ENOENT" ? null : Promise.reject(error));
    const oldIndexHash = oldIndex === null ? null : sha256(oldIndex);
    const newIndex = indexContent(oldIndex, reportNum, pdfRelative, htmlRelative, input.result.cvPayload.page_format, input.eventDate);
    publication = {
      reportNum,
      finalRoot,
      finalReport,
      candidateRoot,
      pdfIndexPath: canonicalIndex,
      oldIndex,
      oldIndexHash,
      newIndex,
      indexChanged: false,
      indexWriteIntent: false,
      reportCreated: false,
      reportPublishIntent: false,
      bundleMoved: false,
      bundleMoveIntent: false,
      bundleChecks: [
        [finalPayload, stagedHashes.payload],
        [finalHtml, stagedHashes.html],
        [finalPdf, stagedHashes.pdf],
        [finalChanges, stagedHashes.changes],
        [finalJob, stagedHashes.job],
      ],
    };
    state.publication = {
      reportNum,
      finalRootRelative: relativeInside(root, finalRoot),
      finalReportRelative: reportRelative,
      oldIndex,
      oldIndexHash,
      newIndex,
      indexChanged: false,
      indexWriteIntent: false,
      reportCreated: false,
      reportPublishIntent: false,
      bundleMoved: false,
      bundleMoveIntent: false,
      bundleChecks: publication.bundleChecks.map(([path, digest]) => [relativeInside(root, path), digest]),
    };
    await saveState(statePath, state, { status: "publishing", stage: "publish.artifacts" });

    publication.bundleMoveIntent = true;
    state.publication.bundleMoveIntent = true;
    await saveState(statePath, state);
    await mkdir(dirname(finalRoot), { recursive: true });
    await rename(candidateRoot, finalRoot);
    publication.bundleMoved = true;
    state.publication.bundleMoved = true;
    await saveState(statePath, state);
    publication.reportPublishIntent = true;
    state.publication.reportPublishIntent = true;
    await saveState(statePath, state);
    publication.reportCreated = await publishExclusive(finalReport, reportBytes);
    state.publication.reportCreated = publication.reportCreated;
    await saveState(statePath, state, { stage: "publish.report" });

    const currentIndex = await readFile(canonicalIndex, "utf8").catch((error) => error?.code === "ENOENT" ? null : Promise.reject(error));
    if ((currentIndex === null ? null : sha256(currentIndex)) !== oldIndexHash) failure("publication_drift", { ...diagnostic, stage: "publish.pdf_index" });
    publication.indexWriteIntent = true;
    state.publication.indexWriteIntent = true;
    await saveState(statePath, state);
    await writeAtomic(canonicalIndex, newIndex);
    publication.indexChanged = true;
    state.publication.indexChanged = true;
    await saveState(statePath, state, { stage: "publish.pdf_index" });

    const publishSources = await preparationSources();
    if (contextHash(role, job, publishSources) !== currentContextHash) {
      failure("context_changed", { ...diagnostic, stage: "publish.context_revalidation" });
    }
    const trackerBefore = await historyRecords();
    const trackerBeforeHash = sha256(JSON.stringify(canonicalJson(trackerBefore)));
    const trackerFileBeforeHash = await fileHash(canonicalTracker);
    const additions = resolve(effectDirectory, "tracker-additions");
    await rm(additions, { recursive: true, force: true });
    await mkdir(additions, { recursive: true, mode: 0o700 });
    const notes = `HereForWork effect ${preparationId}; Source URL=${normalizedUrl(role.url)}; Prepared by HereForWork; ATS=${job.provider}; CV source=${cvProvenance.source}`;
    const addition = [
      Number(reportNum), input.eventDate, role.company, role.title, "Evaluated",
      `${input.result.score.toFixed(1)}/5`, "✅", `[${reportNum}](${reportRelative})`, notes,
      "via=HereForWork", role.location, role.url,
    ].join("\t") + "\n";
    await writePrivate(resolve(additions, `${reportNum}-${preparationId}.tsv`), addition, { flag: "wx" });
    const trackerImmediatelyBefore = await historyRecords();
    if (sha256(JSON.stringify(canonicalJson(trackerImmediatelyBefore))) !== trackerBeforeHash
        || await fileHash(canonicalTracker) !== trackerFileBeforeHash) {
      failure("tracker_drift", diagnostic);
    }
    await saveState(statePath, state, { status: "canonical_pending", stage: "publish.tracker" });
    await command("merge-tracker.mjs", [], {
      CAREER_OPS_TRACKER: canonicalTracker,
      CAREER_OPS_TRACKER_DB: trackerDb,
      CAREER_OPS_PDF_INDEX: canonicalIndex,
      CAREER_OPS_ADDITIONS: additions,
      CAREER_OPS_BATCH_STATE: resolve(effectDirectory, "empty-batch-state.tsv"),
    }, "canonical_write_failed", "publish.tracker", "retry_same_preparation");
    mergeSucceeded = true;
    state.trackerCommitted = true;
    await saveState(statePath, state, { stage: "publish.tracker_verify" });
    const record = exactCanonicalRecord(await historyRecords(), state, role, canonicalTracker, root);
    if (!record) failure("canonical_identity_mismatch", diagnostic);

    await releaseReservation(reportNum);
    reservedReportNum = null;
    state.publication = null;
    await saveState(statePath, state, { status: "committed", stage: "complete" });
    await rm(stagedRoot, { recursive: true, force: true }).catch(() => null);
    return {
      outcome: "completed",
      preparationId,
      contextHash: currentContextHash,
      trackerId: Number(record.id),
      artifacts,
      cvProvenance,
      warnings,
    };
  } catch (error) {
    const original = error instanceof PreparationTransactionError
      ? error
      : new PreparationTransactionError("artifact_commit_failed", {
        diagnosticId: state?.preparationId ?? null,
        stage: state?.stage ?? "preparation.result.commit",
      });
    let rollbackIncomplete = false;
    if (publication && !mergeSucceeded) rollbackIncomplete = await rollbackPublication(publication, state);
    if (reservedReportNum && !mergeSucceeded) await releaseReservation(reservedReportNum);
    if (state && stateOwned) {
      if (!rollbackIncomplete && !mergeSucceeded) {
        state.reportNum = null;
        state.artifacts = null;
        state.cvProvenance = null;
        state.warnings = [];
        state.publication = null;
      }
      const failureError = rollbackIncomplete
        ? new PreparationTransactionError("rollback_failed", { diagnosticId: state.preparationId, stage: original.stage })
        : original;
      await saveState(statePath, state, {
        status: mergeSucceeded || rollbackIncomplete ? "manual_repair_required" : "failed",
        stage: failureError.stage,
        lastFailure: {
          code: failureError.code,
          stage: failureError.stage,
          retryPolicy: failureError.retryPolicy,
          exitCode: failureError.exitCode,
          updatedAt: new Date().toISOString(),
        },
      }).catch(() => null);
      if (mergeSucceeded && failureError.retryPolicy !== "manual_repair_required") {
        throw new PreparationTransactionError("canonical_identity_mismatch", { diagnosticId: state.preparationId });
      }
      if (rollbackIncomplete) throw failureError;
    }
    throw original;
  } finally {
    await rm(lockPath, { recursive: true, force: true }).catch(() => null);
  }
}
