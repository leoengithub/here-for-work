#!/usr/bin/env node
/**
 * Apply typed evaluation receipts from the live sample into the personal
 * HereForWork evaluation_sync / evaluation_receipts tables for the three
 * authorized role IDs only.
 */

import { createHash } from "node:crypto";
import { readFile, writeFile, mkdir } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const evidenceDir = resolve(root, "evidence/HFW-EVAL-EXEC-01");
const db = process.env.HFW_LIVE_DB
  || "/Users/leo/Library/Application Support/com.hereforwork.desktop/here-for-work.sqlite3";

const ROLE_IDS = [
  "5b2a8484-d54b-40f2-b475-3aa572cdc803",
  "35f21536-fc15-4d9c-9c9e-f4bcc1cccb1f",
  "93d4ef6b-0e49-4513-baf8-58d47033f9fb",
];

function sqlLiteral(value) {
  if (value === null || value === undefined) return "NULL";
  return `'${String(value).replaceAll("'", "''")}'`;
}

function runSql(sql) {
  const result = spawnSync("sqlite3", ["-separator", "\t", db], {
    input: `${sql}\n`,
    encoding: "utf8",
  });
  if (result.status !== 0) {
    throw new Error(result.stderr || result.stdout || "sqlite3 failed");
  }
  return result.stdout.trim();
}

function decideState(evaluation) {
  const needsDecision = evaluation.finalDecision === "Research first"
    || evaluation.confidence !== "High"
    || (evaluation.blockers?.length ?? 0) > 0
    || (evaluation.gaps?.length ?? 0) > 0
    || (evaluation.materialUncertainty?.notEvaluatedRiskSignals?.length ?? 0) > 0;
  if (evaluation.finalDecision === "Skip" || evaluation.legitimacyTier === "Suspicious") {
    return { state: "hidden", reason: "canonical_evaluation_not_viable" };
  }
  if (needsDecision) {
    return { state: "needs_decision", reason: "canonical_evaluation_verified" };
  }
  return { state: "ready", reason: "canonical_evaluation_verified" };
}

await mkdir(evidenceDir, { recursive: true });
const syncOutcomes = [];

