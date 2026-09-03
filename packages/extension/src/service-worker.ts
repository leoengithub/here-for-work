import { BROWSER_COMMAND_TYPES } from "./contracts";
import type { BrowserCommand, FileUploadInstruction, FillPlan } from "./contracts";
import { createInstallationIdResolver } from "./identity";
import { retryMessage } from "./message-retry";
import { focusReviewTab } from "./focus-review";
import { isConfirmedNativeResponse, postMessageSafely } from "./native-port";
import { EXPECTED_TAB_POLL_INTERVAL_MS, resolveExpectedTabId } from "./tab-recovery";
import {
  beginDurableCommand,
  clearDurableCommand,
  completeDurableCommand,
} from "./durable-command-cache";
import { installMainWorldFinalizationGuard, releaseMainWorldFinalizationGuard } from "./main-world-guard";
import { requireScriptingApi } from "./scripting-permission";

const HOST_NAME = "com.hereforwork.bridge";
const POLL_INTERVAL_MS = 750;
let nativePort: chrome.runtime.Port | null = null;
let pollTimer: ReturnType<typeof setTimeout> | null = null;
let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
let awaitingResponse = false;
let nativeConfirmed = false;
let pendingResultIdentity: { commandId: string; sessionId: string; driverLeaseId: string | null } | null = null;

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
    && (candidate.driverLeaseId === null || typeof candidate.driverLeaseId === "string")
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
  return resolveExpectedTabId(expectedUrl, {
    queryHttpsTabs: () => chrome.tabs.query({ url: ["https://*/*"] }),
    createBackgroundTab: (url) => chrome.tabs.create({ url, active: false }),
    delay: () => new Promise((resolve) => setTimeout(resolve, EXPECTED_TAB_POLL_INTERVAL_MS)),
  });
}

function sessionTabKey(sessionId: string): string {
  return `browser-session:${sessionId}`;
}

function mainGuardKey(tabId: number): string {
  return `browser-main-guard:v1:${tabId}`;
}

async function installMainGuard(tabId: number): Promise<void> {
  const scripting = requireScriptingApi(chrome.scripting);
  const key = mainGuardKey(tabId);
  const stored = (await chrome.storage.local.get(key))[key] as { tabId?: number; token?: string } | undefined;
  const token = stored?.tabId === tabId && typeof stored.token === "string"
    ? stored.token
    : crypto.randomUUID();
  await scripting.executeScript({
    target: { tabId },
    world: "MAIN",
    func: installMainWorldFinalizationGuard,
    args: [token],
  });
  await chrome.storage.local.set({ [key]: { tabId, token, updatedAt: Date.now() } });
}

