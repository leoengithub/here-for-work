#!/usr/bin/env node

import { mkdir } from "node:fs/promises";
import { builtinModules } from "node:module";
import { dirname, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { build } from "esbuild";

const repositoryRoot = fileURLToPath(new URL("../", import.meta.url));
const entryPoint = resolve(repositoryRoot, "packages/career-ops-adapter/adapter.mjs");
const defaultOutput = resolve(repositoryRoot, "packages/career-ops-adapter/dist/adapter.mjs");
const nodeRuntimeModules = new Set(builtinModules.flatMap((specifier) => [specifier, `node:${specifier}`]));

export async function buildCareerOpsAdapter(outputPath = defaultOutput) {
  const outfile = resolve(outputPath);
  await mkdir(dirname(outfile), { recursive: true });
  const result = await build({
    entryPoints: [entryPoint],
    outfile,
    bundle: true,
    platform: "node",
    format: "esm",
    target: "node22",
    banner: {
      js: 'import { createRequire as __hfwCreateRequire } from "node:module"; const require = __hfwCreateRequire(import.meta.url);',
    },
    legalComments: "none",
    metafile: true,
  });
  const unexpectedRuntimeImports = Object.values(result.metafile.outputs)
    .flatMap((output) => output.imports)
    .filter(({ external, path }) => external && !nodeRuntimeModules.has(path));
  if (unexpectedRuntimeImports.length > 0) {
    const imports = unexpectedRuntimeImports.map(({ path }) => path).join(", ");
    throw new Error(`Bundled career-ops adapter retains external runtime imports: ${imports}`);
  }
  return { outfile, metafile: result.metafile };
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const outputPath = process.argv[2] ? resolve(process.argv[2]) : defaultOutput;
  const { outfile } = await buildCareerOpsAdapter(outputPath);
  process.stdout.write(`${outfile}\n`);
}
