export function createInstallationIdResolver(
  read: () => Promise<unknown>,
  write: (value: string) => Promise<void>,
  create: () => string,
): () => Promise<string> {
  let request: Promise<string> | null = null;

  return () => {
    if (!request) {
      request = (async () => {
        const stored = await read();
        if (typeof stored === "string" && /^[0-9a-f-]{36}$/.test(stored)) return stored;
        const value = create();
        await write(value);
        return value;
      })().catch((error: unknown) => {
        request = null;
        throw error;
      });
    }
    return request;
  };
}
