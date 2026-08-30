export function installFinalizationGuard(doc: Document = document): () => void {
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
  doc.addEventListener("submit", onSubmit, { capture: true });
  doc.addEventListener("click", onClick, { capture: true });
  return () => {
    doc.removeEventListener("submit", onSubmit, { capture: true });
    doc.removeEventListener("click", onClick, { capture: true });
  };
}
