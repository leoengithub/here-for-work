import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdtemp, mkdir, readFile, stat, unlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const adapter = fileURLToPath(new URL("./adapter.mjs", import.meta.url));
const { fetchJob } = await import("./adapter.mjs");

function request(payload, env = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, [adapter], {
      env: { ...process.env, ...env },
      stdio: ["pipe", "pipe", "pipe"],
    });
    const output = [];
    child.stdout.on("data", (chunk) => output.push(chunk));
    child.on("error", reject);
    child.on("close", () => resolve(JSON.parse(Buffer.concat(output).toString("utf8"))));
    child.stdin.end(`${JSON.stringify(payload)}\n`);
  });
}

test("capabilities expose the fixed safety boundary", async () => {
  const response = await request({
    id: "capabilities",
    protocolVersion: 1,
    operation: "capabilities.get",
    input: {},
  });

  assert.equal(response.ok, true);
  assert.deepEqual(response.result.forbiddenOperations, [
    "application.submit",
    "application.finalize",
    "message.send",
    "shell.run",
    "browser.command",
  ]);
  assert.equal(response.result.sourceOfTruth.applicationHistory, "career-ops");
});

test("generic discovery resolves a source listing to its application form", async (context) => {
  const requested = [];
  context.mock.method(globalThis, "fetch", async (input) => {
    const url = String(input);
    requested.push(url);
    if (url === "https://listing.example.test/jobs/42") {
      return new Response(`<!doctype html><html><body>
        <h1>Frontend Engineer</h1>
        <a href="https://apply.example.test/forms/42?source=listing">Apply now</a>
      </body></html>`, { status: 200, headers: { "content-type": "text/html" } });
    }
    return new Response(`<!doctype html><html><head>
      <script type="application/ld+json">${JSON.stringify({
        "@type": "JobPosting",
        title: "Senior Frontend Engineer",
        description: "Build accessible React and TypeScript product interfaces, collaborate with design and backend partners, improve automated testing, and own reliable delivery for a distributed product team across Europe.",
        hiringOrganization: { name: "Example Co" },
        jobLocation: { address: { addressLocality: "Madrid", addressCountry: "Spain" } },
        datePosted: "2026-08-30",
      })}</script>
      </head><body><form><input name="name"><input name="email"><button type="submit">Submit</button></form></body></html>`, {
      status: 200,
      headers: { "content-type": "text/html" },
    });
  });

  const job = await fetchJob({
    company: "Example Co",
    title: "Frontend Engineer",
    location: "Remote",
    url: "https://listing.example.test/jobs/42",
  });

  assert.deepEqual(requested, [
    "https://listing.example.test/jobs/42",
    "https://apply.example.test/forms/42?source=listing",
  ]);
  assert.equal(job.provider, "generic");
  assert.equal(job.url, "https://apply.example.test/forms/42?source=listing");
  assert.equal(job.sourceUrl, "https://listing.example.test/jobs/42");
  assert.equal(job.title, "Senior Frontend Engineer");
  assert.equal(job.descriptionAvailable, true);
});

test("generic discovery preserves the requested application URL when a page cannot be fetched", async (context) => {
  context.mock.method(globalThis, "fetch", async () => {
    throw new Error("login required");
  });

  const job = await fetchJob({
    company: "Private Co",
    title: "Product Engineer",
    location: "Remote",
    url: "https://careers.example.test/private-role",
  });

  assert.equal(job.provider, "generic");
  assert.equal(job.url, "https://careers.example.test/private-role");
  assert.equal(job.descriptionAvailable, false);
  assert.match(job.description, /could not retrieve a complete job description/);
});

test("generic discovery does not follow redirects to a local target", async (context) => {
  let calls = 0;
  context.mock.method(globalThis, "fetch", async () => {
    calls += 1;
    return new Response(null, {
      status: 302,
      headers: { location: "https://localhost/private" },
    });
  });

  const job = await fetchJob({
    company: "Example Co",
    title: "Engineer",
    location: "Remote",
    url: "https://careers.example.test/role",
  });

  assert.equal(calls, 1);
  assert.equal(job.url, "https://careers.example.test/role");
  assert.equal(job.descriptionAvailable, false);
});

