import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdir, readFile, readdir, realpath, stat, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, relative } from "node:path";
import { test } from "node:test";

import {
  commitPreparationTransaction,
  PreparationTransactionError,
} from "./preparation-transaction.mjs";

const PDF = Buffer.from(`%PDF-1.4
1 0 obj
<< /Type /Catalog /Pages 2 0 R >>
endobj
2 0 obj
<< /Type /Pages /Kids [3 0 R] /Count 1 >>
endobj
3 0 obj
<< /Type /Page /Parent 2 0 R >>
endobj
trailer
<< /Root 1 0 R >>
%%EOF
`);

const digest = (value) => createHash("sha256").update(value).digest("hex");

function resultFor(contextHash) {
  return {
    contractVersion: 1,
    contextHash,
    score: 4.2,
    legitimacy: "Proceed with Caution",
    authorizationConfidence: "investigate",
    reportBodyMarkdown: "## Machine Summary\n\nFixture report body that is intentionally long enough for the provider contract.\n\n## A) Role Summary\n\nFixture.\n\n## B) Match with CV\n\nFixture.\n\n## C) Level and Strategy\n\nFixture.\n\n## D) Comp and Demand\n\nFixture.\n\n## E) Customization Plan\n\nFixture.\n\n## F) Interview Plan\n\nFixture.\n\n## G) Posting Legitimacy\n\nFixture.\n\n## Risk Summary\n\nFixture.\n\n## Keywords extracted\n\nReact",
    cvChangesMarkdown: "- Reordered verified React evidence.",
    cvPayload: {
      lang: "en",
      page_format: "a4",
      candidate: { name: "Test Candidate", email: "test@example.test", location: "Madrid" },
      summary: "Frontend engineer with verified React experience.",
      competencies: ["React"],
      experience: [],
      projects: [],
      education: [],
      certifications: [],
      skills: [],
    },
  };
}

