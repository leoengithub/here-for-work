export const PROTOCOL_VERSION = 1 as const;

export type AtsFamily = "ashby" | "greenhouse" | "lever" | "generic";
export type FieldClassification =
  | "safe_verified"
  | "sensitive"
  | "unknown"
  | "unsupported"
  | "unverifiable";

export interface FormField {
  id: string;
  label: string;
  control: "input" | "textarea" | "select";
  inputType: string;
  required: boolean;
  options: string[];
  classification: FieldClassification;
  reason: string;
}

export interface FormSnapshot {
  protocolVersion: 1;
  ats: AtsFamily;
  url: string;
  title: string;
  fields: FormField[];
  fingerprint: string;
}

export interface FillInstruction {
  fieldId: string;
  value: string;
  classification: "safe_verified";
}

export interface FillPlan {
  protocolVersion: 1;
  snapshotFingerprint: string;
  instructions: FillInstruction[];
}

export interface FileUploadInstruction {
  fieldId: string;
  fileName: string;
  mimeType: "application/pdf";
  contentBase64: string;
  sha256: string;
  classification: "safe_verified";
}

export const BROWSER_COMMAND_TYPES = ["inspect_request", "fill_plan", "release_for_review"] as const;
export type BrowserCommandType = typeof BROWSER_COMMAND_TYPES[number];

export interface BrowserCommand {
  protocolVersion: 1;
  ok: true;
  type: "command";
  commandId: string;
  sessionId: string;
  commandType: BrowserCommandType;
  payload: Record<string, unknown>;
}
