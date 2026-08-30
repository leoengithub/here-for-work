import { describe, expect, it, vi } from "vitest";

import { waitForNonEmptyForm } from "./form-readiness";

describe("waitForNonEmptyForm", () => {
  it("waits for a dynamically mounted form", async () => {
    const inspect = vi.fn()
      .mockResolvedValueOnce({ fields: [] })
      .mockResolvedValueOnce({ fields: [{ id: "name" }] });
    const delay = vi.fn().mockResolvedValue(undefined);

    await expect(waitForNonEmptyForm(inspect, delay)).resolves.toEqual({ fields: [{ id: "name" }] });
    expect(delay).toHaveBeenCalledOnce();
  });

  it("rejects a page that never exposes application fields", async () => {
    await expect(waitForNonEmptyForm(
      async () => ({ fields: [] }),
      async () => undefined,
      2,
    )).rejects.toThrow("No application fields appeared");
  });
});
