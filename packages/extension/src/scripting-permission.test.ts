import { describe, expect, it } from "vitest";

import { requireScriptingApi } from "./scripting-permission";

describe("main-world scripting permission", () => {
  it("fails closed when the API is unavailable", () => {
    expect(() => requireScriptingApi(undefined)).toThrow("finalization_guard_permission_required");
  });

  it("returns the explicitly available API", () => {
    const api = { executeScript: () => Promise.resolve([]) } as unknown as typeof chrome.scripting;
    expect(requireScriptingApi(api)).toBe(api);
  });
});
