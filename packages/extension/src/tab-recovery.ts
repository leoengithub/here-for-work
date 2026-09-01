import { selectExpectedTab } from "./tab-selection";
import type { TabCandidate } from "./tab-selection";

export const EXPECTED_TAB_POLL_ATTEMPTS = 20;
export const EXPECTED_TAB_POLL_INTERVAL_MS = 250;

interface CreatedTab extends TabCandidate {
  id?: number;
  pendingUrl?: string;
}

export interface ExpectedTabAccess {
  queryHttpsTabs: () => Promise<TabCandidate[]>;
  createBackgroundTab: (url: string) => Promise<CreatedTab>;
  delay: () => Promise<void>;
}

/**
 * Reattaches to an exact application page opened by HereForWork. If Chrome did
 * not deliver the native launch request to the already-running profile, the
 * approved extension opens the same URL in its own profile as a bounded
 * recovery. The caller's extension + installation identity has already been
 * approved by the native bridge before it can receive this command.
 */
export async function resolveExpectedTabId(
  expectedUrl: string,
  access: ExpectedTabAccess,
  attempts = EXPECTED_TAB_POLL_ATTEMPTS,
): Promise<number> {
  for (let attempt = 0; attempt < attempts; attempt += 1) {
    const match = selectExpectedTab(await access.queryHttpsTabs(), expectedUrl);
    if (match?.id) return match.id;
    if (attempt < attempts - 1) await access.delay();
  }

  let created: CreatedTab;
  try {
    created = await access.createBackgroundTab(expectedUrl);
  } catch {
    throw new Error("The approved Chrome profile could not open the application tab.");
  }
  const createdUrl = created.url || created.pendingUrl;
  if (!created.id || !createdUrl || !selectExpectedTab([{ ...created, url: createdUrl }], expectedUrl)) {
    throw new Error("The approved Chrome profile did not return the expected application tab.");
  }
  return created.id;
}
