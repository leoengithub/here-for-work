export const PROTOCOL_VERSION = 1 as const;

export type AtsFamily = "ashby" | "greenhouse" | "lever" | "generic";
export type FieldClassification =
  | "safe_verified"
  | "grounded_narrative"
  | "compensation"
  | "sensitive"
  | "unknown"
  | "unsupported"
  | "unverifiable";

export interface FormField {
  id: string;
  label: string;
  control: "input" | "textarea" | "select" | "unsupported";
  inputType: string;
  inputMode: string | null;
  required: boolean;
  options: string[];
  language: string | null;
  maxLength: number | null;
  maxWords: number | null;
  minSentences: number | null;
  maxSentences: number | null;
  classification: FieldClassification;
  reason: string;
}

export type FormFlowIssue =
  | "authentication_required"
  | "captcha_or_antibot"
  | "custom_widget"
  | "embedded_frame"
  | "modal_form"
  | "multi_step_form"
  | "no_compatible_fields";

export interface FormFlow {
  disposition: "fillable" | "fallback_eligible" | "human_handoff";
  issues: FormFlowIssue[];
}

export interface FormSnapshot {
  protocolVersion: 1;
  ats: AtsFamily;
  url: string;
  title: string;
  fields: FormField[];
  flow: FormFlow;
  fingerprint: string;
}

export interface FillInstruction {
  fieldId: string;
  value: string;
  classification: "safe_verified" | "grounded_draft" | "canonical_preference";
  required?: boolean;
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

export type FillResultReason =
  | "verified"
  | "user_file_preserved"
  | "unsafe_instruction"
  | "control_missing"
  | "control_hidden"
  | "control_replaced"
  | "control_ambiguous"
  | "unsupported_control"
  | "unsupported_option"
  | "write_failed"
  | "readback_mismatch"
  | "attachment_invalid"
  | "attachment_failed";

export interface FillResult {
  fieldId: string;
  status: "verified" | "skipped" | "failed";
  reasonCode: FillResultReason;
  reason: string | null;
  mutated: boolean;
  readBackSha256: string | null;
}

export const BROWSER_COMMAND_TYPES = ["inspect_request", "fill_plan", "release_for_review", "focus_review"] as const;
export type BrowserCommandType = typeof BROWSER_COMMAND_TYPES[number];

export interface BrowserCommand {
  protocolVersion: 1;
  ok: true;
  type: "command";
  commandId: string;
  sessionId: string;
  commandType: BrowserCommandType;
  driverLeaseId: string | null;
  payload: Record<string, unknown>;
}
