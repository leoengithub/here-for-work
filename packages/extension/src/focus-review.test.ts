import { describe, expect, it, vi } from "vitest";

import { focusReviewTab } from "./focus-review";

describe("focusReviewTab", () => {
  it("only activates the released tab and focuses its existing window", async () => {
    const getTab = vi.fn(async () => ({ windowId: 73 }));
    const activateTab = vi.fn(async () => undefined);
    const focusWindow = vi.fn(async () => undefined);

    await expect(focusReviewTab(42, { getTab, activateTab, focusWindow }))
      .resolves.toEqual({ ok: true });
    expect(getTab).toHaveBeenCalledWith(42);
    expect(activateTab).toHaveBeenCalledWith(42);
    expect(focusWindow).toHaveBeenCalledWith(73);
  });

  it("fails without issuing activation when the remembered window is gone", async () => {
    const activateTab = vi.fn(async () => undefined);
    const focusWindow = vi.fn(async () => undefined);
    await expect(focusReviewTab(42, {
      getTab: async () => ({}),
      activateTab,
      focusWindow,
    })).rejects.toThrow("review_tab_unavailable");
    expect(activateTab).not.toHaveBeenCalled();
    expect(focusWindow).not.toHaveBeenCalled();
  });

  it("returns a typed failure when the remembered tab no longer exists", async () => {
    const activateTab = vi.fn(async () => undefined);
    const focusWindow = vi.fn(async () => undefined);
    await expect(focusReviewTab(42, {
      getTab: async () => { throw new Error("No tab with id: 42"); },
      activateTab,
      focusWindow,
    })).rejects.toThrow("review_tab_unavailable");
    expect(activateTab).not.toHaveBeenCalled();
    expect(focusWindow).not.toHaveBeenCalled();
  });
});
