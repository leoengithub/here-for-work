import { describe, expect, it, vi } from "vitest";
import { isConfirmedNativeResponse, postMessageSafely } from "./native-port";

describe("native port writes", () => {
  it("reports a successful write", () => {
    const postMessage = vi.fn();

    expect(postMessageSafely({ postMessage }, { type: "hello" })).toBe(true);
    expect(postMessage).toHaveBeenCalledWith({ type: "hello" });
  });

  it("contains Chrome's disconnected-port exception", () => {
    const postMessage = vi.fn(() => {
      throw new Error("Attempting to use a disconnected port object");
    });

    expect(postMessageSafely({ postMessage }, { type: "poll" })).toBe(false);
  });

  it("does not treat a native-host error as a confirmed app handshake", () => {
    expect(isConfirmedNativeResponse({ protocolVersion: 1, ok: false, error: "app_not_running" })).toBe(false);
    expect(isConfirmedNativeResponse({ protocolVersion: 1, ok: true, type: "hello_ack" })).toBe(true);
  });
});
