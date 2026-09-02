import type { AtsFamily, FieldClassification, FormField, FormFlowIssue, FormSnapshot } from "./contracts";
import { PROTOCOL_VERSION } from "./contracts";

const supportedHosts: Record<string, AtsFamily> = {
  "jobs.ashbyhq.com": "ashby",
  "boards.greenhouse.io": "greenhouse",
  "job-boards.greenhouse.io": "greenhouse",
  "job-boards.eu.greenhouse.io": "greenhouse",
  "jobs.lever.co": "lever",
  "jobs.eu.lever.co": "lever",
};

export function detectAts(hostname: string): AtsFamily {
  return supportedHosts[hostname.toLowerCase()] ?? "generic";
}

function text(value: string | null | undefined): string {
  return (value ?? "").replace(/\s+/g, " ").trim();
}

function labelFor(element: HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement): string {
  const labels = Array.from(element.ownerDocument.querySelectorAll<HTMLLabelElement>("label[for]"));
  const explicit = element.id ? labels.find((label) => label.htmlFor === element.id) ?? null : null;
  const wrapping = element.closest("label");
  const labelledBy = element.getAttribute("aria-labelledby")
    ?.split(/\s+/)
    .map((id) => element.ownerDocument.getElementById(id)?.textContent ?? "")
    .filter(Boolean)
    .join(" ");
  const legend = element.closest("fieldset")?.querySelector("legend")?.textContent;
  return text(
    explicit?.innerText
      || explicit?.textContent
      || wrapping?.innerText
      || wrapping?.textContent
      || element.getAttribute("aria-label")
      || labelledBy
      || legend
      || element.getAttribute("placeholder")
      || element.name,
  );
}

function describedText(element: HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement): string {
  return text(element.getAttribute("aria-describedby")
    ?.split(/\s+/)
    .map((id) => element.ownerDocument.getElementById(id)?.textContent ?? "")
    .filter(Boolean)
    .join(" "));
}

function detectedWordLimit(value: string): number | null {
  const match = value.match(/\b(?:up to|max(?:imum)?|limit(?:ed to)?|no more than)\s+(\d{1,5})\s+words?\b/i);
  if (!match) return null;
  const limit = Number(match[1]);
  return Number.isInteger(limit) && limit > 0 && limit <= 3_000 ? limit : null;
}

function detectedSentenceRange(value: string): { minSentences: number | null; maxSentences: number | null } {
  const match = value.match(/\b(\d{1,2})\s*(?:-|–|—|to)\s*(\d{1,2})\s+sentences?\b/i);
  if (!match) return { minSentences: null, maxSentences: null };
  const minimum = Number(match[1]);
  const maximum = Number(match[2]);
  if (!Number.isInteger(minimum) || !Number.isInteger(maximum) || minimum < 1 || maximum < minimum || maximum > 50) {
    return { minSentences: null, maxSentences: null };
  }
  return { minSentences: minimum, maxSentences: maximum };
}

function detectedLanguage(element: HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement): string | null {
  const language = element.closest("[lang]")?.getAttribute("lang")?.trim() ?? "";
  return language.length <= 35 && /^[a-z]{2,8}(?:-[a-z0-9]{1,8})*$/i.test(language) ? language : null;
}

export function isControlVisible(element: Element): boolean {
  for (let current: Element | null = element; current; current = current.parentElement) {
    if (current.hasAttribute("hidden") || current.getAttribute("aria-hidden") === "true") return false;
    const inlineStyle = (current as HTMLElement).style;
    if (inlineStyle?.display === "none" || inlineStyle?.visibility === "hidden" || inlineStyle?.visibility === "collapse") return false;
    const style = current.ownerDocument.defaultView?.getComputedStyle(current);
    if (style?.display === "none" || style?.visibility === "hidden" || style?.visibility === "collapse") return false;
  }
  return true;
}

function flowIssuePriority(issue: FormFlowIssue): number {
  return ["authentication_required", "active_antibot_challenge", "captcha_or_antibot", "custom_widget"].includes(issue) ? 2 : 1;
}

