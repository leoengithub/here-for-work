export function requireScriptingApi(api: typeof chrome.scripting | undefined): typeof chrome.scripting {
  if (!api?.executeScript) throw new Error("finalization_guard_permission_required");
  return api;
}
