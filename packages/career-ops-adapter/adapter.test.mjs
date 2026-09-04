import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdtemp, mkdir, readFile, stat, symlink, unlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const adapter = fileURLToPath(new URL("./adapter.mjs", import.meta.url));
const { compensationApplicationAnswer, contextHash, fetchJob } = await import("./adapter.mjs");

test("structured compensation preferences reject prose, conversion, and invalid ranges", () => {
  const source = (value) => [{ relativePath: "config/profile.yml", value }];
  const valid = `compensation:\n  application_answer:\n    currency: EUR\n    basis: gross\n    period: annual\n    minimum: 50000\n    maximum: 55000\n    single_value: 52000\n    modalities: [employee, eor, contractor, b2b]\n    allow_currency_conversion: false\n    allow_period_conversion: false\n`;
  assert.deepEqual(compensationApplicationAnswer(source(valid)), {
    currency: "EUR",
    minimum: 50000,
    maximum: 55000,
    single: 52000,
    provenance: ["config/profile.yml:compensation.application_answer"],
  });
  assert.equal(compensationApplicationAnswer(source('compensation:\n  target_range: "EUR 50,000-55,000 gross annual"\n')), null);
  assert.equal(compensationApplicationAnswer(source(valid.replace("allow_currency_conversion: false", "allow_currency_conversion: true"))), null);
  assert.equal(compensationApplicationAnswer(source(valid.replace("single_value: 52000", "single_value: 56000"))), null);
  assert.equal(compensationApplicationAnswer(source(`${valid}    unexpected: 1\n`)), null);
});

test("history snapshots accept the complete bounded maximum-sized output", async () => {
  const fixtureRoot = await mkdtemp(join(tmpdir(), "hfw-history-output-"));
  const records = Array.from({ length: 5000 }, (_, index) => ({
    id: index + 1,
    date: "2026-09-03",
    company: `Company ${index + 1}`,
    role: "Frontend Engineer",
    score: "4.2/5",
    status: "Evaluated",
    pdf: "yes",
    report: `[${String(index + 1).padStart(3, "0")}](/reports/${index + 1}.md)`,
    notes: "bounded history fixture ".repeat(12),
  }));
  await writeFile(join(fixtureRoot, "tracker.mjs"), `process.stdout.write(${JSON.stringify(JSON.stringify(records))});\n`);

  const response = await request(
    { id: "history-large", protocolVersion: 1, operation: "history.snapshot", input: { limit: 5000 } },
    {
      HFW_CAREER_OPS_ROOT: fixtureRoot,
      HFW_CAREER_OPS_INDEX: join(fixtureRoot, "applications.db"),
      HFW_CAREER_OPS_STAGING: join(fixtureRoot, "staging"),
    },
  );

  assert.equal(response.ok, true);
  assert.equal(response.result.records.length, 5000);
  assert.equal(response.result.records.at(-1).id, 5000);
});

test("history snapshots fail closed with an explicit diagnostic when output exceeds the bound", async () => {
  const fixtureRoot = await mkdtemp(join(tmpdir(), "hfw-history-oversize-"));
  await writeFile(join(fixtureRoot, "tracker.mjs"), "process.stdout.write('[' + ' '.repeat(16 * 1024 * 1024 + 1) + ']');\n");

  const response = await request(
    { id: "history-oversize", protocolVersion: 1, operation: "history.snapshot", input: { limit: 5000 } },
    {
      HFW_CAREER_OPS_ROOT: fixtureRoot,
      HFW_CAREER_OPS_INDEX: join(fixtureRoot, "applications.db"),
      HFW_CAREER_OPS_STAGING: join(fixtureRoot, "staging"),
    },
  );

  assert.equal(response.ok, false);
  assert.equal(response.error.code, "CAREER_OPS_OUTPUT_TOO_LARGE");
  assert.equal(response.error.diagnostics, "stdout_oversize: received more than 16777216 bytes");
  assert.match(response.error.message, /stdout exceeded the 16777216-byte output limit/);
});

function request(payload, env = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, [adapter], {
      env: { ...process.env, ...env },
      stdio: ["pipe", "pipe", "pipe"],
    });
    const output = [];
    child.stdout.on("data", (chunk) => output.push(chunk));
    child.on("error", reject);
    child.on("close", () => resolve(JSON.parse(Buffer.concat(output).toString("utf8"))));
    child.stdin.end(`${JSON.stringify(payload)}\n`);
  });
}

function runCommand(command, args, cwd) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, { cwd, stdio: ["ignore", "pipe", "pipe"] });
    const stdout = [];
    const stderr = [];
    child.stdout.on("data", (chunk) => stdout.push(chunk));
    child.stderr.on("data", (chunk) => stderr.push(chunk));
    child.on("error", reject);
    child.on("close", (status) => resolve({
      status,
      stdout: Buffer.concat(stdout).toString("utf8"),
      stderr: Buffer.concat(stderr).toString("utf8"),
    }));
  });
}

const evaluationReport = `# Evaluation: Example Co — Frontend Engineer

## Machine Summary

\`\`\`yaml
company: Example Co
role: Frontend Engineer
score: 4.2
legitimacy_tier: High Confidence
archetype: Frontend Engineer
final_decision: Consider
hard_stops: []
soft_gaps:
  - Confirm the team hiring timeline.
top_strengths:
  - Strong React and TypeScript delivery evidence.
risk_level: Low
confidence: High
next_action: Confirm the employment arrangement before preparing.
authorization_confidence: investigate
authorization_evidence:
  - Authorization details are not stated; verify with the employer at https://jobs.example.test/roles/7.
authorization_scope: job-specific
engagement_mechanism: unknown
authorization_question: Confirm whether the role supports the candidate's authorization path.
work_auth: unstated
discard_reasons: []
via: null
company_confidential: false
advertised_comp: null
reports_to: null
risk_summary:
  legitimacy: high_confidence
  classification: not_evaluated
  culture: not_evaluated
  interview_redflags: not_evaluated
  ai_infra: not_evaluated
  ai_screening_disclosure: not_evaluated
\`\`\`

## A) Role Summary
Fixture.
## B) CV Match
Fixture.
## C) Level and Strategy
Fixture.
## D) Compensation and Demand
Fixture.
## E) Personalization Plan
Fixture.
## F) Interview Plan
Fixture.
## G) Posting Legitimacy
Fixture.
## Risk Summary
Fixture.
`;

const legacyEvaluationReport = `# Legacy evaluation: Example Co — Frontend Engineer

## A) Role Summary
Legacy role summary.
## B) Match With CV
Legacy match.
## C) Level And Strategy
Legacy strategy.
## D) Compensation And Demand
Legacy compensation.
## E) Company And Hiring Evidence
Legacy company evidence.
## F) Risks
Legacy risks.
## G) Posting Legitimacy
Legacy legitimacy.
## Risk Summary
Legacy risk summary.
## Sources
- Job posting: https://jobs.example.test/roles/7

## Machine Summary

\`\`\`yaml
company: Example Co
role: Frontend Engineer
score: 4.2
status: Review
url: https://jobs.example.test/roles/7
legitimacy: High Confidence
location: Remote, Europe
remote: Remote
work_auth: Authorization path is unconfirmed
authorization_confidence: Investigar
authorization_evidence: The role's authorization path is not stated
authorization_scope: Job-specific requirement; company-wide context is unconfirmed
engagement_mechanism: Unknown
authorization_question: Confirm the legal employment path before applying.
advertised_comp: null
primary_fit:
  - React
  - TypeScript
risks:
  - Authorization path unconfirmed
  - Level requires confirmation
recommendation: Review and verify the authorization path before preparing.
risk_summary:
  authorization: high
  technical_gap: medium
\`\`\`
`;

