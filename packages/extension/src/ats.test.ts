import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it, vi } from "vitest";
import { classifyField, detectAts, inspectForm } from "./ats";
import { BROWSER_COMMAND_TYPES } from "./contracts";
import type { FileUploadInstruction, FillPlan } from "./contracts";
import { applyFillPlan } from "./fill";
import { installFinalizationGuard } from "./guard";

function fixture(name: string): Document {
  const html = readFileSync(resolve(import.meta.dirname, `../fixtures/${name}.html`), "utf8");
  return new DOMParser().parseFromString(html, "text/html");
}

describe("ATS trust boundary", () => {
  it("has no terminal browser command", () => {
    expect(BROWSER_COMMAND_TYPES).toEqual(["inspect_request", "fill_plan", "release_for_review", "focus_review"]);
    expect(BROWSER_COMMAND_TYPES.some((command) => /submit|finalize|send/.test(command))).toBe(false);
  });

  it("keeps the checked-in browser protocol command allowlist in sync", () => {
    const schema = JSON.parse(readFileSync(resolve(import.meta.dirname, "../../../contracts/browser-bridge.schema.json"), "utf8"));
    expect(schema.$defs.commandType.enum).toEqual(BROWSER_COMMAND_TYPES);
  });
  it("uses optimized families for known ATS hosts and a generic family everywhere else", () => {
    expect(detectAts("jobs.ashbyhq.com")).toBe("ashby");
    expect(detectAts("job-boards.greenhouse.io")).toBe("greenhouse");
    expect(detectAts("jobs.lever.co")).toBe("lever");
    expect(detectAts("careers.example.com")).toBe("generic");
  });

  it("allows verified CV identity facts while keeping declarations sensitive", () => {
    expect(classifyField("Phone", "tel").classification).toBe("safe_verified");
    expect(classifyField("Will you require sponsorship?", "select").classification).toBe("sensitive");
    expect(classifyField("Expected annual salary (EUR)", "number").classification).toBe("compensation");
    expect(classifyField("Name", "text").classification).toBe("safe_verified");
    expect(classifyField("Email", "email").classification).toBe("safe_verified");
    expect(classifyField("", "email").classification).toBe("safe_verified");
    expect(classifyField("", "text", "given-name").classification).toBe("safe_verified");
    expect(classifyField("LinkedIn URL", "url").classification).toBe("safe_verified");
    expect(classifyField("GitHub URL", "url").classification).toBe("safe_verified");
    expect(classifyField("Portfolio website", "url").classification).toBe("safe_verified");
    expect(classifyField("Resume / CV", "file").classification).toBe("safe_verified");
    expect(classifyField("Cover letter", "file").classification).toBe("unsupported");
    expect(classifyField("Tell us about a front-end problem you solved", "textarea").classification).toBe("grounded_narrative");
    expect(classifyField("Why are you interested in working at Example Co?", "textarea").classification).toBe("grounded_narrative");
    expect(classifyField("Tell us something", "textarea").classification).toBe("unknown");
  });

  for (const [name, url, family] of [
    ["ashby", "https://jobs.ashbyhq.com/acme/role", "ashby"],
    ["greenhouse", "https://job-boards.greenhouse.io/acme/jobs/42", "greenhouse"],
    ["lever", "https://jobs.lever.co/acme/role", "lever"],
  ] as const) {
    it(`inspects the ${name} fixture without terminal controls`, async () => {
      const snapshot = await inspectForm(fixture(name), new URL(url));
      expect(snapshot.ats).toBe(family);
      expect(snapshot.fields.length).toBeGreaterThan(0);
      expect(snapshot.fields.some((field) => field.inputType === "submit")).toBe(false);
    });
  }

  it("inspects an unknown application site and degrades at individual fields", async () => {
    const snapshot = await inspectForm(fixture("generic"), new URL("https://careers.example.com/apply/42"));

    expect(snapshot.ats).toBe("generic");
    expect(snapshot.fields.map((field) => field.id)).toEqual([
      "full-name", "contact", "declaration", "motivation", "answer",
    ]);
    expect(snapshot.fields.map((field) => field.classification)).toEqual([
      "safe_verified", "safe_verified", "unverifiable", "grounded_narrative", "unknown",
    ]);
    expect(snapshot.fields.some((field) => field.inputType === "submit")).toBe(false);
  });

  it("assigns unique field ids when a generic form repeats names", async () => {
    const doc = new DOMParser().parseFromString(
      "<form><input name='item'><input name='item'><button type='submit'>Submit</button></form>",
      "text/html",
    );
    const snapshot = await inspectForm(doc, new URL("https://apply.example.com/form"));

    expect(snapshot.fields.map((field) => field.id)).toEqual(["item", "item--2"]);
  });

  it("inspects only controls on the currently visible multi-step form panel", async () => {
    const doc = new DOMParser().parseFromString(`
      <form>
        <section style="display: none">
          <label for="first-name">First name</label>
          <input id="first-name" autocomplete="given-name">
        </section>
        <section>
          <label for="location">Current location</label>
          <input id="location">
        </section>
      </form>
    `, "text/html");

    const snapshot = await inspectForm(doc, new URL("https://apply.example.com/form"));

    expect(snapshot.fields.map((field) => field.id)).toEqual(["location"]);
    expect(snapshot.flow).toEqual({ disposition: "fallback_eligible", issues: ["multi_step_form"] });
    expect(doc.querySelector<HTMLInputElement>("#first-name")?.dataset.hfwFieldId).toBeUndefined();
    expect(doc.querySelector<HTMLInputElement>("#location")?.dataset.hfwFieldId).toBe("location");
  });

  it("does not fill a control that became hidden after inspection", async () => {
    const doc = new DOMParser().parseFromString(`
      <form><section><label for="email">Email</label><input id="email" type="email"></section></form>
    `, "text/html");
    const snapshot = await inspectForm(doc, new URL("https://apply.example.com/form"));
    const section = doc.querySelector("section");
    section?.setAttribute("hidden", "");
    const plan: FillPlan = {
      protocolVersion: 1,
      snapshotFingerprint: snapshot.fingerprint,
      instructions: [{ fieldId: "email", value: "verified@example.test", classification: "safe_verified" }],
    } as unknown as FillPlan;

    const result = await applyFillPlan(plan, snapshot.fingerprint, doc);

    expect(result).toEqual([{
      fieldId: "email",
      status: "failed",
      reasonCode: "control_hidden",
      reason: "The inspected control is no longer visible.",
      mutated: false,
      readBackSha256: null,
    }]);
    expect(doc.querySelector<HTMLInputElement>("#email")?.value).toBe("");
  });

  it("continues safe fields while skipping a non-safe instruction", async () => {
    const doc = fixture("greenhouse");
    const snapshot = await inspectForm(doc, new URL("https://job-boards.greenhouse.io/acme/jobs/42"));
    const plan = {
      protocolVersion: 1,
      snapshotFingerprint: snapshot.fingerprint,
      instructions: [
        { fieldId: "email", value: "verified@example.test", classification: "safe_verified" },
        { fieldId: "auth", value: "Yes", classification: "sensitive" },
      ],
    } as unknown as FillPlan;

    const result = await applyFillPlan(plan, snapshot.fingerprint, doc);

    expect(result.map((item) => item.status)).toEqual(["verified", "skipped"]);
    expect(doc.querySelector<HTMLInputElement>("#email")?.value).toBe("verified@example.test");
  });

  it("attaches the manifest-verified tailored CV to an unambiguous PDF control", async () => {
    const doc = new DOMParser().parseFromString(`
      <form><label for="resume">Resume / CV</label><input id="resume" type="file" accept="application/pdf"></form>
    `, "text/html");
    const snapshot = await inspectForm(doc, new URL("https://apply.example.com/form"));
    const bytes = Uint8Array.from([37, 80, 68, 70, 45, 49]);
    const digest = await crypto.subtle.digest("SHA-256", bytes);
    const hash = Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, "0")).join("");
    const upload: FileUploadInstruction = {
      fieldId: "resume",
      fileName: "Leonardo_Gomez_Frontend_Engineer.pdf",
      mimeType: "application/pdf",
      contentBase64: btoa(String.fromCharCode(...bytes)),
      sha256: hash,
      classification: "safe_verified",
    };
    const plan: FillPlan = {
      protocolVersion: 1,
      snapshotFingerprint: snapshot.fingerprint,
      instructions: [{
        fieldId: "resume",
        value: "output/role/cv.pdf",
        classification: "safe_verified",
      }],
    };

    const results = await applyFillPlan(plan, snapshot.fingerprint, doc, [upload], (element, file) => {
      Object.defineProperty(element, "files", {
        configurable: true,
        value: { 0: file, length: 1, item: () => file },
      });
    });

    expect(results).toEqual([{
      fieldId: "resume",
      status: "verified",
      reasonCode: "verified",
      reason: null,
      mutated: true,
      readBackSha256: hash,
    }]);
    expect(results.filter((item) => item.fieldId === "resume")).toHaveLength(1);
    expect(doc.querySelector<HTMLInputElement>("#resume")?.files?.item(0)?.name).toBe("Leonardo_Gomez_Frontend_Engineer.pdf");
  });

  it("preserves a CV the user already selected", async () => {
    const doc = new DOMParser().parseFromString(`
      <form><label for="resume">Resume</label><input id="resume" type="file" accept=".pdf"></form>
    `, "text/html");
    const snapshot = await inspectForm(doc, new URL("https://apply.example.com/form"));
    const element = doc.querySelector<HTMLInputElement>("#resume")!;
    const selected = new File(["user"], "my-selected-cv.pdf", { type: "application/pdf" });
    Object.defineProperty(element, "files", {
      configurable: true,
      value: { 0: selected, length: 1, item: () => selected },
    });
    const upload: FileUploadInstruction = {
      fieldId: "resume",
      fileName: "Leonardo_Gomez_Frontend_Engineer.pdf",
      mimeType: "application/pdf",
      contentBase64: "",
      sha256: "a".repeat(64),
      classification: "safe_verified",
    };

    const results = await applyFillPlan({
      protocolVersion: 1,
      snapshotFingerprint: snapshot.fingerprint,
      instructions: [],
    }, snapshot.fingerprint, doc, [upload]);

    expect(results).toEqual([{
      fieldId: "resume",
      status: "skipped",
      reasonCode: "user_file_preserved",
      reason: "The CV already selected by the user was preserved.",
      mutated: false,
      readBackSha256: null,
    }]);
    expect(element.files?.item(0)?.name).toBe("my-selected-cv.pdf");
  });

  it("fills grounded drafts, verified country, and a structured canonical salary for review", async () => {
    const doc = fixture("prep10");
    const snapshot = await inspectForm(doc, new URL("https://careers.example.com/apply/42"));
    expect(snapshot.fields.map((field) => field.classification)).toEqual([
      "grounded_narrative",
      "grounded_narrative",
      "safe_verified",
      "compensation",
      "safe_verified",
    ]);
    expect(snapshot.fields[0]).toMatchObject({ language: "en", maxLength: 600, maxWords: 100, minSentences: 2, maxSentences: 3 });
    expect(snapshot.fields.some((field) => field.inputType === "submit")).toBe(false);

    const bytes = Uint8Array.from([37, 80, 68, 70, 45, 49]);
    const digest = await crypto.subtle.digest("SHA-256", bytes);
    const hash = Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, "0")).join("");
    const results = await applyFillPlan({
      protocolVersion: 1,
      snapshotFingerprint: snapshot.fingerprint,
      instructions: [
        { fieldId: "frontend-story", value: "I traced a slow rendering path to duplicated requests. I reduced the critical work and improved the experience.", classification: "grounded_draft" },
        { fieldId: "company-interest", value: "The role aligns with my verified frontend product work.", classification: "grounded_draft" },
        { fieldId: "work-country", value: "Spain", classification: "safe_verified" },
        { fieldId: "salary", value: "52000", classification: "canonical_preference" },
      ],
    } as unknown as FillPlan, snapshot.fingerprint, doc, [{
      fieldId: "resume",
      fileName: "Leonardo_Gomez_Frontend_Engineer.pdf",
      mimeType: "application/pdf",
      contentBase64: btoa(String.fromCharCode(...bytes)),
      sha256: hash,
      classification: "safe_verified",
    }], (element, file) => {
      Object.defineProperty(element, "files", {
        configurable: true,
        value: { 0: file, length: 1, item: () => file },
      });
    });

    expect(results.map((item) => item.status)).toEqual(["verified", "verified", "verified", "verified", "verified"]);
    expect(doc.querySelector<HTMLTextAreaElement>("#frontend-story")?.value).toContain("slow rendering path");
    expect(doc.querySelector<HTMLTextAreaElement>("#company-interest")?.value).toContain("frontend product work");
    expect(doc.querySelector<HTMLSelectElement>("#work-country")?.value).toBe("Spain");
    expect(doc.querySelector<HTMLInputElement>("#salary")?.value).toBe("52000");
    expect(doc.querySelector<HTMLInputElement>("#resume")?.files?.item(0)?.name).toBe("Leonardo_Gomez_Frontend_Engineer.pdf");
  });

  it("waits for framework rewrites before accepting read-back", async () => {
    const doc = new DOMParser().parseFromString(
      "<form><label for='name'>Name</label><input id='name' autocomplete='name'></form>",
      "text/html",
    );
    const snapshot = await inspectForm(doc, new URL("https://apply.example.com/form"));
    const input = doc.querySelector<HTMLInputElement>("#name")!;
    input.addEventListener("change", () => setTimeout(() => { input.value = "framework rewrite"; }, 0));

    const results = await applyFillPlan({
      protocolVersion: 1,
      snapshotFingerprint: snapshot.fingerprint,
      instructions: [{ fieldId: "name", value: "Safe Value", classification: "safe_verified" }],
    }, snapshot.fingerprint, doc, [], undefined, { settleDelaysMs: [10, 10] });

    expect(results[0]).toMatchObject({ status: "failed", reasonCode: "readback_mismatch", mutated: true });
  });

  it("detects a control replaced after its events settle", async () => {
    const doc = new DOMParser().parseFromString(
      "<form><label for='email'>Email</label><input id='email' type='email'></form>",
      "text/html",
    );
    const snapshot = await inspectForm(doc, new URL("https://apply.example.com/form"));
    const input = doc.querySelector<HTMLInputElement>("#email")!;
    input.addEventListener("change", () => setTimeout(() => {
      const replacement = input.cloneNode() as HTMLInputElement;
      input.replaceWith(replacement);
    }, 0));

    const results = await applyFillPlan({
      protocolVersion: 1,
      snapshotFingerprint: snapshot.fingerprint,
      instructions: [{ fieldId: "email", value: "safe@example.test", classification: "safe_verified" }],
    }, snapshot.fingerprint, doc, [], undefined, { settleDelaysMs: [10, 10] });

    expect(results[0]).toMatchObject({ status: "failed", reasonCode: "control_replaced", mutated: true });
  });

  it("verifies native selects by option text after change events settle", async () => {
    const doc = new DOMParser().parseFromString(
      "<form><label for='country'>Country</label><select id='country'><option value='ES'>Spain</option></select></form>",
      "text/html",
    );
    const snapshot = await inspectForm(doc, new URL("https://apply.example.com/form"));
    const results = await applyFillPlan({
      protocolVersion: 1,
      snapshotFingerprint: snapshot.fingerprint,
      instructions: [{ fieldId: "country", value: "Spain", classification: "safe_verified" }],
    }, snapshot.fingerprint, doc, [], undefined, { settleDelaysMs: [] });

    expect(results[0]).toMatchObject({ status: "verified", reasonCode: "verified" });
    expect(doc.querySelector<HTMLSelectElement>("#country")?.value).toBe("ES");
  });

  it("rejects duplicate planned fields before mutating the page", async () => {
    const doc = new DOMParser().parseFromString(
      "<form><label for='email'>Email</label><input id='email' type='email'></form>",
      "text/html",
    );
    const snapshot = await inspectForm(doc, new URL("https://apply.example.com/form"));
    const plan: FillPlan = {
      protocolVersion: 1,
      snapshotFingerprint: snapshot.fingerprint,
      instructions: [
        { fieldId: "email", value: "one@example.test", classification: "safe_verified" },
        { fieldId: "email", value: "two@example.test", classification: "safe_verified" },
      ],
    };

    await expect(applyFillPlan(plan, snapshot.fingerprint, doc)).rejects.toThrow("invalid_fill_plan_duplicate_field");
    expect(doc.querySelector<HTMLInputElement>("#email")?.value).toBe("");
  });

  it.each([
    ["embedded frame", "<iframe title='Application form' src='https://forms.example.test/apply'></iframe>", "embedded_frame"],
    ["modal", "<div role='dialog'><form><input name='email' type='email'></form></div>", "modal_form"],
  ])("classifies %s flows as fallback eligible", async (_name, html, issue) => {
    const snapshot = await inspectForm(new DOMParser().parseFromString(html, "text/html"), new URL("https://apply.example.com/form"));
    expect(snapshot.flow.disposition).toBe("fallback_eligible");
    expect(snapshot.flow.issues).toContain(issue);
  });

  it.each([
    ["authentication", "<form><input type='password'></form>", "authentication_required"],
    ["CAPTCHA", "<form><iframe title='reCAPTCHA' src='https://captcha.example.test/widget'></iframe><input name='email' type='email'></form>", "captcha_or_antibot"],
    ["custom widget", "<form><input name='email' type='email'><div role='combobox' aria-label='Country'></div></form>", "custom_widget"],
    ["active anti-bot challenge", "<div aria-modal='true'><iframe title='Security challenge'></iframe></div><form><input name='email' type='email'></form>", "active_antibot_challenge"],
  ])("classifies %s as a human handoff", async (_name, html, issue) => {
    const snapshot = await inspectForm(new DOMParser().parseFromString(html, "text/html"), new URL("https://apply.example.com/form"));
    expect(snapshot.flow.disposition).toBe("human_handoff");
    expect(snapshot.flow.issues).toContain(issue);
  });

  it("does not misclassify a passive CAPTCHA iframe as an embedded application form", async () => {
    const snapshot = await inspectForm(new DOMParser().parseFromString(
      "<form><iframe title='reCAPTCHA' src='https://captcha.example.test/widget'></iframe><input name='email' type='email'></form>",
      "text/html",
    ), new URL("https://apply.example.com/form"));
    expect(snapshot.flow.issues).toContain("captcha_or_antibot");
    expect(snapshot.flow.issues).not.toContain("embedded_frame");
  });

  it("fills and settles a safe native field without interacting with an unsupported custom widget", async () => {
    const doc = new DOMParser().parseFromString(
      "<form><input id='email' type='email'><div id='country' role='combobox' aria-label='Country'></div></form>",
      "text/html",
    );
    const snapshot = await inspectForm(doc, new URL("https://apply.example.com/form"));
    const widget = doc.querySelector<HTMLElement>("#country")!;
    const widgetClick = vi.fn();
    widget.addEventListener("click", widgetClick);

    const results = await applyFillPlan({
      protocolVersion: 1,
      snapshotFingerprint: snapshot.fingerprint,
      instructions: [{ fieldId: "email", value: "safe@example.test", classification: "safe_verified" }],
    }, snapshot.fingerprint, doc, [], undefined, { settleDelaysMs: [] });

    expect(results).toHaveLength(1);
    expect(results[0]).toMatchObject({ fieldId: "email", status: "verified" });
    expect(widgetClick).not.toHaveBeenCalled();
    expect(widget.textContent).toBe("");
  });

  it("detects form drift before any fill", async () => {
    const doc = new DOMParser().parseFromString(
      "<form><label for='email'>Email</label><input id='email' type='email'></form>",
      "text/html",
    );
    const snapshot = await inspectForm(doc, new URL("https://apply.example.com/form"));
    const fingerprint = vi.fn().mockResolvedValue("f".repeat(64));
    await expect(applyFillPlan({
      protocolVersion: 1,
      snapshotFingerprint: snapshot.fingerprint,
      instructions: [{ fieldId: "email", value: "safe@example.test", classification: "safe_verified" }],
    }, snapshot.fingerprint, doc, [], undefined, { currentFingerprint: fingerprint })).rejects.toThrow("form_drift_before_fill");
    expect(doc.querySelector<HTMLInputElement>("#email")?.value).toBe("");
  });

  it("guards the terminal control only until release", () => {
    const doc = fixture("ashby");
    const release = installFinalizationGuard(doc);
    const guarded = new Event("submit", { bubbles: true, cancelable: true });
    doc.querySelector("form")?.dispatchEvent(guarded);
    expect(guarded.defaultPrevented).toBe(true);

    release();
    const released = new Event("submit", { bubbles: true, cancelable: true });
    doc.querySelector("form")?.dispatchEvent(released);
    expect(released.defaultPrevented).toBe(false);
  });

  it("blocks direct finalization attempted by page listeners during fill and restores only on release", async () => {
    const doc = document;
    const priorMarkup = doc.body.innerHTML;
    doc.body.innerHTML = "<form><input id='email' type='email'><button type='submit'>Submit application</button></form>";
    const FormConstructor = doc.defaultView!.HTMLFormElement;
    const prototype = FormConstructor.prototype;
    const originalSubmit = Object.getOwnPropertyDescriptor(prototype, "submit");
    const originalRequestSubmit = Object.getOwnPropertyDescriptor(prototype, "requestSubmit");
    const directSubmit = vi.fn();
    const directRequestSubmit = vi.fn();
    Object.defineProperty(prototype, "submit", { configurable: true, writable: true, value: directSubmit });
    Object.defineProperty(prototype, "requestSubmit", { configurable: true, writable: true, value: directRequestSubmit });
    const form = doc.querySelector<HTMLFormElement>("form")!;
    const button = doc.querySelector<HTMLButtonElement>("button")!;
    const terminalClick = vi.fn();
    button.addEventListener("click", (event) => {
      event.preventDefault();
      terminalClick();
    });
    const attemptFinalization = () => {
      form.submit();
      form.requestSubmit();
      button.click();
    };
    form.addEventListener("input", attemptFinalization);
    form.addEventListener("change", attemptFinalization);

    try {
      const release = installFinalizationGuard(doc);
      const snapshot = await inspectForm(doc, new URL("https://apply.example.com/form"));
      const results = await applyFillPlan({
        protocolVersion: 1,
        snapshotFingerprint: snapshot.fingerprint,
        instructions: [{ fieldId: "email", value: "safe@example.test", classification: "safe_verified" }],
      }, snapshot.fingerprint, doc, [], undefined, { settleDelaysMs: [] });

      expect(results[0]?.status).toBe("verified");
      expect(directSubmit).not.toHaveBeenCalled();
      expect(directRequestSubmit).not.toHaveBeenCalled();
      expect(terminalClick).not.toHaveBeenCalled();

      release();
      form.submit();
      form.requestSubmit();
      button.click();
      expect(directSubmit).toHaveBeenCalledTimes(1);
      expect(directRequestSubmit).toHaveBeenCalledTimes(1);
      expect(terminalClick).toHaveBeenCalledTimes(1);
    } finally {
      if (originalSubmit) Object.defineProperty(prototype, "submit", originalSubmit);
      if (originalRequestSubmit) Object.defineProperty(prototype, "requestSubmit", originalRequestSubmit);
      doc.body.innerHTML = priorMarkup;
    }
  });
});
