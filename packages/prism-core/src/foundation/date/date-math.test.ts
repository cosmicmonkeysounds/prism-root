import { describe, it, expect } from "vitest";
import {
  parseDate,
  formatDate,
  todayISO,
  addDays,
  addMonths,
  addYears,
  diffDays,
  diffMonths,
  daysInMonth,
} from "./date-math.js";

describe("parseDate", () => {
  it("returns a UTC midnight timestamp for a YYYY-MM-DD string", () => {
    const ms = parseDate("2026-03-15");
    const d = new Date(ms);
    expect(d.getUTCFullYear()).toBe(2026);
    expect(d.getUTCMonth()).toBe(2);
    expect(d.getUTCDate()).toBe(15);
    expect(d.getUTCHours()).toBe(0);
    expect(d.getUTCMinutes()).toBe(0);
    expect(d.getUTCSeconds()).toBe(0);
  });

  it("correctly parses Jan 1", () => {
    const ms = parseDate("2026-01-01");
    const d = new Date(ms);
    expect(d.getUTCFullYear()).toBe(2026);
    expect(d.getUTCMonth()).toBe(0);
    expect(d.getUTCDate()).toBe(1);
  });

  it("correctly parses Dec 31", () => {
    const ms = parseDate("2026-12-31");
    const d = new Date(ms);
    expect(d.getUTCFullYear()).toBe(2026);
    expect(d.getUTCMonth()).toBe(11);
    expect(d.getUTCDate()).toBe(31);
  });
});

describe("formatDate", () => {
  it("formats a UTC timestamp back to YYYY-MM-DD", () => {
    const ms = Date.UTC(2026, 2, 15);
    expect(formatDate(ms)).toBe("2026-03-15");
  });

  it("pads month and day with leading zeros", () => {
    const ms = Date.UTC(2026, 0, 5);
    expect(formatDate(ms)).toBe("2026-01-05");
  });

  it("round-trips through parseDate", () => {
    const iso = "2026-07-04";
    expect(formatDate(parseDate(iso))).toBe(iso);
  });
});

describe("todayISO", () => {
  it("returns a valid YYYY-MM-DD string", () => {
    const today = todayISO();
    expect(today).toMatch(/^\d{4}-\d{2}-\d{2}$/);
  });

  it("returns a date that round-trips through parseDate/formatDate", () => {
    const today = todayISO();
    expect(formatDate(parseDate(today))).toBe(today);
  });
});

describe("addDays", () => {
  it("adds positive days", () => {
    expect(addDays("2026-01-01", 1)).toBe("2026-01-02");
    expect(addDays("2026-01-01", 30)).toBe("2026-01-31");
    expect(addDays("2026-01-31", 1)).toBe("2026-02-01");
  });

  it("adds zero days", () => {
    expect(addDays("2026-03-15", 0)).toBe("2026-03-15");
  });

  it("subtracts days with negative n", () => {
    expect(addDays("2026-01-05", -4)).toBe("2026-01-01");
    expect(addDays("2026-03-01", -1)).toBe("2026-02-28");
  });

  it("crosses month boundaries correctly", () => {
    expect(addDays("2026-01-31", 1)).toBe("2026-02-01");
    expect(addDays("2026-12-31", 1)).toBe("2027-01-01");
  });

  it("crosses DST-transition dates without drift (UTC-safe)", () => {
    expect(addDays("2026-03-07", 1)).toBe("2026-03-08");
    expect(addDays("2026-03-08", 1)).toBe("2026-03-09");
    expect(addDays("2026-10-31", 1)).toBe("2026-11-01");
    expect(addDays("2026-11-01", 1)).toBe("2026-11-02");
  });
});