async function harness(options = {}) {
  const base = await import("node:fs/promises").then(({ mkdtemp }) => mkdtemp(join(tmpdir(), "hfw-preparation-transaction-")));
  const root = join(base, "career-ops");
  const stagingRoot = join(base, "adapter-state");
  await mkdir(join(root, "data"), { recursive: true });
  await mkdir(join(root, "reports"), { recursive: true });
  await mkdir(stagingRoot, { recursive: true });
  await writeFile(join(root, "data/applications.md"), "# fixture\n");
  await writeFile(join(root, "cv.md"), "verified source v1\n");
  const canonicalRoot = await realpath(root);
  const source = { revision: "v1" };
  const records = [];
  const calls = [];
  let historyCalls = 0;
  let released = 0;
  const role = {
    company: "Example Co",
    title: "Frontend Engineer",
    location: "Remote Europe",
    url: "https://jobs.example.test/role-1",
  };
  const job = {
    title: role.title,
    company: role.company,
    location: role.location,
    url: role.url,
    sourceUrl: role.url,
    description: "A sufficiently long live job description covering React, TypeScript, accessibility, testing, collaboration, and reliable product delivery across a distributed European team.",
    descriptionAvailable: true,
    postedAt: null,
    provider: "generic",
  };
  const contextHash = () => digest(source.revision);
  const input = {
    preparationId: options.preparationId ?? "11111111-1111-4111-8111-111111111111",
    eventDate: "2026-09-01",
    ...role,
    job,
    result: resultFor(contextHash()),
  };
  const fallbackPath = join(base, "reviewed.pdf");
  const fallbackBytes = options.fallbackBytes ?? PDF;
  await writeFile(fallbackPath, fallbackBytes);

  const historyRecords = async () => {
    historyCalls += 1;
    if (options.trackerDriftAt === historyCalls) {
      records.push({
        id: 7,
        status: "Evaluated",
        report: "—",
        notes: "external writer",
      });
    }
    if (options.trackerFileDriftAt === historyCalls) {
      await writeFile(join(root, "data/applications.md"), "# fixture\nexternal writer\n");
    }
    return structuredClone(records);
  };
  const runCareerOpsScript = async (script, args, env = {}) => {
    calls.push({ script, args, env });
    if (options.failScript === script) {
      const error = new Error(`${"x".repeat(80_000)} /Users/private/name user@example.test`);
      error.exitCode = options.exitCode ?? 9;
      throw error;
    }
    if (script === "reserve-report-num.mjs") {
      if (args[0] === "--release") {
        released += 1;
        return { output: "", diagnostics: "" };
      }
      return { output: "042\n", diagnostics: "" };
    }
    if (script === "build-cv-html.mjs") {
      await mkdir(dirname(args[1]), { recursive: true });
      await writeFile(args[1], options.invalidHtml
        ? "not html"
        : "<html><body>verified fixture</body></html>");
      return { output: "", diagnostics: "" };
    }
    if (script === "verify-cv-facts.mjs") {
      return { output: JSON.stringify({ verdict: options.factVerdict ?? "pass" }), diagnostics: "" };
    }
    if (script === "generate-pdf.mjs") {
      await mkdir(dirname(args[1]), { recursive: true });
      await writeFile(args[1], PDF);
      const reportNum = args.find((value) => value.startsWith("--report="))?.slice(9);
      const format = args.find((value) => value.startsWith("--format="))?.slice(9);
      const rel = (path) => relative(canonicalRoot, path).split("\\").join("/");
      await writeFile(env.CAREER_OPS_PDF_INDEX, `# report\tpdf\thtml\tformat\tdate — written by generate-pdf.mjs, do not edit\n${reportNum}\t${rel(args[1])}\t${rel(args[0])}\t${format}\t2026-09-01\n`);
      if (options.changeContextAfterRender) source.revision = "v2";
      return { output: "", diagnostics: "" };
    }
    if (script === "merge-tracker.mjs") {
      const report = (await readdir(env.CAREER_OPS_ADDITIONS)).find((name) => name.endsWith(".tsv"));
      const cells = (await readFile(join(env.CAREER_OPS_ADDITIONS, report), "utf8")).trim().split("\t");
      if (options.mutateReportBeforeMergeFailure) {
        const reportFile = (await readdir(join(root, "reports"))).find((name) => name.endsWith(".md"));
        await writeFile(join(root, "reports", reportFile), "external report change\n");
        throw Object.assign(new Error("merge failed"), { exitCode: 8 });
      }
      if (options.mutateBundleBeforeMergeFailure) {
        const outputRoot = join(root, "output");
        const bundle = (await readdir(outputRoot)).find((name) => name.startsWith("042-") && !name.startsWith(".hfw"));
        await writeFile(join(outputRoot, bundle, "cv/tailored/v001/cv.pdf"), Buffer.concat([PDF, Buffer.from("external") ]));
        throw Object.assign(new Error("merge failed"), { exitCode: 8 });
      }
      if (options.mutateIndexBeforeMergeFailure) {
        await writeFile(env.CAREER_OPS_PDF_INDEX, `${await readFile(env.CAREER_OPS_PDF_INDEX, "utf8")}external\trow\n`);
        throw Object.assign(new Error("merge failed"), { exitCode: 8 });
      }
      records.push({
        id: Number(cells[0]),
        status: cells[4],
        report: options.wrongReportAfterMerge ? "[999](reports/wrong.md)" : cells[7],
        notes: cells[8],
      });
      return { output: "", diagnostics: "" };
    }
    throw new Error(`Unexpected script ${script}`);
  };
  const transactionOptions = {
    input,
    role,
    root,
    trackerDb: join(root, "data/applications.db"),
    stagingRoot,
    fallbackConfiguration: options.fallback === true
      ? JSON.stringify({ path: fallbackPath, sha256: options.fallbackExpectedHash ?? digest(fallbackBytes) })
      : options.fallbackConfiguration ?? null,
    preparationSources: async () => [{ relativePath: "cv.md", sha256: digest(source.revision) }],
    contextHash,
    assertPreparationResult: (result, expected) => {
      if (result?.contextHash !== expected) throw new PreparationTransactionError("context_changed");
    },
    runCareerOpsScript,
    historyRecords,
    reportHeader: (_role, _result, date, reportNum, pdfPath) => `# Evaluation\n\n**Date:** ${date}\n**URL:** ${role.url}\n**PDF:** ${pdfPath}\n\n---\n\nFixture report ${reportNum}\n`,
  };
  return {
    base,
    root,
    stagingRoot,
    fallbackPath,
    input,
    role,
    source,
    records,
    calls,
    get released() { return released; },
    commit: () => commitPreparationTransaction(transactionOptions),
  };
}

