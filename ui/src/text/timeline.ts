// Text shown by the timeline (src/features/timeline, and the grid's
// headings). docs/04-browsing.md, "View switch" and "Timeline view".

const MONTHS = [
  "January",
  "February",
  "March",
  "April",
  "May",
  "June",
  "July",
  "August",
  "September",
  "October",
  "November",
  "December",
];
const DAYS = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];

/** A whole number with digit grouping: 2,140. */
function group(n: number): string {
  return n.toLocaleString("en-US");
}

const photos = (n: number) => (n === 1 ? "1 photo" : `${group(n)} photos`);

/**
 * The words of a group key, read as written and never through a time zone:
 * "Saturday 14 July 2026", "July 2026" or "No date".
 */
function keyLabel(key: string): string {
  const y = Number(key.slice(0, 4));
  const m = Number(key.slice(5, 7));
  const month = MONTHS[m - 1];
  if (key === "none" || !Number.isInteger(y) || month === undefined) return "No date";
  if (key.length === 7) return `${month} ${y}`;
  const d = Number(key.slice(8, 10));
  const weekday = DAYS[new Date(Date.UTC(y, m - 1, d)).getUTCDay()];
  return `${weekday} ${d} ${month} ${y}`;
}

export const timelineText = {
  /** The view switch (docs/04-browsing.md, "View switch"). */
  views: {
    label: "Show the photos as",
    grid: "Grid",
    timeline: "Timeline",
    map: "Map",
  },
  /** The unit switch. */
  unit: {
    label: "Group by",
    day: "Day",
    month: "Month",
  },
  keyLabel,
  /** "Saturday 14 July 2026 · 37 photos", "July 2026 · 412 photos", "No date · 5 photos". */
  heading: (key: string, n: number) => `${keyLabel(key)} · ${photos(n)}`,
  /** The month rail (docs/04-browsing.md, "Month rail"). */
  rail: {
    label: "Jump to a month",
    /** "Jul". */
    month: (m: number) => (MONTHS[m - 1] ?? "").slice(0, 3),
    /** A month's accessible name: "July 2026". */
    monthName: (y: number, m: number) => `${MONTHS[m - 1] ?? ""} ${y}`,
  },
  /** The heading button's name: what a click does. */
  selectGroup: (key: string, n: number) => `${keyLabel(key)} · ${photos(n)}. Select these photos`,
};