async function fakeCareerOps() {
  const root = await mkdtemp(join(tmpdir(), "hfw-career-ops-"));
  const staging = join(root, "staging");
  await mkdir(join(root, "data"), { recursive: true });
  await mkdir(join(root, "modes/heuristics"), { recursive: true });
  await mkdir(join(root, "config"), { recursive: true });
  await mkdir(join(root, "providers"), { recursive: true });
  await mkdir(join(root, "node_modules/playwright"), { recursive: true });
  await mkdir(staging, { recursive: true });
  await writeFile(join(root, "data/applications.md"), "# fixture\n");
  await writeFile(join(root, "state.json"), "[]\n");
  for (const path of ["modes/_shared.md", "modes/oferta.md", "modes/pdf.md", "modes/apply.md", "modes/_profile.md", "modes/heuristics/recruiter-side.md", "cv.md"]) {
    await writeFile(join(root, path), `# ${path}\nVerified fixture facts for Test Candidate at Example Co.\n`);
  }
  await writeFile(join(root, "config/profile.yml"), "name: Test Candidate\nemail: test@example.test\nlocation: Madrid\n");
  await writeFile(join(root, "config/cv-facts.json"), "{\"allow_metrics\":[],\"allow_facts\":[],\"forbidden_phrases\":[],\"warn_phrases\":[]}\n");
  await writeFile(join(root, "providers/_http.mjs"), "export async function fetchJson() { return {}; }\n");
  const chromiumFixture = join(root, "fixture-chromium");
  await writeFile(chromiumFixture, "#!/bin/sh\nexit 0\n", { mode: 0o755 });
  await writeFile(join(root, "node_modules/playwright/index.mjs"), `
    export const chromium = { executablePath: () => ${JSON.stringify(chromiumFixture)} };
  `);
  await writeFile(join(root, "providers/ashby.mjs"), `
    export default {
      async fetch(entry) {
        return [{
          title: "Frontend Engineer",
          company: entry.name,
          location: "Remote Europe",
          url: "https://jobs.ashbyhq.com/example/role-1",
          description: "A live frontend engineering role requiring React, TypeScript, accessible interfaces, product judgment, testing, and collaboration across a distributed European team. This fixture is deliberately long enough for the bounded preparation contract.",
          postedAt: Date.parse("2026-08-30T08:00:00Z")
        }];
      }
    };
  `);
  await writeFile(join(root, "tracker.mjs"), `
    import { readFile } from "node:fs/promises";
    const rows = JSON.parse(await readFile(new URL("./state.json", import.meta.url), "utf8"));
    process.stdout.write(JSON.stringify(rows));
  `);
  await writeFile(join(root, "merge-tracker.mjs"), `
    import { readFile, readdir, writeFile } from "node:fs/promises";
    const stateUrl = new URL("./state.json", import.meta.url);
    const rows = JSON.parse(await readFile(stateUrl, "utf8"));
    const files = (await readdir(process.env.CAREER_OPS_ADDITIONS)).filter((name) => name.endsWith(".tsv"));
    for (const name of files) {
      const parts = (await readFile(process.env.CAREER_OPS_ADDITIONS + "/" + name, "utf8")).trim().split("\\t");
      if (!rows.some((row) => row.notes.includes(parts[8]))) rows.push({
        id: Number(parts[0]), date: parts[1], company: parts[2], role: parts[3],
        score: parts[4], status: parts[5], pdf: parts[6], report: parts[7], notes: parts[8]
      });
    }
    await writeFile(stateUrl, JSON.stringify(rows));
  `);
  await writeFile(join(root, "set-status.mjs"), `
    import { readFile, writeFile } from "node:fs/promises";
    const stateUrl = new URL("./state.json", import.meta.url);
    const rows = JSON.parse(await readFile(stateUrl, "utf8"));
    const rowId = Number(process.argv[process.argv.indexOf("--row") + 1]);
    const status = process.argv[process.argv.indexOf("--row") + 2];
    const note = process.argv[process.argv.indexOf("--note") + 1];
    const row = rows.find((candidate) => candidate.id === rowId);
    if (!row) process.exit(2);
    row.status = status;
    if (!row.notes.includes(note)) row.notes += "; " + note;
    await writeFile(stateUrl, JSON.stringify(rows));
    process.stdout.write(JSON.stringify({ ok: true }));
  `);
  await writeFile(join(root, "reserve-report-num.mjs"), `
    if (process.argv.includes("--release")) process.exit(0);
    process.stdout.write("042\\n");
  `);
  await writeFile(join(root, "build-cv-html.mjs"), `
    import { mkdir, readFile, writeFile } from "node:fs/promises";
    import { dirname } from "node:path";
    const input = process.argv[2];
    const output = process.argv[3];
    const payload = JSON.parse(await readFile(input, "utf8"));
    await mkdir(dirname(output), { recursive: true });
    await writeFile(output, "<html><body>" + payload.candidate.name + " — " + payload.summary + "</body></html>");
  `);
  await writeFile(join(root, "verify-cv-facts.mjs"), `process.stdout.write(JSON.stringify({ verdict: "pass" }));\n`);
  await writeFile(join(root, "generate-pdf.mjs"), `
    import { mkdir, writeFile } from "node:fs/promises";
    import { dirname } from "node:path";
    if (!process.argv.includes("--allow-reorder")) {
      console.error("tailored CV generation must opt into the career-ops reorder guard");
      process.exit(2);
    }
    const output = process.argv[3];
    await mkdir(dirname(output), { recursive: true });
    await writeFile(output, "%PDF-1.4 fixture");
  `);
  await writeFile(join(root, "application-answers.mjs"), `
    import { readFile, writeFile } from "node:fs/promises";
    const value = (name) => process.argv[process.argv.indexOf(name) + 1];
    const reportPath = value("--report");
    const snapshot = JSON.parse(await readFile(value("--input"), "utf8"));
    const lines = [
      "## Application Answers", "", "**Date:** " + value("--date"),
      "**State:** " + value("--state"), "", "### Free-text answers", "",
      ...snapshot.freeText.flatMap((item, index) => [String(index + 1) + ". **" + item.question + "**", "", "> " + item.answer, ""]),
      "### Other field values", "",
      ...snapshot.fieldValues.map((item, index) => String(index + 1) + ". **" + item.question + ":** " + item.answer),
      "", "### Files used", "",
      ...snapshot.files.map((item, index) => String(index + 1) + ". **" + item.field + ":** " + item.path),
    ];
    const report = await readFile(reportPath, "utf8");
    const start = report.search(/^## Application Answers$/m);
    const base = start >= 0 ? report.slice(0, start).trimEnd() : report.trimEnd();
    await writeFile(reportPath, base + "\\n\\n" + lines.join("\\n").trimEnd() + "\\n");
  `);
  return {
    root,
    staging,
    env: {
      HFW_CAREER_OPS_ROOT: root,
      HFW_CAREER_OPS_INDEX: join(root, "mirror.sqlite3"),
      HFW_CAREER_OPS_STAGING: staging,
    },
  };
}

