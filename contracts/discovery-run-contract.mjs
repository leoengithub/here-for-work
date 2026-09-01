import { createHash } from "node:crypto";

const topLevelKeys = [
  "contract",
  "schemaVersion",
  "windowId",
  "runId",
  "supersedesRunId",
  "source",
  "coverage",
  "generatedAt",
  "status",
  "findings",
  "issues",
  "integrity",
];
const requiredTopLevelKeys = topLevelKeys.filter((key) => key !== "supersedesRunId");

function fail(message) {
  throw new Error(`Invalid discovery artifact: ${message}`);
}

function object(value, path) {
  if (!value || typeof value !== "object" || Array.isArray(value)) fail(`${path} must be an object.`);
  return value;
}

function exactKeys(value, allowed, required, path) {
  const keys = Object.keys(value);
  const unknown = keys.find((key) => !allowed.includes(key));
  if (unknown) fail(`${path} contains unknown property ${unknown}.`);
  const missing = required.find((key) => !Object.hasOwn(value, key));
  if (missing) fail(`${path} is missing ${missing}.`);
}

function text(value, path, { min = 1, max = 4000 } = {}) {
  if (typeof value !== "string" || value.length < min || value.length > max) {
    fail(`${path} must be a string between ${min} and ${max} characters.`);
  }
}

function legacyText(value, path, min = 1) {
  if (typeof value !== "string" || value.length < min) {
    fail(`${path} must be a string of at least ${min} characters.`);
  }
}

function dateTime(value, path) {
  text(value, path, { max: 100 });
  const match = /^(\d{4}-\d{2}-\d{2})T(\d{2}):(\d{2}):(\d{2})(?:\.\d+)?(?:Z|[+-](\d{2}):(\d{2}))$/.exec(value);
  if (!match) {
    fail(`${path} must be an ISO date-time with a timezone.`);
  }
  date(match[1], path);
  const [, , hours, minutes, seconds, offsetHours, offsetMinutes] = match;
  if (Number(hours) > 23
      || Number(minutes) > 59
      || Number(seconds) > 59
      || (offsetHours !== undefined && (Number(offsetHours) > 23 || Number(offsetMinutes) > 59))
      || !Number.isFinite(Date.parse(value))) {
    fail(`${path} must be a real ISO date-time with a valid timezone offset.`);
  }
}

function date(value, path) {
  text(value, path, { max: 10 });
  if (!/^\d{4}-\d{2}-\d{2}$/.test(value)) fail(`${path} must be an ISO date.`);
  const parsed = new Date(`${value}T00:00:00Z`);
  if (!Number.isFinite(parsed.valueOf()) || parsed.toISOString().slice(0, 10) !== value) {
    fail(`${path} must be a real ISO date.`);
  }
}

function postedAt(value, path) {
  if (value === null) return;
  if (/^\d{4}-\d{2}-\d{2}$/.test(value)) {
    date(value, path);
    return;
  }
  dateTime(value, path);
}

function identifier(value, path, { min = 1, max = 200 } = {}) {
  text(value, path, { min, max });
  if (!/^[A-Za-z0-9][A-Za-z0-9._:-]*$/.test(value)) {
    fail(`${path} must use only letters, numbers, dot, underscore, colon, or hyphen.`);
  }
}

function nullableHttps(value, path) {
  if (value === null) return;
  text(value, path, { max: 2000 });
  let url;
  try { url = new URL(value); } catch { fail(`${path} must be a valid HTTPS URL or null.`); }
  if (url.protocol !== "https:") fail(`${path} must be a valid HTTPS URL or null.`);
}

