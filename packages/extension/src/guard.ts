export function installFinalizationGuard(doc: Document = document): () => void {
  const FormConstructor = doc.defaultView?.HTMLFormElement;
  const formPrototype = FormConstructor?.prototype;
  const originalSubmit = formPrototype
    ? Object.getOwnPropertyDescriptor(formPrototype, "submit")
    : undefined;
  const originalRequestSubmit = formPrototype
    ? Object.getOwnPropertyDescriptor(formPrototype, "requestSubmit")
    : undefined;
  const blockedFinalization = () => undefined;
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
  if (formPrototype) {
    Object.defineProperty(formPrototype, "submit", {
      configurable: true,
      writable: true,
      value: blockedFinalization,
    });
    Object.defineProperty(formPrototype, "requestSubmit", {
      configurable: true,
      writable: true,
      value: blockedFinalization,
    });
  }
  doc.addEventListener("submit", onSubmit, { capture: true });
  doc.addEventListener("click", onClick, { capture: true });
  return () => {
    doc.removeEventListener("submit", onSubmit, { capture: true });
    doc.removeEventListener("click", onClick, { capture: true });
    if (formPrototype) {
      if (originalSubmit) Object.defineProperty(formPrototype, "submit", originalSubmit);
      else delete (formPrototype as { submit?: unknown }).submit;
      if (originalRequestSubmit) Object.defineProperty(formPrototype, "requestSubmit", originalRequestSubmit);
      else delete (formPrototype as { requestSubmit?: unknown }).requestSubmit;
    }
  };
}