describe("addMonths", () => {
  it("adds positive months", () => {
    expect(addMonths("2026-01-15", 1)).toBe("2026-02-15");
    expect(addMonths("2026-01-15", 12)).toBe("2027-01-15");
  });

  it("adds zero months", () => {
    expect(addMonths("2026-03-15", 0)).toBe("2026-03-15");
  });

  it("subtracts months with negative n", () => {
    expect(addMonths("2026-03-15", -1)).toBe("2026-02-15");
    expect(addMonths("2026-01-15", -1)).toBe("2025-12-15");
  });

  it("clamps to last day of month — non-leap year (Jan 31 + 1 = Feb 28)", () => {
    expect(addMonths("2026-01-31", 1)).toBe("2026-02-28");
  });

  it("clamps to last day of month — leap year (Jan 31 + 1 = Feb 29)", () => {
    expect(addMonths("2024-01-31", 1)).toBe("2024-02-29");
  });

  it("clamps to last day of month — Mar 31 + 1 = Apr 30", () => {
    expect(addMonths("2026-03-31", 1)).toBe("2026-04-30");
  });

  it("clamps to last day of month — Jan 31 + 3 = Apr 30", () => {
    expect(addMonths("2026-01-31", 3)).toBe("2026-04-30");
  });

  it("does not clamp when target month has enough days", () => {
    expect(addMonths("2026-01-28", 1)).toBe("2026-02-28");
    expect(addMonths("2026-02-28", 1)).toBe("2026-03-28");
  });

  it("crosses year boundaries", () => {
    expect(addMonths("2026-11-15", 3)).toBe("2027-02-15");
  });
});

describe("addYears", () => {
  it("adds positive years", () => {
    expect(addYears("2026-03-15", 1)).toBe("2027-03-15");
    expect(addYears("2026-03-15", 10)).toBe("2036-03-15");
  });

  it("handles leap day: Feb 29 + 1 year = Feb 28 (non-leap)", () => {
    expect(addYears("2024-02-29", 1)).toBe("2025-02-28");
  });

  it("handles leap day: Feb 29 + 4 years = Feb 29 (next leap)", () => {
    expect(addYears("2024-02-29", 4)).toBe("2028-02-29");
  });

  it("subtracts years with negative n", () => {
    expect(addYears("2026-06-15", -2)).toBe("2024-06-15");
  });
});

describe("diffDays", () => {
  it("returns positive diff for future dates", () => {
    expect(diffDays("2026-01-01", "2026-01-08")).toBe(7);
  });

  it("returns negative diff for past dates", () => {
    expect(diffDays("2026-01-08", "2026-01-01")).toBe(-7);
  });

  it("returns zero for same date", () => {
    expect(diffDays("2026-03-15", "2026-03-15")).toBe(0);
  });

  it("counts correctly across month boundary", () => {
    expect(diffDays("2026-01-29", "2026-02-02")).toBe(4);
  });

  it("counts correctly across year boundary", () => {
    expect(diffDays("2025-12-30", "2026-01-03")).toBe(4);
  });

  it("handles DST-transition dates without drift", () => {
    expect(diffDays("2026-03-07", "2026-03-08")).toBe(1);
    expect(diffDays("2026-03-08", "2026-03-09")).toBe(1);
    expect(diffDays("2026-10-31", "2026-11-01")).toBe(1);
    expect(diffDays("2026-11-01", "2026-11-02")).toBe(1);
  });

  it("counts leap year Feb correctly", () => {
    expect(diffDays("2024-01-01", "2024-03-01")).toBe(60);
    expect(diffDays("2026-01-01", "2026-03-01")).toBe(59);
  });
});

describe("diffMonths", () => {
  it("returns positive diff for future months", () => {
    expect(diffMonths("2026-01-01", "2026-04-01")).toBe(3);
  });

  it("returns negative diff for past months", () => {
    expect(diffMonths("2026-04-01", "2026-01-01")).toBe(-3);
  });

  it("crosses year boundaries", () => {
    expect(diffMonths("2025-11-01", "2026-03-01")).toBe(4);
  });

  it("returns zero for same month", () => {
    expect(diffMonths("2026-03-01", "2026-03-15")).toBe(0);
  });
});

describe("daysInMonth", () => {
  it("returns 31 for January", () => {
    expect(daysInMonth(2026, 0)).toBe(31);
  });

  it("returns 28 for February in a non-leap year", () => {
    expect(daysInMonth(2023, 1)).toBe(28);
    expect(daysInMonth(2026, 1)).toBe(28);
  });

  it("returns 29 for February in a leap year", () => {
    expect(daysInMonth(2024, 1)).toBe(29);
    expect(daysInMonth(2000, 1)).toBe(29);
  });

  it("returns 30 for April", () => {
    expect(daysInMonth(2026, 3)).toBe(30);
  });

  it("returns 31 for December", () => {
    expect(daysInMonth(2026, 11)).toBe(31);
  });

  it("century non-leap year: 1900 Feb = 28", () => {
    expect(daysInMonth(1900, 1)).toBe(28);
  });

  it("400-year divisible leap year: 2000 Feb = 29", () => {
    expect(daysInMonth(2000, 1)).toBe(29);
  });
});
