import { describe, expect, it } from "vitest";

import { selectExpectedTab } from "./tab-selection";

describe("selectExpectedTab", () => {
  it("prefers the selected matching tab when duplicate URLs are open", () => {
    const selected = selectExpectedTab([
      { id: 1, url: "https://jobs.ashbyhq.com/acme/role/application", active: false },
      { id: 2, url: "https://jobs.ashbyhq.com/acme/role/application", active: true },
    ], "https://jobs.ashbyhq.com/acme/role/application");

    expect(selected?.id).toBe(2);
  });

  it("falls back to the newest matching tab when none is selected", () => {
    const selected = selectExpectedTab([
      { id: 1, url: "https://jobs.ashbyhq.com/acme/role/application#old", active: false },
      { id: 2, url: "https://jobs.ashbyhq.com/acme/role/application", active: false },
    ], "https://jobs.ashbyhq.com/acme/role/application");

    expect(selected?.id).toBe(2);
  });

  it("ignores tracking query parameters added by the application site", () => {
    const selected = selectExpectedTab([
      { id: 9, url: "https://careers.example.com/apply/42?source=linkedin", active: true },
    ], "https://careers.example.com/apply/42");

    expect(selected?.id).toBe(9);
  });

  it("ignores non-matching tabs", () => {
    expect(selectExpectedTab([
      { id: 1, url: "https://jobs.ashbyhq.com/acme/other/application", active: true },
    ], "https://jobs.ashbyhq.com/acme/role/application")).toBeNull();
  });
});
