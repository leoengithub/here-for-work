import { inspectForm } from "./ats";
import type { FileUploadInstruction, FillPlan } from "./contracts";
import { applyFillPlan } from "./fill";
import { waitForNonEmptyForm } from "./form-readiness";
import { installFinalizationGuard } from "./guard";

let activeFingerprint: string | null = null;
let releaseGuard: (() => void) | null = null;
let verifiedFillCompleted = false;
let activeRequiredFields = new Set<string>();

function activateFinalizationGuard(): void {
  releaseGuard?.();
  releaseGuard = installFinalizationGuard();
  verifiedFillCompleted = false;
}

function releaseFinalizationGuard(): void {
  releaseGuard?.();
  releaseGuard = null;
  activeFingerprint = null;
  activeRequiredFields = new Set();
  verifiedFillCompleted = false;
}

async function applyPlan(plan: FillPlan, uploads: FileUploadInstruction[] = []) {
  const results = await applyFillPlan(plan, activeFingerprint, document, uploads, undefined, {
    currentFingerprint: async () => (await inspectForm()).fingerprint,
  });
  const plannedIds = new Set([...plan.instructions.map((instruction) => instruction.fieldId), ...uploads.map((upload) => upload.fieldId)]);
  const requiredIds = [...activeRequiredFields].filter((fieldId) => plannedIds.has(fieldId));
  const accepted = (fieldId: string) => results.some((item) => item.fieldId === fieldId
    && (item.status === "verified" || item.reasonCode === "user_file_preserved"));
  verifiedFillCompleted = results.some((item) => item.status === "verified")
    && requiredIds.every(accepted);
  return results;
}

chrome.runtime.onMessage.addListener((message, _sender, respond) => {
  if (!message || typeof message !== "object") return false;
  if (message.type === "inspect") {
    activateFinalizationGuard();
    void waitForNonEmptyForm(
      () => inspectForm(),
      () => new Promise((resolve) => setTimeout(resolve, 250)),
    ).then((snapshot) => {
      activeFingerprint = snapshot.fingerprint;
      activeRequiredFields = new Set(snapshot.fields.filter((field) => field.required).map((field) => field.id));
      respond({ ok: true, snapshot });
    }).catch((error) => {
      releaseFinalizationGuard();
      respond({ ok: false, error: error instanceof Error ? error.message : String(error) });
    });
    return true;
  }
  if (message.type === "fill_plan") {
    void applyPlan(message.plan, Array.isArray(message.uploads) ? message.uploads : []).then((results) => respond({ ok: true, results })).catch((error) => {
      respond({ ok: false, error: error instanceof Error ? error.message : String(error) });
    });
    return true;
  }
  if (message.type === "release_for_review") {
    if (message.connectionCheck !== true && !verifiedFillCompleted) {
      respond({ ok: false, error: "verified_fill_required" });
      return false;
    }
    releaseFinalizationGuard();
    respond({ ok: true });
  }
  return false;
});
