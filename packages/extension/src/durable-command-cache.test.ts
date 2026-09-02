import { describe, expect, it } from "vitest";

import {
  beginDurableCommand,
  clearDurableCommand,
  commandCacheKey,
  COMMAND_CACHE_MAX_ENTRIES,
  COMMAND_CACHE_TTL_MS,
  completeDurableCommand,
} from "./durable-command-cache";
import type { DurableStorage } from "./durable-command-cache";

function memoryStorage(): DurableStorage & { values: Record<string, unknown> } {
  const values: Record<string, unknown> = {};
  return {
    values,
    async get(keys = null) {
      if (keys === null) return { ...values };
      const selected = Array.isArray(keys) ? keys : [keys];
      return Object.fromEntries(selected.filter((key) => key in values).map((key) => [key, values[key]]));
    },
    async set(items) { Object.assign(values, items); },
    async remove(keys) { for (const key of Array.isArray(keys) ? keys : [keys]) delete values[key]; },
  };
}

const identity = { commandId: "command", sessionId: "session", driverLeaseId: "lease" };

describe("durable fill command cache", () => {
  it("returns a completed result across a fresh caller without re-executing", async () => {
    const storage = memoryStorage();
    expect(await beginDurableCommand(storage, identity, 10)).toEqual({ state: "started" });
    await completeDurableCommand(storage, identity, { ok: true }, 11);
    expect(await beginDurableCommand(storage, identity, 12)).toEqual({ state: "completed", result: { ok: true } });
  });

  it("turns a restart during fill into explicit uncertainty", async () => {
    const storage = memoryStorage();
    await beginDurableCommand(storage, identity, 10);
    expect(await beginDurableCommand(storage, identity, 11)).toEqual({ state: "uncertain" });
    await clearDurableCommand(storage, identity);
    expect(await beginDurableCommand(storage, identity, 12)).toEqual({ state: "started" });
  });

  it("keys by command, session, and lease and prunes expired or excess entries", async () => {
    const storage = memoryStorage();
    for (let index = 0; index < COMMAND_CACHE_MAX_ENTRIES + 3; index += 1) {
      const current = { commandId: `command-${index}`, sessionId: "session", driverLeaseId: `lease-${index}` };
      await completeDurableCommand(storage, current, index, 100 + index);
    }
    expect(Object.keys(storage.values)).toHaveLength(COMMAND_CACHE_MAX_ENTRIES);
    expect(storage.values[commandCacheKey({ commandId: "command-0", sessionId: "session", driverLeaseId: "lease-0" })]).toBeUndefined();

    const fresh = { commandId: "fresh", sessionId: "session", driverLeaseId: null };
    await beginDurableCommand(storage, fresh, COMMAND_CACHE_TTL_MS + 10_000);
    expect(Object.keys(storage.values)).toEqual([commandCacheKey(fresh)]);
  });
});
