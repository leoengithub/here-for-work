import { describe, expect, it, vi } from "vitest";
import { createInstallationIdResolver } from "./identity";

describe("installation identity", () => {
  it("coalesces concurrent first-load requests into one stored UUID", async () => {
    const write = vi.fn(async () => undefined);
    const create = vi.fn(() => "197f9e80-f9fa-4237-9113-83c9746c440c");
    const resolve = createInstallationIdResolver(async () => undefined, write, create);

    const [first, second] = await Promise.all([resolve(), resolve()]);

    expect(first).toBe(second);
    expect(create).toHaveBeenCalledTimes(1);
    expect(write).toHaveBeenCalledTimes(1);
  });

  it("reuses a valid stored identity without writing", async () => {
    const stored = "6f2885ee-cebf-478c-bd89-9926e5e19f64";
    const write = vi.fn(async () => undefined);
    const resolve = createInstallationIdResolver(async () => stored, write, crypto.randomUUID);

    await expect(resolve()).resolves.toBe(stored);
    expect(write).not.toHaveBeenCalled();
  });
});
