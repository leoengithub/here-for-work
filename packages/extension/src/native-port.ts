type NativePortWriter = Pick<chrome.runtime.Port, "postMessage">;

export function postMessageSafely(port: NativePortWriter, message: unknown): boolean {
  try {
    port.postMessage(message);
    return true;
  } catch {
    return false;
  }
}

export function isConfirmedNativeResponse(message: unknown): message is { protocolVersion: 1; ok: true; type: string } {
  if (!message || typeof message !== "object") return false;
  const candidate = message as { protocolVersion?: unknown; ok?: unknown; type?: unknown };
  return candidate.protocolVersion === 1 && candidate.ok === true && typeof candidate.type === "string";
}