async function expectFailure(run, code, policy) {
  await assert.rejects(run, (error) => {
    assert.equal(error instanceof PreparationTransactionError, true);
    assert.equal(error.code, code);
    if (policy) assert.equal(error.retryPolicy, policy);
    assert.doesNotMatch(error.message, /user@example|Users\/private|x{40}/);
    return true;
  });
}

test("transaction commits through upstream CLIs and exact replay preserves identity", async () => {
  const fixture = await harness();
  const committed = await fixture.commit();
  assert.equal(committed.cvProvenance.source, "tailored_generated");
  assert.equal(committed.cvProvenance.tailored, true);
  assert.equal(committed.warnings.length, 0);
  assert.equal(fixture.records.length, 1);
  assert.match(fixture.records[0].notes, /HereForWork effect 11111111/);
  assert.match(fixture.records[0].notes, /Source URL=https:\/\/jobs\.example\.test\/role-1/);
  assert.deepEqual(await fixture.commit(), committed);
  assert.equal(fixture.calls.filter(({ script, args }) => script === "merge-tracker.mjs" && args.length === 0).length, 1);
});

test("replay rejects an incorrect canonical report link", async () => {
  const fixture = await harness();
  await fixture.commit();
  fixture.records[0].report = "[042](reports/not-the-committed-report.md)";
  await expectFailure(fixture.commit(), "canonical_identity_mismatch", "manual_repair_required");
});

test("pre-tracker failures compensate published state and keep structured policy", async (context) => {
  const cases = [
    ["reserve-report-num.mjs", "report_number_unavailable", "retry_same_preparation"],
    ["build-cv-html.mjs", "cv_build_failed", "retry_same_preparation"],
    ["verify-cv-facts.mjs", "cv_fact_check_failed", "fresh_preparation_provider_run"],
    ["generate-pdf.mjs", "pdf_fallback_not_configured", "repair_runtime_then_retry"],
    ["merge-tracker.mjs", "canonical_write_failed", "retry_same_preparation"],
  ];
  for (const [script, code, policy] of cases) {
    await context.test(script, async () => {
      const fixture = await harness({ failScript: script });
      await expectFailure(fixture.commit(), code, policy);
      assert.equal(fixture.records.some((record) => record.notes?.includes("HereForWork effect")), false);
      assert.equal((await readdir(join(fixture.root, "reports"))).filter((name) => name.endsWith(".md")).length, 0);
    });
  }
});

test("successful commands cannot bypass HTML or fact-result validation", async () => {
  const invalidHtml = await harness({ invalidHtml: true });
  await expectFailure(invalidHtml.commit, "cv_build_failed", "retry_same_preparation");

  const blockedFacts = await harness({ factVerdict: "block" });
  await expectFailure(blockedFacts.commit, "cv_fact_check_failed", "fresh_preparation_provider_run");
});

test("context changes fail before commit and after staging", async () => {
  const stale = await harness();
  stale.source.revision = "v2";
  await expectFailure(stale.commit(), "context_changed", "fresh_preparation_provider_run");
  assert.equal(stale.calls.length, 0);

  const changedAfterRender = await harness({ changeContextAfterRender: true });
  await expectFailure(changedAfterRender.commit(), "context_changed", "fresh_preparation_provider_run");
  assert.equal(changedAfterRender.records.length, 0);
});

test("PDF render failure recovers only through the hash-bound user-reviewed fallback", async () => {
  const fixture = await harness({ failScript: "generate-pdf.mjs", fallback: true });
  const committed = await fixture.commit();
  assert.deepEqual(committed.cvProvenance, {
    source: "user_reviewed_fallback",
    tailored: false,
    sourceSha256: digest(PDF),
    renderRecovery: {
      code: "pdf_generation_failed",
      stage: "stage.pdf",
      exitCode: 9,
      detail: "Preparation commit failed with pdf_generation_failed.",
    },
  });
  assert.equal(committed.warnings[0].recoveredBy, "user_reviewed_fallback");
  const report = await readFile(join(fixture.root, committed.artifacts.report.path), "utf8");
  assert.match(report, /User-reviewed fallback PDF/);
  assert.match(report, /not tailored/);
  const changes = await readFile(join(fixture.root, committed.artifacts.cvChanges.path), "utf8");
  assert.match(changes, /proposed changes only and were not applied/);
});

