import type { FillPlan } from "./contracts";
import { isControlVisible } from "./ats";

function writeValue(element: HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement, value: string): void {
  if (element instanceof HTMLSelectElement) {
    const option = Array.from(element.options).find((candidate) => candidate.value === value || candidate.text.trim() === value);
    if (!option) throw new Error("Requested select option was not present.");
    element.value = option.value;
  } else {
    const prototype = element instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
    const setter = Object.getOwnPropertyDescriptor(prototype, "value")?.set;
    if (!setter) throw new Error("Control value setter is unavailable.");
    setter.call(element, value);
  }
  element.dispatchEvent(new Event("input", { bubbles: true }));
  element.dispatchEvent(new Event("change", { bubbles: true }));
}

export async function applyFillPlan(plan: FillPlan, activeFingerprint: string | null, doc: Document = document) {
  if (plan.protocolVersion !== 1 || plan.snapshotFingerprint !== activeFingerprint) {
    throw new Error("The fill plan does not match the inspected live form.");
  }
  const results = [];
  for (const instruction of plan.instructions) {
    if (instruction.classification !== "safe_verified") {
      results.push({ fieldId: instruction.fieldId, status: "skipped", reason: "Only safe verified fields may be filled." });
      continue;
    }
    const element = Array.from(
      doc.querySelectorAll<HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement>("[data-hfw-field-id]"),
    ).find((candidate) => candidate.dataset.hfwFieldId === instruction.fieldId);
    if (!element || !isControlVisible(element) || element instanceof HTMLInputElement && element.type === "file") {
      results.push({ fieldId: instruction.fieldId, status: "skipped", reason: "Control is unavailable or unsupported." });
      continue;
    }
    try {
      writeValue(element, instruction.value);
      results.push({
        fieldId: instruction.fieldId,
        status: element.value === instruction.value ? "verified" : "failed",
        reason: element.value === instruction.value ? null : "Read-back did not match.",
      });
    } catch (error) {
      results.push({ fieldId: instruction.fieldId, status: "failed", reason: error instanceof Error ? error.message : String(error) });
    }
  }
  return results;
}
