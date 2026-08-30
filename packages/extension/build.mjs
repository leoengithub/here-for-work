import { mkdir, copyFile } from "node:fs/promises";
import { resolve } from "node:path";
import { build } from "esbuild";

const root = resolve(import.meta.dirname);
const outdir = resolve(root, "dist");
await mkdir(outdir, { recursive: true });
await build({
  entryPoints: {
    content: resolve(root, "src/content.ts"),
    "service-worker": resolve(root, "src/service-worker.ts"),
    popup: resolve(root, "src/popup.ts"),
  },
  bundle: true,
  format: "esm",
  platform: "browser",
  target: "chrome120",
  outdir,
  sourcemap: true,
});
for (const file of ["manifest.json", "popup.html", "popup.css"]) {
  await copyFile(resolve(root, file), resolve(outdir, file));
}
