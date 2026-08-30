import { describe, expect, it, vi } from "vitest";

import { retryMessage } from "./message-retry";

describe("retryMessage", () => {
  it("retries until a newly injected content script responds", async () => {
    const send = vi.fn()
      .mockRejectedValueOnce(new Error("Receiving end does not exist"))
      .mockResolvedValueOnce({ ok: true });
    const delay = vi.fn().mockResolvedValue(undefined);

    await expect(retryMessage(send, delay)).resolves.toEqual({ ok: true });
    expect(delay).toHaveBeenCalledOnce();
  });

  it("returns the final transport error after the bound", async () => {
    const error = new Error("Receiving end does not exist");
    await expect(retryMessage(
      async () => { throw error; },
      async () => undefined,
      2,
    )).rejects.toBe(error);
  });
});
