import type { AtsFamily, FieldClassification, FormField, FormSnapshot } from "./contracts";
import { PROTOCOL_VERSION } from "./contracts";

const supportedHosts: Record<string, AtsFamily> = {
  "jobs.ashbyhq.com": "ashby",
  "boards.greenhouse.io": "greenhouse",
  "job-boards.greenhouse.io": "greenhouse",
  "jobs.lever.co": "lever",
  "jobs.eu.lever.co": "lever",
};

export function detectAts(hostname: string): AtsFamily | null {
  return supportedHosts[hostname.toLowerCase()] ?? null;
}

function text(value: string | null | undefined): string {
  return (value ?? "").replace(/\s+/g, " ").trim();
}

function labelFor(element: HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement): string {
  const labels = Array.from(element.ownerDocument.querySelectorAll<HTMLLabelElement>("label[for]"));
  const explicit = element.id ? labels.find((label) => label.htmlFor === element.id) ?? null : null;
  const wrapping = element.closest("label");
  return text(
    explicit?.innerText
      ?? wrapping?.innerText
      ?? element.getAttribute("aria-label")
      ?? element.getAttribute("placeholder")
      ?? element.name,
  );
}

export function classifyField(label: string, inputType: string): { classification: FieldClassification; reason: string } {
  const normalized = `${label} ${inputType}`.toLowerCase();
  const semanticLabel = label.trim().toLowerCase();
  if (inputType === "file") return { classification: "unsupported", reason: "File attachment stays with the user." };
  if (/password|social security|passport|national id|date of birth|birth date/.test(normalized)) {
    return { classification: "sensitive", reason: "Identity or account data requires the user." };
  }
  if (/salary|compensation|work authori[sz]ation|sponsor|visa|gender|race|ethnic|veteran|disab|pronoun|phone/.test(normalized)) {
    return { classification: "sensitive", reason: "A sensitive declaration requires explicit review." };
  }
  if (inputType === "checkbox" || inputType === "radio") {
    return { classification: "unverifiable", reason: "A declaration cannot be inferred from control state." };
  }
  if (/^(first name|given name|last name|family name|surname|full name|your name|name)\b/.test(semanticLabel) || /e-?mail/.test(semanticLabel)) {
    return { classification: "safe_verified", reason: "May be filled only from a verified career-ops fact." };
  }
  return { classification: "unknown", reason: "No allowlisted meaning was recognized." };
}

export async function inspectForm(doc: Document = document, pageUrl = new URL(doc.location.href)): Promise<FormSnapshot> {
  if (doc.defaultView?.navigator.webdriver) {
    throw new Error("This browser session reports automation control. Reopen the page in an ordinary Chrome profile and complete it manually.");
  }
  const ats = detectAts(pageUrl.hostname);
  if (!ats) throw new Error("This host is not a supported ATS.");
  const controls = Array.from(
    doc.querySelectorAll<HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement>("input, textarea, select"),
  ).filter((element) => {
    const inputType = element instanceof HTMLInputElement ? element.type.toLowerCase() : "";
    return !element.disabled && inputType !== "hidden" && inputType !== "submit" && inputType !== "button";
  });
  const fields: FormField[] = controls.map((element, index) => {
    const label = labelFor(element);
    const inputType = element instanceof HTMLInputElement ? element.type.toLowerCase() : element.tagName.toLowerCase();
    const classification = classifyField(label, inputType);
    const id = element.id || element.name || `field-${index}`;
    element.dataset.hfwFieldId = id;
    return {
      id,
      label,
      control: element.tagName.toLowerCase() as FormField["control"],
      inputType,
      required: element.required || element.getAttribute("aria-required") === "true",
      options: element instanceof HTMLSelectElement ? Array.from(element.options).map((option) => text(option.text)) : [],
      ...classification,
    };
  });
  const fingerprintInput = JSON.stringify({ ats, url: pageUrl.href, fields });
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(fingerprintInput));
  const fingerprint = Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, "0")).join("");
  return {
    protocolVersion: PROTOCOL_VERSION,
    ats,
    url: pageUrl.href,
    title: text(doc.title),
    fields,
    fingerprint,
  };
}
