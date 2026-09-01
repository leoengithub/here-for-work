import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
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
    const plan = {
      protocolVersion: 1,
      snapshotFingerprint: snapshot.fingerprint,
      instructions: [{ fieldId: "email", value: "verified@example.test", classification: "safe_verified" }],
    } as unknown as FillPlan;

    const result = await applyFillPlan(plan, snapshot.fingerprint, doc);

    expect(result).toEqual([{ fieldId: "email", status: "skipped", reason: "Control is unavailable or unsupported." }]);
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

    expect(results).toEqual([{ fieldId: "resume", status: "verified", reason: null }]);
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
      reason: "The CV already selected by the user was preserved.",
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
});
