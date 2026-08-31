const MADRID_TIME_ZONE = "Europe/Madrid";
const DATE_ONLY = /^(\d{4})-(\d{2})-(\d{2})$/;
const ZONED_DATE_TIME = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}(?::\d{2}(?:\.\d{1,9})?)?(?:Z|[+-]\d{2}:\d{2})$/;

type CalendarDate = { year: number; month: number; day: number };

function validCalendarDate(year: number, month: number, day: number): CalendarDate | null {
  const candidate = new Date(Date.UTC(year, month - 1, day));
  if (
    candidate.getUTCFullYear() !== year
    || candidate.getUTCMonth() !== month - 1
    || candidate.getUTCDate() !== day
  ) return null;
  return { year, month, day };
}

function calendarDateInMadrid(value: Date): CalendarDate {
  const parts = new Intl.DateTimeFormat("en-US", {
    timeZone: MADRID_TIME_ZONE,
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
  }).formatToParts(value);
  const part = (type: Intl.DateTimeFormatPartTypes) => Number(parts.find((item) => item.type === type)?.value);
  return { year: part("year"), month: part("month"), day: part("day") };
}

function sourceCalendarDate(postedAt: string): CalendarDate | null {
  const dateOnly = DATE_ONLY.exec(postedAt);
  if (dateOnly) return validCalendarDate(Number(dateOnly[1]), Number(dateOnly[2]), Number(dateOnly[3]));
  if (!ZONED_DATE_TIME.test(postedAt)) return null;
  const parsed = new Date(postedAt);
  if (Number.isNaN(parsed.valueOf())) return null;
  return calendarDateInMadrid(parsed);
}

function dayNumber(value: CalendarDate): number {
  return Math.floor(Date.UTC(value.year, value.month - 1, value.day) / 86_400_000);
}

export function formatPublicationAge(postedAt: string | null, now = new Date()): string | null {
  if (!postedAt || Number.isNaN(now.valueOf())) return null;
  const posted = sourceCalendarDate(postedAt);
  if (!posted) return null;
  const difference = dayNumber(calendarDateInMadrid(now)) - dayNumber(posted);
  if (difference < 0) return null;
  if (difference === 0) return "Today";
  if (difference === 1) return "1 day ago";
  return `${difference} days ago`;
}
