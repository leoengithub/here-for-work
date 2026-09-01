import { describe, expect, it, vi } from "vitest";

import { resolveExpectedTabId } from "./tab-recovery";

describe("resolveExpectedTabId", () => {
  it("reattaches to an exact tab delivered by the selected-profile launcher", async () => {
    const queryHttpsTabs = vi.fn()
      .mockResolvedValueOnce([])
      .mockResolvedValueOnce([{
        id: 42,
        url: "https://jobs.ashbyhq.com/acme/role?utm_source=hfw",
        active: false,
      }]);
    const createBackgroundTab = vi.fn();

    await expect(resolveExpectedTabId(
      "https://jobs.ashbyhq.com/acme/role",
      { queryHttpsTabs, createBackgroundTab, delay: async () => undefined },
      2,
    )).resolves.toBe(42);
    expect(createBackgroundTab).not.toHaveBeenCalled();
  });

  it("opens a background fallback tab from the already-approved extension profile", async () => {
    const createBackgroundTab = vi.fn().mockResolvedValue({
      id: 73,
      url: "https://jobs.ashbyhq.com/acme/role",
      active: false,
    });

    await expect(resolveExpectedTabId(
      "https://jobs.ashbyhq.com/acme/role",
      {
        queryHttpsTabs: async () => [],
        createBackgroundTab,
        delay: async () => undefined,
      },
      2,
    )).resolves.toBe(73);
    expect(createBackgroundTab).toHaveBeenCalledWith("https://jobs.ashbyhq.com/acme/role");
  });

  it("accepts Chrome's pending URL while the inactive fallback tab is still loading", async () => {
    await expect(resolveExpectedTabId(
      "https://careers.example.com/apply/42",
      {
        queryHttpsTabs: async () => [],
        createBackgroundTab: async () => ({
          id: 84,
          url: "",
          pendingUrl: "https://careers.example.com/apply/42",
          active: false,
        }),
        delay: async () => undefined,
      },
      1,
    )).resolves.toBe(84);
  });

  it("does not accept a fallback tab that navigated to a different application", async () => {
    await expect(resolveExpectedTabId(
      "https://jobs.ashbyhq.com/acme/role",
      {
        queryHttpsTabs: async () => [],
        createBackgroundTab: async () => ({
          id: 91,
          url: "https://jobs.ashbyhq.com/acme/other-role",
          active: false,
        }),
        delay: async () => undefined,
      },
      1,
    )).rejects.toThrow("did not return the expected application tab");
  });
});