async function evaluationFixture({ report = evaluationReport, tracker = {}, initializeGit = true, canonicalTracker = "root" } = {}) {
  const fixtureRoot = await mkdtemp(join(tmpdir(), "hfw-evaluation-result-"));
  await mkdir(join(fixtureRoot, "batch"), { recursive: true });
  await mkdir(join(fixtureRoot, "modes"), { recursive: true });
  await mkdir(join(fixtureRoot, "templates"), { recursive: true });
  if (canonicalTracker === "data") {
    await mkdir(join(fixtureRoot, "data"), { recursive: true });
    await writeFile(join(fixtureRoot, "data/applications.md"), "# canonical tracker fixture\n");
  } else {
    await writeFile(join(fixtureRoot, "applications.md"), "# canonical tracker fixture\n");
  }
  await writeFile(join(fixtureRoot, "batch/batch-prompt.md"), "#### Machine Summary\nauthorization_confidence:\nauthorization_evidence:\nauthorization_scope:\nengagement_mechanism:\nauthorization_question:\n");
  await writeFile(join(fixtureRoot, "tracker-parse.mjs"), "// documented tracker projection\n");
  await writeFile(join(fixtureRoot, "modes/oferta.md"), "## Machine Summary\n");
  await writeFile(join(fixtureRoot, "templates/states.yml"), "states: []\n");
  await writeFile(join(fixtureRoot, "application-artifacts.mjs"), "// applicationArtifactPaths writeReuseDecision schema_version: 1\n");
  await writeFile(join(fixtureRoot, "build-cv-html.mjs"), "// <input.json> <output.html> page_format\n");
  await writeFile(join(fixtureRoot, "verify-cv-facts.mjs"), "// --json verdict\n");
  await writeFile(join(fixtureRoot, "generate-pdf.mjs"), "// resolvePdfIndexPath # report\\tpdf\\thtml\\tformat\\tdate\n");
  await writeFile(join(fixtureRoot, "tracker-utils.mjs"), "// CAREER_OPS_PDF_INDEX resolvePdfIndexPath\n");
  const trackerRecord = {
    id: 7, date: "2026-09-02", company: tracker.company ?? "Example Co", role: tracker.role ?? "Frontend Engineer",
    score: tracker.score ?? "4.2/5", status: tracker.status ?? "Evaluated", pdf: "❌",
    report: tracker.report ?? (canonicalTracker === "data" ? "[007](../reports/007-example.md)" : "[007](reports/007-example.md)"), notes: tracker.notes ?? "synthetic fixture",
  };
  await writeFile(join(fixtureRoot, "tracker.mjs"), `const row = ${JSON.stringify(trackerRecord)};
if (process.argv.includes("--json")) process.stdout.write(JSON.stringify([row]));
// SELECT id, date, company, role, score, status, pdf, report, notes FROM applications
`);
  await writeFile(join(fixtureRoot, "package.json"), '{"version":"1.31.0"}\n');
  await writeFile(join(fixtureRoot, "VERSION"), "1.31.0\n");
  const reportPath = join(fixtureRoot, "reports/007-example.md");
  await mkdir(join(fixtureRoot, "reports"), { recursive: true });
  await writeFile(reportPath, report);
  if (initializeGit) {
    for (const [command, args] of [
      ["git", ["init", "--quiet"]],
      ["git", ["add", "."]],
      ["git", ["-c", "user.name=HereForWork Test", "-c", "user.email=test@hereforwork.local", "commit", "--quiet", "-m", "fixture"]],
    ]) {
      const result = await runCommand(command, args, fixtureRoot);
      assert.equal(result.status, 0, result.stderr);
    }
  }
  const capabilities = await request({ id: "evaluation-capabilities", protocolVersion: 1, operation: "capabilities.get", input: {} }, { HFW_CAREER_OPS_ROOT: fixtureRoot });
  const compatibilityFingerprint = capabilities.result.capabilities.find(({ id }) => id === "evaluation.result.read.v1").compatibilityFingerprint;
  if (initializeGit) assert.match(compatibilityFingerprint, /^[a-f0-9]{64}$/);
  const reportBytes = Buffer.from(report);
  return {
    root: fixtureRoot,
    env: { HFW_CAREER_OPS_ROOT: fixtureRoot, HFW_CAREER_OPS_INDEX: join(fixtureRoot, "applications.db"), HFW_CAREER_OPS_STAGING: join(fixtureRoot, "staging") },
    input: { reportPath: "reports/007-example.md", reportSha256: createHash("sha256").update(reportBytes).digest("hex"), trackerId: 7, compatibilityFingerprint },
  };
}

async function readEvaluation(fixture, input = fixture.input) {
  return request({ id: "evaluation-read", protocolVersion: 1, operation: "evaluation.result.read.v1", input }, fixture.env);
}

async function inspectArtifacts(fixture, overrides = {}) {
  const capabilities = await request({ id: "artifact-capabilities", protocolVersion: 1, operation: "capabilities.get", input: {} }, fixture.env);
  const artifactCompatibilityFingerprint = capabilities.result.capabilities
    .find(({ id }) => id === "artifacts.inspect.v1").compatibilityFingerprint;
  return request({
    id: "artifact-inspection",
    protocolVersion: 1,
    operation: "artifacts.inspect.v1",
    input: {
      preparationId: "11111111-1111-4111-8111-111111111111",
      contextHash: "c".repeat(64),
      company: "Example Co",
      title: "Frontend Engineer",
      reportPath: fixture.input.reportPath,
      reportSha256: fixture.input.reportSha256,
      trackerId: fixture.input.trackerId,
      evaluationCompatibilityFingerprint: fixture.input.compatibilityFingerprint,
      artifactCompatibilityFingerprint,
      ...overrides,
    },
  }, fixture.env);
}

test("capabilities expose the fixed safety boundary", async () => {
  const expectedOperations = [
    "capabilities.get",
    "health.check",
    "history.snapshot",
    "evaluation.full_ag.run.v1",
    "evaluation.result.read.v1",
    "artifacts.inspect.v1",
    "profile.queue_filters.get",
    "preparation.context.get",
    "preparation.result.recover",
    "preparation.result.commit",
    "preparation.artifacts.delete",
    "answers.context.get",
    "answers.result.validate",
    "answers.result.commit",
    "role.discard",
    "role.discard.undo",
    "application.applied.confirm",
  ];
  const response = await request({
    id: "capabilities",
    protocolVersion: 1,
    operation: "capabilities.get",
    input: {},
  });

  assert.equal(response.ok, true);
  assert.deepEqual(response.result.forbiddenOperations, [
    "application.submit",
    "application.finalize",
    "message.send",
    "shell.run",
    "browser.command",
  ]);
  assert.equal(response.result.sourceOfTruth.applicationHistory, "career-ops");
  assert.deepEqual(response.result.operations, expectedOperations);
  assert.equal(new Set(response.result.operations).size, expectedOperations.length);
  assert.equal(response.result.contract, "hereforwork.career-ops-capabilities");
  assert.equal(response.result.schemaVersion, 1);
  assert.equal(response.result.upstreamRevision, null);
  assert.equal(response.result.upstreamDeclaredVersion, null);
  assert.deepEqual(response.result.autoPdfScoreThreshold, { value: null, source: "unavailable" });
  assert.equal(response.result.capabilities.length, 8);
  assert.ok(response.result.capabilities.every(({ status }) => status === "unavailable"));
  assert.ok(response.result.diagnostics.some(({ code }) => code === "upstream_revision_unavailable"));

  const schema = JSON.parse(await readFile(fileURLToPath(
    new URL("../../contracts/career-ops-capabilities.schema.json", import.meta.url),
  ), "utf8"));
  assert.deepEqual(schema.$defs.operation.enum, expectedOperations);
  assert.equal(schema.properties.operations.items.$ref, "#/$defs/operation");
  assert.equal(schema.properties.operations.uniqueItems, true);
  assert.equal(schema.properties.operations.minItems, expectedOperations.length);
  assert.equal(schema.properties.operations.maxItems, expectedOperations.length);
});

test("capability probes pin exact revision, declared version, threshold, and fail-closed statuses", async () => {
  const root = await mkdtemp(join(tmpdir(), "hfw-capabilities-"));
  await mkdir(join(root, "config"), { recursive: true });
  await Promise.all([
    writeFile(join(root, "package.json"), '{"version":"1.31.0"}\n'),
    writeFile(join(root, "VERSION"), "1.31.0\n"),
    writeFile(join(root, "config/profile.yml"), "auto_pdf_score_threshold: 3.5\n"),
    ...["scan-ats-full.mjs", "discover-ats.mjs", "tracker.mjs", "merge-tracker.mjs", "set-status.mjs"]
      .map((name) => writeFile(join(root, name), "// capability fixture\n")),
  ]);
  for (const [command, args] of [
    ["git", ["init", "--quiet"]],
    ["git", ["add", "."]],
    ["git", ["-c", "user.name=HereForWork Test", "-c", "user.email=test@hereforwork.local", "commit", "--quiet", "-m", "fixture"]],
  ]) {
    const result = await runCommand(command, args, root);
    assert.equal(result.status, 0, result.stderr);
  }

  const response = await request({
    id: "capability-probes",
    protocolVersion: 1,
    operation: "capabilities.get",
    input: {},
  }, { HFW_CAREER_OPS_ROOT: root });

  assert.equal(response.ok, true, JSON.stringify(response));
  assert.match(response.result.upstreamRevision, /^[0-9a-f]{40}$/);
  assert.equal(response.result.upstreamDeclaredVersion, "1.31.0");
  assert.deepEqual(response.result.autoPdfScoreThreshold, { value: 3.5, source: "configured" });
  assert.deepEqual(response.result.capabilities.map(({ id }) => id), [
    "discovery.reverse_ats.run.v1",
    "discovery.company_ats.preview.v1",
    "liveness.role.read.v1",
    "evaluation.full_ag.run.v1",
    "evaluation.result.read.v1",
    "artifacts.inspect.v1",
    "browser.review_fallback.v1",
    "canonical.applied.write.v1",
  ]);
  assert.equal(response.result.capabilities.find(({ id }) => id === "canonical.applied.write.v1").status, "degraded");
  assert.equal(response.result.capabilities.find(({ id }) => id === "discovery.reverse_ats.run.v1").status, "degraded");
  assert.equal(response.result.capabilities.find(({ id }) => id === "discovery.company_ats.preview.v1").status, "degraded");
  assert.equal(response.result.capabilities.find(({ id }) => id === "liveness.role.read.v1").status, "unavailable");
  assert.equal(response.result.capabilities.find(({ id }) => id === "browser.review_fallback.v1").status, "unavailable");
  assert.ok(response.result.capabilities.every(({ sourceRevision }) => sourceRevision === response.result.upstreamRevision));
  assert.ok(response.result.diagnostics.some(({ code }) => code === "isolated_execution_required"));
  assert.ok(response.result.diagnostics.some(({ code }) => code === "public_browser_fallback_unavailable"));
  assert.ok(response.result.diagnostics.some(({ code }) => code === "canonical_writer_compatibility_unverified"));
  assert.ok(!response.result.diagnostics.some(({ code }) => code === "auto_pdf_threshold_product_mismatch"));

  await unlink(join(root, "config/profile.yml"));
  const defaulted = await request({
    id: "capability-default-threshold",
    protocolVersion: 1,
    operation: "capabilities.get",
    input: {},
  }, { HFW_CAREER_OPS_ROOT: root });
  assert.deepEqual(defaulted.result.autoPdfScoreThreshold, { value: 3, source: "upstream_default" });
  assert.ok(defaulted.result.diagnostics.some(({ code }) => code === "auto_pdf_threshold_product_mismatch"));
});

