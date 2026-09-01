import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { test } from "node:test";

import {
  computeDiscoveryRunDigest,
  parseDiscoveryArtifact,
  validateDiscoveryRun,
} from "./discovery-run-contract.mjs";

const legacyFixture = JSON.parse(await readFile(new URL("../examples/discovery-dataset.example.json", import.meta.url)));
const runFixture = JSON.parse(await readFile(new URL("../examples/discovery-run.example.json", import.meta.url)));

function clone(value) {
  return structuredClone(value);
}

function seal(run) {
  run.integrity.digest = computeDiscoveryRunDigest(run);
  return run;
}

function reverseObjectKeys(value) {
  if (Array.isArray(value)) return value.map(reverseObjectKeys);
  if (!value || typeof value !== "object") return value;
  return Object.fromEntries(Object.keys(value).reverse().map((key) => [key, reverseObjectKeys(value[key])]));
}

test("the unchanged schema-v1 dataset remains a supported legacy artifact", () => {
  const parsed = parseDiscoveryArtifact(legacyFixture);

  assert.equal(parsed.kind, "legacy_dataset");
  assert.equal(parsed.value.schemaVersion, 1);
  assert.equal(parsed.value.findings.length, 1);
});

test("legacy parsing validates the complete schema-v1 finding shape", () => {
  const missingField = clone(legacyFixture);
  delete missingField.findings[0].sourceRoleId;
  assert.throws(() => parseDiscoveryArtifact(missingField), /missing sourceRoleId/);

  const unknownField = clone(legacyFixture);
  unknownField.findings[0].matchScore = { status: "not_scored" };
  assert.throws(() => parseDiscoveryArtifact(unknownField), /unknown property matchScore/);
});

test("a discovery run preserves scored and explicitly unscored career-ops states", () => {
  const parsed = parseDiscoveryArtifact(runFixture);

  assert.equal(parsed.kind, "discovery_run");
  assert.deepEqual(parsed.value.findings.map((finding) => finding.matchScore.status), ["scored", "not_scored"]);
  assert.equal(parsed.value.findings[0].matchScore.scale, "career_ops_1_to_5");
  assert.equal(parsed.value.findings[0].matchScore.value, 4.5);
  assert.equal(parsed.value.findings[1].matchScore.value, undefined);
});

test("integrity is deterministic across object, finding, evidence, and issue order", () => {
  const baseline = clone(runFixture);
  baseline.issues = [
    { issueId: "warning:b", code: "warning_b", message: "Second synthetic warning.", retryable: false },
    { issueId: "warning:a", code: "warning_a", message: "First synthetic warning.", retryable: false },
  ];
  seal(baseline);
  const reordered = reverseObjectKeys(clone(baseline));
  reordered.findings.reverse();
  reordered.findings.forEach((finding) => finding.evidence.reverse());
  reordered.issues.reverse();

  assert.equal(computeDiscoveryRunDigest(reordered), baseline.integrity.digest);
});

test("integrity fails closed when covered content changes", () => {
  const changed = clone(runFixture);
  changed.findings[0].title = "Changed title";

  assert.throws(() => validateDiscoveryRun(changed), /digest does not match/);
});

test("fabricated score authority and percentage metadata are rejected", () => {
  const fabricatedAuthority = clone(runFixture);
  fabricatedAuthority.findings[0].matchScore.authority = "here-for-work";
  seal(fabricatedAuthority);
  assert.throws(() => validateDiscoveryRun(fabricatedAuthority), /authority must be career-ops/);

  const percentage = clone(runFixture);
  percentage.findings[0].matchScore.percentage = 90;
  seal(percentage);
  assert.throws(() => validateDiscoveryRun(percentage), /unknown property percentage/);
});

test("scored provenance is required and not_scored cannot carry a value", () => {
  const missingVersion = clone(runFixture);
  delete missingVersion.findings[0].matchScore.sourceVersion;
  seal(missingVersion);
  assert.throws(() => validateDiscoveryRun(missingVersion), /missing sourceVersion/);

  const inventedValue = clone(runFixture);
  inventedValue.findings[1].matchScore.value = 4;
  seal(inventedValue);
  assert.throws(() => validateDiscoveryRun(inventedValue), /unknown property value/);
});

test("published attempt identity supports partial-to-completed retry without weakening replay identity", () => {
  const partial = clone(runFixture);
  partial.runId = runFixture.supersedesRunId;
  delete partial.supersedesRunId;
  partial.status = "partial";
  partial.issues = [{ issueId: "source-timeout", code: "source_timeout", message: "One source page timed out.", retryable: true }];
  assert.doesNotThrow(() => validateDiscoveryRun(seal(partial)));

  assert.equal(runFixture.windowId, partial.windowId);
  assert.notEqual(runFixture.runId, partial.runId);
  assert.equal(runFixture.supersedesRunId, partial.runId);
  assert.doesNotThrow(() => validateDiscoveryRun(runFixture));

  const selfSuperseding = clone(runFixture);
  selfSuperseding.supersedesRunId = selfSuperseding.runId;
  seal(selfSuperseding);
  assert.throws(() => validateDiscoveryRun(selfSuperseding), /must differ from run.runId/);
});

test("duplicate stable ids are rejected at every scoped collection", () => {
  const duplicateFinding = clone(runFixture);
  duplicateFinding.findings[1].findingId = duplicateFinding.findings[0].findingId;
  assert.throws(() => validateDiscoveryRun(seal(duplicateFinding)), /duplicate findingId/);

  const duplicateEvidence = clone(runFixture);
  duplicateEvidence.findings[0].evidence[1].evidenceId = duplicateEvidence.findings[0].evidence[0].evidenceId;
  assert.throws(() => validateDiscoveryRun(seal(duplicateEvidence)), /duplicate evidenceId/);

  const duplicateIssue = clone(runFixture);
  duplicateIssue.issues = [
    { issueId: "warning:same", code: "warning_a", message: "First warning.", retryable: false },
    { issueId: "warning:same", code: "warning_b", message: "Second warning.", retryable: false },
  ];
  assert.throws(() => validateDiscoveryRun(seal(duplicateIssue)), /duplicate issueId/);
});

test("calendar-impossible timestamps and premature generation are rejected", () => {
  const impossibleDate = clone(runFixture);
  impossibleDate.generatedAt = "2026-02-30T13:05:00+02:00";
  assert.throws(() => validateDiscoveryRun(seal(impossibleDate)), /real ISO date/);

  const generatedBeforeCoverage = clone(runFixture);
  generatedBeforeCoverage.generatedAt = "2026-09-01T12:59:59+02:00";
  assert.throws(() => validateDiscoveryRun(seal(generatedBeforeCoverage)), /must not be before/);
});

test("partial and failed runs remain explicit and non-successful", () => {
  const partial = clone(runFixture);
  partial.status = "partial";
  partial.issues = [{ issueId: "source-timeout", code: "source_timeout", message: "One source page timed out.", retryable: true }];
  assert.doesNotThrow(() => validateDiscoveryRun(seal(partial)));

  const failedWithFindings = clone(partial);
  failedWithFindings.status = "failed";
  seal(failedWithFindings);
  assert.throws(() => validateDiscoveryRun(failedWithFindings), /failed runs cannot contain findings/);
});
