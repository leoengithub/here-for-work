import type {
  FileUploadInstruction,
  FillPlan,
  FillResult,
  FillResultReason,
} from "./contracts";
import { isControlVisible } from "./ats";

type NativeControl = HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement;
type FileAttacher = (element: HTMLInputElement, file: File, doc: Document) => void;

export interface FillSettlingOptions {
  currentFingerprint?: () => Promise<string | null>;
  delay?: (milliseconds: number) => Promise<void>;
  settleDelaysMs?: number[];
}

function normalizedValue(value: string): string {
  return value.replace(/\r\n?/g, "\n").trim();
}

function writeValue(element: NativeControl, value: string): { expected: string; read: () => string } {
  let read = () => element.value;
  if (element instanceof HTMLSelectElement) {
    const option = Array.from(element.options).find(
      (candidate) => candidate.value === value || candidate.text.trim() === value,
    );
    if (!option) throw new Error("unsupported_option");
    element.value = option.value;
    if (option.text.trim() === value) read = () => element.selectedOptions.item(0)?.text.trim() ?? "";
  } else {
    const prototype = element instanceof HTMLTextAreaElement
      ? HTMLTextAreaElement.prototype
      : HTMLInputElement.prototype;
    const setter = Object.getOwnPropertyDescriptor(prototype, "value")?.set;
    if (!setter) throw new Error("write_failed");
    setter.call(element, value);
  }
  element.dispatchEvent(new Event("input", { bubbles: true }));
  element.dispatchEvent(new Event("change", { bubbles: true }));
  return { expected: normalizedValue(value), read };
}

function defaultFileAttacher(element: HTMLInputElement, file: File, doc: Document): void {
  const DataTransferConstructor = doc.defaultView?.DataTransfer;
  if (!DataTransferConstructor) throw new Error("attachment_failed");
  const transfer = new DataTransferConstructor();
  transfer.items.add(file);
  const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "files")?.set;
  if (!setter) throw new Error("attachment_failed");
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

async function sha256Bytes(bytes: Uint8Array): Promise<string> {
  const digest = await globalThis.crypto.subtle.digest("SHA-256", exactArrayBuffer(bytes));
  return Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, "0")).join("");
}

async function sha256Text(value: string): Promise<string> {
  return sha256Bytes(new TextEncoder().encode(normalizedValue(value)));
}

function result(
  fieldId: string,
  status: FillResult["status"],
  reasonCode: FillResultReason,
  reason: string | null,
  mutated: boolean,
  readBackSha256: string | null = null,
): FillResult {
  return { fieldId, status, reasonCode, reason, mutated, readBackSha256 };
}

function controlsFor(doc: Document, fieldId: string): NativeControl[] {
  return Array.from(doc.querySelectorAll<NativeControl>("input[data-hfw-field-id], textarea[data-hfw-field-id], select[data-hfw-field-id]"))
    .filter((candidate) => candidate.dataset.hfwFieldId === fieldId);
}

function assertUniquePlan(plan: FillPlan, uploads: FileUploadInstruction[]): void {
  const instructionIds = new Set<string>();
  for (const instruction of plan.instructions) {
    if (!instruction.fieldId || instructionIds.has(instruction.fieldId)) throw new Error("invalid_fill_plan_duplicate_field");
    instructionIds.add(instruction.fieldId);
  }
  const uploadIds = new Set<string>();
  for (const upload of uploads) {
    if (!upload.fieldId || uploadIds.has(upload.fieldId)) throw new Error("invalid_fill_plan_duplicate_field");
    uploadIds.add(upload.fieldId);
  }
}

async function settle(
  expectedElement: NativeControl,
  fieldId: string,
  expectedValue: string,
  readValue: () => string,
  doc: Document,
  options: FillSettlingOptions,
): Promise<FillResult> {
  const delay = options.delay ?? ((milliseconds) => new Promise((resolve) => setTimeout(resolve, milliseconds)));
  for (const milliseconds of options.settleDelaysMs ?? [25, 50, 100]) await delay(milliseconds);
  const matches = controlsFor(doc, fieldId);
  if (!expectedElement.isConnected || matches.length === 1 && matches[0] !== expectedElement) {
    return result(fieldId, "failed", "control_replaced", "The inspected control was replaced before verification settled.", true);
  }
  if (matches.length === 0) {
    return result(fieldId, "failed", "control_missing", "The inspected control disappeared before verification settled.", true);
  }
  if (matches.length !== 1) {
    return result(fieldId, "failed", "control_ambiguous", "The field identity became ambiguous before verification settled.", true);
  }
  if (!isControlVisible(expectedElement)) {
    return result(fieldId, "failed", "control_hidden", "The control became hidden before verification settled.", true);
  }
  const readBack = normalizedValue(readValue());
  const readBackSha256 = await sha256Text(readBack);
  return readBack === expectedValue
    ? result(fieldId, "verified", "verified", null, true, readBackSha256)
    : result(fieldId, "failed", "readback_mismatch", "Settled read-back did not match the planned value.", true, readBackSha256);
}

