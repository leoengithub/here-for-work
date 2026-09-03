#!/usr/bin/env node

import { randomUUID } from "node:crypto";
import { access, link, mkdir, open, readFile, unlink } from "node:fs/promises";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

import {
  computeDiscoveryRunDigest,
  parseDiscoveryArtifact,
  validateDiscoveryRun,
} from "../contracts/discovery-run-contract.mjs";

export const DISCOVERY_RUN_CONTRACT = "hereforwork.discovery-run";
export const DISCOVERY_RUN_SCHEMA_VERSION = 1;
export const DEFAULT_OUTPUT_DIR = resolve("inbox/discovery-runs");
export const MAX_DRAFT_BYTES = 8 * 1024 * 1024;
export const MAX_FINDINGS = 10_000;
export const MAX_ISSUES = 1_000;
export const SUPPORTED_SOURCE_IDS = Object.freeze(["eu-job-radar", "frontend-role-scan"]);

const draftKeys = [
  "windowId",
  "runId",
  "supersedesRunId",
  "source",
  "coverage",
  "generatedAt",
  "status",
  "findings",
  "issues",
];

function invalid(message) {
  throw new Error(`Invalid discovery draft: ${message}`);
}

function assertDraftShape(draft) {
  if (!draft || typeof draft !== "object" || Array.isArray(draft)) {
    invalid("draft must be a JSON object.");
  }

  const unknown = Object.keys(draft).find((key) => !draftKeys.includes(key));
  if (unknown) invalid(`draft contains unsupported property ${unknown}.`);
  for (const key of draftKeys.filter((key) => key !== "supersedesRunId")) {
    if (!Object.hasOwn(draft, key)) invalid(`draft is missing ${key}.`);
  }
  if (Object.hasOwn(draft, "supersedesRunId") && draft.supersedesRunId === null) {
    invalid("draft.supersedesRunId must be omitted when there is no retry.");
  }
  if (!Array.isArray(draft.findings)) invalid("draft.findings must be an array.");
  if (!Array.isArray(draft.issues)) invalid("draft.issues must be an array.");
  if (!draft.source || typeof draft.source !== "object" || Array.isArray(draft.source)) {
    invalid("draft.source must be an object.");
  }
  if (!SUPPORTED_SOURCE_IDS.includes(draft.source.sourceId)) {
    invalid(`draft.source.sourceId must be one of ${SUPPORTED_SOURCE_IDS.join(", ")}.`);
  }
  if (draft.findings.length > MAX_FINDINGS) {
    invalid(`draft.findings exceeds the ${MAX_FINDINGS}-finding limit.`);
  }
  if (draft.issues.length > MAX_ISSUES) {
    invalid(`draft.issues exceeds the ${MAX_ISSUES}-issue limit.`);
  }
}

/**
 * Turn a structured source draft into a validated, digest-sealed envelope.
 *
 * The draft intentionally excludes contract, schemaVersion, and integrity. Those
 * fields are producer-owned so a scheduled source cannot accidentally publish a
 * stale digest or a different contract version.
 */
export function sealDiscoveryRunDraft(draft) {
  assertDraftShape(draft);
  const run = {
    contract: DISCOVERY_RUN_CONTRACT,
    schemaVersion: DISCOVERY_RUN_SCHEMA_VERSION,
    ...structuredClone(draft),
  };
  run.integrity = {
    algorithm: "sha256",
    canonicalization: "hfw-discovery-run-v1",
    coverage: "all_top_level_fields_except_integrity",
    digest: computeDiscoveryRunDigest(run),
  };
  return validateDiscoveryRun(run);
}

async function readInput(inputPath) {
  const bytes = await readFile(inputPath === "-" ? 0 : inputPath);
  if (bytes.byteLength > MAX_DRAFT_BYTES) {
    throw new Error(`Discovery draft exceeds the ${MAX_DRAFT_BYTES}-byte input limit.`);
  }
  let value;
  try {
    value = JSON.parse(bytes.toString("utf8"));
  } catch (error) {
    throw new Error(`Invalid discovery draft: input must be valid JSON (${error.message}).`);
  }
  return value;
}