function validateEvidence(value, path) {
  object(value, path);
  const keys = ["evidenceId", "kind", "reference", "url", "observedAt", "summary", "contentSha256"];
  exactKeys(value, keys, ["evidenceId", "kind", "reference", "observedAt"], path);
  identifier(value.evidenceId, `${path}.evidenceId`, { max: 200 });
  if (!["source_listing", "company_page", "authorization", "legitimacy", "other"].includes(value.kind)) {
    fail(`${path}.kind is unsupported.`);
  }
  text(value.reference, `${path}.reference`, { max: 2000 });
  if (Object.hasOwn(value, "url")) nullableHttps(value.url, `${path}.url`);
  dateTime(value.observedAt, `${path}.observedAt`);
  if (Object.hasOwn(value, "summary")) text(value.summary, `${path}.summary`);
  if (Object.hasOwn(value, "contentSha256") && !/^[a-f0-9]{64}$/.test(value.contentSha256)) {
    fail(`${path}.contentSha256 must be a lowercase SHA-256 digest.`);
  }
}

function validateMatchScore(value, path) {
  object(value, path);
  if (value.status === "scored") {
    const keys = ["status", "scale", "value", "authority", "sourceVersion", "scoredAt"];
    exactKeys(value, keys, keys, path);
    if (value.scale !== "career_ops_1_to_5") fail(`${path}.scale must be career_ops_1_to_5.`);
    if (!Number.isFinite(value.value) || value.value < 1 || value.value > 5) {
      fail(`${path}.value must be on the native 1–5 scale.`);
    }
    if (value.authority !== "career-ops") fail(`${path}.authority must be career-ops.`);
    text(value.sourceVersion, `${path}.sourceVersion`, { max: 200 });
    dateTime(value.scoredAt, `${path}.scoredAt`);
    return;
  }
  if (value.status === "not_scored") {
    const keys = ["status", "reason", "authority", "sourceVersion", "checkedAt"];
    exactKeys(value, keys, keys, path);
    if (!["not_evaluated", "unavailable", "deferred", "insufficient_evidence"].includes(value.reason)) {
      fail(`${path}.reason is unsupported.`);
    }
    if (value.authority !== "career-ops") fail(`${path}.authority must be career-ops.`);
    text(value.sourceVersion, `${path}.sourceVersion`, { max: 200 });
    dateTime(value.checkedAt, `${path}.checkedAt`);
    return;
  }
  fail(`${path}.status must be scored or not_scored.`);
}

function validateFinding(value, path, runSource) {
  object(value, path);
  const required = [
    "findingId", "sourceId", "source", "sourceRoleId", "company", "title", "location",
    "discoveredAt", "applicationUrl", "normalizedKey", "queueGroup", "eligibilitySummary",
    "uncertainty", "evidence", "matchScore",
  ];
  const allowed = [...required, "postedAt", "legitimacy"];
  exactKeys(value, allowed, required, path);
  for (const key of ["source", "sourceRoleId", "company", "title", "location", "normalizedKey", "eligibilitySummary"]) {
    text(value[key], `${path}.${key}`, { max: key === "eligibilitySummary" ? 2000 : 500 });
  }
  identifier(value.findingId, `${path}.findingId`, { max: 200 });
  identifier(value.sourceId, `${path}.sourceId`, { max: 100 });
  if (value.normalizedKey.length < 3) fail(`${path}.normalizedKey must be at least 3 characters.`);
  if (value.sourceId !== runSource.sourceId) fail(`${path}.sourceId must match the run sourceId.`);
  if (value.source !== runSource.displayName) fail(`${path}.source must match the run displayName.`);
  dateTime(value.discoveredAt, `${path}.discoveredAt`);
  nullableHttps(value.applicationUrl, `${path}.applicationUrl`);
  if (Object.hasOwn(value, "postedAt")) postedAt(value.postedAt, `${path}.postedAt`);
  if (!["strong_match", "other_new", "needs_decision"].includes(value.queueGroup)) fail(`${path}.queueGroup is unsupported.`);
  if (value.uncertainty !== null) text(value.uncertainty, `${path}.uncertainty`, { max: 2000 });
  if (Object.hasOwn(value, "legitimacy") && !["high_confidence", "proceed_with_caution", "suspicious"].includes(value.legitimacy)) {
    fail(`${path}.legitimacy is unsupported.`);
  }
  if (!Array.isArray(value.evidence) || value.evidence.length === 0) fail(`${path}.evidence must not be empty.`);
  value.evidence.forEach((item, index) => validateEvidence(item, `${path}.evidence[${index}]`));
  if (new Set(value.evidence.map((item) => item.evidenceId)).size !== value.evidence.length) {
    fail(`${path}.evidence contains duplicate evidenceId values.`);
  }
  validateMatchScore(value.matchScore, `${path}.matchScore`);
}