for (const roleId of ROLE_IDS) {
  const receiptPath = resolve(evidenceDir, `receipt-${roleId}.json`);
  const payload = JSON.parse(await readFile(receiptPath, "utf8"));
  if (!payload.ok || !payload.receipt) {
    syncOutcomes.push({
      roleId,
      ok: false,
      error: payload.error ?? "missing receipt",
      evaluationSync: runSql(
        `SELECT state, reason, current_receipt_key FROM evaluation_sync WHERE role_id = ${sqlLiteral(roleId)};`,
      ),
    });
    continue;
  }
  const receipt = payload.receipt;
  const meta = runSql(
    `SELECT company, title, normalized_key FROM roles WHERE id = ${sqlLiteral(roleId)};`,
  );
  const [company, title, normalizedKey] = meta.split("\t");
  const occurrences = runSql(
    `SELECT group_concat(identity, '|') FROM (
       SELECT source_id || ':' || source_role_id || ':' || payload_hash AS identity
         FROM source_occurrences WHERE role_id = ${sqlLiteral(roleId)}
        ORDER BY source_id, source_role_id
     );`,
  );
  const sourceHash = createHash("sha256")
    .update(`${normalizedKey}\n${occurrences || ""}`)
    .digest("hex");

  const decision = decideState(receipt.evaluation);
  const receiptIdentity = {
    roleId,
    trackerId: receipt.canonical.trackerId,
    report: receipt.report,
    upstreamRevision: receipt.upstreamRevision,
    compatibilityFingerprint: receipt.compatibilityFingerprint,
    sourceIdentityHash: sourceHash,
  };
  const receiptKey = createHash("sha256")
    .update(JSON.stringify(receiptIdentity))
    .digest("hex");
  const now = new Date().toISOString();
  const strengths = JSON.stringify((receipt.evaluation.strengths || []).slice(0, 3));
  const blockers = JSON.stringify((receipt.evaluation.blockers || []).slice(0, 3));
  const gaps = JSON.stringify((receipt.evaluation.gaps || []).slice(0, 3));
  const materialUncertainty = JSON.stringify({
    confidence: receipt.evaluation.materialUncertainty?.confidence ?? receipt.evaluation.confidence,
    authorizationQuestion: receipt.evaluation.materialUncertainty?.authorizationQuestion
      ?? receipt.evaluation.authorization?.question,
    notEvaluatedRiskSignals: receipt.evaluation.materialUncertainty?.notEvaluatedRiskSignals ?? [],
  });

  const sql = `
BEGIN;
UPDATE roles
   SET canonical_tracker_id = ${Number(receipt.canonical.trackerId)},
       canonical_status = ${sqlLiteral(receipt.canonical.status)},
       updated_at = ${sqlLiteral(now)}
 WHERE id = ${sqlLiteral(roleId)};
INSERT INTO evaluation_receipts(
  receipt_key, role_id, tracker_id, report_path, report_hash,
  upstream_revision, compatibility_fingerprint, source_identity_hash,
  native_score, final_decision, legitimacy, risk_level, strengths_json,
  blockers_json, gaps_json, compensation, authorization_confidence,
  authorization_question, material_uncertainty_json, created_at
) VALUES (
  ${sqlLiteral(receiptKey)},
  ${sqlLiteral(roleId)},
  ${Number(receipt.canonical.trackerId)},
  ${sqlLiteral(receipt.report.path)},
  ${sqlLiteral(receipt.report.sha256)},
  ${sqlLiteral(receipt.upstreamRevision)},
  ${sqlLiteral(receipt.compatibilityFingerprint)},
  ${sqlLiteral(sourceHash)},
  ${Number(receipt.evaluation.score)},
  ${sqlLiteral(receipt.evaluation.finalDecision)},
  ${sqlLiteral(receipt.evaluation.legitimacyTier)},
  ${sqlLiteral(receipt.evaluation.riskLevel)},
  ${sqlLiteral(strengths)},
  ${sqlLiteral(blockers)},
  ${sqlLiteral(gaps)},
  ${sqlLiteral(receipt.evaluation.compensation?.advertised ?? null)},
  ${sqlLiteral(receipt.evaluation.authorization?.confidence)},
  ${sqlLiteral(receipt.evaluation.authorization?.question)},
  ${sqlLiteral(materialUncertainty)},
  ${sqlLiteral(now)}
)
ON CONFLICT(receipt_key) DO UPDATE SET risk_level = excluded.risk_level
  WHERE evaluation_receipts.risk_level IS NULL;
UPDATE evaluation_sync
   SET state = ${sqlLiteral(decision.state)},
       reason = ${sqlLiteral(decision.reason)},
       current_receipt_key = ${sqlLiteral(receiptKey)},
       input_hash = ${sqlLiteral(receiptKey)},
       lease_expires_at = NULL,
       updated_at = ${sqlLiteral(now)}
 WHERE role_id = ${sqlLiteral(roleId)};
COMMIT;
SELECT e.state, e.reason, e.current_receipt_key, r.canonical_tracker_id, r.canonical_status
  FROM evaluation_sync e
  JOIN roles r ON r.id = e.role_id
 WHERE e.role_id = ${sqlLiteral(roleId)};
`;
  const statusLine = runSql(sql);
  const [state, reason, currentReceiptKey, trackerId, canonicalStatus] = statusLine.split("\t");
  syncOutcomes.push({
    roleId,
    company,
    title,
    ok: true,
    evaluationSync: {
      state,
      reason,
      currentReceiptKey,
      trackerId: Number(trackerId),
      canonicalStatus,
    },
    nativeScore: receipt.evaluation.score,
    reportPath: receipt.report.path,
  });
}

await writeFile(
  resolve(evidenceDir, "evaluation-sync-states.json"),
  `${JSON.stringify({ generatedAt: new Date().toISOString(), syncOutcomes }, null, 2)}\n`,
);
console.log(JSON.stringify(syncOutcomes, null, 2));
if (syncOutcomes.some((item) => !item.ok)) process.exit(1);
