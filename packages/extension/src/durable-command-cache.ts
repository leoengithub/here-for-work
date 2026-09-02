export const COMMAND_CACHE_PREFIX = "browser-command-result:v2:";
export const COMMAND_CACHE_TTL_MS = 24 * 60 * 60 * 1_000;
export const COMMAND_CACHE_MAX_ENTRIES = 32;

export interface DurableStorage {
  get(keys?: string | string[] | null): Promise<Record<string, unknown>>;
  set(items: Record<string, unknown>): Promise<void>;
  remove(keys: string | string[]): Promise<void>;
}

export interface CommandIdentity {
  commandId: string;
  sessionId: string;
  driverLeaseId: string | null;
}

export type DurableCommandEntry = CommandIdentity & {
  state: "in_progress" | "completed";
  updatedAt: number;
  result?: unknown;
};

export function commandCacheKey(identity: CommandIdentity): string {
  return `${COMMAND_CACHE_PREFIX}${identity.commandId}:${identity.sessionId}:${identity.driverLeaseId ?? "none"}`;
}

function isEntry(value: unknown): value is DurableCommandEntry {
  if (!value || typeof value !== "object") return false;
  const entry = value as Partial<DurableCommandEntry>;
  return typeof entry.commandId === "string"
    && typeof entry.sessionId === "string"
    && (entry.driverLeaseId === null || typeof entry.driverLeaseId === "string")
    && (entry.state === "in_progress" || entry.state === "completed")
    && typeof entry.updatedAt === "number";
}

async function prune(storage: DurableStorage, now: number): Promise<void> {
  const all = await storage.get(null);
  const managed = Object.entries(all).filter(([key]) => key.startsWith(COMMAND_CACHE_PREFIX));
  const invalid = managed.filter(([, value]) => !isEntry(value)).map(([key]) => key);
  const entries = managed
    .flatMap(([key, value]) => key.startsWith(COMMAND_CACHE_PREFIX) && isEntry(value)
      ? [[key, value] as const]
      : [])
    .sort((left, right) => right[1].updatedAt - left[1].updatedAt);
  const expiredOrExcess = entries
    .filter(([, entry], index) => now - entry.updatedAt > COMMAND_CACHE_TTL_MS || index >= COMMAND_CACHE_MAX_ENTRIES)
    .map(([key]) => key);
  const removable = [...invalid, ...expiredOrExcess];
  if (removable.length) await storage.remove(removable);
}

export async function beginDurableCommand(
  storage: DurableStorage,
  identity: CommandIdentity,
  now = Date.now(),
): Promise<{ state: "started" } | { state: "uncertain" } | { state: "completed"; result: unknown }> {
  await prune(storage, now);
  const key = commandCacheKey(identity);
  const current = (await storage.get(key))[key];
  if (isEntry(current)
      && current.commandId === identity.commandId
      && current.sessionId === identity.sessionId
      && current.driverLeaseId === identity.driverLeaseId) {
    if (current.state === "completed") return { state: "completed", result: current.result };
    return { state: "uncertain" };
  }
  await storage.set({
    [key]: { ...identity, state: "in_progress", updatedAt: now } satisfies DurableCommandEntry,
  });
  return { state: "started" };
}

export async function completeDurableCommand(
  storage: DurableStorage,
  identity: CommandIdentity,
  result: unknown,
  now = Date.now(),
): Promise<void> {
  await storage.set({
    [commandCacheKey(identity)]: {
      ...identity,
      state: "completed",
      updatedAt: now,
      result,
    } satisfies DurableCommandEntry,
  });
  await prune(storage, now);
}

export async function clearDurableCommand(storage: DurableStorage, identity: CommandIdentity): Promise<void> {
  await storage.remove(commandCacheKey(identity));
}
