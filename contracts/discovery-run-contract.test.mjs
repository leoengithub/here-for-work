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

test("a discovery run preserves scored and explicitly unscored career-ops states", () => {
  const parsed = parseDiscoveryArtifact(runFixture);

  assert.equal(parsed.kind, "discovery_run");
  assert.deepEqual(parsed.value.findings.map((finding) => finding.matchScore.status), ["scored", "not_scored"]);
  assert.equal(parsed.value.findings[0].matchScore.scale, "career_ops_1_to_5");
  assert.equal(parsed.value.findings[0].matchScore.value, 4.5);
  assert.equal(parsed.value.findings[1].matchScore.value, undefined);
});

test("integrity is deterministic across object and finding order", () => {
  const reordered = reverseObjectKeys(clone(runFixture));
  reordered.findings.reverse();

  assert.equal(computeDiscoveryRunDigest(reordered), runFixture.integrity.digest);
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
