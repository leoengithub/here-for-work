export async function retryMessage<T>(
  send: () => Promise<T>,
  delay: () => Promise<void>,
  attempts = 20,
): Promise<T> {
  let lastError: unknown = new Error("The extension content script was unavailable.");
  for (let attempt = 0; attempt < attempts; attempt += 1) {
    try {
      return await send();
    } catch (error) {
      lastError = error;
      if (attempt < attempts - 1) await delay();
    }
  }
  throw lastError;
}
