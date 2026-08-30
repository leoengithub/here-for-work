import { BROWSER_COMMAND_TYPES } from "./contracts";
import type { BrowserCommand, FillPlan } from "./contracts";
import { createInstallationIdResolver } from "./identity";
import { retryMessage } from "./message-retry";
import { isConfirmedNativeResponse, postMessageSafely } from "./native-port";
import { selectExpectedTab } from "./tab-selection";

const HOST_NAME = "com.hereforwork.bridge";
const POLL_INTERVAL_MS = 750;
let nativePort: chrome.runtime.Port | null = null;
let pollTimer: ReturnType<typeof setTimeout> | null = null;
let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
let awaitingResponse = false;
let nativeConfirmed = false;

const installationId = createInstallationIdResolver(
  async () => (await chrome.storage.local.get("installationId")).installationId,
  async (value) => chrome.storage.local.set({ installationId: value }),
  () => crypto.randomUUID(),
);

function scheduleReconnect(): void {
  if (reconnectTimer) clearTimeout(reconnectTimer);
  reconnectTimer = setTimeout(() => {
    reconnectTimer = null;
    connect();
  }, 2_000);
}

function resetNativePort(port: chrome.runtime.Port | null): void {
  if (port && nativePort !== port) return;
  nativePort = null;
  nativeConfirmed = false;
  awaitingResponse = false;
  if (pollTimer) {
    clearTimeout(pollTimer);
    pollTimer = null;
  }
  scheduleReconnect();
}

async function postNative(message: Record<string, unknown>): Promise<boolean> {
  const port = nativePort;
  if (!port) return false;
  try {
    const value = await installationId();
    if (nativePort !== port) return false;
    if (!postMessageSafely(port, { ...message, installationId: value })) {
      resetNativePort(port);
      return false;
    }
    return true;
  } catch {
    resetNativePort(port);
    return false;
  }
}

function isBrowserCommand(message: unknown): message is BrowserCommand {
  if (!message || typeof message !== "object") return false;
  const candidate = message as Partial<BrowserCommand>;
  return candidate.protocolVersion === 1
    && candidate.ok === true
    && candidate.type === "command"
    && typeof candidate.commandId === "string"
    && typeof candidate.sessionId === "string"
    && BROWSER_COMMAND_TYPES.some((type) => type === candidate.commandType)
    && Boolean(candidate.payload)
    && typeof candidate.payload === "object";
}

function schedulePoll(): void {
  if (pollTimer) clearTimeout(pollTimer);
  pollTimer = setTimeout(() => {
    pollTimer = null;
    if (!nativePort || awaitingResponse) {
      schedulePoll();
      return;
    }
    awaitingResponse = true;
    void postNative({ protocolVersion: 1, type: "poll" }).then((sent) => {
      if (!sent) awaitingResponse = false;
    });
  }, POLL_INTERVAL_MS);
}

async function activeTabId(): Promise<number> {
  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
  if (!tab?.id) throw new Error("No active application page.");
  return tab.id;
}

async function expectedTabId(expectedUrl: string): Promise<number> {
  for (let attempt = 0; attempt < 20; attempt += 1) {
    const tabs = await chrome.tabs.query({ url: ["https://*/*"] });
    const match = selectExpectedTab(tabs, expectedUrl);
    if (match?.id) return match.id;
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
  throw new Error("The selected role is not open in this Chrome profile.");
}

function sessionTabKey(sessionId: string): string {
  return `browser-session:${sessionId}`;
}

async function rememberedTabId(sessionId: string): Promise<number> {
  const key = sessionTabKey(sessionId);
  const stored = await chrome.storage.session.get(key);
  const tabId = stored[key];
  if (typeof tabId !== "number") throw new Error("The inspected ATS tab is no longer available.");
  return tabId;
}

async function executeCommand(command: BrowserCommand): Promise<unknown> {
  if (command.commandType === "inspect_request") {
    const expectedUrl = command.payload.expectedUrl;
    const tabId = typeof expectedUrl === "string" ? await expectedTabId(expectedUrl) : await activeTabId();
    const result = await retryMessage(
      () => chrome.tabs.sendMessage(tabId, { type: "inspect", allowEmpty: typeof expectedUrl === "string" }),
      () => new Promise((resolve) => setTimeout(resolve, 250)),
    );
    if (result?.ok) await chrome.storage.session.set({ [sessionTabKey(command.sessionId)]: tabId });
    return result;
  }
  if (command.commandType === "fill_plan") {
    const tabId = await rememberedTabId(command.sessionId);
    const plan = command.payload.plan as FillPlan | undefined;
    if (!plan) throw new Error("Fill plan is missing.");
    return chrome.tabs.sendMessage(tabId, { type: "fill_plan", plan });
  }
  if (command.commandType === "release_for_review") {
    let tabId: number;
    try {
      tabId = await rememberedTabId(command.sessionId);
    } catch {
      const expectedUrl = command.payload.expectedUrl;
      tabId = typeof expectedUrl === "string" ? await expectedTabId(expectedUrl) : await activeTabId();
    }
    const result = await chrome.tabs.sendMessage(tabId, { type: "release_for_review" });
    await chrome.storage.session.remove(sessionTabKey(command.sessionId));
    return result;
  }
  throw new Error("Unsupported browser command.");
}

function returnCommandResult(command: BrowserCommand): void {
  void executeCommand(command).then((result) => {
    void postNative({
      protocolVersion: 1,
      type: "command_result",
      commandId: command.commandId,
      sessionId: command.sessionId,
      commandType: command.commandType,
      status: "completed",
      result,
    });
  }).catch((error) => {
    void postNative({
      protocolVersion: 1,
      type: "command_result",
      commandId: command.commandId,
      sessionId: command.sessionId,
      commandType: command.commandType,
      status: "failed",
      error: error instanceof Error ? error.message : String(error),
    });
  });
}

function connect(): void {
  if (nativePort) return;
  try {
    const port = chrome.runtime.connectNative(HOST_NAME);
    nativePort = port;
    nativeConfirmed = false;
    port.onDisconnect.addListener(() => {
      void chrome.runtime.lastError?.message;
      resetNativePort(port);
    });
    port.onMessage.addListener((message) => {
      if (nativePort !== port) return;
      awaitingResponse = false;
      nativeConfirmed = isConfirmedNativeResponse(message);
      if (!nativeConfirmed) {
        schedulePoll();
        return;
      }
      if (message.type === "command" && isBrowserCommand(message)) returnCommandResult(message);
      schedulePoll();
    });
    void postNative({ protocolVersion: 1, type: "hello" });
  } catch {
    resetNativePort(null);
  }
}

async function wakeNativeConnection(): Promise<void> {
  const hadPort = Boolean(nativePort);
  connect();
  if (!hadPort) return;
  awaitingResponse = false;
  await postNative({ protocolVersion: 1, type: "hello" });
}

chrome.runtime.onInstalled.addListener(connect);
chrome.runtime.onStartup.addListener(connect);
connect();

chrome.runtime.onMessage.addListener((message, _sender, respond) => {
  if (message?.type === "connection_status") {
    void (async () => {
      await wakeNativeConnection();
      await new Promise((resolve) => setTimeout(resolve, 250));
      const value = await installationId();
      respond({
        connected: nativeConfirmed,
        extensionId: chrome.runtime.id,
        installationId: value,
      });
    })();
    return true;
  }
});
