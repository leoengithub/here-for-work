import { requireSuccessfulRelease } from "./popup-result";

export {};

const status = document.querySelector<HTMLElement>("[data-status]");
const inspect = document.querySelector<HTMLButtonElement>("[data-inspect]");
const release = document.querySelector<HTMLButtonElement>("[data-release]");
const installation = document.querySelector<HTMLInputElement>("[data-installation]");
const copyInstallation = document.querySelector<HTMLButtonElement>("[data-copy-installation]");

async function activeTab(): Promise<chrome.tabs.Tab | null> {
  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
  return tab ?? null;
}

void chrome.runtime.sendMessage({ type: "connection_status" }).then((result) => {
  if (!status) return;
  status.textContent = result?.connected ? "Connected to HereForWork" : "Open HereForWork to connect";
  if (installation) installation.value = result?.installationId ?? "";
});

copyInstallation?.addEventListener("click", () => {
  if (!installation?.value) return;
  void navigator.clipboard.writeText(installation.value).then(() => {
    if (status) status.textContent = "Installation ID copied.";
  });
});

inspect?.addEventListener("click", () => {
  void activeTab().then(async (tab) => {
    if (!tab?.id) throw new Error("No active tab.");
    const result = await chrome.runtime.sendMessage({ type: "manual_inspect", tabId: tab.id });
    if (status) status.textContent = result?.ok ? `${result.snapshot.fields.length} fields inspected; final action is guarded.` : result?.error;
  }).catch((error) => {
    if (status) status.textContent = String(error);
  });
});

release?.addEventListener("click", () => {
  void activeTab().then(async (tab) => {
    if (!tab?.id) throw new Error("No active tab.");
    const result = await chrome.runtime.sendMessage({ type: "manual_release", tabId: tab.id });
    requireSuccessfulRelease(result);
    if (status) status.textContent = "Released for your review. HereForWork cannot trigger the final action.";
  }).catch((error) => {
    if (status) status.textContent = String(error);
  });
});