async function releaseMainGuard(tabId: number): Promise<void> {
  const scripting = requireScriptingApi(chrome.scripting);
  const key = mainGuardKey(tabId);
  const stored = (await chrome.storage.local.get(key))[key] as { tabId?: number; token?: string } | undefined;
  if (stored?.tabId !== tabId || typeof stored.token !== "string") throw new Error("finalization_guard_unavailable");
  const [execution] = await scripting.executeScript({
    target: { tabId },
    world: "MAIN",
    func: releaseMainWorldFinalizationGuard,
    args: [stored.token],
  });
  if (execution?.result !== true) throw new Error("finalization_guard_unavailable");
  await chrome.storage.local.remove(key);
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
    await installMainGuard(tabId);
    let result;
    try {
      result = await retryMessage(
        () => chrome.tabs.sendMessage(tabId, { type: "inspect" }),
        () => new Promise((resolve) => setTimeout(resolve, 250)),
      );
    } catch (error) {
      await releaseMainGuard(tabId).catch(() => undefined);
      throw error;
    }
    if (!result?.ok) await releaseMainGuard(tabId).catch(() => undefined);
    if (result?.ok) await chrome.storage.session.set({ [sessionTabKey(command.sessionId)]: tabId });
    return result;
  }
  if (command.commandType === "fill_plan") {
    const tabId = await rememberedTabId(command.sessionId);
    const plan = command.payload.plan as FillPlan | undefined;
    if (!plan) throw new Error("Fill plan is missing.");
    const uploads = Array.isArray(command.payload.uploads)
      ? command.payload.uploads as FileUploadInstruction[]
      : [];
    return chrome.tabs.sendMessage(tabId, { type: "fill_plan", plan, uploads });
  }
  if (command.commandType === "release_for_review") {
    let tabId: number;
    try {
      tabId = await rememberedTabId(command.sessionId);
    } catch {
      const expectedUrl = command.payload.expectedUrl;
      tabId = typeof expectedUrl === "string" ? await expectedTabId(expectedUrl) : await activeTabId();
    }
    const result = await chrome.tabs.sendMessage(tabId, {
      type: "release_for_review",
      connectionCheck: command.payload.connectionCheck === true,
    });
    if (result?.ok) await releaseMainGuard(tabId);
    return result;
  }
  if (command.commandType === "focus_review") {
    const tabId = await rememberedTabId(command.sessionId).catch(() => {
      throw new Error("review_tab_unavailable");
    });
    return focusReviewTab(tabId, {
      getTab: (id) => chrome.tabs.get(id),
      activateTab: (id) => chrome.tabs.update(id, { active: true }),
      focusWindow: (windowId) => chrome.windows.update(windowId, { focused: true }),
    });
  }
  throw new Error("Unsupported browser command.");
}

function returnCommandResult(command: BrowserCommand): void {
  void (async () => {
    let cacheIdentity: typeof pendingResultIdentity = null;
    if (command.commandType === "fill_plan") {
      const identity = { commandId: command.commandId, sessionId: command.sessionId, driverLeaseId: command.driverLeaseId };
      const cached = await beginDurableCommand(chrome.storage.local, identity);
      cacheIdentity = identity;
      if (cached.state === "completed") return { result: cached.result, cacheIdentity };
      if (cached.state === "uncertain") throw new Error("fill_restart_uncertain");
    }
    const result = await executeCommand(command);
    if (command.commandType === "fill_plan") {
      await completeDurableCommand(chrome.storage.local, {
        commandId: command.commandId,
        sessionId: command.sessionId,
        driverLeaseId: command.driverLeaseId,
      }, result);
      return { result, cacheIdentity };
    }
    return { result, cacheIdentity: null };
  })().then(({ result, cacheIdentity }) => {
    pendingResultIdentity = cacheIdentity;
    void postNative({
      protocolVersion: 1,
      type: "command_result",
      commandId: command.commandId,
      sessionId: command.sessionId,
      commandType: command.commandType,
      driverLeaseId: command.driverLeaseId,
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
      driverLeaseId: command.driverLeaseId,
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
      if (message.type === "result_ack" && pendingResultIdentity) {
        void clearDurableCommand(chrome.storage.local, pendingResultIdentity);
        pendingResultIdentity = null;
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
  if (message?.type === "manual_inspect" && typeof message.tabId === "number") {
    void (async () => {
      await installMainGuard(message.tabId);
      const result = await chrome.tabs.sendMessage(message.tabId, { type: "inspect" });
      if (!result?.ok) await releaseMainGuard(message.tabId).catch(() => undefined);
      return result;
    })().then(respond).catch((error) => respond({ ok: false, error: error instanceof Error ? error.message : String(error) }));
    return true;
  }
  if (message?.type === "manual_release" && typeof message.tabId === "number") {
    void (async () => {
      const result = await chrome.tabs.sendMessage(message.tabId, { type: "release_for_review" });
      if (!result?.ok) return result;
      await releaseMainGuard(message.tabId);
      return result;
    })().then(respond).catch((error) => respond({ ok: false, error: error instanceof Error ? error.message : String(error) }));
    return true;
  }
});