export async function applyFillPlan(
  plan: FillPlan,
  activeFingerprint: string | null,
  doc: Document = document,
  uploads: FileUploadInstruction[] = [],
  attachFile: FileAttacher = defaultFileAttacher,
  settling: FillSettlingOptions = {},
): Promise<FillResult[]> {
  if (plan.protocolVersion !== 1 || plan.snapshotFingerprint !== activeFingerprint) throw new Error("snapshot_mismatch");
  assertUniquePlan(plan, uploads);
  if (settling.currentFingerprint && await settling.currentFingerprint() !== plan.snapshotFingerprint) {
    throw new Error("form_drift_before_fill");
  }

  const results: FillResult[] = [];
  const uploadFieldIds = new Set(uploads.map((upload) => upload.fieldId));
  for (const instruction of plan.instructions) {
    if (uploadFieldIds.has(instruction.fieldId)) continue;
    if (!["safe_verified", "grounded_draft", "canonical_preference"].includes(instruction.classification)) {
      results.push(result(instruction.fieldId, "skipped", "unsafe_instruction", "Only verified facts, career-ops-grounded drafts, or structured canonical preferences may be filled.", false));
      continue;
    }
    const matches = controlsFor(doc, instruction.fieldId);
    if (matches.length === 0) {
      results.push(result(instruction.fieldId, "failed", "control_missing", "The inspected control is no longer present.", false));
      continue;
    }
    if (matches.length !== 1) {
      results.push(result(instruction.fieldId, "failed", "control_ambiguous", "The inspected field identity is ambiguous.", false));
      continue;
    }
    const element = matches[0];
    if (!isControlVisible(element)) {
      results.push(result(instruction.fieldId, "failed", "control_hidden", "The inspected control is no longer visible.", false));
      continue;
    }
    if (element instanceof HTMLInputElement && element.type === "file") {
      results.push(result(instruction.fieldId, "failed", "unsupported_control", "File controls require a verified upload descriptor.", false));
      continue;
    }
    try {
      const written = writeValue(element, instruction.value);
      results.push(await settle(element, instruction.fieldId, written.expected, written.read, doc, settling));
    } catch (error) {
      const reasonCode = error instanceof Error && error.message === "unsupported_option" ? "unsupported_option" : "write_failed";
      results.push(result(instruction.fieldId, "failed", reasonCode, reasonCode === "unsupported_option" ? "The requested select option is no longer present." : "The browser could not write this control.", false));
    }
  }

  for (const upload of uploads) {
    if (upload.classification !== "safe_verified" || upload.mimeType !== "application/pdf" || !upload.fileName.toLowerCase().endsWith(".pdf") || !/^[a-f0-9]{64}$/.test(upload.sha256)) {
      results.push(result(upload.fieldId, "failed", "attachment_invalid", "Only a manifest-verified career-ops PDF may be attached.", false));
      continue;
    }
    const matches = controlsFor(doc, upload.fieldId).filter((candidate): candidate is HTMLInputElement => candidate instanceof HTMLInputElement && candidate.type === "file");
    if (matches.length === 0) {
      results.push(result(upload.fieldId, "failed", "control_missing", "The inspected CV control is no longer present.", false));
      continue;
    }
    if (matches.length !== 1) {
      results.push(result(upload.fieldId, "failed", "control_ambiguous", "The inspected CV field identity is ambiguous.", false));
      continue;
    }
    const element = matches[0];
    if (!isControlVisible(element) || element.disabled) {
      results.push(result(upload.fieldId, "failed", "control_hidden", "The inspected CV control is unavailable.", false));
      continue;
    }
    if (element.files?.length) {
      results.push(result(upload.fieldId, "skipped", "user_file_preserved", "The CV already selected by the user was preserved.", false));
      continue;
    }
    if (!acceptsPdf(element)) {
      results.push(result(upload.fieldId, "failed", "unsupported_control", "This control does not accept PDF files.", false));
      continue;
    }
    try {
      const bytes = bytesFromBase64(upload.contentBase64);
      if (await sha256Bytes(bytes) !== upload.sha256) throw new Error("attachment_invalid");
      const FileConstructor = doc.defaultView?.File ?? File;
      const file = new FileConstructor([exactArrayBuffer(bytes)], upload.fileName, { type: upload.mimeType });
      attachFile(element, file, doc);
      const delay = settling.delay ?? ((milliseconds: number) => new Promise((resolve) => setTimeout(resolve, milliseconds)));
      for (const milliseconds of settling.settleDelaysMs ?? [25, 50, 100]) await delay(milliseconds);
      const current = controlsFor(doc, upload.fieldId);
      if (!element.isConnected || current.length === 1 && current[0] !== element) {
        results.push(result(upload.fieldId, "failed", "control_replaced", "The CV control was replaced before verification settled.", true));
        continue;
      }
      const attached = element.files?.item(0);
      const verified = current.length === 1 && isControlVisible(element) && element.files?.length === 1 && attached?.name === upload.fileName && attached.size === bytes.length && attached.type === upload.mimeType;
      results.push(verified
        ? result(upload.fieldId, "verified", "verified", null, true, upload.sha256)
        : result(upload.fieldId, "failed", "readback_mismatch", "Settled attached-file read-back did not match.", true));
    } catch (error) {
      const reasonCode = error instanceof Error && error.message === "attachment_invalid" ? "attachment_invalid" : "attachment_failed";
      results.push(result(upload.fieldId, "failed", reasonCode, reasonCode === "attachment_invalid" ? "The verified CV bytes did not match their manifest hash." : "The browser could not attach the verified CV.", false));
    }
  }
  return results;
}
