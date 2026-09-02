export interface FocusReviewBrowser {
  getTab(tabId: number): Promise<{ windowId?: number }>;
  activateTab(tabId: number): Promise<unknown>;
  focusWindow(windowId: number): Promise<unknown>;
}

export async function focusReviewTab(
  tabId: number,
  browser: FocusReviewBrowser,
): Promise<{ ok: true }> {
  const tab = await browser.getTab(tabId).catch(() => {
    throw new Error("review_tab_unavailable");
  });
  if (typeof tab.windowId !== "number") {
    throw new Error("review_tab_unavailable");
  }
  await browser.activateTab(tabId);
  await browser.focusWindow(tab.windowId);
  return { ok: true };
}