test("artifacts.inspect.v1 probe is degraded with a fingerprint when the contract markers are present", async () => {
  const root = await mkdtemp(join(tmpdir(), "hfw-artifact-probe-ok-"));
  await Promise.all([
    writeFile(join(root, "package.json"), '{"version":"1.31.0"}\n'),
    writeFile(join(root, "application-artifacts.mjs"), "// applicationArtifactPaths writeReuseDecision schema_version: 1\n"),
    writeFile(join(root, "build-cv-html.mjs"), "// <input.json> <output.html> page_format\n"),
    writeFile(join(root, "verify-cv-facts.mjs"), "// --json verdict\n"),
    writeFile(join(root, "generate-pdf.mjs"), "// resolvePdfIndexPath # report\\tpdf\\thtml\\tformat\\tdate\n"),
    writeFile(join(root, "tracker-utils.mjs"), "// CAREER_OPS_PDF_INDEX resolvePdfIndexPath\n"),
  ]);
  for (const [command, args] of [
    ["git", ["init", "--quiet"]],
    ["git", ["add", "."]],
    ["git", ["-c", "user.name=HereForWork Test", "-c", "user.email=test@hereforwork.local", "commit", "--quiet", "-m", "fixture"]],
  ]) {
    const result = await runCommand(command, args, root);
    assert.equal(result.status, 0, result.stderr);
  }

  const response = await request({
    id: "artifact-probe-ok",
    protocolVersion: 1,
    operation: "capabilities.get",
    input: {},
  }, { HFW_CAREER_OPS_ROOT: root });
  assert.equal(response.ok, true, JSON.stringify(response));
  const artifact = response.result.capabilities.find(({ id }) => id === "artifacts.inspect.v1");
  assert.equal(artifact.status, "degraded");
  assert.match(artifact.compatibilityFingerprint, /^[a-f0-9]{64}$/);
  assert.equal(artifact.sourceRevision, response.result.upstreamRevision);
  assert.ok(response.result.diagnostics.some(({ code, capabilityId }) => (
    code === "safe_shape_probe_required" && capabilityId === "artifacts.inspect.v1"
  )));
});

test("artifacts.inspect.v1 probe fails closed when contract markers drift", async () => {
  const root = await mkdtemp(join(tmpdir(), "hfw-artifact-probe-miss-"));
  await Promise.all([
    writeFile(join(root, "package.json"), '{"version":"1.31.0"}\n'),
    writeFile(join(root, "application-artifacts.mjs"), "// applicationArtifactPaths writeReuseDecision schema_version: 1\n"),
    // Missing <input.json> <output.html> CLI contract marker.
    writeFile(join(root, "build-cv-html.mjs"), "// page_format only\n"),
    writeFile(join(root, "verify-cv-facts.mjs"), "// --json verdict\n"),
    writeFile(join(root, "generate-pdf.mjs"), "// resolvePdfIndexPath # report\\tpdf\\thtml\\tformat\\tdate\n"),
    writeFile(join(root, "tracker-utils.mjs"), "// CAREER_OPS_PDF_INDEX resolvePdfIndexPath\n"),
  ]);
  for (const [command, args] of [
    ["git", ["init", "--quiet"]],
    ["git", ["add", "."]],
    ["git", ["-c", "user.name=HereForWork Test", "-c", "user.email=test@hereforwork.local", "commit", "--quiet", "-m", "fixture"]],
  ]) {
    const result = await runCommand(command, args, root);
    assert.equal(result.status, 0, result.stderr);
  }

  const drifted = await request({
    id: "artifact-probe-drift",
    protocolVersion: 1,
    operation: "capabilities.get",
    input: {},
  }, { HFW_CAREER_OPS_ROOT: root });
  assert.equal(drifted.ok, true, JSON.stringify(drifted));
  const driftedCapability = drifted.result.capabilities.find(({ id }) => id === "artifacts.inspect.v1");
  assert.equal(driftedCapability.status, "unavailable");
  assert.equal(driftedCapability.compatibilityFingerprint, null);
  assert.ok(drifted.result.diagnostics.some(({ code, capabilityId }) => (
    code === "structured_provenance_unavailable" && capabilityId === "artifacts.inspect.v1"
  )));

  await unlink(join(root, "tracker-utils.mjs"));
  await writeFile(join(root, "build-cv-html.mjs"), "// <input.json> <output.html> page_format\n");
  const missing = await request({
    id: "artifact-probe-missing",
    protocolVersion: 1,
    operation: "capabilities.get",
    input: {},
  }, { HFW_CAREER_OPS_ROOT: root });
  const missingCapability = missing.result.capabilities.find(({ id }) => id === "artifacts.inspect.v1");
  assert.equal(missingCapability.status, "unavailable");
  assert.equal(missingCapability.compatibilityFingerprint, null);
});

