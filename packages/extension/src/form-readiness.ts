export async function waitForNonEmptyForm<T extends { fields: unknown[] }>(
  inspect: () => Promise<T>,
  delay: () => Promise<void>,
  attempts = 20,
): Promise<T> {
  for (let attempt = 0; attempt < attempts; attempt += 1) {
    const snapshot = await inspect();
    if (snapshot.fields.length > 0) return snapshot;
    if (attempt < attempts - 1) await delay();
  }
  throw new Error("No application fields appeared on the selected application page.");
}
