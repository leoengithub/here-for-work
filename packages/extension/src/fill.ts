import type { FileUploadInstruction, FillPlan } from "./contracts";
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

type FileAttacher = (element: HTMLInputElement, file: File, doc: Document) => void;

function defaultFileAttacher(element: HTMLInputElement, file: File, doc: Document): void {
  const DataTransferConstructor = doc.defaultView?.DataTransfer;
  if (!DataTransferConstructor) throw new Error("This browser cannot attach a file programmatically.");
  const transfer = new DataTransferConstructor();
  transfer.items.add(file);
  const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "files")?.set;
  if (!setter) throw new Error("File control setter is unavailable.");
  setter.call(element, transfer.files);
  element.dispatchEvent(new Event("input", { bubbles: true }));
  element.dispatchEvent(new Event("change", { bubbles: true }));
}

function acceptsPdf(element: HTMLInputElement): boolean {
  const accepted = element.accept
    .split(",")
    .map((value) => value.trim().toLowerCase())
    .filter(Boolean);
  return accepted.length === 0
    || accepted.some((value) => [".pdf", "application/pdf", "application/*", "*/*"].includes(value));
}

function bytesFromBase64(value: string): Uint8Array {
  const decoded = atob(value);
  return Uint8Array.from(decoded, (character) => character.charCodeAt(0));
}

function exactArrayBuffer(bytes: Uint8Array): ArrayBuffer {
  const buffer = new ArrayBuffer(bytes.length);
  new Uint8Array(buffer).set(bytes);
  return buffer;
}

async function sha256(bytes: Uint8Array): Promise<string> {
  const digest = await globalThis.crypto.subtle.digest("SHA-256", exactArrayBuffer(bytes));
  return Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, "0")).join("");
}

export async function applyFillPlan(
  plan: FillPlan,
  activeFingerprint: string | null,
  doc: Document = document,
  uploads: FileUploadInstruction[] = [],
  attachFile: FileAttacher = defaultFileAttacher,
) {
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
  for (const upload of uploads) {
    if (upload.classification !== "safe_verified"
        || upload.mimeType !== "application/pdf"
        || !upload.fileName.toLowerCase().endsWith(".pdf")
        || !/^[a-f0-9]{64}$/.test(upload.sha256)) {
      results.push({ fieldId: upload.fieldId, status: "skipped", reason: "Only a verified tailored PDF may be attached." });
      continue;
    }
    const element = Array.from(doc.querySelectorAll<HTMLInputElement>("input[type=file][data-hfw-field-id]"))
      .find((candidate) => candidate.dataset.hfwFieldId === upload.fieldId);
    if (!element || !isControlVisible(element) || element.disabled) {
      results.push({ fieldId: upload.fieldId, status: "skipped", reason: "CV control is unavailable or unsupported." });
      continue;
    }
    if (element.files?.length) {
      results.push({ fieldId: upload.fieldId, status: "skipped", reason: "The CV already selected by the user was preserved." });
      continue;
    }
    if (!acceptsPdf(element)) {
      results.push({ fieldId: upload.fieldId, status: "skipped", reason: "This control does not accept PDF files." });
      continue;
    }
    try {
      const bytes = bytesFromBase64(upload.contentBase64);
      if (await sha256(bytes) !== upload.sha256) throw new Error("The verified CV bytes did not match their manifest hash.");
      const FileConstructor = doc.defaultView?.File ?? File;
      const file = new FileConstructor([exactArrayBuffer(bytes)], upload.fileName, { type: upload.mimeType });
      attachFile(element, file, doc);
      const attached = element.files?.item(0);
      const verified = element.files?.length === 1
        && attached?.name === upload.fileName
        && attached.size === bytes.length
        && attached.type === upload.mimeType;
      results.push({
        fieldId: upload.fieldId,
        status: verified ? "verified" : "failed",
        reason: verified ? null : "Attached-file read-back did not match.",
      });
    } catch (error) {
      results.push({ fieldId: upload.fieldId, status: "failed", reason: error instanceof Error ? error.message : String(error) });
    }
  }
  return results;
}
