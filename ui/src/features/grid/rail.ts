// The timeline's month rail (docs/04-browsing.md, "Month rail"): the years
// and months of a result's groups, worked out from the group keys alone.
// Pure, so it is tested without a browser.

/** The rail's width. */
export const RAIL = 44;
/** The height of one line of the rail: a year or a month. */
export const RAIL_LINE = 20;
/** The space above and below the rail's list. */
export const RAIL_PAD = 8;

export interface RailMonth {
  /** `YYYY-MM`. */
  key: string;
  /** 1 to 12. */
  month: number;
  /** The month's first group: its heading is where a click goes. */
  group: number;
}

export interface RailYear {
  year: number;
  /** The year's first group. */
  group: number;
  months: RailMonth[];
}

/**
 * The years and months that have photos, in the groups' order, each with
 * its first group. The "none" group (no date) is left out.
 */
export function railYears(groups: readonly { key: string }[]): RailYear[] {
  const years: RailYear[] = [];
  groups.forEach((g, i) => {
    if (g.key === "none" || g.key.length < 7) return;
    const year = Number(g.key.slice(0, 4));
    const month = Number(g.key.slice(5, 7));
    const key = g.key.slice(0, 7);
    let y = years.at(-1);
    if (y === undefined || y.year !== year) {
      y = { year, group: i, months: [] };
      years.push(y);
    }
    if (y.months.at(-1)?.key !== key) y.months.push({ key, month, group: i });
  });
  return years;
}

/** The rail shows only when the result has two months or more. */
export function railShown(years: readonly RailYear[]): boolean {
  let months = 0;
  for (const y of years) months += y.months.length;
  return months >= 2;
}

/** True when every year and month fits in a rail `height` pixels tall; otherwise only the years show. */
export function railFits(years: readonly RailYear[], height: number): boolean {
  let lines = years.length;
  for (const y of years) lines += y.months.length;
  return lines * RAIL_LINE + 2 * RAIL_PAD <= height;
}
