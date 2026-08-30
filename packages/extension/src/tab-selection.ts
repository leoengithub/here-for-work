export interface TabCandidate {
  id?: number;
  url?: string;
  active: boolean;
}

function comparableUrl(value: string): string {
  const url = new URL(value);
  url.hash = "";
  return url.toString();
}

export function selectExpectedTab(tabs: TabCandidate[], expectedUrl: string): TabCandidate | null {
  const matches = tabs.filter(
    (tab) => tab.id && tab.url && comparableUrl(tab.url) === comparableUrl(expectedUrl),
  );
  return matches.find((tab) => tab.active) ?? matches[matches.length - 1] ?? null;
}
