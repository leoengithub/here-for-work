import { readFileSync, readdirSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";

import { describe, expect, it } from "vitest";

const sourceDirectory = dirname(fileURLToPath(import.meta.url));
const extensionDirectory = dirname(sourceDirectory);

function productionTypeScriptFiles(directory: string): string[] {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) return productionTypeScriptFiles(path);
    return entry.isFile() && entry.name.endsWith(".ts") && !entry.name.endsWith(".test.ts") ? [path] : [];
  });
}

describe("extension permissions", () => {
  it("declares only the fixed permissions and does not widen host access", () => {
    const manifest = JSON.parse(readFileSync(join(extensionDirectory, "manifest.json"), "utf8"));
    expect(manifest.permissions).toEqual(["nativeMessaging", "scripting", "storage"]);
    expect(manifest.host_permissions).toEqual(["<all_urls>"]);
  });

  it("uses chrome.scripting only in the service worker's main-world guard boundary", () => {
    const usages = productionTypeScriptFiles(sourceDirectory).flatMap((path) => {
      const count = readFileSync(path, "utf8").match(/(?<!typeof )chrome\.scripting/g)?.length ?? 0;
      return count ? [{ path: relative(sourceDirectory, path), count }] : [];
    });
    expect(usages).toEqual([{ path: "service-worker.ts", count: 2 }]);
    const serviceWorker = readFileSync(join(sourceDirectory, "service-worker.ts"), "utf8");
    expect(serviceWorker).toMatch(/installMainGuard[\s\S]*requireScriptingApi\(chrome\.scripting\)/);
    expect(serviceWorker).toMatch(/releaseMainGuard[\s\S]*requireScriptingApi\(chrome\.scripting\)/);
  });
});
