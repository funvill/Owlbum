import { describe, expect, it } from "vitest";
import { RAIL_LINE, RAIL_PAD, railFits, railShown, railYears } from "./rail";

const keys = (...k: string[]) => k.map((key) => ({ key }));

describe("month rail", () => {
  it("lists each year and only the months that have photos, with each one's first group", () => {
    const years = railYears(keys("2025-12-30", "2025-12-31", "2026-01-02", "2026-07-14", "2026-07-15", "none"));
    expect(years).toEqual([
      { year: 2025, group: 0, months: [{ key: "2025-12", month: 12, group: 0 }] },
      {
        year: 2026,
        group: 2,
        months: [
          { key: "2026-01", month: 1, group: 2 },
          { key: "2026-07", month: 7, group: 3 },
        ],
      },
    ]);
  });

  it("works on Month groups too", () => {
    expect(railYears(keys("2024-03", "2024-05")).map((y) => y.months.map((m) => m.month))).toEqual([[3, 5]]);
  });

  it("shows only with two months or more", () => {
    expect(railShown(railYears(keys("2026-07-14", "2026-07-15", "none")))).toBe(false);
    expect(railShown(railYears(keys("none")))).toBe(false);
    expect(railShown(railYears(keys("2026-06-30", "2026-07-01")))).toBe(true);
  });

  it("collapses to years when the years and months are taller than the rail", () => {
    // Two years and three months: five lines.
    const years = railYears(keys("2025-12", "2026-01", "2026-02"));
    const tall = 5 * RAIL_LINE + 2 * RAIL_PAD;
    expect(railFits(years, tall)).toBe(true);
    expect(railFits(years, tall - 1)).toBe(false);
  });
});