function outputName(run) {
  return `discovery-run--${run.source.sourceId}--${run.runId}.json`;
}

async function existingPublication(finalPath, run) {
  try {
    await access(finalPath);
  } catch (error) {
    if (error.code === "ENOENT") return null;
    throw error;
  }

  let existing;
  try {
    existing = parseDiscoveryArtifact(await readFile(finalPath, "utf8")).value;
  } catch (error) {
    throw new Error(`Cannot republish ${finalPath}: existing file is invalid (${error.message}).`);
  }
  if (existing.contract !== DISCOVERY_RUN_CONTRACT
      || existing.source.sourceId !== run.source.sourceId
      || existing.runId !== run.runId) {
    throw new Error(`Cannot republish ${finalPath}: existing identity does not match.`);
  }
  if (existing.integrity.digest !== run.integrity.digest) {
    throw new Error(`Cannot republish ${finalPath}: run identity already has a different digest.`);
  }
  return { path: finalPath, replay: true, digest: run.integrity.digest };
}

/**
 * Publish one immutable envelope. A same-digest publication is an idempotent
 * replay; a same-identity/different-digest publication is rejected.
 */
export async function writeDiscoveryRun(run, outputDir = DEFAULT_OUTPUT_DIR) {
  validateDiscoveryRun(run);
  const destination = resolve(outputDir);
  await mkdir(destination, { recursive: true });
  const finalPath = resolve(destination, outputName(run));
  const replay = await existingPublication(finalPath, run);
  if (replay) return replay;

  const serialized = `${JSON.stringify(run, null, 2)}\n`;
  const temporaryPath = `${finalPath}.${process.pid}.${randomUUID()}.partial`;
  const handle = await open(temporaryPath, "wx", 0o600);
  try {
    await handle.writeFile(serialized, "utf8");
    await handle.sync();
  } finally {
    await handle.close();
  }

  try {
    // link() publishes without replacing a file created by a concurrent retry.
    // Both paths are in the same directory/filesystem; consumers only see the
    // final .json name and never the .partial staging file.
    await link(temporaryPath, finalPath);
  } catch (error) {
    if (error.code !== "EEXIST") throw error;
    const concurrent = await existingPublication(finalPath, run);
    if (concurrent) return concurrent;
    throw error;
  } finally {
    await unlink(temporaryPath).catch((error) => {
      if (error.code !== "ENOENT") throw error;
    });
  }
  return { path: finalPath, replay: false, digest: run.integrity.digest };
}

function parseArgs(argv) {
  const args = { input: "-", outputDir: DEFAULT_OUTPUT_DIR, help: false };
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === "--help" || argument === "-h") {
      args.help = true;
    } else if (argument === "--input") {
      args.input = argv[++index];
    } else if (argument === "--output-dir") {
      args.outputDir = argv[++index];
    } else {
      throw new Error(`Unknown argument ${argument}.`);
    }
  }
  if (args.input === undefined || args.outputDir === undefined) {
    throw new Error("--input and --output-dir require a value.");
  }
  return args;
}

export function usage() {
  return `Usage: node scripts/emit-discovery-run.mjs [--input FILE|-] [--output-dir DIR]

Reads one structured JSON draft (not Markdown or TSV), seals it with the
hereforwork.discovery-run v1 digest, validates it, and atomically publishes:
  DIR/discovery-run--SOURCE_ID--RUN_ID.json

The default output directory is inbox/discovery-runs. Reusing the same sourceId/runId
with the same digest is a no-op; a different digest is rejected.`;
}

async function main(argv = process.argv.slice(2)) {
  const args = parseArgs(argv);
  if (args.help) {
    console.log(usage());
    return;
  }
  const run = sealDiscoveryRunDraft(await readInput(args.input));
  const publication = await writeDiscoveryRun(run, args.outputDir);
  console.log(JSON.stringify({ ...publication, status: publication.replay ? "replayed" : "published" }));
}

const thisFile = fileURLToPath(import.meta.url);
if (process.argv[1] && resolve(process.argv[1]) === thisFile) {
  main().catch((error) => {
    console.error(error.message);
    process.exitCode = 1;
  });
}
