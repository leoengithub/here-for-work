import { inspectForm } from "./ats";
import type { FileUploadInstruction, FillPlan } from "./contracts";
import { applyFillPlan } from "./fill";
import { waitForNonEmptyForm } from "./form-readiness";
import { installFinalizationGuard } from "./guard";

let activeFingerprint: string | null = null;
let releaseGuard: (() => void) | null = null;

function activateFinalizationGuard(): void {
  releaseGuard?.();
  releaseGuard = installFinalizationGuard();
}

function releaseFinalizationGuard(): void {
  releaseGuard?.();
  releaseGuard = null;
  activeFingerprint = null;
}

async function applyPlan(plan: FillPlan, uploads: FileUploadInstruction[] = []) {
  return applyFillPlan(plan, activeFingerprint, document, uploads);
}

chrome.runtime.onMessage.addListener((message, _sender, respond) => {
  if (!message || typeof message !== "object") return false;
  if (message.type === "inspect") {
    activateFinalizationGuard();
    void waitForNonEmptyForm(
      () => inspectForm(),
      () => new Promise((resolve) => setTimeout(resolve, 250)),
    ).catch((error) => {
      if (message.allowEmpty === true) return inspectForm();
      throw error;
    }).then((snapshot) => {
      activeFingerprint = snapshot.fingerprint;
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
    releaseFinalizationGuard();
    respond({ ok: true });
  }
  return false;
});
