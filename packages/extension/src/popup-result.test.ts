import { describe, expect, it } from "vitest";

import { requireSuccessfulRelease } from "./popup-result";

describe("popup release result", () => {
  it("accepts only an explicit successful release", () => {
    expect(() => requireSuccessfulRelease({ ok: true })).not.toThrow();
    expect(() => requireSuccessfulRelease({ ok: false, error: "verified_fill_required" }))
      .toThrow("verified_fill_required");
    expect(() => requireSuccessfulRelease(undefined)).toThrow("could not be released");
  });
});