function validateIssue(value, path) {
  object(value, path);
  const keys = ["issueId", "code", "message", "retryable"];
  exactKeys(value, keys, keys, path);
  identifier(value.issueId, `${path}.issueId`, { max: 200 });
  text(value.code, `${path}.code`, { max: 200 });
  text(value.message, `${path}.message`, { max: 2000 });
  if (typeof value.retryable !== "boolean") fail(`${path}.retryable must be boolean.`);
}

function utf8Compare(left, right) {
  return Buffer.compare(Buffer.from(left, "utf8"), Buffer.from(right, "utf8"));
}

function canonicalJson(value) {
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(",")}]`;
  if (!value || typeof value !== "object") return JSON.stringify(value);
  return `{${Object.keys(value).sort(utf8Compare).map((key) => `${JSON.stringify(key)}:${canonicalJson(value[key])}`).join(",")}}`;
}

function integrityPayload(run) {
  const payload = structuredClone(run);
  delete payload.integrity;
  payload.findings?.sort((left, right) => utf8Compare(left.findingId, right.findingId));
  for (const finding of payload.findings ?? []) {
    finding.evidence?.sort((left, right) => utf8Compare(left.evidenceId, right.evidenceId));
  }
  payload.issues?.sort((left, right) => utf8Compare(left.issueId, right.issueId));
  return payload;
}

export function canonicalizeDiscoveryRun(run) {
  return canonicalJson(integrityPayload(run));
}

export function computeDiscoveryRunDigest(run) {
  return createHash("sha256").update(canonicalizeDiscoveryRun(run), "utf8").digest("hex");
}

export function validateDiscoveryRun(run) {
  object(run, "run");
  exactKeys(run, topLevelKeys, requiredTopLevelKeys, "run");
  if (run.contract !== "hereforwork.discovery-run") fail("run.contract is unsupported.");
  if (run.schemaVersion !== 1) fail("run.schemaVersion is unsupported.");
  identifier(run.windowId, "run.windowId", { min: 8, max: 128 });
  identifier(run.runId, "run.runId", { min: 8, max: 128 });
  if (Object.hasOwn(run, "supersedesRunId")) {
    identifier(run.supersedesRunId, "run.supersedesRunId", { min: 8, max: 128 });
    if (run.supersedesRunId === run.runId) fail("run.supersedesRunId must differ from run.runId.");
  }

  object(run.source, "run.source");
  const sourceKeys = ["sourceId", "displayName", "producer", "producerVersion"];
  exactKeys(run.source, sourceKeys, sourceKeys, "run.source");
  identifier(run.source.sourceId, "run.source.sourceId", { max: 100 });
  text(run.source.displayName, "run.source.displayName", { max: 200 });
  text(run.source.producer, "run.source.producer", { max: 100 });
  text(run.source.producerVersion, "run.source.producerVersion", { max: 200 });

  object(run.coverage, "run.coverage");
  const coverageKeys = ["windowStart", "windowEnd", "timezone"];
  exactKeys(run.coverage, coverageKeys, coverageKeys, "run.coverage");
  dateTime(run.coverage.windowStart, "run.coverage.windowStart");
  dateTime(run.coverage.windowEnd, "run.coverage.windowEnd");
  text(run.coverage.timezone, "run.coverage.timezone", { max: 100 });
  if (Date.parse(run.coverage.windowStart) > Date.parse(run.coverage.windowEnd)) {
    fail("run.coverage.windowStart must not be after windowEnd.");
  }
  dateTime(run.generatedAt, "run.generatedAt");
  if (Date.parse(run.generatedAt) < Date.parse(run.coverage.windowEnd)) {
    fail("run.generatedAt must not be before run.coverage.windowEnd.");
  }
  if (!Array.isArray(run.findings)) fail("run.findings must be an array.");
  if (!Array.isArray(run.issues)) fail("run.issues must be an array.");
  if (!["completed", "partial", "failed"].includes(run.status)) fail("run.status is unsupported.");
  if (run.status !== "completed" && run.issues.length === 0) fail(`${run.status} runs require at least one issue.`);
  if (run.status === "failed" && run.findings.length !== 0) fail("failed runs cannot contain findings; use partial instead.");
  run.findings.forEach((finding, index) => validateFinding(finding, `run.findings[${index}]`, run.source));
  run.issues.forEach((issue, index) => validateIssue(issue, `run.issues[${index}]`));
  if (new Set(run.findings.map((finding) => finding.findingId)).size !== run.findings.length) fail("run.findings contains duplicate findingId values.");
  if (new Set(run.issues.map((issue) => issue.issueId)).size !== run.issues.length) fail("run.issues contains duplicate issueId values.");

  object(run.integrity, "run.integrity");
  const integrityKeys = ["algorithm", "canonicalization", "coverage", "digest"];
  exactKeys(run.integrity, integrityKeys, integrityKeys, "run.integrity");
  if (run.integrity.algorithm !== "sha256") fail("run.integrity.algorithm must be sha256.");
  if (run.integrity.canonicalization !== "hfw-discovery-run-v1") fail("run.integrity.canonicalization is unsupported.");
  if (run.integrity.coverage !== "all_top_level_fields_except_integrity") fail("run.integrity.coverage is unsupported.");
  if (!/^[a-f0-9]{64}$/.test(run.integrity.digest)) fail("run.integrity.digest must be a lowercase SHA-256 digest.");
  if (computeDiscoveryRunDigest(run) !== run.integrity.digest) fail("run.integrity.digest does not match the canonical payload.");
  return run;
}

function validateLegacyDataset(dataset) {
  object(dataset, "dataset");
  exactKeys(dataset, ["schemaVersion", "generatedAt", "findings"], ["schemaVersion", "generatedAt", "findings"], "dataset");
  if (dataset.schemaVersion !== 1) fail("dataset.schemaVersion is unsupported.");
  dateTime(dataset.generatedAt, "dataset.generatedAt");
  if (!Array.isArray(dataset.findings)) fail("dataset.findings must be an array.");
  dataset.findings.forEach((finding, index) => {
    const path = `dataset.findings[${index}]`;
    object(finding, path);
    const required = [
      "sourceId", "source", "sourceRoleId", "company", "title", "location",
      "discoveredAt", "applicationUrl", "normalizedKey", "queueGroup",
      "eligibilitySummary", "uncertainty",
    ];
    exactKeys(finding, [...required, "postedAt", "legitimacy"], required, path);
    for (const key of ["sourceId", "source", "sourceRoleId", "company", "title", "location", "normalizedKey", "eligibilitySummary"]) {
      legacyText(finding[key], `${path}.${key}`, key === "normalizedKey" ? 3 : 1);
    }
    dateTime(finding.discoveredAt, `${path}.discoveredAt`);
    if (Object.hasOwn(finding, "postedAt")) postedAt(finding.postedAt, `${path}.postedAt`);
    nullableHttps(finding.applicationUrl, `${path}.applicationUrl`);
    if (!["strong_match", "other_new", "needs_decision"].includes(finding.queueGroup)) {
      fail(`${path}.queueGroup is unsupported.`);
    }
    if (finding.uncertainty !== null) legacyText(finding.uncertainty, `${path}.uncertainty`);
    if (Object.hasOwn(finding, "legitimacy")
        && !["high_confidence", "proceed_with_caution", "suspicious"].includes(finding.legitimacy)) {
      fail(`${path}.legitimacy is unsupported.`);
    }
  });
  return dataset;
}

export function parseDiscoveryArtifact(input) {
  const value = typeof input === "string" ? JSON.parse(input) : input;
  if (value?.contract === "hereforwork.discovery-run") {
    return { kind: "discovery_run", value: validateDiscoveryRun(value) };
  }
  return { kind: "legacy_dataset", value: validateLegacyDataset(value) };
}
