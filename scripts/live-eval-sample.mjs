#!/usr/bin/env node
/**
 * Live sample for T1: evaluate exactly three authorized role URLs via
 * evaluation.full_ag.run.v1 and write typed receipt evidence.
 *
 * Does not submit applications. Does not evaluate any other roles.
 */

import { mkdir, writeFile } from "node:fs/promises";
import { spawn } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const adapter = resolve(root, "packages/career-ops-adapter/adapter.mjs");
const careerOpsRoot = process.env.HFW_CAREER_OPS_ROOT || "/Users/leo/Work/career-ops";
const trackerIndex = process.env.HFW_CAREER_OPS_INDEX || resolve(careerOpsRoot, "data/applications.db");
const staging = process.env.HFW_CAREER_OPS_STAGING || resolve(root, "evidence/HFW-EVAL-EXEC-01/staging");
const evidenceDir = resolve(root, "evidence/HFW-EVAL-EXEC-01");

const SAMPLE = [
  {
    roleId: "5b2a8484-d54b-40f2-b475-3aa572cdc803",
    company: "BCNC GROUP",
    title: "Senior React Frontend Developer",
    url: "https://www.linkedin.com/jobs/view/4441477799/",
  },
  {
    roleId: "35f21536-fc15-4d9c-9c9e-f4bcc1cccb1f",
    company: "HASH",
    title: "Frontend Engineer, EU Remote",
    url: "https://hash.ai/careers",
  },
  {
    roleId: "93d4ef6b-0e49-4513-baf8-58d47033f9fb",
    company: "Nunegal Consulting",
    title: "Desarrollador/a React",
    url: "https://www.linkedin.com/jobs/view/4450479500/",
  },
];

const ONLY = new Set(
  (process.env.HFW_LIVE_EVAL_ONLY || "")
    .split(",")
    .map((value) => value.trim())
    .filter(Boolean),
);
const SELECTED = ONLY.size > 0 ? SAMPLE.filter((role) => ONLY.has(role.roleId)) : SAMPLE;

function request(operation, input = {}) {
  return new Promise((resolvePromise, reject) => {
    const child = spawn(process.execPath, [adapter], {
      env: {
        ...process.env,
        HFW_CAREER_OPS_ROOT: careerOpsRoot,
        HFW_CAREER_OPS_INDEX: trackerIndex,
        HFW_CAREER_OPS_STAGING: staging,
      },
      stdio: ["pipe", "pipe", "pipe"],
    });
    let stdout = "";
    let stderr = "";
    child.stdout.on("data", (chunk) => {
      stdout += chunk.toString("utf8");
    });
    child.stderr.on("data", (chunk) => {
      stderr += chunk.toString("utf8");
    });
    child.on("error", reject);
    child.on("close", (code) => {
      const line = stdout.trim().split("\n").filter(Boolean).at(-1);
      if (!line) {
        reject(new Error(`adapter produced no response (code=${code}): ${stderr.slice(0, 500)}`));
        return;
      }
      try {
        resolvePromise(JSON.parse(line));
      } catch (error) {
        reject(new Error(`adapter returned non-JSON: ${line.slice(0, 500)}`));
      }
    });
    child.stdin.write(JSON.stringify({
      id: `${operation}-${Date.now()}`,
      protocolVersion: 1,
      operation,
      input,
    }));
    child.stdin.write("\n");
    child.stdin.end();
  });
}

await mkdir(evidenceDir, { recursive: true });
await mkdir(staging, { recursive: true });

const capabilities = await request("capabilities.get", {});
if (!capabilities.ok) {
  throw new Error(`capabilities.get failed: ${JSON.stringify(capabilities)}`);
}
const fullAg = capabilities.result.capabilities.find((item) => item.id === "evaluation.full_ag.run.v1");
if (fullAg?.status !== "degraded" || !fullAg.compatibilityFingerprint) {
  throw new Error(`evaluation.full_ag.run.v1 is not usable: ${JSON.stringify(fullAg)}`);
}

const outcomes = [];
for (const role of SELECTED) {
  process.stdout.write(`Evaluating ${role.company} — ${role.title}...\n`);
  const startedAt = new Date().toISOString();
  const response = await request("evaluation.full_ag.run.v1", {
    url: role.url,
    company: role.company,
    title: role.title,
    compatibilityFingerprint: fullAg.compatibilityFingerprint,
    source: "HereForWork-live-sample",
  });
  const finishedAt = new Date().toISOString();
  const outcome = {
    roleId: role.roleId,
    company: role.company,
    title: role.title,
    url: role.url,
    startedAt,
    finishedAt,
    ok: Boolean(response.ok),
    error: response.ok ? null : response.error ?? response,
    receipt: response.ok ? response.result : null,
  };
  outcomes.push(outcome);
  await writeFile(
    resolve(evidenceDir, `receipt-${role.roleId}.json`),
    `${JSON.stringify(outcome, null, 2)}\n`,
  );
  process.stdout.write(
    response.ok
      ? `  ok tracker=${response.result.canonical.trackerId} score=${response.result.canonical.score}\n`
      : `  FAIL ${JSON.stringify(response.error ?? response).slice(0, 400)}\n`,
  );
}

const summary = {
  generatedAt: new Date().toISOString(),
  upstreamRevision: capabilities.result.upstreamRevision,
  fullAgCapability: fullAg,
  outcomes: outcomes.map((item) => ({
    roleId: item.roleId,
    company: item.company,
    title: item.title,
    ok: item.ok,
    trackerId: item.receipt?.canonical?.trackerId ?? null,
    score: item.receipt?.canonical?.score ?? null,
    reportPath: item.receipt?.report?.path ?? null,
    stateHint: item.ok ? "receipt_ready_for_sync" : "needs_attention",
    error: item.error,
  })),
};
await writeFile(resolve(evidenceDir, "summary.json"), `${JSON.stringify(summary, null, 2)}\n`);
process.stdout.write(`${JSON.stringify(summary, null, 2)}\n`);
if (outcomes.some((item) => !item.ok)) process.exit(1);