function flowIssues(doc: Document, nativeControls: Array<HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement>, customControls: HTMLElement[]): FormFlowIssue[] {
  const issues = new Set<FormFlowIssue>();
  const visibleControls = nativeControls.filter((control) => isControlVisible(control));
  if (visibleControls.some((control) => control instanceof HTMLInputElement && control.type.toLowerCase() === "password")) {
    issues.add("authentication_required");
  }
  const captcha = Array.from(doc.querySelectorAll<HTMLElement | HTMLIFrameElement>(
    "[data-sitekey], [class*='captcha' i], [id*='captcha' i], iframe[src*='captcha' i], iframe[title*='challenge' i]",
  )).some((element) => isControlVisible(element));
  if (captcha) issues.add("captcha_or_antibot");
  const activeAntibot = Array.from(doc.querySelectorAll<HTMLElement | HTMLIFrameElement>(
    "[aria-modal='true'] iframe[title*='challenge' i], dialog[open] iframe[title*='challenge' i], [data-hfw-active-antibot='true']",
  )).some((element) => isControlVisible(element));
  if (activeAntibot) issues.add("active_antibot_challenge");
  const relevantFrame = Array.from(doc.querySelectorAll<HTMLIFrameElement>("iframe")).some((frame) => {
    if (!isControlVisible(frame)) return false;
    const identity = `${frame.src} ${frame.title} ${frame.name}`.toLowerCase();
    return /apply|application|job|form/.test(identity) && !/captcha|challenge/.test(identity);
  });
  if (relevantFrame) issues.add("embedded_frame");
  if (customControls.some((control) => isControlVisible(control))) issues.add("custom_widget");
  if (visibleControls.some((control) => control.closest("dialog[open], [role='dialog'], [aria-modal='true']"))) {
    issues.add("modal_form");
  }
  const hiddenStepControl = nativeControls.some((control) => {
    if (control instanceof HTMLInputElement && ["hidden", "submit", "button"].includes(control.type.toLowerCase())) return false;
    return !control.disabled && !isControlVisible(control) && Boolean(control.closest("form"));
  });
  if (visibleControls.length > 0 && hiddenStepControl) issues.add("multi_step_form");
  if (visibleControls.filter((control) => !(control instanceof HTMLInputElement && ["hidden", "submit", "button"].includes(control.type.toLowerCase()))).length === 0) {
    issues.add("no_compatible_fields");
  }
  return [...issues].sort((left, right) => left.localeCompare(right));
}

export function classifyField(label: string, inputType: string, autocomplete = ""): { classification: FieldClassification; reason: string } {
  const normalized = `${label} ${inputType} ${autocomplete}`.toLowerCase();
  const semanticLabel = label.trim().toLowerCase();
  if (inputType === "file") {
    if (/\b(resume|résumé|cv|curriculum vitae)\b/.test(semanticLabel) && !/cover letter/.test(semanticLabel)) {
      return { classification: "safe_verified", reason: "May receive only the manifest-verified career-ops PDF." };
    }
    return { classification: "unsupported", reason: "Only an unambiguous resume or CV PDF control is supported." };
  }
  if (/password|social security|passport|national id|date of birth|birth date/.test(normalized)) {
    return { classification: "sensitive", reason: "Identity or account data requires the user." };
  }
  if (/salary|compensation|remuneration|pay expectation/.test(normalized)) {
    return { classification: "compensation", reason: "May receive only a matching canonical career-ops compensation preference; human review remains required." };
  }
  if (/work authori[sz]ation|sponsor|visa|gender|race|ethnic|veteran|disab|pronoun/.test(normalized)) {
    return { classification: "sensitive", reason: "A sensitive declaration requires explicit review." };
  }
  if ((inputType === "text" || inputType === "textarea")
      && /\b(why (?:are you interested|this (?:company|role)|do you want)|tell us about|describe (?:a|an|your)|share (?:a|an) example|problem you solved|challenge you (?:faced|solved)|motivation|por qu[eé]|cu[eé]ntanos|describe (?:un|una|tu))\b/.test(semanticLabel)) {
    return { classification: "grounded_narrative", reason: "May receive only a career-ops grounded draft with source provenance; human review remains required." };
  }
  if (inputType === "checkbox" || inputType === "radio") {
    return { classification: "unverifiable", reason: "A declaration cannot be inferred from control state." };
  }
  const autocompleteTokens = autocomplete.toLowerCase().split(/\s+/);
  if (inputType === "email"
      || inputType === "tel"
      || autocompleteTokens.some((token) => ["name", "given-name", "family-name", "email", "tel", "country", "country-name"].includes(token))
      || /^(first name|given name|last name|family name|surname|full name|your name|name)\b/.test(semanticLabel)
      || /e-?mail|phone|mobile|telephone|linkedin|github|portfolio|personal website|website url|current location|city|country|current company|current employer|current job title|school|university|college|degree|field of study|graduation year/.test(semanticLabel)) {
    return { classification: "safe_verified", reason: "May be filled only from a verified career-ops fact." };
  }
  return { classification: "unknown", reason: "No allowlisted meaning was recognized." };
}

