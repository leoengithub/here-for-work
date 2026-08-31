import { describe, expect, it } from "vitest";
import { formatPublicationAge } from "./publication-age";

describe("formatPublicationAge", () => {
  it("uses calendar days in Europe/Madrid", () => {
    const now = new Date("2026-08-31T10:00:00+02:00");
    expect(formatPublicationAge("2026-08-31", now)).toBe("Today");
    expect(formatPublicationAge("2026-08-30", now)).toBe("1 day ago");
    expect(formatPublicationAge("2026-08-27", now)).toBe("4 days ago");
  });

  it("uses Madrid's date when a timestamp crosses a UTC calendar boundary", () => {
    const now = new Date("2026-08-31T00:10:00+02:00");
    expect(formatPublicationAge("2026-08-30T22:05:00Z", now)).toBe("Today");
  });

  it("stays calendar-based across Madrid's daylight-saving transition", () => {
    const now = new Date("2026-03-30T00:15:00+02:00");
    expect(formatPublicationAge("2026-03-29", now)).toBe("1 day ago");
  });

  it("omits missing, invalid, ambiguous, and future values", () => {
    const now = new Date("2026-08-31T10:00:00+02:00");
    expect(formatPublicationAge(null, now)).toBeNull();
    expect(formatPublicationAge("not-a-date", now)).toBeNull();
    expect(formatPublicationAge("2026-02-30", now)).toBeNull();
    expect(formatPublicationAge("2026-08-30T10:00:00", now)).toBeNull();
    expect(formatPublicationAge("2026-09-01", now)).toBeNull();
  });
});
