import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdtemp, readFile, realpath, writeFile } from "node:fs/promises";
import { builtinModules } from "node:module";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { pathToFileURL } from "node:url";

import { buildCareerOpsAdapter } from "../../scripts/build-career-ops-adapter.mjs";

const nodeRuntimeModules = new Set(builtinModules.flatMap((specifier) => [specifier, `node:${specifier}`]));

function runNode(args, { cwd, input } = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, args, {
      cwd,
      env: { PATH: process.env.PATH },
      stdio: ["pipe", "pipe", "pipe"],
    });
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
    child.stdin.end(input);
  });
}

test("packaged adapter is self-contained and parses profile YAML outside the repository", async () => {
  const isolatedDirectory = await realpath(await mkdtemp(join(tmpdir(), "hfw-packaged-adapter-")));
  const bundledAdapter = join(isolatedDirectory, "adapter.mjs");
  const { metafile } = await buildCareerOpsAdapter(bundledAdapter);
  const runtimeImports = Object.values(metafile.outputs)
    .flatMap((output) => output.imports)
    .filter(({ external }) => external)
    .map(({ path }) => path);
  assert.ok(runtimeImports.length > 0, "expected the bundle to retain Node built-in imports");
  assert.ok(runtimeImports.every((specifier) => nodeRuntimeModules.has(specifier)), runtimeImports.join(", "));

  const request = JSON.stringify({
    id: "isolated-capabilities",
    protocolVersion: 1,
    operation: "capabilities.get",
    input: {},
  });
  const capabilities = await runNode([bundledAdapter], {
    cwd: isolatedDirectory,
    input: `${request}\n`,
  });
  assert.equal(capabilities.status, 0, capabilities.stderr);
  assert.equal(JSON.parse(capabilities.stdout).result.sourceOfTruth.profileFacts, "career-ops");

  const profileRunner = join(isolatedDirectory, "profile-yaml-check.mjs");
  const profileYaml = `compensation:
  application_answer:
    currency: EUR
    basis: gross
    period: annual
    minimum: 50000
    maximum: 55000
    single_value: 52000
    modalities: [employee, eor, contractor, b2b]
    allow_currency_conversion: false
    allow_period_conversion: false
`;
  await writeFile(profileRunner, `
    import { compensationApplicationAnswer } from ${JSON.stringify(pathToFileURL(bundledAdapter).href)};
    const result = compensationApplicationAnswer([{
      relativePath: "config/profile.yml",
      value: ${JSON.stringify(profileYaml)},
    }]);
    process.stdout.write(JSON.stringify(result));
  `);
  const profile = await runNode([profileRunner], { cwd: isolatedDirectory });
  assert.equal(profile.status, 0, profile.stderr);
  assert.deepEqual(JSON.parse(profile.stdout), {
    currency: "EUR",
    minimum: 50000,
    maximum: 55000,
    single: 52000,
    provenance: ["config/profile.yml:compensation.application_answer"],
  });

  const bundledSource = await readFile(bundledAdapter, "utf8");
  assert.doesNotMatch(bundledSource, /from\s+["']yaml["']|import\s*\(\s*["']yaml["']/);
});