test("capabilities reject unknown inputs and report malformed threshold without exposing a path", async () => {
  const root = await mkdtemp(join(tmpdir(), "hfw-capabilities-invalid-"));
  await mkdir(join(root, "config"), { recursive: true });
  await writeFile(join(root, "config/profile.yml"), "auto_pdf_score_threshold: many\n");
  const malformed = await request({
    id: "capability-invalid-threshold",
    protocolVersion: 1,
    operation: "capabilities.get",
    input: {},
  }, { HFW_CAREER_OPS_ROOT: root });
  assert.deepEqual(malformed.result.autoPdfScoreThreshold, { value: null, source: "unavailable" });
  const diagnostic = malformed.result.diagnostics.find(({ code }) => code === "auto_pdf_threshold_invalid");
  assert.ok(diagnostic);
  assert.doesNotMatch(JSON.stringify(malformed.result), new RegExp(root.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")));

  const unknown = await request({
    id: "capability-unknown-input",
    protocolVersion: 1,
    operation: "capabilities.get",
    input: { path: root },
  }, { HFW_CAREER_OPS_ROOT: root });
  assert.equal(unknown.ok, false);
  assert.match(unknown.error.message, /unknown field/);
});

test("evaluation result read returns a typed, native-score projection", async () => {
  const fixture = await evaluationFixture();
  const response = await readEvaluation(fixture);
  assert.equal(response.ok, true, JSON.stringify(response));
  assert.equal(response.result.contract, "hereforwork.career-ops-evaluation-result");
  assert.equal(response.result.canonical.score, 4.2);
  assert.equal(response.result.evaluation.score, 4.2);
  assert.equal(response.result.evaluation.authorization.confidence, "investigate");
  assert.deepEqual(response.result.evaluation.materialUncertainty.notEvaluatedRiskSignals, [
    "classification", "culture", "interviewRedflags", "aiInfra", "aiScreeningDisclosure",
  ]);
  assert.equal(response.result.evaluation.authorization.evidence.length, 1);
});

test("evaluation result read accepts the explicitly recognized legacy Machine Summary shape", async () => {
  const fixture = await evaluationFixture({ report: legacyEvaluationReport });
  const response = await readEvaluation(fixture);
  assert.equal(response.ok, true, JSON.stringify(response));
  assert.equal(response.result.canonical.score, 4.2);
  assert.equal(response.result.evaluation.finalDecision, "Consider");
  assert.equal(response.result.evaluation.riskLevel, "High");
  assert.equal(response.result.evaluation.confidence, "Low");
  assert.deepEqual(response.result.evaluation.strengths, ["React", "TypeScript"]);
  assert.equal(Object.hasOwn(response.result.evaluation, "company"), false);
  assert.equal(Object.hasOwn(response.result.evaluation, "role"), false);
  assert.equal(response.result.evaluation.authorization.evidence[0], "The role's authorization path is not stated");
  assert.ok(response.result.evaluation.authorization.evidence.some((item) => /https:\/\/jobs\.example\.test\/roles\/7/.test(item)));
  assert.deepEqual(response.result.evaluation.materialUncertainty.notEvaluatedRiskSignals, [
    "classification", "culture", "interviewRedflags", "aiInfra", "aiScreeningDisclosure",
  ]);
});

test("evaluation result read rejects an ambiguous legacy Machine Summary shape", async () => {
  const fixture = await evaluationFixture({
    report: legacyEvaluationReport.replace("technical_gap: medium", "unknown_risk: medium"),
  });
  const response = await readEvaluation(fixture);
  assert.equal(response.ok, false);
  assert.match(response.error.message, /unknown or missing field/);
});

test("legacy Machine Summary cannot replace the canonical native tracker score", async () => {
  const fixture = await evaluationFixture({ report: legacyEvaluationReport, tracker: { score: "4.1/5" } });
  const response = await readEvaluation(fixture);
  assert.equal(response.ok, false);
  assert.match(response.error.message, /does not match the canonical tracker score/);
});

test("artifact inspection reuses the canonical report but refreshes an unproven CV without writing", async () => {
  const fixture = await evaluationFixture();
  const before = await stat(join(fixture.root, "reports/007-example.md"));
  const response = await inspectArtifacts(fixture);
  const after = await stat(join(fixture.root, "reports/007-example.md"));
  assert.equal(response.ok, true, JSON.stringify(response));
  assert.equal(response.result.report.action, "reuse");
  assert.equal(response.result.cv.action, "refresh");
  assert.equal(response.result.cv.reason, "no_hfw_bundle");
  assert.equal(response.result.trackerId, 7);
  assert.equal(after.mtimeMs, before.mtimeMs);
});

test("artifact inspection reuses only an exact HFW manifest and detects PDF drift", async () => {
  const fixture = await evaluationFixture();
  const preparationId = "11111111-1111-4111-8111-111111111111";
  const root = join(fixture.root, "output/007-example-co-frontend-engineer/cv/tailored/v001");
  await mkdir(root, { recursive: true });
  const html = Buffer.from("<html><body>fixture</body></html>");
  const pdf = Buffer.from("%PDF-1.4 fixture %%EOF");
  const changes = Buffer.from("verified changes\n");
  await Promise.all([
    writeFile(join(root, "cv.html"), html),
    writeFile(join(root, "cv.pdf"), pdf),
    writeFile(join(root, "changes.md"), changes),
  ]);
  const capabilities = await request({ id: "artifact-capabilities", protocolVersion: 1, operation: "capabilities.get", input: {} }, fixture.env);
  const artifactFingerprint = capabilities.result.capabilities.find(({ id }) => id === "artifacts.inspect.v1").compatibilityFingerprint;
  const revision = capabilities.result.upstreamRevision;
  await mkdir(join(fixture.env.HFW_CAREER_OPS_STAGING, preparationId), { recursive: true });
  await writeFile(join(fixture.env.HFW_CAREER_OPS_STAGING, preparationId, "commit-state.json"), JSON.stringify({
    schemaVersion: 3,
    preparationId,
    contextHash: "c".repeat(64),
    status: "committed",
    artifacts: {
      report: { path: fixture.input.reportPath, sha256: fixture.input.reportSha256 },
      cvHtml: { path: "output/007-example-co-frontend-engineer/cv/tailored/v001/cv.html", sha256: createHash("sha256").update(html).digest("hex") },
      cvPdf: { path: "output/007-example-co-frontend-engineer/cv/tailored/v001/cv.pdf", sha256: createHash("sha256").update(pdf).digest("hex") },
      cvChanges: { path: "output/007-example-co-frontend-engineer/cv/tailored/v001/changes.md", sha256: createHash("sha256").update(changes).digest("hex") },
    },
    cvProvenance: { source: "tailored_generated", tailored: true, sourceSha256: null, renderRecovery: null },
    cvFormat: "a4",
    canonicalEvaluation: {
      trackerId: 7,
      reportPath: fixture.input.reportPath,
      reportSha256: fixture.input.reportSha256,
      upstreamRevision: revision,
      evaluationCompatibilityFingerprint: fixture.input.compatibilityFingerprint,
      artifactCompatibilityFingerprint: artifactFingerprint,
    },
  }));
  const reusable = await inspectArtifacts(fixture);
  assert.equal(reusable.ok, true, JSON.stringify(reusable));
  assert.equal(reusable.result.cv.action, "reuse");

  await writeFile(join(root, "cv.pdf"), "changed");
  const stale = await inspectArtifacts(fixture);
  assert.equal(stale.ok, true, JSON.stringify(stale));
  assert.equal(stale.result.cv.action, "refresh");
  assert.equal(stale.result.cv.scope, "pdf_only");
});

test("artifact inspection fails closed on fingerprint, report identity, and symlink drift", async () => {
  const fixture = await evaluationFixture();
  const badFingerprint = await inspectArtifacts(fixture, { artifactCompatibilityFingerprint: "a".repeat(64) });
  assert.equal(badFingerprint.ok, false);
  assert.match(badFingerprint.error.message, /unavailable or has drifted/);

  const wrongTracker = await inspectArtifacts(fixture, { trackerId: 8 });
  assert.equal(wrongTracker.ok, false);

  const outside = join(await mkdtemp(join(tmpdir(), "hfw-artifact-outside-")), "outside.md");
  await writeFile(outside, evaluationReport);
  await unlink(join(fixture.root, "reports/007-example.md"));
  await symlink(outside, join(fixture.root, "reports/007-example.md"));
  const escaped = await inspectArtifacts(fixture);
  assert.equal(escaped.ok, false);
});

test("evaluation result read fails closed without an exact upstream revision", async () => {
  const fixture = await evaluationFixture({ initializeGit: false });
  const capabilities = await request({ id: "evaluation-capabilities", protocolVersion: 1, operation: "capabilities.get", input: {} }, fixture.env);
  const readCapability = capabilities.result.capabilities.find(({ id }) => id === "evaluation.result.read.v1");
  assert.equal(capabilities.result.upstreamRevision, null);
  assert.equal(readCapability.status, "unavailable");
  assert.equal(readCapability.compatibilityFingerprint, null);
  assert.equal((await readEvaluation(fixture, {
    ...fixture.input,
    compatibilityFingerprint: "a".repeat(64),
  })).ok, false);
});

test("evaluation result read resolves canonical tracker links relative to data applications.md", async () => {
  const fixture = await evaluationFixture({ canonicalTracker: "data" });
  const response = await readEvaluation(fixture);
  assert.equal(response.ok, true, JSON.stringify(response));
});

test("evaluation result read rejects canonical tracker report escapes and symlink targets", async () => {
  const escaped = await evaluationFixture({
    canonicalTracker: "data",
    tracker: { report: "[007](../../../etc/passwd)" },
  });
  assert.equal((await readEvaluation(escaped)).ok, false, "canonical tracker escape");

  const symlinked = await evaluationFixture({
    canonicalTracker: "data",
    tracker: { report: "[007](../reports/linked-report.md)" },
  });
  const outside = await mkdtemp(join(tmpdir(), "hfw-evaluation-report-outside-"));
  await writeFile(join(outside, "linked-report.md"), evaluationReport);
  await symlink(join(outside, "linked-report.md"), join(symlinked.root, "reports/linked-report.md"));
  assert.equal((await readEvaluation(symlinked)).ok, false, "canonical tracker symlink");
});

test("evaluation result read rejects the fail-closed fixture matrix", async () => {
  const cases = [
    ["missing A-G section", evaluationReport.replace("## D) Compensation and Demand", "## D-omitted Compensation and Demand")],
    ["unknown Machine Summary field", evaluationReport.replace("company: Example Co", "unknown_field: nope\ncompany: Example Co")],
    ["out-of-range score", evaluationReport.replace("score: 4.2", "score: 6")],
    ["advisory rank cannot become canonical score", evaluationReport.replace("score: 4.2", "rank: 0.99\nscore: 4.2")],
    ["malformed YAML", evaluationReport.replace("authorization_confidence: investigate", "authorization_confidence: [investigate")],
    ["empty authorization evidence", evaluationReport.replace("authorization_evidence:\n  - Authorization details are not stated; verify with the employer at https://jobs.example.test/roles/7.", "authorization_evidence: []")],
  ];
  for (const [label, report] of cases) {
    const fixture = await evaluationFixture({ report });
    const response = await readEvaluation(fixture);
    assert.equal(response.ok, false, label);
  }
  const mismatch = await evaluationFixture({ tracker: { company: "Other Co" } });
  assert.equal((await readEvaluation(mismatch)).ok, false, "report/tracker mismatch");
  const stale = await evaluationFixture();
  await writeFile(join(stale.root, "reports/007-example.md"), `${evaluationReport}\nchanged\n`);
  assert.equal((await readEvaluation(stale)).ok, false, "stale report hash");
  const escaped = await evaluationFixture();
  const escapedResponse = await readEvaluation(escaped, { ...escaped.input, reportPath: "../secret.md" });
  assert.equal(escapedResponse.ok, false, "path escape");
  const drift = await evaluationFixture();
  await writeFile(join(drift.root, "tracker.mjs"), "// format drift\n");
  assert.equal((await readEvaluation(drift)).ok, false, "upstream format drift");
});

test("evaluation result operation preserves the no-submit boundary", async () => {
  const fixture = await evaluationFixture();
  const response = await request({ id: "submit", protocolVersion: 1, operation: "application.submit", input: {} }, fixture.env);
  assert.equal(response.ok, false);
  assert.match(response.error.message, /Unsupported operation/);
});

test("generic discovery resolves a source listing to its application form", async (context) => {
  const requested = [];
  context.mock.method(globalThis, "fetch", async (input) => {
    const url = String(input);
    requested.push(url);
    if (url === "https://listing.example.test/jobs/42") {
      return new Response(`<!doctype html><html><body>
        <h1>Frontend Engineer</h1>
        <a href="https://apply.example.test/forms/42?source=listing">Apply now</a>
      </body></html>`, { status: 200, headers: { "content-type": "text/html" } });
    }
    return new Response(`<!doctype html><html><head>
      <script type="application/ld+json">${JSON.stringify({
        "@type": "JobPosting",
        title: "Senior Frontend Engineer",
        description: "Build accessible React and TypeScript product interfaces, collaborate with design and backend partners, improve automated testing, and own reliable delivery for a distributed product team across Europe.",
        hiringOrganization: { name: "Example Co" },
        jobLocation: { address: { addressLocality: "Madrid", addressCountry: "Spain" } },
        datePosted: "2026-08-30",
      })}</script>
      </head><body><form><input name="name"><input name="email"><button type="submit">Submit</button></form></body></html>`, {
      status: 200,
      headers: { "content-type": "text/html" },
    });
  });

  const job = await fetchJob({
    company: "Example Co",
    title: "Frontend Engineer",
    location: "Remote",
    url: "https://listing.example.test/jobs/42",
  });

  assert.deepEqual(requested, [
    "https://listing.example.test/jobs/42",
    "https://apply.example.test/forms/42?source=listing",
  ]);
  assert.equal(job.provider, "generic");
  assert.equal(job.url, "https://apply.example.test/forms/42?source=listing");
  assert.equal(job.sourceUrl, "https://listing.example.test/jobs/42");
  assert.equal(job.title, "Senior Frontend Engineer");
  assert.equal(job.descriptionAvailable, true);
});

test("generic discovery preserves the requested application URL when a page cannot be fetched", async (context) => {
  context.mock.method(globalThis, "fetch", async () => {
    throw new Error("login required");
  });

  const job = await fetchJob({
    company: "Private Co",
    title: "Product Engineer",
    location: "Remote",
    url: "https://careers.example.test/private-role",
  });

  assert.equal(job.provider, "generic");
  assert.equal(job.url, "https://careers.example.test/private-role");
  assert.equal(job.descriptionAvailable, false);
  assert.match(job.description, /could not retrieve a complete job description/);
});

test("generic discovery does not follow redirects to a local target", async (context) => {
  let calls = 0;
  context.mock.method(globalThis, "fetch", async () => {
    calls += 1;
    return new Response(null, {
      status: 302,
      headers: { location: "https://localhost/private" },
    });
  });

  const job = await fetchJob({
    company: "Example Co",
    title: "Engineer",
    location: "Remote",
    url: "https://careers.example.test/role",
  });

  assert.equal(calls, 1);
  assert.equal(job.url, "https://careers.example.test/role");
  assert.equal(job.descriptionAvailable, false);
});

test("preparation context hash is stable across typed JSON key reordering", () => {
  const role = {
    company: "Example Co",
    title: "Frontend Engineer",
    location: "Remote Europe",
    url: "https://example.test/jobs/1",
  };
  const browserJob = {
    title: "Frontend Engineer",
    company: "Example Co",
    location: "Remote Europe",
    url: "https://example.test/apply/1",
    sourceUrl: "https://example.test/jobs/1",
    description: "A sufficiently descriptive live job fixture.",
    descriptionAvailable: true,
    postedAt: null,
    provider: "generic",
  };
  const rustRoundTripJob = {
    company: browserJob.company,
    description: browserJob.description,
    descriptionAvailable: browserJob.descriptionAvailable,
    location: browserJob.location,
    postedAt: browserJob.postedAt,
    provider: browserJob.provider,
    sourceUrl: browserJob.sourceUrl,
    title: browserJob.title,
    url: browserJob.url,
  };
  const sources = [{ relativePath: "cv.md", sha256: "a".repeat(64) }];

  assert.equal(
    contextHash(role, browserJob, sources),
    contextHash(role, rustRoundTripJob, sources),
  );
});

async function fakeCareerOps() {
  const root = await mkdtemp(join(tmpdir(), "hfw-career-ops-"));
  const staging = join(root, "staging");
  await mkdir(join(root, "data"), { recursive: true });
  await mkdir(join(root, "batch"), { recursive: true });
  await mkdir(join(root, "templates"), { recursive: true });
  await mkdir(join(root, "reports"), { recursive: true });
  await mkdir(join(root, "modes/heuristics"), { recursive: true });
  await mkdir(join(root, "config"), { recursive: true });
  await mkdir(join(root, "providers"), { recursive: true });
  await mkdir(join(root, "node_modules/playwright"), { recursive: true });
  await mkdir(staging, { recursive: true });
  await writeFile(join(root, "data/applications.md"), "# fixture\n");
  await writeFile(join(root, "state.json"), "[]\n");
  for (const path of ["modes/_shared.md", "modes/oferta.md", "modes/pdf.md", "modes/apply.md", "modes/_profile.md", "modes/heuristics/recruiter-side.md", "cv.md"]) {
    await writeFile(join(root, path), `# ${path}\nVerified fixture facts for Test Candidate at Example Co.\n`);
  }
  await writeFile(join(root, "config/profile.yml"), `candidate:
  full_name: "Test Candidate"
  email: "test@example.test"
  phone: "+34 600 000 000"
  location: "Madrid, Spain"
  linkedin: "https://linkedin.example/test"
  portfolio_url: "https://portfolio.example/test"
  github: "https://github.example/test"
target_roles:
  primary:
    - "Frontend Engineer"
  archetypes:
    - name: "Frontend Engineer"
      level: "Experienced"
      fit: "primary"
compensation:
  target_range: "50,000–55,000 gross annual"
  currency: "EUR"
  application_answer:
    currency: "EUR"
    basis: "gross"
    period: "annual"
    minimum: 50000
    maximum: 55000
    single_value: 52000
    modalities: ["employee", "eor", "contractor", "b2b"]
    allow_currency_conversion: false
    allow_period_conversion: false
  location_flexibility: "Target geography: Spain first, followed by Lisbon. Remote roles are an active search lane when they provide sponsorship or another authorization path."
location:
  country: "Spain"
  city: "Madrid"
`);
  await writeFile(join(root, "modes/_profile.md"), `# Verified profile policy
## Your Comp Targets
Approved compensation preference:
- Currency: EUR. Basis: gross annual.
- Target range: EUR 50,000–55,000.
- When an EUR annual form requires one number, use EUR 52,000.
- Apply this preference to any engagement modality.
- Do not invent currency conversions or period conversions.
`);
  await writeFile(join(root, "config/cv-facts.json"), "{\"allow_metrics\":[],\"allow_facts\":[],\"forbidden_phrases\":[],\"warn_phrases\":[]}\n");
  await writeFile(join(root, "providers/_http.mjs"), "export async function fetchJson() { return {}; }\n");
  const chromiumFixture = join(root, "fixture-chromium");
  await writeFile(chromiumFixture, "#!/bin/sh\nexit 0\n", { mode: 0o755 });
  await writeFile(join(root, "node_modules/playwright/index.mjs"), `
    export const chromium = { executablePath: () => ${JSON.stringify(chromiumFixture)} };
  `);
  await writeFile(join(root, "providers/ashby.mjs"), `
    export default {
      async fetch(entry) {
        return [{
          title: "Frontend Engineer",
          company: entry.name,
          location: "Remote Europe",
          url: "https://jobs.ashbyhq.com/example/role-1",
          description: "A live frontend engineering role requiring React, TypeScript, accessible interfaces, product judgment, testing, and collaboration across a distributed European team. This fixture is deliberately long enough for the bounded preparation contract.",
          postedAt: Date.parse("2026-08-30T08:00:00Z")
        }];
      }
    };
  `);
  await writeFile(join(root, "tracker.mjs"), `
    import { readFile } from "node:fs/promises";
    const rows = JSON.parse(await readFile(new URL("./state.json", import.meta.url), "utf8"));
    process.stdout.write(JSON.stringify(rows));
    // SELECT id, date, company, role, score, status, pdf, report, notes FROM applications
  `);
  await writeFile(join(root, "tracker-parse.mjs"), "export const parseTracker = (value) => value;\n");
  await writeFile(join(root, "merge-tracker.mjs"), `
    import { readFile, readdir, writeFile } from "node:fs/promises";
    const stateUrl = new URL("./state.json", import.meta.url);
    const rows = JSON.parse(await readFile(stateUrl, "utf8"));
    const files = (await readdir(process.env.CAREER_OPS_ADDITIONS)).filter((name) => name.endsWith(".tsv"));
    for (const name of files) {
      const parts = (await readFile(process.env.CAREER_OPS_ADDITIONS + "/" + name, "utf8")).trim().split("\\t");
      const statusFirst = ["Evaluated", "Discarded", "Applied"].includes(parts[4]);
      if (!rows.some((row) => row.notes.includes(parts[8]))) rows.push({
        id: Number(parts[0]), date: parts[1], company: parts[2], role: parts[3],
        status: statusFirst ? parts[4] : parts[5], score: statusFirst ? parts[5] : parts[4],
        pdf: parts[6], report: parts[7], notes: parts[8]
      });
    }
    await writeFile(stateUrl, JSON.stringify(rows));
  `);
  await writeFile(join(root, "set-status.mjs"), `
    import { readFile, writeFile } from "node:fs/promises";
    const stateUrl = new URL("./state.json", import.meta.url);
    const rows = JSON.parse(await readFile(stateUrl, "utf8"));
    const rowId = Number(process.argv[process.argv.indexOf("--row") + 1]);
    const status = process.argv[process.argv.indexOf("--row") + 2];
    const note = process.argv[process.argv.indexOf("--note") + 1];
    const row = rows.find((candidate) => candidate.id === rowId);
    if (!row) process.exit(2);
    row.status = status;
    if (!row.notes.includes(note)) row.notes += "; " + note;
    await writeFile(stateUrl, JSON.stringify(rows));
    process.stdout.write(JSON.stringify({ ok: true }));
  `);
  await writeFile(join(root, "reserve-report-num.mjs"), `
    if (process.argv.includes("--release")) process.exit(0);
    process.stdout.write("042\\n");
  `);
  await writeFile(join(root, "build-cv-html.mjs"), `
    // <input.json> <output.html> page_format
    import { mkdir, readFile, writeFile } from "node:fs/promises";
    import { dirname } from "node:path";
    const input = process.argv[2];
    const output = process.argv[3];
    const payload = JSON.parse(await readFile(input, "utf8"));
    await mkdir(dirname(output), { recursive: true });
    await writeFile(output, "<html><body>" + payload.candidate.name + " — " + payload.summary + "</body></html>");
  `);
  await writeFile(join(root, "verify-cv-facts.mjs"), `// --json verdict\nprocess.stdout.write(JSON.stringify({ verdict: "pass" }));\n`);
  await writeFile(join(root, "generate-pdf.mjs"), `
    // resolvePdfIndexPath # report\\tpdf\\thtml\\tformat\\tdate
    import { mkdir, writeFile } from "node:fs/promises";
    import { dirname } from "node:path";
    if (!process.argv.includes("--allow-reorder")) {
      console.error("tailored CV generation must opt into the career-ops reorder guard");
      process.exit(2);
    }
    const input = process.argv[2];
    const output = process.argv[3];
    const report = process.argv.find((value) => value.startsWith("--report="))?.split("=")[1] ?? "";
    const format = process.argv.find((value) => value.startsWith("--format="))?.split("=")[1] ?? "a4";
    await mkdir(dirname(output), { recursive: true });
    await writeFile(output, "%PDF-1.4\\n1 0 obj\\n<< /Type /Catalog /Pages 2 0 R >>\\nendobj\\n2 0 obj\\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\\nendobj\\n3 0 obj\\n<< /Type /Page /Parent 2 0 R >>\\nendobj\\ntrailer\\n<< /Root 1 0 R >>\\n%%EOF\\n");
    const root = process.env.CAREER_OPS_TRACKER.endsWith("data/applications.md")
      ? dirname(dirname(process.env.CAREER_OPS_TRACKER)) : dirname(process.env.CAREER_OPS_TRACKER);
    const relative = (path) => path.slice(root.length + 1);
    await writeFile(process.env.CAREER_OPS_PDF_INDEX, "# report\\tpdf\\thtml\\tformat\\tdate — written by generate-pdf.mjs, do not edit\\n" + [report, relative(output), relative(input), format, "2026-08-30"].join("\\t") + "\\n");
  `);
  await writeFile(join(root, "tracker-utils.mjs"), "// CAREER_OPS_PDF_INDEX resolvePdfIndexPath\n");
  await writeFile(join(root, "hfw-preparation-commit.mjs"), "process.exit(99);\n");
  await writeFile(join(root, "application-artifacts.mjs"), "// applicationArtifactPaths writeReuseDecision schema_version: 1\n");
  await writeFile(join(root, "batch/batch-prompt.md"), "#### Machine Summary\nauthorization_confidence:\nauthorization_evidence:\nauthorization_scope:\nengagement_mechanism:\nauthorization_question:\n");
  await writeFile(join(root, "templates/states.yml"), "states: []\n");
  await writeFile(join(root, "package.json"), '{"version":"1.31.0"}\n');
  await writeFile(join(root, "VERSION"), "1.31.0\n");
  await writeFile(join(root, "application-answers.mjs"), `
    import { readFile, writeFile } from "node:fs/promises";
    const value = (name) => process.argv[process.argv.indexOf(name) + 1];
    const reportPath = value("--report");
    const snapshot = JSON.parse(await readFile(value("--input"), "utf8"));
    const lines = [
      "## Application Answers", "", "**Date:** " + value("--date"),
      "**State:** " + value("--state"), "", "### Free-text answers", "",
      ...snapshot.freeText.flatMap((item, index) => [String(index + 1) + ". **" + item.question + "**", "", "> " + item.answer, ""]),
      "### Other field values", "",
      ...snapshot.fieldValues.map((item, index) => String(index + 1) + ". **" + item.question + ":** " + item.answer),
      "", "### Files used", "",
      ...snapshot.files.map((item, index) => String(index + 1) + ". **" + item.field + ":** " + item.path),
    ];
    const report = await readFile(reportPath, "utf8");
    const start = report.search(/^## Application Answers$/m);
    const base = start >= 0 ? report.slice(0, start).trimEnd() : report.trimEnd();
    await writeFile(reportPath, base + "\\n\\n" + lines.join("\\n").trimEnd() + "\\n");
  `);
  for (const [command, args] of [
    ["git", ["init", "--quiet"]],
    ["git", ["add", "."]],
    ["git", ["-c", "user.name=HereForWork Test", "-c", "user.email=test@hereforwork.local", "commit", "--quiet", "-m", "fixture"]],
  ]) {
    const result = await runCommand(command, args, root);
    assert.equal(result.status, 0, result.stderr);
  }
  return {
    root,
    staging,
    env: {
      HFW_CAREER_OPS_ROOT: root,
      HFW_CAREER_OPS_INDEX: join(root, "applications.db"),
      HFW_CAREER_OPS_STAGING: staging,
    },
  };
}

async function seedCanonicalEvaluation(fixture, id = 42) {
  const reportPath = `reports/${String(id).padStart(3, "0")}-example.md`;
  await writeFile(join(fixture.root, reportPath), evaluationReport);
  await writeFile(join(fixture.root, "state.json"), JSON.stringify([{
    id, date: "2026-08-30", company: "Example Co", role: "Frontend Engineer",
    score: "4.2/5", status: "Evaluated", pdf: "❌",
    report: `[${String(id).padStart(3, "0")}](../${reportPath})`, notes: "canonical fixture",
  }]));
  const capabilities = await request({ id: "seed-capabilities", protocolVersion: 1, operation: "capabilities.get", input: {} }, fixture.env);
  const find = (capabilityId) => capabilities.result.capabilities.find(({ id: value }) => value === capabilityId).compatibilityFingerprint;
  return {
    trackerId: id,
    reportPath,
    reportSha256: createHash("sha256").update(evaluationReport).digest("hex"),
    upstreamRevision: capabilities.result.upstreamRevision,
    evaluationCompatibilityFingerprint: find("evaluation.result.read.v1"),
    artifactCompatibilityFingerprint: find("artifacts.inspect.v1"),
  };
}

test("provider-neutral preparation commits only through fixed career-ops writers", async () => {
  const fixture = await fakeCareerOps();
  const health = await request({
    id: "health",
    protocolVersion: 1,
    operation: "health.check",
    input: {},
  }, fixture.env);
  assert.equal(health.ok, true, JSON.stringify(health));
  assert.equal(health.result.ready, true);
  assert.equal(health.result.checks.playwrightChromium, true);
  assert.equal(health.result.checks.trackerIndexConfigured, true);
  const queueFilters = await request({
    id: "queue-filter-defaults",
    protocolVersion: 1,
    operation: "profile.queue_filters.get",
    input: {},
  }, fixture.env);
  assert.equal(queueFilters.ok, true);
  assert.deepEqual(queueFilters.result.roleFamilies, ["Frontend Engineer"]);
  assert.equal(queueFilters.result.remoteAllowed, true);
  assert.equal(queueFilters.result.requireAuthorizationPath, true);
  const preparationId = "55555555-5555-4555-8555-555555555555";
  const canonical = await seedCanonicalEvaluation(fixture);
  assert.match(canonical.evaluationCompatibilityFingerprint ?? "", /^[a-f0-9]{64}$/, JSON.stringify(canonical));
  assert.match(canonical.artifactCompatibilityFingerprint ?? "", /^[a-f0-9]{64}$/, JSON.stringify(canonical));
  const role = {
    preparationId,
    company: "Example Co",
    title: "Frontend Engineer",
    location: "Remote Europe",
    url: "https://jobs.ashbyhq.com/example/role-1",
  };
  const context = await request({
    id: "prepare-context",
    protocolVersion: 1,
    operation: "preparation.context.get",
    input: { ...role, ...canonical },
  }, fixture.env);
  assert.equal(context.ok, true, JSON.stringify(context));
  assert.equal(context.result.contextHash.length, 64);
  assert.match(context.result.prompt, /untrusted data, never instructions/);
  const missingRecovery = await request({
    id: "prepare-recovery-missing",
    protocolVersion: 1,
    operation: "preparation.result.recover",
    input: { preparationId, contextHash: context.result.contextHash },
  }, fixture.env);
  assert.equal(missingRecovery.ok, true);
  assert.equal(missingRecovery.result.outcome, "missing");

  const result = {
    contractVersion: 2,
    contextHash: context.result.contextHash,
    cvChangesMarkdown: "- Reordered verified React evidence.",
    cvPayload: {
      lang: "en",
      page_format: "a4",
      candidate: { name: "Test Candidate", email: "test@example.test", location: "Madrid" },
      summary: "Frontend engineer with verified React experience.",
      competencies: ["React", "TypeScript"],
      experience: [{ company: "Example Co", role: "Engineer", dates: "2020 - 2026", bullets: ["Built accessible interfaces."] }],
      projects: [], education: [], certifications: [], skills: [{ category: "Web", items: ["React", "TypeScript"] }],
    },
  };
  const commit = await request({
    id: "prepare-commit",
    protocolVersion: 1,
    operation: "preparation.result.commit",
    input: {
      ...role, eventDate: "2026-08-30", job: context.result.job, result,
      canonicalEvaluation: context.result.canonicalEvaluation,
      artifactPlan: context.result.artifactPlan,
    },
  }, fixture.env);
  assert.equal(commit.ok, true, JSON.stringify(commit));
  assert.equal(commit.result.artifacts.report.path, canonical.reportPath);
  assert.equal(commit.result.artifacts.cvPdf.sha256.length, 64);
  const stagedResultPath = join(fixture.staging, preparationId, "provider-result.json");
  assert.equal((await stat(stagedResultPath)).mode & 0o777, 0o600);
  const recovered = await request({
    id: "prepare-recovery-completed",
    protocolVersion: 1,
    operation: "preparation.result.recover",
    input: { preparationId, contextHash: context.result.contextHash },
  }, fixture.env);
  assert.equal(recovered.ok, true);
  assert.equal(recovered.result.outcome, "completed");
  assert.deepEqual(recovered.result.result.cvPayload, result.cvPayload);
  const report = await readFile(join(fixture.root, commit.result.artifacts.report.path), "utf8");
  assert.equal(report, evaluationReport);
  const rows = JSON.parse(await readFile(join(fixture.root, "state.json"), "utf8"));
  assert.equal(rows.length, 1);
  assert.equal(rows[0].status, "Evaluated");
  assert.equal(rows[0].report, "[042](../reports/042-example.md)");

  const replay = await request({
    id: "prepare-commit-replay",
    protocolVersion: 1,
    operation: "preparation.result.commit",
    input: {
      ...role, eventDate: "2026-08-30", job: context.result.job, result,
      canonicalEvaluation: context.result.canonicalEvaluation,
      artifactPlan: context.result.artifactPlan,
    },
  }, fixture.env);
  assert.equal(replay.ok, true, JSON.stringify(replay));
  assert.equal(replay.result.artifacts.report.sha256, commit.result.artifacts.report.sha256);

  const snapshot = {
    protocolVersion: 1,
    ats: "ashby",
    url: role.url,
    title: "Example Co application",
    fingerprint: "d".repeat(64),
    fields: [
      { id: "email", label: "Email", control: "input", inputType: "email", required: true, options: [], classification: "safe_verified", reason: "Verified profile fact." },
      { id: "why", label: "Why are you interested in working at Example Co? (2-3 sentences)", control: "textarea", inputType: "textarea", required: true, options: [], language: "en", maxLength: 400, maxWords: 60, minSentences: 2, maxSentences: 3, classification: "grounded_narrative", reason: "Needs grounded drafting." },
      { id: "phone", label: "Phone", control: "input", inputType: "tel", required: false, options: [], classification: "safe_verified", reason: "Verified profile fact." },
      { id: "country", label: "Which country will you work from?", control: "select", inputType: "select", required: true, options: ["Portugal", "Spain"], classification: "safe_verified", reason: "Verified profile fact." },
      { id: "salary", label: "Expected annual salary (EUR)", control: "input", inputType: "number", inputMode: "numeric", required: false, options: [], classification: "compensation", reason: "Canonical compensation preference." },
      { id: "monthlySalary", label: "Expected monthly salary (EUR)", control: "input", inputType: "number", inputMode: "numeric", required: false, options: [], classification: "compensation", reason: "Canonical compensation preference has no monthly conversion." },
      { id: "usdSalary", label: "Expected annual salary (USD)", control: "input", inputType: "number", inputMode: "numeric", required: false, options: [], classification: "compensation", reason: "Canonical compensation preference has no USD conversion." },
      { id: "unlabeled", label: "", control: "input", inputType: "text", required: false, options: [], classification: "unknown", reason: "No visible label." },
    ],
  };
  const answerContext = await request({
    id: "answer-context",
    protocolVersion: 1,
    operation: "answers.context.get",
    input: { preparationId, reportPath: commit.result.artifacts.report.path, snapshot },
  }, fixture.env);
  assert.equal(answerContext.ok, true);
  assert.match(answerContext.result.prompt, /grounded_narrative/);
  assert.match(answerContext.result.prompt, /minSentences/);
  assert.match(answerContext.result.prompt, /pending human review/);
  const answerResult = {
    contractVersion: 1,
    contextHash: answerContext.result.contextHash,
    answers: [
      { fieldId: "email", answer: "untrusted-model@example.test", provenance: ["config/profile.yml"] },
      { fieldId: "why", answer: "The role matches my verified React and accessibility work. Its product scope also matches my source-backed frontend experience.", provenance: ["cv.md", canonical.reportPath] },
      { fieldId: "phone", answer: "+34 600 000 000", provenance: ["config/profile.yml"] },
      { fieldId: "country", answer: "Portugal", provenance: ["config/profile.yml"] },
      { fieldId: "salary", answer: "52000", provenance: ["config/profile.yml"] },
      { fieldId: "monthlySalary", answer: "4333", provenance: ["config/profile.yml"] },
      { fieldId: "usdSalary", answer: "60000", provenance: ["config/profile.yml"] },
      { fieldId: "unlabeled", answer: "Provider output must not promote an unknown field.", provenance: ["cv.md"] },
    ],
  };
  const answers = await request({
    id: "answer-validate",
    protocolVersion: 1,
    operation: "answers.result.validate",
    input: { preparationId, reportPath: commit.result.artifacts.report.path, snapshot, result: answerResult },
  }, fixture.env);
  assert.equal(answers.ok, true);
  assert.deepEqual(answers.result.fillPlan.instructions, [
    { fieldId: "email", value: "test@example.test", classification: "safe_verified", required: true },
    { fieldId: "why", value: "The role matches my verified React and accessibility work. Its product scope also matches my source-backed frontend experience.", classification: "grounded_draft", required: true },
    { fieldId: "phone", value: "+34 600 000 000", classification: "safe_verified", required: false },
    { fieldId: "country", value: "Spain", classification: "safe_verified", required: true },
    { fieldId: "salary", value: "52000", classification: "canonical_preference", required: false },
  ]);
  assert.deepEqual(answers.result.reviewItems.find((item) => item.fieldId === "why"), {
    fieldId: "why",
    label: "Why are you interested in working at Example Co? (2-3 sentences)",
    decision: "fill_draft",
    answer: "The role matches my verified React and accessibility work. Its product scope also matches my source-backed frontend experience.",
    provenance: ["cv.md", canonical.reportPath],
    draftPolicy: { language: "en", maxLength: 400, maxWords: 60, minSentences: 2, maxSentences: 3 },
  });
  assert.equal(answers.result.reviewItems.find((item) => item.fieldId === "phone").decision, "fill");
  assert.equal(answers.result.reviewItems.find((item) => item.fieldId === "country").answer, "Spain");
  assert.deepEqual(answers.result.reviewItems.find((item) => item.fieldId === "salary"), {
    fieldId: "salary",
    label: "Expected annual salary (EUR)",
    decision: "fill_preference",
    answer: "52000",
    provenance: ["config/profile.yml:compensation.application_answer"],
  });
  assert.equal(answers.result.reviewItems.find((item) => item.fieldId === "monthlySalary").decision, "skip");
  assert.equal(answers.result.reviewItems.find((item) => item.fieldId === "usdSalary").decision, "skip");
  assert.equal(answers.result.reviewItems.find((item) => item.fieldId === "unlabeled").answer, null);

  const unsupportedProvenance = structuredClone(answerResult);
  unsupportedProvenance.answers.find((item) => item.fieldId === "why").provenance = ["modes/apply.md"];
  const provenanceRejected = await request({
    id: "answer-validate-provenance",
    protocolVersion: 1,
    operation: "answers.result.validate",
    input: { preparationId, reportPath: commit.result.artifacts.report.path, snapshot, result: unsupportedProvenance },
  }, fixture.env);
  assert.equal(provenanceRejected.result.reviewItems.find((item) => item.fieldId === "why").decision, "skip");
  assert.equal(provenanceRejected.result.fillPlan.instructions.some((item) => item.fieldId === "why"), false);

  const overlongDraft = structuredClone(answerResult);
  overlongDraft.answers.find((item) => item.fieldId === "why").answer = "verified ".repeat(61).trim();
  const lengthRejected = await request({
    id: "answer-validate-length",
    protocolVersion: 1,
    operation: "answers.result.validate",
    input: { preparationId, reportPath: commit.result.artifacts.report.path, snapshot, result: overlongDraft },
  }, fixture.env);
  assert.equal(lengthRejected.result.reviewItems.find((item) => item.fieldId === "why").decision, "skip");
  assert.equal(lengthRejected.result.fillPlan.instructions.some((item) => item.fieldId === "why"), false);

  const tooFewSentences = structuredClone(answerResult);
  tooFewSentences.answers.find((item) => item.fieldId === "why").answer = "I improved a verified path from 2.5 seconds to 1.2 seconds.";
  const sentenceRejected = await request({
    id: "answer-validate-sentences",
    protocolVersion: 1,
    operation: "answers.result.validate",
    input: { preparationId, reportPath: commit.result.artifacts.report.path, snapshot, result: tooFewSentences },
  }, fixture.env);
  assert.equal(sentenceRejected.result.reviewItems.find((item) => item.fieldId === "why").decision, "skip");
  assert.equal(sentenceRejected.result.fillPlan.instructions.some((item) => item.fieldId === "why"), false);

  const answerCommit = await request({
    id: "answer-commit",
    protocolVersion: 1,
    operation: "answers.result.commit",
    input: {
      preparationId,
      reportPath: commit.result.artifacts.report.path,
      contextHash: answers.result.contextHash,
      reviewItems: answers.result.reviewItems,
      fillResults: [
        { fieldId: "email", status: "verified", reasonCode: "verified", reason: null, mutated: true, readBackSha256: "1".repeat(64) },
        { fieldId: "why", status: "verified", reasonCode: "verified", reason: null, mutated: true, readBackSha256: "2".repeat(64) },
        { fieldId: "phone", status: "verified", reasonCode: "verified", reason: null, mutated: true, readBackSha256: "3".repeat(64) },
        { fieldId: "country", status: "verified", reasonCode: "verified", reason: null, mutated: true, readBackSha256: "4".repeat(64) },
        { fieldId: "salary", status: "verified", reasonCode: "verified", reason: null, mutated: true, readBackSha256: "5".repeat(64) },
        { fieldId: "monthlySalary", status: "skipped", reasonCode: "unsupported_control", reason: "No canonical monthly conversion.", mutated: false, readBackSha256: null },
        { fieldId: "usdSalary", status: "skipped", reasonCode: "unsupported_control", reason: "No canonical USD conversion.", mutated: false, readBackSha256: null },
        { fieldId: "unlabeled", status: "skipped", reasonCode: "unsupported_control", reason: "No visible label.", mutated: false, readBackSha256: null },
      ],
      cvPdfPath: commit.result.artifacts.cvPdf.path,
      eventDate: "2026-08-30",
    },
  }, fixture.env);
  assert.equal(answerCommit.ok, true);
  assert.equal(answerCommit.result.report.sha256.length, 64);
  const reportWithAnswers = await readFile(join(fixture.root, commit.result.artifacts.report.path), "utf8");
  assert.match(reportWithAnswers, /## Application Answers/);
  assert.match(reportWithAnswers, /\*\*Email:\*\* test@example\.test/);
  assert.match(reportWithAnswers, /verified React and accessibility work/);
  assert.match(reportWithAnswers, /\*\*Phone:\*\* \+34 600 000 000/);
  assert.match(reportWithAnswers, /\*\*Which country will you work from\?:\*\* Spain/);
  assert.match(reportWithAnswers, /\*\*Expected annual salary \(EUR\):\*\* 52000/);
  assert.doesNotMatch(reportWithAnswers, /4333|60000/);
  assert.doesNotMatch(reportWithAnswers, /Provider output must not promote/);
  assert.doesNotMatch(reportWithAnswers, /\*\*:\*\*/);

  const cleanup = await request({
    id: "preparation-cleanup",
    protocolVersion: 1,
    operation: "preparation.artifacts.delete",
    input: {
      preparationId,
      reportPath: commit.result.artifacts.report.path,
      cvPdfPath: commit.result.artifacts.cvPdf.path,
    },
  }, fixture.env);
  assert.equal(cleanup.ok, true, JSON.stringify(cleanup));
  assert.equal(await readFile(join(fixture.root, commit.result.artifacts.report.path), "utf8"), reportWithAnswers);
  await assert.rejects(readFile(join(fixture.root, commit.result.artifacts.cvPdf.path)));
  await assert.rejects(readFile(join(fixture.staging, preparationId, "commit-state.json")));

  const repeatedCleanup = await request({
    id: "preparation-cleanup-retry",
    protocolVersion: 1,
    operation: "preparation.artifacts.delete",
    input: {
      preparationId,
      reportPath: commit.result.artifacts.report.path,
      cvPdfPath: commit.result.artifacts.cvPdf.path,
    },
  }, fixture.env);
  assert.equal(repeatedCleanup.ok, true, JSON.stringify(repeatedCleanup));
});

test("failed preparation cleanup removes only its bounded staging and is idempotent", async () => {
  const fixture = await fakeCareerOps();
  const preparationId = "88888888-8888-4888-8888-888888888888";
  const adapterStaging = join(fixture.staging, preparationId);
  const generatedStaging = join(
    fixture.root,
    "output/.hfw-preparation-staging",
    preparationId,
    "candidate",
  );
  await mkdir(adapterStaging, { recursive: true });
  await mkdir(generatedStaging, { recursive: true });
  await writeFile(join(adapterStaging, "provider-result.json"), "{}\n");
  await writeFile(join(generatedStaging, "cv.html"), "staged only\n");

  for (const id of ["failed-preparation-cleanup", "failed-preparation-cleanup-retry"]) {
    const cleanup = await request({
      id,
      protocolVersion: 1,
      operation: "preparation.artifacts.delete",
      input: { preparationId, reportPath: null, cvPdfPath: null },
    }, fixture.env);
    assert.equal(cleanup.ok, true, JSON.stringify(cleanup));
  }

  await assert.rejects(stat(adapterStaging));
  await assert.rejects(stat(join(fixture.root, "output/.hfw-preparation-staging", preparationId)));
  assert.equal((await stat(join(fixture.root, "data/applications.md"))).isFile(), true);
});

test("preparation recovery remains compatible with a valid staged v1 result", async () => {
  const fixture = await fakeCareerOps();
  const preparationId = "66666666-6666-4666-8666-666666666666";
  const contextHash = "a".repeat(64);
  const result = {
    contractVersion: 1,
    contextHash,
    score: 4.2,
    legitimacy: "High Confidence",
    authorizationConfidence: "investigate",
    reportBodyMarkdown: [
      "## Machine Summary", "Legacy staged result.",
      "## A) Role", "## B) Match", "## C) Level", "## D) Compensation",
      "## E) Customization", "## F) Interview", "## G) Legitimacy",
      "## Risk Summary", "No active content.", "## Keywords extracted",
      "React TypeScript accessibility ".repeat(8),
    ].join("\n\n"),
    cvPayload: { candidate: { photo: "" } },
    cvChangesMarkdown: "- Reordered verified evidence.",
  };
  const directory = join(fixture.staging, preparationId);
  await mkdir(directory, { recursive: true });
  await writeFile(join(directory, "provider-result.json"), `${JSON.stringify(result)}\n`);

  const recovered = await request({
    id: "recover-v1",
    protocolVersion: 1,
    operation: "preparation.result.recover",
    input: { preparationId, contextHash },
  }, fixture.env);

  assert.equal(recovered.ok, true, JSON.stringify(recovered));
  assert.equal(recovered.result.outcome, "completed");
  assert.deepEqual(recovered.result.result, result);
});

test("preparation accepts career-ops profiles without optional cv-facts config", async () => {
  const fixture = await fakeCareerOps();
  const canonical = await seedCanonicalEvaluation(fixture);
  await unlink(join(fixture.root, "config/cv-facts.json"));

  const context = await request({
    id: "prepare-with-default-fact-gate",
    protocolVersion: 1,
    operation: "preparation.context.get",
    input: {
      preparationId: "77777777-7777-4777-8777-777777777777",
      company: "Example Co",
      title: "Frontend Engineer",
      location: "Remote Europe",
      url: "https://jobs.ashbyhq.com/example/role-1",
      ...canonical,
    },
  }, fixture.env);

  assert.equal(context.ok, true);
  assert.equal(context.result.sourceHashes["config/cv-facts.json"], undefined);
  assert.doesNotMatch(context.result.prompt, /career_ops_source path="config\/cv-facts\.json"/);
});

test("Discard, Undo, and confirmed Applied use canonical writers idempotently", async () => {
  const fixture = await fakeCareerOps();
  const discardKey = "11111111-1111-4111-8111-111111111111";
  const role = {
    company: "Example Co",
    title: "Frontend Engineer",
    location: "Remote Europe",
    url: "https://jobs.example.test/role-1",
    eventDate: "2026-08-30",
  };
  const discard = await request({
    id: "discard",
    protocolVersion: 1,
    operation: "role.discard",
    input: { ...role, idempotencyKey: discardKey, reason: "Not a fit" },
  }, fixture.env);
  assert.equal(discard.ok, true);
  assert.equal(discard.result.effect.status, "Discarded");

  const replay = await request({
    id: "discard-replay",
    protocolVersion: 1,
    operation: "role.discard",
    input: { ...role, idempotencyKey: discardKey, reason: "Not a fit" },
  }, fixture.env);
  assert.equal(replay.ok, true);
  assert.equal(JSON.parse(await readFile(join(fixture.root, "state.json"), "utf8")).length, 1);

  const undoKey = "22222222-2222-4222-8222-222222222222";
  const undo = await request({
    id: "undo",
    protocolVersion: 1,
    operation: "role.discard.undo",
    input: {
      idempotencyKey: undoKey,
      discardEffectKey: discardKey,
      trackerId: discard.result.effect.trackerId,
      eventDate: "2026-08-30",
    },
  }, fixture.env);
  assert.equal(undo.ok, true);
  assert.equal(undo.result.effect.status, "Evaluated");

  const appliedKey = "33333333-3333-4333-8333-333333333333";
  const applied = await request({
    id: "applied",
    protocolVersion: 1,
    operation: "application.applied.confirm",
    input: {
      ...role,
      idempotencyKey: appliedKey,
      trackerId: discard.result.effect.trackerId,
      userConfirmed: true,
    },
  }, fixture.env);
  assert.equal(applied.ok, true);
  assert.equal(applied.result.effect.status, "Applied");

  const staleDiscard = await request({
    id: "stale-discard",
    protocolVersion: 1,
    operation: "role.discard",
    input: { ...role, idempotencyKey: "44444444-4444-4444-8444-444444444444" },
  }, fixture.env);
  assert.equal(staleDiscard.ok, false);
  assert.match(staleDiscard.error.message, /already Applied/);
});

test("Applied is rejected without explicit user confirmation", async () => {
  const response = await request({
    id: "not-confirmed",
    protocolVersion: 1,
    operation: "application.applied.confirm",
    input: {},
  });
  assert.equal(response.ok, false);
  assert.match(response.error.message, /explicit outcome confirmation/);
});

test("unknown operations are rejected", async () => {
  const response = await request({
    id: "unknown",
    protocolVersion: 1,
    operation: "shell.run",
    input: {},
  });

  assert.equal(response.ok, false);
  assert.match(response.error.message, /Unsupported operation/);
});
