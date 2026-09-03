import assert from "node:assert/strict";
import { mkdtemp, readFile, readdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";

import {
  MAX_DRAFT_BYTES,
  sealDiscoveryRunDraft,
  writeDiscoveryRun,
} from "./emit-discovery-run.mjs";
import {
  computeDiscoveryRunDigest,
  validateDiscoveryRun,
} from "../contracts/discovery-run-contract.mjs";

const fixture = JSON.parse(await readFile(new URL("../examples/discovery-run.example.json", import.meta.url)));

function draftFromFixture() {
  const { contract, schemaVersion, integrity, ...draft } = structuredClone(fixture);
  return draft;
}

async function withOutput(callback) {
  const directory = await mkdtemp(join(tmpdir(), "hfw-discovery-producer-"));
  try {
    return await callback(directory);
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
}

test("seals a structured source draft into a directly importable envelope", () => {
  const run = sealDiscoveryRunDraft(draftFromFixture());

  assert.equal(run.contract, "hereforwork.discovery-run");
  assert.equal(run.schemaVersion, 1);
  assert.equal(run.integrity.digest, computeDiscoveryRunDigest(run));
  assert.doesNotThrow(() => validateDiscoveryRun(run));
});

test("does not accept producer-owned contract or integrity fields", () => {
  const draft = draftFromFixture();
  draft.integrity = {};
  assert.throws(() => sealDiscoveryRunDraft(draft), /unsupported property integrity/);
});

test("invalid identities and missing evaluation provenance fail closed", () => {
  const unsupportedSource = draftFromFixture();
  unsupportedSource.source.sourceId = "untrusted-source";
  assert.throws(() => sealDiscoveryRunDraft(unsupportedSource), /must be one of/);

  const invalidIdentity = draftFromFixture();
  invalidIdentity.findings[0].sourceId = "other-source";
  assert.throws(() => sealDiscoveryRunDraft(invalidIdentity), /sourceId must match/);

  const missingEvaluation = draftFromFixture();
  delete missingEvaluation.findings[0].matchScore;
  assert.throws(() => sealDiscoveryRunDraft(missingEvaluation), /missing matchScore/);
});

test("partial and failed producer outcomes retain contract semantics", () => {
  const partial = draftFromFixture();
  partial.status = "partial";
  partial.issues = [{
    issueId: "source-timeout",
    code: "source_timeout",
    message: "The source timed out before all pages were checked.",
    retryable: true,
  }];
  assert.doesNotThrow(() => sealDiscoveryRunDraft(partial));

  const failed = draftFromFixture();
  failed.status = "failed";
  failed.findings = [];
  failed.issues = [{
    issueId: "source-unavailable",
    code: "source_unavailable",
    message: "The source was unavailable for this coverage window.",
    retryable: true,
  }];
  assert.doesNotThrow(() => sealDiscoveryRunDraft(failed));
});

test("publishes atomically and makes exact replays no-ops", async () => {
  await withOutput(async (directory) => {
    const run = sealDiscoveryRunDraft(draftFromFixture());
    const first = await writeDiscoveryRun(run, directory);
    assert.equal(first.replay, false);
    assert.match(first.path, /discovery-run--eu-job-radar--01990df0-4d80-7ab0-b4f1-7d83a11b2c00\.json$/);
    assert.deepEqual(await readdir(directory), ["discovery-run--eu-job-radar--01990df0-4d80-7ab0-b4f1-7d83a11b2c00.json"]);

    const replay = await writeDiscoveryRun(run, directory);
    assert.equal(replay.replay, true);

    const changed = structuredClone(run);
    changed.findings[0].title = "A different source-generated title";
    changed.integrity.digest = computeDiscoveryRunDigest(changed);
    await assert.rejects(() => writeDiscoveryRun(changed, directory), /different digest/);
  });
});

test("keeps the contract's UTF-8 bounds and producer input limit", () => {
  const valid = draftFromFixture();
  valid.source.displayName = "😀".repeat(50);
  valid.findings.forEach((finding) => { finding.source = valid.source.displayName; });
  assert.doesNotThrow(() => sealDiscoveryRunDraft(valid));

  const tooLong = structuredClone(valid);
  tooLong.source.displayName = "😀".repeat(51);
  tooLong.findings.forEach((finding) => { finding.source = tooLong.source.displayName; });
  assert.throws(() => sealDiscoveryRunDraft(tooLong), /UTF-8 bytes/);
  assert.equal(MAX_DRAFT_BYTES, 8 * 1024 * 1024);
});