test("fallback rejects a missing, changed, or structurally invalid PDF", async (context) => {
  const missing = await harness({ failScript: "generate-pdf.mjs", fallbackConfiguration: JSON.stringify({ path: join(tmpdir(), "missing-hfw.pdf"), sha256: "a".repeat(64) }) });
  await expectFailure(missing.commit(), "pdf_fallback_unavailable", "repair_runtime_then_retry");

  await context.test("changed", async () => {
    const changed = await harness({
      failScript: "generate-pdf.mjs",
      fallback: true,
      fallbackExpectedHash: "a".repeat(64),
    });
    await expectFailure(changed.commit(), "pdf_fallback_changed", "manual_repair_required");
  });

  await context.test("invalid", async () => {
    const invalid = await harness({
      failScript: "generate-pdf.mjs",
      fallback: true,
      fallbackBytes: Buffer.from("%PDF-1.4 invalid %%EOF"),
    });
    await expectFailure(invalid.commit(), "pdf_fallback_invalid", "manual_repair_required");
  });
});

test("concurrent tracker drift aborts before merge and rolls files back", async () => {
  const fixture = await harness({ trackerDriftAt: 2 });
  await expectFailure(fixture.commit(), "tracker_drift", "retry_same_preparation");
  assert.equal(fixture.calls.some(({ script }) => script === "merge-tracker.mjs"), false);
  assert.equal((await readdir(join(fixture.root, "reports"))).filter((name) => name.endsWith(".md")).length, 0);
});

test("direct canonical tracker drift aborts even before its index changes", async () => {
  const fixture = await harness({ trackerFileDriftAt: 2 });
  await expectFailure(fixture.commit, "tracker_drift", "retry_same_preparation");
  assert.equal(fixture.records.length, 0);
});

test("merge success followed by post-verification failure preserves evidence for manual repair", async () => {
  const fixture = await harness({ wrongReportAfterMerge: true });
  await expectFailure(fixture.commit(), "canonical_identity_mismatch", "manual_repair_required");
  assert.equal(fixture.records.length, 1);
  assert.equal((await readdir(join(fixture.root, "reports"))).filter((name) => name.endsWith(".md")).length, 1);
  const state = JSON.parse(await readFile(join(fixture.stagingRoot, fixture.input.preparationId, "commit-state.json"), "utf8"));
  assert.equal(state.status, "manual_repair_required");
  assert.equal(state.trackerCommitted, true);
});

test("rollback drift in report, bundle, or PDF index becomes manual repair", async (context) => {
  for (const option of ["mutateReportBeforeMergeFailure", "mutateBundleBeforeMergeFailure", "mutateIndexBeforeMergeFailure"]) {
    await context.test(option, async () => {
      const fixture = await harness({ [option]: true });
      await expectFailure(fixture.commit(), "rollback_failed", "manual_repair_required");
      const state = JSON.parse(await readFile(join(fixture.stagingRoot, fixture.input.preparationId, "commit-state.json"), "utf8"));
      assert.equal(state.status, "manual_repair_required");
    });
  }
});

test("provider result and transaction state remain private and subprocess failures stay bounded", async () => {
  const fixture = await harness({ failScript: "build-cv-html.mjs" });
  await expectFailure(fixture.commit(), "cv_build_failed");
  const effect = join(fixture.stagingRoot, fixture.input.preparationId);
  assert.equal((await stat(join(effect, "provider-result.json"))).mode & 0o777, 0o600);
  assert.equal((await stat(join(effect, "commit-state.json"))).mode & 0o777, 0o600);
  const state = JSON.parse(await readFile(join(effect, "commit-state.json"), "utf8"));
  assert.equal(JSON.stringify(state).includes("user@example.test"), false);
  assert.equal(JSON.stringify(state).length < 20_000, true);
});
