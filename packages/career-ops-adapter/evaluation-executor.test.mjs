import assert from "node:assert/strict";
import { mkdtemp, mkdir, writeFile, chmod } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import {
  findEvaluatedTrackerByUrl,
  fullAgCompatibilityProbe,
  offerIdFromUrl,
  runFullAgEvaluation,
} from "./evaluation-executor.mjs";

test("offer ids are stable positive integers derived from the URL", () => {
  const first = offerIdFromUrl("https://example.com/jobs/1");
  const second = offerIdFromUrl("https://example.com/jobs/1/");
  assert.equal(first, second);
  assert.ok(first >= 100_000 && first < 900_000);
  assert.notEqual(first, offerIdFromUrl("https://example.com/jobs/2"));
});

test("tracker URL matching requires an Evaluated row with the exact URL", () => {
  const records = [
    { id: 1, status: "Evaluated", notes: "Source URL: https://example.com/a" },
    { id: 2, status: "Applied", notes: "Source URL: https://example.com/b" },
  ];
  assert.equal(findEvaluatedTrackerByUrl(records, "https://example.com/a").id, 1);
  assert.equal(findEvaluatedTrackerByUrl(records, "https://example.com/b"), null);
  assert.equal(findEvaluatedTrackerByUrl([
    ...records,
    { id: 3, status: "Evaluated", notes: "https://example.com/a again" },
  ], "https://example.com/a"), null);
});

test("full A-G probe fails closed without batch surfaces", async () => {
  const root = await mkdtemp(join(tmpdir(), "hfw-full-ag-probe-"));
  const probe = await fullAgCompatibilityProbe(root);
  assert.equal(probe.fingerprint, null);
});

test("evaluation.full_ag.run.v1 fails closed on capability drift and busy batch input", async () => {
  const root = await mkdtemp(join(tmpdir(), "hfw-full-ag-run-"));
  await mkdir(join(root, "batch"), { recursive: true });
  await writeFile(join(root, "batch/batch-input.tsv"), "id\turl\tsource\tnotes\n1\thttps://other.example/job\tScan\tpending\n");
  await writeFile(join(root, "batch/batch-runner.sh"), "#!/bin/bash\nexit 0\n");
  await chmod(join(root, "batch/batch-runner.sh"), 0o755);

  await assert.rejects(
    () => runFullAgEvaluation({
      root,
      input: {
        url: "https://example.com/role",
        company: "Example",
        title: "Frontend Engineer",
        compatibilityFingerprint: "a".repeat(64),
      },
      capabilityManifest: async () => ({
        upstreamRevision: "b".repeat(40),
        capabilities: [
          {
            id: "evaluation.full_ag.run.v1",
            status: "unavailable",
            interfaceClass: "conditional",
            sourceRevision: "b".repeat(40),
            compatibilityFingerprint: null,
          },
          {
            id: "evaluation.result.read.v1",
            status: "degraded",
            compatibilityFingerprint: "c".repeat(64),
          },
        ],
      }),
      readEvaluationResult: async () => {
        throw new Error("should not read");
      },
      runTracker: async () => ({ output: "[]" }),
    }),
    /unavailable or has drifted/,
  );

  await assert.rejects(
    () => runFullAgEvaluation({
      root,
      input: {
        url: "https://example.com/role",
        company: "Example",
        title: "Frontend Engineer",
        compatibilityFingerprint: "a".repeat(64),
      },
      capabilityManifest: async () => ({
        upstreamRevision: "b".repeat(40),
        capabilities: [
          {
            id: "evaluation.full_ag.run.v1",
            status: "degraded",
            interfaceClass: "conditional",
            sourceRevision: "b".repeat(40),
            compatibilityFingerprint: "a".repeat(64),
          },
          {
            id: "evaluation.result.read.v1",
            status: "degraded",
            compatibilityFingerprint: "c".repeat(64),
          },
        ],
      }),
      readEvaluationResult: async () => {
        throw new Error("should not read");
      },
      runTracker: async () => ({ output: "[]" }),
    }),
    /other unfinished offers/,
  );
});

test("evaluation.full_ag.run.v1 reuses an existing Evaluated tracker without launching batch", async () => {
  const root = await mkdtemp(join(tmpdir(), "hfw-full-ag-reuse-"));
  await mkdir(join(root, "reports"), { recursive: true });
  const reportPath = "reports/007-example.md";
  await writeFile(join(root, reportPath), "# fixture\n");
  const receipt = await runFullAgEvaluation({
    root,
    input: {
      url: "https://example.com/role",
      company: "Example",
      title: "Frontend Engineer",
      compatibilityFingerprint: "a".repeat(64),
    },
    capabilityManifest: async () => ({
      upstreamRevision: "b".repeat(40),
      capabilities: [
        {
          id: "evaluation.full_ag.run.v1",
          status: "degraded",
          interfaceClass: "conditional",
          sourceRevision: "b".repeat(40),
          compatibilityFingerprint: "a".repeat(64),
        },
        {
          id: "evaluation.result.read.v1",
          status: "degraded",
          compatibilityFingerprint: "c".repeat(64),
        },
      ],
    }),
    readEvaluationResult: async (input) => {
      assert.equal(input.trackerId, 7);
      assert.equal(input.reportPath, reportPath);
      assert.equal(input.compatibilityFingerprint, "c".repeat(64));
      return {
        contract: "hereforwork.career-ops-evaluation-result",
        schemaVersion: 1,
        upstreamRevision: "b".repeat(40),
        compatibilityFingerprint: "c".repeat(64),
        report: { path: reportPath, sha256: input.reportSha256 },
        role: { company: "Example", title: "Frontend Engineer" },
        canonical: {
          trackerId: 7,
          status: "Evaluated",
          score: 4.2,
          reportPath,
        },
        evaluation: { score: 4.2 },
      };
    },
    runTracker: async () => ({
      output: JSON.stringify([
        {
          id: 7,
          status: "Evaluated",
          report: "[007](reports/007-example.md)",
          notes: "Source URL: https://example.com/role",
        },
      ]),
    }),
  });
  assert.equal(receipt.contract, "hereforwork.career-ops-evaluation-receipt");
  assert.equal(receipt.canonical.trackerId, 7);
  assert.equal(receipt.execution.executed, false);
  assert.equal(receipt.execution.surface, "batch/batch-runner.sh");
});
