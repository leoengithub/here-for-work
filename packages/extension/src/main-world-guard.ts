type GuardState = {
  onSubmit: (event: Event) => void;
  onClick: (event: Event) => void;
  submit: PropertyDescriptor | undefined;
  requestSubmit: PropertyDescriptor | undefined;
};

export function installMainWorldFinalizationGuard(token: string): void {
  const holder = window as unknown as Record<string, unknown>;
  const key = `__hereForWorkFinalizationGuard_${token}`;
  if (holder[key]) return;
  const prototype = HTMLFormElement.prototype;
  const submit = Object.getOwnPropertyDescriptor(prototype, "submit");
  const requestSubmit = Object.getOwnPropertyDescriptor(prototype, "requestSubmit");
  const onSubmit = (event: Event) => {
    event.preventDefault();
    event.stopImmediatePropagation();
  };
  const onClick = (event: Event) => {
    const target = event.target instanceof Element ? event.target.closest("button, input") : null;
    if (!target) return;
    const type = target.getAttribute("type")?.toLowerCase();
    const copy = (target.textContent ?? target.getAttribute("value") ?? "").trim().toLowerCase();
    if (type === "submit" || /^(submit|send application|apply)$/.test(copy)) {
      event.preventDefault();
      event.stopImmediatePropagation();
    }
  };
  const blocked = () => undefined;
  Object.defineProperty(prototype, "submit", { configurable: true, writable: true, value: blocked });
  Object.defineProperty(prototype, "requestSubmit", { configurable: true, writable: true, value: blocked });
  document.addEventListener("submit", onSubmit, { capture: true });
  document.addEventListener("click", onClick, { capture: true });
  Object.defineProperty(holder, key, {
    configurable: true,
    enumerable: false,
    value: { onSubmit, onClick, submit, requestSubmit } satisfies GuardState,
  });
}

export function releaseMainWorldFinalizationGuard(token: string): boolean {
  const holder = window as unknown as Record<string, unknown>;
  const key = `__hereForWorkFinalizationGuard_${token}`;
  const state = holder[key] as GuardState | undefined;
  if (!state) return false;
  document.removeEventListener("submit", state.onSubmit, { capture: true });
  document.removeEventListener("click", state.onClick, { capture: true });
  if (state.submit) Object.defineProperty(HTMLFormElement.prototype, "submit", state.submit);
  else delete (HTMLFormElement.prototype as { submit?: unknown }).submit;
  if (state.requestSubmit) Object.defineProperty(HTMLFormElement.prototype, "requestSubmit", state.requestSubmit);
  else delete (HTMLFormElement.prototype as { requestSubmit?: unknown }).requestSubmit;
  delete holder[key];
  return true;
}