export async function inspectForm(doc: Document = document, pageUrl = new URL(doc.location.href)): Promise<FormSnapshot> {
  if (doc.defaultView?.navigator.webdriver) {
    throw new Error("This browser session reports automation control. Reopen the page in an ordinary Chrome profile and complete it manually.");
  }
  const ats = detectAts(pageUrl.hostname);
  const allControls = Array.from(
    doc.querySelectorAll<HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement>("input, textarea, select"),
  );
  const customControls = Array.from(doc.querySelectorAll<HTMLElement>(
    "[role='combobox']:not(input), [role='listbox'], [contenteditable='true']",
  )).filter((element) => !element.closest("[data-hfw-ignore]"));
  for (const element of allControls) delete element.dataset.hfwFieldId;
  for (const element of customControls) delete element.dataset.hfwFieldId;
  const controls = allControls.filter((element) => {
    const inputType = element instanceof HTMLInputElement ? element.type.toLowerCase() : "";
    return !element.disabled
      && inputType !== "hidden"
      && inputType !== "submit"
      && inputType !== "button"
      && isControlVisible(element);
  });
  const ids = new Map<string, number>();
  const fields: FormField[] = controls.map((element, index) => {
    const label = labelFor(element);
    const helpText = describedText(element);
    const sentenceRange = detectedSentenceRange(`${label} ${helpText}`);
    const inputType = element instanceof HTMLInputElement ? element.type.toLowerCase() : element.tagName.toLowerCase();
    const classification = classifyField(label, inputType, element.getAttribute("autocomplete") ?? "");
    const baseId = element.id || element.name || `field-${index}`;
    const occurrence = ids.get(baseId) ?? 0;
    ids.set(baseId, occurrence + 1);
    const id = occurrence === 0 ? baseId : `${baseId}--${occurrence + 1}`;
    element.dataset.hfwFieldId = id;
    return {
      id,
      label,
      control: element.tagName.toLowerCase() as FormField["control"],
      inputType,
      inputMode: element instanceof HTMLInputElement ? element.inputMode || null : null,
      required: element.required || element.getAttribute("aria-required") === "true",
      options: element instanceof HTMLSelectElement ? Array.from(element.options).map((option) => text(option.text)) : [],
      language: detectedLanguage(element),
      maxLength: element instanceof HTMLInputElement || element instanceof HTMLTextAreaElement
        ? element.maxLength > 0 && element.maxLength <= 12_000 ? element.maxLength : null
        : null,
      maxWords: detectedWordLimit(`${label} ${helpText}`),
      ...sentenceRange,
      ...classification,
    };
  });
  for (const [index, element] of customControls.entries()) {
    if (!isControlVisible(element)) continue;
    const baseId = element.id || element.getAttribute("name") || `unsupported-widget-${index}`;
    const occurrence = ids.get(baseId) ?? 0;
    ids.set(baseId, occurrence + 1);
    const id = occurrence === 0 ? baseId : `${baseId}--${occurrence + 1}`;
    element.dataset.hfwFieldId = id;
    fields.push({
      id,
      label: text(element.getAttribute("aria-label") || element.textContent || element.getAttribute("placeholder")),
      control: "unsupported",
      inputType: element.getAttribute("role") || "contenteditable",
      inputMode: null,
      required: element.getAttribute("aria-required") === "true",
      options: [],
      language: null,
      maxLength: null,
      maxWords: null,
      minSentences: null,
      maxSentences: null,
      classification: "unsupported",
      reason: "This custom widget requires human completion.",
    });
  }
  const issues = flowIssues(doc, allControls, customControls);
  const flow = {
    disposition: issues.some((issue) => flowIssuePriority(issue) === 2)
      ? "human_handoff" as const
      : issues.length > 0
        ? "fallback_eligible" as const
        : "fillable" as const,
    issues,
  };
  const fingerprintInput = JSON.stringify({ ats, url: pageUrl.href, fields, flow });
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(fingerprintInput));
  const fingerprint = Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, "0")).join("");
  return {
    protocolVersion: PROTOCOL_VERSION,
    ats,
    url: pageUrl.href,
    title: text(doc.title),
    fields,
    flow,
    fingerprint,
  };
}
