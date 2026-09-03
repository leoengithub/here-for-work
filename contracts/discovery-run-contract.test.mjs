import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { test } from "node:test";

import {
  canonicalizeDiscoveryRun,
  computeDiscoveryRunDigest,
  isNativeCareerOpsScore,
  parseDiscoveryArtifact,
  validateDiscoveryRun,
} from "./discovery-run-contract.mjs";

const legacyFixture = JSON.parse(await readFile(new URL("../examples/discovery-dataset.example.json", import.meta.url)));
const runFixture = JSON.parse(await readFile(new URL("../examples/discovery-run.example.json", import.meta.url)));
const numberParityFixtures = JSON.parse(await readFile(new URL("./discovery-run-number-parity.json", import.meta.url)));

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

test("canonical JSON number formatting is shared with the Rust importer", () => {
  for (const fixture of numberParityFixtures) {
    assert.equal(
      canonicalizeDiscoveryRun({ value: fixture.value }),
      `{"value":{"$hfwCanonicalNumberV1":"${fixture.expected}"}}`,
    );
  }
  assert.throws(() => canonicalizeDiscoveryRun({ value: Number.NaN }), /finite/);
});

test("career-ops score digest parity covers randomized valid f64 values", async () => {
  const precision = JSON.parse(await readFile(new URL("./discovery-run-score-precision.json", import.meta.url)));
  const rejected = clone(runFixture);
  rejected.findings[0].matchScore.value = precision.adversarial;
  assert.doesNotThrow(() => validateDiscoveryRun(seal(rejected)));
  assert.equal(isNativeCareerOpsScore(4.25), true);
  let state = precision.seed >>> 0;
  const aggregate = createHash("sha256");
  for (let index = 0; index < precision.count; index += 1) {
    state = (Math.imul(state, 1664525) + 1013904223) >>> 0;
    const candidate = 1 + (state / 0x100000000) * 4;
    assert.equal(isNativeCareerOpsScore(candidate), true);
    const digest = createHash("sha256")
      .update(canonicalizeDiscoveryRun({ value: candidate }), "utf8")
      .digest("hex");
    aggregate.update(digest, "ascii");
  }
  assert.equal(aggregate.digest("hex"), precision.aggregateSha256);
});

test("text bounds use UTF-8 bytes across astral Unicode", () => {
  const valid = clone(runFixture);
  valid.source.displayName = "😀".repeat(50);
  valid.findings.forEach((finding) => { finding.source = valid.source.displayName; });
  assert.doesNotThrow(() => validateDiscoveryRun(seal(valid)));

  const tooLong = clone(valid);
  tooLong.source.displayName = "😀".repeat(51);
  tooLong.findings.forEach((finding) => { finding.source = tooLong.source.displayName; });
  assert.throws(() => validateDiscoveryRun(seal(tooLong)), /UTF-8 bytes/);
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

test("timestamps use the strict uppercase timezone contract", () => {
  for (const value of [
    "2026-09-01 13:00:00+02:00",
    "2026-09-01t13:00:00+02:00",
    "2026-09-01T13:00:00z",
    "2026-09-01T13:00:60+02:00",
    "2026-09-01T13:00:00.1234567890+02:00",
    `2026-09-01T13:00:00.${"1".repeat(76)}+02:00`,
  ]) {
    const invalid = clone(runFixture);
    invalid.generatedAt = value;
    assert.throws(() => validateDiscoveryRun(seal(invalid)), /ISO date-time|real ISO|string between/);
  }
  const valid = clone(runFixture);
  valid.generatedAt = "2026-09-01T13:00:00.123456789+02:00";
  assert.doesNotThrow(() => validateDiscoveryRun(seal(valid)));
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
