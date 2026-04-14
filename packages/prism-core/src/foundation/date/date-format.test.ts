import { describe, it, expect } from "vitest";
import {
  dayOfWeek,
  weekStart,
  weekEnd,
  monthStart,
  monthEnd,
  quarterStart,
  yearStart,
  formatDisplayDate,
  formatShortDate,
  getYear,
  getMonth,
  getDay,
} from "./date-format.js";

describe("dayOfWeek", () => {
  it("returns 0 for Sunday", () => {
    expect(dayOfWeek("2026-03-15")).toBe(0);
  });

  it("returns 1 for Monday", () => {
    expect(dayOfWeek("2026-03-16")).toBe(1);
  });

  it("returns 6 for Saturday", () => {
    expect(dayOfWeek("2026-03-21")).toBe(6);
  });

  it("returns 3 for Wednesday", () => {
    expect(dayOfWeek("2026-03-18")).toBe(3);
  });
});

describe("weekStart", () => {
  it("returns Monday for a Wednesday input", () => {
    expect(weekStart("2026-03-18")).toBe("2026-03-16");
  });

  it("returns the previous Monday for a Sunday input", () => {
    expect(weekStart("2026-03-15")).toBe("2026-03-09");
  });

  it("returns the same date for a Monday input", () => {
    expect(weekStart("2026-03-16")).toBe("2026-03-16");
  });

  it("returns the correct Monday for a Saturday input", () => {
    expect(weekStart("2026-03-21")).toBe("2026-03-16");
  });

  it("crosses month boundary correctly", () => {
    expect(weekStart("2026-03-03")).toBe("2026-03-02");
    expect(weekStart("2026-03-01")).toBe("2026-02-23");
  });
});

describe("weekEnd", () => {
  it("returns the following Sunday for a Wednesday", () => {
    expect(weekEnd("2026-03-18")).toBe("2026-03-22");
  });

  it("returns Sunday for a Monday input", () => {
    expect(weekEnd("2026-03-16")).toBe("2026-03-22");
  });

  it("returns the same Sunday for a Sunday input", () => {
    expect(weekEnd("2026-03-15")).toBe("2026-03-15");
  });
});

describe("monthStart", () => {
  it("returns the first day of the month", () => {
    expect(monthStart("2026-03-15")).toBe("2026-03-01");
    expect(monthStart("2026-03-01")).toBe("2026-03-01");
    expect(monthStart("2026-03-31")).toBe("2026-03-01");
  });

  it("handles February", () => {
    expect(monthStart("2026-02-28")).toBe("2026-02-01");
  });
});

describe("monthEnd", () => {
  it("returns the last day of a 31-day month", () => {
    expect(monthEnd("2026-03-15")).toBe("2026-03-31");
  });

  it("returns the last day of a 30-day month", () => {
    expect(monthEnd("2026-04-10")).toBe("2026-04-30");
  });

  it("returns Feb 28 for a non-leap year", () => {
    expect(monthEnd("2026-02-01")).toBe("2026-02-28");
  });

  it("returns Feb 29 for a leap year", () => {
    expect(monthEnd("2024-02-01")).toBe("2024-02-29");
  });
});

describe("quarterStart", () => {
  it("returns Jan 1 for Q1 (Jan–Mar)", () => {
    expect(quarterStart("2026-01-15")).toBe("2026-01-01");
    expect(quarterStart("2026-02-28")).toBe("2026-01-01");
    expect(quarterStart("2026-03-31")).toBe("2026-01-01");
  });

  it("returns Apr 1 for Q2 (Apr–Jun)", () => {
    expect(quarterStart("2026-04-01")).toBe("2026-04-01");
    expect(quarterStart("2026-05-20")).toBe("2026-04-01");
    expect(quarterStart("2026-06-30")).toBe("2026-04-01");
  });

  it("returns Jul 1 for Q3 (Jul–Sep)", () => {
    expect(quarterStart("2026-07-01")).toBe("2026-07-01");
    expect(quarterStart("2026-08-15")).toBe("2026-07-01");
    expect(quarterStart("2026-09-30")).toBe("2026-07-01");
  });

  it("returns Oct 1 for Q4 (Oct–Dec)", () => {
    expect(quarterStart("2026-10-01")).toBe("2026-10-01");
    expect(quarterStart("2026-11-15")).toBe("2026-10-01");
    expect(quarterStart("2026-12-31")).toBe("2026-10-01");
  });
});

describe("yearStart", () => {
  it("returns Jan 1 of the year", () => {
    expect(yearStart("2026-03-15")).toBe("2026-01-01");
    expect(yearStart("2026-12-31")).toBe("2026-01-01");
    expect(yearStart("2026-01-01")).toBe("2026-01-01");
  });
});

describe("formatDisplayDate", () => {
  it('formats a date as "Mar 15, 2026"', () => {
    expect(formatDisplayDate("2026-03-15")).toBe("Mar 15, 2026");
  });

  it("formats Jan 1", () => {
    expect(formatDisplayDate("2026-01-01")).toBe("Jan 1, 2026");
  });

  it("does not shift by timezone (UTC-pinned)", () => {
    expect(formatDisplayDate("2026-03-15")).toContain("15");
  });
});

describe("formatShortDate", () => {
  it('formats a date as "Mar 15"', () => {
    expect(formatShortDate("2026-03-15")).toBe("Mar 15");
  });

  it("formats Jan 1", () => {
    expect(formatShortDate("2026-01-01")).toBe("Jan 1");
  });
});

describe("getYear", () => {
  it("extracts the year as a number", () => {
    expect(getYear("2026-03-15")).toBe(2026);
    expect(getYear("2000-01-01")).toBe(2000);
  });
});

describe("getMonth", () => {
  it("extracts the 1-indexed month", () => {
    expect(getMonth("2026-03-15")).toBe(3);
    expect(getMonth("2026-01-01")).toBe(1);
    expect(getMonth("2026-12-31")).toBe(12);
  });
});

describe("getDay", () => {
  it("extracts the day of month", () => {
    expect(getDay("2026-03-15")).toBe(15);
    expect(getDay("2026-01-01")).toBe(1);
    expect(getDay("2026-12-31")).toBe(31);
  });
});
