import { mkdirSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const outputDirectory = resolve(root, "src/generated");
const contracts = [
  ["answer-draft.schema.json", "answer-draft.d.ts"],
  ["browser-bridge.schema.json", "browser-bridge.d.ts"],
  ["career-ops-capabilities.schema.json", "career-ops-capabilities.d.ts"],
  ["evaluation-result.schema.json", "evaluation-result.d.ts"],
  ["discovery-dataset.schema.json", "discovery-dataset.d.ts"],
  ["discovery-run.schema.json", "discovery-run.d.ts"],
  ["preparation-result.schema.json", "preparation-result.d.ts"],
  ["provider-probe.schema.json", "provider-probe.d.ts"],
];

mkdirSync(outputDirectory, { recursive: true });

for (const [inputName, outputName] of contracts) {
  const result = spawnSync(
    resolve(root, "node_modules/.bin/json2ts"),
    [
      "--input",
      resolve(root, "contracts", inputName),
      "--output",
      resolve(outputDirectory, outputName),
      "--cwd",
      resolve(root, "contracts"),
      "--unknownAny",
      "--maxItems",
      "-1",
    ],
    { cwd: root, stdio: "inherit" },
  );
  if (result.status !== 0) process.exit(result.status ?? 1);
}