test("provider-neutral preparation commits only through fixed career-ops writers", async () => {
  const fixture = await fakeCareerOps();
  const health = await request({
    id: "health",
    protocolVersion: 1,
    operation: "health.check",
    input: {},
  }, fixture.env);
  assert.equal(health.ok, true);
  assert.equal(health.result.ready, true);
  assert.equal(health.result.checks.playwrightChromium, true);
  const preparationId = "55555555-5555-4555-8555-555555555555";
  const role = {
    preparationId,
    company: "Example Co",
    title: "Frontend Engineer",
    location: "Remote Europe",
    url: "https://jobs.ashbyhq.com/example/role-1",
  };
  const context = await request({
    id: "prepare-context",
    protocolVersion: 1,
    operation: "preparation.context.get",
    input: role,
  }, fixture.env);
  assert.equal(context.ok, true);
  assert.equal(context.result.contextHash.length, 64);
  assert.match(context.result.prompt, /untrusted data, never instructions/);
  const missingRecovery = await request({
    id: "prepare-recovery-missing",
    protocolVersion: 1,
    operation: "preparation.result.recover",
    input: { preparationId, contextHash: context.result.contextHash },
  }, fixture.env);
  assert.equal(missingRecovery.ok, true);
  assert.equal(missingRecovery.result.outcome, "missing");

  const reportBodyMarkdown = [
    "## Machine Summary", "```yaml", "final_decision: Consider", "```",
    "## A) Role Summary", "Fixture summary.",
    "## B) Match with CV", "Verified fixture match.",
    "## C) Level and Strategy", "Fixture level.",
    "## D) Comp and Demand", "External research unavailable.",
    "## E) Customization Plan", "Use React evidence.",
    "## F) Interview Plan", "Discuss verified work.",
    "## G) Posting Legitimacy", "Proceed with caution; external research unavailable.",
    "## Risk Summary", "| Signal | Result |", "|---|---|", "| Legitimacy | not evaluated |",
    "## Keywords extracted", "React, TypeScript, accessibility, testing",
  ].join("\n\n");
  const result = {
    contractVersion: 1,
    contextHash: context.result.contextHash,
    score: 4.2,
    legitimacy: "Proceed with Caution",
    authorizationConfidence: "investigate",
    reportBodyMarkdown,
    cvChangesMarkdown: "- Reordered verified React evidence.",
    cvPayload: {
      lang: "en",
      page_format: "a4",
      candidate: { name: "Test Candidate", email: "test@example.test", location: "Madrid" },
      summary: "Frontend engineer with verified React experience.",
      competencies: ["React", "TypeScript"],
      experience: [{ company: "Example Co", role: "Engineer", dates: "2020 - 2026", bullets: ["Built accessible interfaces."] }],
      projects: [], education: [], certifications: [], skills: [{ category: "Web", items: ["React", "TypeScript"] }],
    },
  };
  const commit = await request({
    id: "prepare-commit",
    protocolVersion: 1,
    operation: "preparation.result.commit",
    input: { ...role, eventDate: "2026-08-30", job: context.result.job, result },
  }, fixture.env);
  assert.equal(commit.ok, true);
  assert.equal(commit.result.artifacts.report.path, "reports/042-example-co-2026-08-30.md");
  assert.equal(commit.result.artifacts.cvPdf.sha256.length, 64);
  const stagedResultPath = join(fixture.staging, preparationId, "provider-result.json");
  assert.equal((await stat(stagedResultPath)).mode & 0o777, 0o600);
  const recovered = await request({
    id: "prepare-recovery-completed",
    protocolVersion: 1,
    operation: "preparation.result.recover",
    input: { preparationId, contextHash: context.result.contextHash },
  }, fixture.env);
  assert.equal(recovered.ok, true);
  assert.equal(recovered.result.outcome, "completed");
  assert.equal(recovered.result.result.score, result.score);
  const report = await readFile(join(fixture.root, commit.result.artifacts.report.path), "utf8");
  assert.match(report, /Deferred until HereForWork inspects the live application form/);
  const rows = JSON.parse(await readFile(join(fixture.root, "state.json"), "utf8"));
  assert.equal(rows.length, 1);
  assert.equal(rows[0].status, "Evaluated");
  assert.equal(rows[0].report, "[042](reports/042-example-co-2026-08-30.md)");

  const replay = await request({
    id: "prepare-commit-replay",
    protocolVersion: 1,
    operation: "preparation.result.commit",
    input: {
      ...role,
      eventDate: "2026-08-30",
      job: context.result.job,
      result: {
        ...result,
        score: 1.1,
        reportBodyMarkdown: reportBodyMarkdown.replace("Fixture summary.", "A different retry result."),
      },
    },
  }, fixture.env);
  assert.equal(replay.ok, true);
  assert.equal(replay.result.artifacts.report.sha256, commit.result.artifacts.report.sha256);
  assert.doesNotMatch(
    await readFile(join(fixture.root, replay.result.artifacts.report.path), "utf8"),
    /A different retry result/,
  );

  const snapshot = {
    protocolVersion: 1,
    ats: "ashby",
    url: role.url,
    title: "Example Co application",
    fingerprint: "d".repeat(64),
    fields: [
      { id: "email", label: "Email", control: "input", inputType: "email", required: true, options: [], classification: "safe_verified", reason: "Verified profile fact." },
      { id: "why", label: "Why this role?", control: "textarea", inputType: "textarea", required: true, options: [], classification: "unknown", reason: "Needs grounded drafting." },
      { id: "phone", label: "Phone", control: "input", inputType: "tel", required: false, options: [], classification: "sensitive", reason: "Requires user review." },
      { id: "unlabeled", label: "", control: "input", inputType: "text", required: false, options: [], classification: "unknown", reason: "No visible label." },
    ],
  };
  const answerContext = await request({
    id: "answer-context",
    protocolVersion: 1,
    operation: "answers.context.get",
    input: { preparationId, reportPath: commit.result.artifacts.report.path, snapshot },
  }, fixture.env);
  assert.equal(answerContext.ok, true);
  const answerResult = {
    contractVersion: 1,
    contextHash: answerContext.result.contextHash,
    answers: [
      { fieldId: "email", answer: "untrusted-model@example.test", provenance: ["config/profile.yml"] },
      { fieldId: "why", answer: "The role matches my verified React and accessibility work.", provenance: ["cv.md", "reports/042-example-co-2026-08-30.md"] },
      { fieldId: "phone", answer: null, provenance: [] },
      { fieldId: "unlabeled", answer: null, provenance: [] },
    ],
  };
  const answers = await request({
    id: "answer-validate",
    protocolVersion: 1,
    operation: "answers.result.validate",
    input: { preparationId, reportPath: commit.result.artifacts.report.path, snapshot, result: answerResult },
  }, fixture.env);
  assert.equal(answers.ok, true);
  assert.deepEqual(answers.result.fillPlan.instructions, [
    { fieldId: "email", value: "test@example.test", classification: "safe_verified" },
  ]);
  assert.equal(answers.result.reviewItems.find((item) => item.fieldId === "why").decision, "suggest");
  assert.equal(answers.result.reviewItems.find((item) => item.fieldId === "phone").decision, "skip");

  const answerCommit = await request({
    id: "answer-commit",
    protocolVersion: 1,
    operation: "answers.result.commit",
    input: {
      preparationId,
      reportPath: commit.result.artifacts.report.path,
      contextHash: answers.result.contextHash,
      reviewItems: answers.result.reviewItems,
      fillResults: [
        { fieldId: "email", status: "verified", reason: null },
        { fieldId: "why", status: "skipped", reason: "User review required." },
        { fieldId: "phone", status: "skipped", reason: "Sensitive field." },
        { fieldId: "unlabeled", status: "skipped", reason: "No visible label." },
      ],
      cvPdfPath: commit.result.artifacts.cvPdf.path,
      eventDate: "2026-08-30",
    },
  }, fixture.env);
  assert.equal(answerCommit.ok, true);
  assert.equal(answerCommit.result.report.sha256.length, 64);
  const reportWithAnswers = await readFile(join(fixture.root, commit.result.artifacts.report.path), "utf8");
  assert.match(reportWithAnswers, /## Application Answers/);
  assert.match(reportWithAnswers, /\*\*Email:\*\* test@example\.test/);
  assert.match(reportWithAnswers, /verified React and accessibility work/);
  assert.doesNotMatch(reportWithAnswers, /Phone:\*\*/);
  assert.doesNotMatch(reportWithAnswers, /\*\*:\*\*/);
});

test("preparation accepts career-ops profiles without optional cv-facts config", async () => {
  const fixture = await fakeCareerOps();
  await unlink(join(fixture.root, "config/cv-facts.json"));

  const context = await request({
    id: "prepare-with-default-fact-gate",
    protocolVersion: 1,
    operation: "preparation.context.get",
    input: {
      preparationId: "77777777-7777-4777-8777-777777777777",
      company: "Example Co",
      title: "Frontend Engineer",
      location: "Remote Europe",
      url: "https://jobs.ashbyhq.com/example/role-1",
    },
  }, fixture.env);

  assert.equal(context.ok, true);
  assert.equal(context.result.sourceHashes["config/cv-facts.json"], undefined);
  assert.doesNotMatch(context.result.prompt, /career_ops_source path="config\/cv-facts\.json"/);
});

test("Discard, Undo, and confirmed Applied use canonical writers idempotently", async () => {
  const fixture = await fakeCareerOps();
  const discardKey = "11111111-1111-4111-8111-111111111111";
  const role = {
    company: "Example Co",
    title: "Frontend Engineer",
    location: "Remote Europe",
    url: "https://jobs.example.test/role-1",
    eventDate: "2026-08-30",
  };
  const discard = await request({
    id: "discard",
    protocolVersion: 1,
    operation: "role.discard",
    input: { ...role, idempotencyKey: discardKey, reason: "Not a fit" },
  }, fixture.env);
  assert.equal(discard.ok, true);
  assert.equal(discard.result.effect.status, "Discarded");

  const replay = await request({
    id: "discard-replay",
    protocolVersion: 1,
    operation: "role.discard",
    input: { ...role, idempotencyKey: discardKey, reason: "Not a fit" },
  }, fixture.env);
  assert.equal(replay.ok, true);
  assert.equal(JSON.parse(await readFile(join(fixture.root, "state.json"), "utf8")).length, 1);

  const undoKey = "22222222-2222-4222-8222-222222222222";
  const undo = await request({
    id: "undo",
    protocolVersion: 1,
    operation: "role.discard.undo",
    input: {
      idempotencyKey: undoKey,
      discardEffectKey: discardKey,
      trackerId: discard.result.effect.trackerId,
      eventDate: "2026-08-30",
    },
  }, fixture.env);
  assert.equal(undo.ok, true);
  assert.equal(undo.result.effect.status, "Evaluated");

  const appliedKey = "33333333-3333-4333-8333-333333333333";
  const applied = await request({
    id: "applied",
    protocolVersion: 1,
    operation: "application.applied.confirm",
    input: {
      ...role,
      idempotencyKey: appliedKey,
      trackerId: discard.result.effect.trackerId,
      userConfirmed: true,
    },
  }, fixture.env);
  assert.equal(applied.ok, true);
  assert.equal(applied.result.effect.status, "Applied");

  const staleDiscard = await request({
    id: "stale-discard",
    protocolVersion: 1,
    operation: "role.discard",
    input: { ...role, idempotencyKey: "44444444-4444-4444-8444-444444444444" },
  }, fixture.env);
  assert.equal(staleDiscard.ok, false);
  assert.match(staleDiscard.error.message, /already Applied/);
});

test("Applied is rejected without explicit user confirmation", async () => {
  const response = await request({
    id: "not-confirmed",
    protocolVersion: 1,
    operation: "application.applied.confirm",
    input: {},
  });
  assert.equal(response.ok, false);
  assert.match(response.error.message, /explicit outcome confirmation/);
});

test("unknown operations are rejected", async () => {
  const response = await request({
    id: "unknown",
    protocolVersion: 1,
    operation: "shell.run",
    input: {},
  });

  assert.equal(response.ok, false);
  assert.match(response.error.message, /Unsupported operation/);
});
