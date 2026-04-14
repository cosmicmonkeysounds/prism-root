import { describe, it, expect } from "vitest";
import {
  isBetween,
  isBefore,
  isAfter,
  isToday,
  isPast,
  isFuture,
  minDate,
  maxDate,
  clampDate,
  dateRange,
  weeksInRange,
  monthsInRange,
} from "./date-query.js";
import { todayISO, addDays } from "./date-math.js";

describe("isBetween", () => {
  it("returns true when date is within range", () => {
    expect(isBetween("2026-03-15", "2026-03-01", "2026-03-31")).toBe(true);
  });

  it("returns true for lower boundary (inclusive)", () => {
    expect(isBetween("2026-03-01", "2026-03-01", "2026-03-31")).toBe(true);
  });

  it("returns true for upper boundary (inclusive)", () => {
    expect(isBetween("2026-03-31", "2026-03-01", "2026-03-31")).toBe(true);
  });

  it("returns false when date is before range", () => {
    expect(isBetween("2026-02-28", "2026-03-01", "2026-03-31")).toBe(false);
  });

  it("returns false when date is after range", () => {
    expect(isBetween("2026-04-01", "2026-03-01", "2026-03-31")).toBe(false);
  });
});

describe("isBefore", () => {
  it("returns true when date is strictly before other", () => {
    expect(isBefore("2026-03-14", "2026-03-15")).toBe(true);
  });

  it("returns false for equal dates", () => {
    expect(isBefore("2026-03-15", "2026-03-15")).toBe(false);
  });

  it("returns false when date is after other", () => {
    expect(isBefore("2026-03-16", "2026-03-15")).toBe(false);
  });

  it("works across year boundaries", () => {
    expect(isBefore("2025-12-31", "2026-01-01")).toBe(true);
    expect(isBefore("2026-01-01", "2025-12-31")).toBe(false);
  });
});

describe("isAfter", () => {
  it("returns true when date is strictly after other", () => {
    expect(isAfter("2026-03-16", "2026-03-15")).toBe(true);
  });

  it("returns false for equal dates", () => {
    expect(isAfter("2026-03-15", "2026-03-15")).toBe(false);
  });

  it("returns false when date is before other", () => {
    expect(isAfter("2026-03-14", "2026-03-15")).toBe(false);
  });
});

describe("isToday", () => {
  it("returns true for today", () => {
    expect(isToday(todayISO())).toBe(true);
  });

  it("returns false for yesterday", () => {
    expect(isToday(addDays(todayISO(), -1))).toBe(false);
  });

  it("returns false for tomorrow", () => {
    expect(isToday(addDays(todayISO(), 1))).toBe(false);
  });
});

describe("isPast", () => {
  it("returns true for yesterday", () => {
    expect(isPast(addDays(todayISO(), -1))).toBe(true);
  });

  it("returns false for today", () => {
    expect(isPast(todayISO())).toBe(false);
  });

  it("returns false for tomorrow", () => {
    expect(isPast(addDays(todayISO(), 1))).toBe(false);
  });
});

describe("isFuture", () => {
  it("returns true for tomorrow", () => {
    expect(isFuture(addDays(todayISO(), 1))).toBe(true);
  });

  it("returns false for today", () => {
    expect(isFuture(todayISO())).toBe(false);
  });

  it("returns false for yesterday", () => {
    expect(isFuture(addDays(todayISO(), -1))).toBe(false);
  });
});

describe("minDate", () => {
  it("returns the earlier date", () => {
    expect(minDate("2026-03-10", "2026-03-15")).toBe("2026-03-10");
    expect(minDate("2026-03-15", "2026-03-10")).toBe("2026-03-10");
  });

  it("returns either when equal", () => {
    expect(minDate("2026-03-15", "2026-03-15")).toBe("2026-03-15");
  });

  it("handles cross-year comparisons", () => {
    expect(minDate("2025-12-31", "2026-01-01")).toBe("2025-12-31");
  });
});

describe("maxDate", () => {
  it("returns the later date", () => {
    expect(maxDate("2026-03-10", "2026-03-15")).toBe("2026-03-15");
    expect(maxDate("2026-03-15", "2026-03-10")).toBe("2026-03-15");
  });

  it("returns either when equal", () => {
    expect(maxDate("2026-03-15", "2026-03-15")).toBe("2026-03-15");
  });
});

describe("clampDate", () => {
  it("returns the date unchanged when within range", () => {
    expect(clampDate("2026-03-15", "2026-03-01", "2026-03-31")).toBe(
      "2026-03-15",
    );
  });

  it("clamps to min when date is before range", () => {
    expect(clampDate("2026-02-01", "2026-03-01", "2026-03-31")).toBe(
      "2026-03-01",
    );
  });

  it("clamps to max when date is after range", () => {
    expect(clampDate("2026-04-15", "2026-03-01", "2026-03-31")).toBe(
      "2026-03-31",
    );
  });

  it("returns min when date equals min", () => {
    expect(clampDate("2026-03-01", "2026-03-01", "2026-03-31")).toBe(
      "2026-03-01",
    );
  });

  it("returns max when date equals max", () => {
    expect(clampDate("2026-03-31", "2026-03-01", "2026-03-31")).toBe(
      "2026-03-31",
    );
  });
});

describe("dateRange", () => {
  it("generates 5 dates from Jan 29 to Feb 2", () => {
    const range = dateRange("2026-01-29", "2026-02-02");
    expect(range).toEqual([
      "2026-01-29",
      "2026-01-30",
      "2026-01-31",
      "2026-02-01",
      "2026-02-02",
    ]);
  });

  it("returns a single date when from === to", () => {
    expect(dateRange("2026-03-15", "2026-03-15")).toEqual(["2026-03-15"]);
  });

  it("returns empty array when from > to", () => {
    expect(dateRange("2026-03-15", "2026-03-10")).toEqual([]);
  });

  it("crosses month boundary", () => {
    const range = dateRange("2026-03-30", "2026-04-02");
    expect(range).toEqual([
      "2026-03-30",
      "2026-03-31",
      "2026-04-01",
      "2026-04-02",
    ]);
  });
});

describe("weeksInRange", () => {
  it("returns Mondays within range", () => {
    const weeks = weeksInRange("2026-03-15", "2026-03-28");
    expect(weeks).toEqual(["2026-03-16", "2026-03-23"]);
  });

  it("includes the Monday of from-date when from is a Monday", () => {
    const weeks = weeksInRange("2026-03-16", "2026-03-22");
    expect(weeks).toEqual(["2026-03-16"]);
  });

  it("returns empty when no Monday falls in range", () => {
    const weeks = weeksInRange("2026-03-17", "2026-03-21");
    expect(weeks).toEqual([]);
  });
});

describe("monthsInRange", () => {
  it("generates month-start strings for each month in range", () => {
    const months = monthsInRange("2026-01-15", "2026-04-10");
    expect(months).toEqual([
      "2026-01-01",
      "2026-02-01",
      "2026-03-01",
      "2026-04-01",
    ]);
  });

  it("includes a single month when from and to are in the same month", () => {
    expect(monthsInRange("2026-03-01", "2026-03-31")).toEqual(["2026-03-01"]);
  });

  it("crosses year boundaries", () => {
    const months = monthsInRange("2025-11-01", "2026-02-01");
    expect(months).toEqual([
      "2025-11-01",
      "2025-12-01",
      "2026-01-01",
      "2026-02-01",
    ]);
  });
});
