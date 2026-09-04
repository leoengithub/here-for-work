#!/usr/bin/env node
/**
 * Q7=B one-shot stuck-preparation cleanup wrapper.
 *
 * Uses the Rust store API via `reset-stuck-preparations` so selection and
 * mutation rules stay single-sourced with store tests.
 *
 * Defaults:
 *   --db  $HFW_LIVE_DB or the personal desktop sqlite path
 *   --force-role-id  coches.net Front End Engineer (authorized retry after T3)
 *
 * Examples:
 *   node scripts/reset-stuck-preparations.mjs --dry-run
 *   node scripts/reset-stuck-preparations.mjs --apply
 */

import { mkdir, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const evidenceDir = resolve(root, "evidence/HFW-PREP-CLEANUP-01");
const defaultDb =
  process.env.HFW_LIVE_DB
  || "/Users/leo/Library/Application Support/com.hereforwork.desktop/here-for-work.sqlite3";
const defaultForceRoleId = "8acfb1ba-79a1-4e22-beb0-43c9c5e813e4";

function parseArgs(argv) {
  let db = defaultDb;
  let mode = "dry-run";
  const forceRoleIds = [];
  let writeEvidence = true;
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === "--db") {
      db = argv[++i];
    } else if (arg === "--dry-run") {
      mode = "dry-run";
    } else if (arg === "--apply") {
      mode = "apply";
    } else if (arg === "--force-role-id") {
      forceRoleIds.push(argv[++i]);
    } else if (arg === "--no-evidence") {
      writeEvidence = false;
    } else if (arg === "--help" || arg === "-h") {
      console.log(`Usage: node scripts/reset-stuck-preparations.mjs [--dry-run|--apply] [--db path] [--force-role-id uuid]...`);
      process.exit(0);
    } else {
      throw new Error(`unknown argument: ${arg}`);
    }
  }
  if (forceRoleIds.length === 0) {
    forceRoleIds.push(defaultForceRoleId);
  }
  return { db, mode, forceRoleIds, writeEvidence };
}

function runSql(db, sql) {
  const result = spawnSync("sqlite3", ["-separator", "\t", db], {
    input: `${sql}\n`,
    encoding: "utf8",
  });
  if (result.status !== 0) {
    throw new Error(result.stderr || result.stdout || "sqlite3 failed");
  }
  return result.stdout.trim();
}

function snapshotRoles(db, roleIds) {
  if (roleIds.length === 0) return [];
  const literals = roleIds.map((id) => `'${id.replaceAll("'", "''")}'`).join(", ");
  const raw = runSql(
    db,
    `SELECT r.id, r.company, r.title, r.preparation_state, r.canonical_status,
            COALESCE(e.state, ''),
            COALESCE((
              SELECT pj.status || '/' || COALESCE(pj.error_class, '')
                FROM preparation_jobs pj
               WHERE pj.role_id = r.id AND pj.status != 'cancelled'
               ORDER BY pj.updated_at DESC, pj.id DESC LIMIT 1
            ), '')
       FROM roles r
       LEFT JOIN evaluation_sync e ON e.role_id = r.id
      WHERE r.id IN (${literals})
      ORDER BY r.company, r.title;`,
  );
  if (!raw) return [];
  return raw.split("\n").map((line) => {
    const [roleId, company, title, preparationState, canonicalStatus, evalSync, latestJob] =
      line.split("\t");
    return {
      roleId,
      company,
      title,
      preparationState,
      canonicalStatus: canonicalStatus || null,
      evaluationSync: evalSync || null,
      latestJob: latestJob || null,
    };
  });
}

function runBin({ db, mode, forceRoleIds }) {
  const args = [
    "run",
    "--quiet",
    "--manifest-path",
    resolve(root, "src-tauri/Cargo.toml"),
    "--bin",
    "reset-stuck-preparations",
    "--",
    "--db",
    db,
    mode === "apply" ? "--apply" : "--dry-run",
  ];
  for (const roleId of forceRoleIds) {
    args.push("--force-role-id", roleId);
  }
  const result = spawnSync("cargo", args, {
    cwd: root,
    encoding: "utf8",
  });
  if (result.status !== 0) {
    throw new Error(result.stderr || result.stdout || "cargo run failed");
  }
  return result.stdout.trim();
}

function parseJsonLines(stdout) {
  const documents = [];
  for (const line of stdout.split("\n")) {
    const trimmed = line.trim();
    if (!trimmed.startsWith("{")) continue;
    documents.push(JSON.parse(trimmed));
  }
  return documents;
}

const options = parseArgs(process.argv.slice(2));
const watchRoleIds = [
  ...new Set([
    ...options.forceRoleIds,
    "93d4ef6b-0e49-4513-baf8-58d47033f9fb", // Nunegal zombie
    "5b2a8484-d54b-40f2-b475-3aa572cdc803", // BCNC (should stay)
    "35f21536-fc15-4d9c-9c9e-f4bcc1cccb1f", // HASH (should stay)
    "db09eb41-26d8-4c11-83cd-2559ad31bff5", // Ashby Discarded
    "5583aeb6-55a6-413e-b0a2-d796c89bf48f", // KoreLabs Applied
  ]),
];

const before = snapshotRoles(options.db, watchRoleIds);
const stdout = runBin(options);
const documents = parseJsonLines(stdout);
const after = snapshotRoles(options.db, watchRoleIds);

const evidence = {
  mode: options.mode,
  db: options.db,
  forceRoleIds: options.forceRoleIds,
  before,
  after,
  toolOutput: documents,
};

console.log(JSON.stringify(evidence, null, 2));

if (options.writeEvidence) {
  await mkdir(evidenceDir, { recursive: true });
  const stamp = options.mode === "apply" ? "apply" : "dry-run";
  await writeFile(
    resolve(evidenceDir, `${stamp}-snapshot.json`),
    `${JSON.stringify(evidence, null, 2)}\n`,
    "utf8",
  );
}
