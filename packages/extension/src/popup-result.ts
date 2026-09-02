export function requireSuccessfulRelease(result: unknown): void {
  if (result && typeof result === "object" && (result as { ok?: unknown }).ok === true) return;
  const error = result && typeof result === "object" && typeof (result as { error?: unknown }).error === "string"
    ? (result as { error: string }).error
    : "The form could not be released for review.";
  throw new Error(error);
}
