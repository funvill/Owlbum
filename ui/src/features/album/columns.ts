// The review row's column widths (docs/04-browsing.md, "Column widths";
// docs/09-visual-design.md, "The three columns"). Pure, so it is tested
// without a browser.

export const GRID_DEFAULT = 420;
export const GRID_MIN = 200;
export const DETAILS_DEFAULT = 340;
export const DETAILS_MIN = 260;
export const DETAILS_MAX = 560;
export const PREVIEW_MIN = 300;
/** Two 6 px splitters. */
export const SPLITTERS = 12;
/** One splitter. */
export const SPLITTER = 6;
/** The bottom strip's height (docs/04-browsing.md, "Hiding and moving the panels"). */
export const STRIP_DEFAULT = 200;
export const STRIP_MIN = 120;
/** Above the bottom strip the preview keeps at least this height. */
export const PREVIEW_MIN_HEIGHT = 200;

export interface Columns {
  grid: number;
  gridMax: number;
  details: number;
  detailsMax: number;
}

const clamp = (x: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, x));

/** Which columns are in the row: a hidden column, or thumbnails at the bottom, leave it. */
export interface InRow {
  grid: boolean;
  details: boolean;
}

/**
 * The widths shown for the stored widths in a row `room` pixels wide (null
 * until it has been measured). As the row narrows the preview gives up room
 * first, down to 300; then the details column, down to 260; then the
 * thumbnail column, down to 200. The stored widths are never changed here.
 * A column not in the row (`inRow`) takes no room and has no splitter.
 */
export function columnWidths(
  room: number | null,
  grid: number,
  details: number,
  inRow: InRow = { grid: true, details: true },
): Columns {
  const splitters = (inRow.grid ? SPLITTER : 0) + (inRow.details ? SPLITTER : 0);
  const free = room === null ? Infinity : room - PREVIEW_MIN - splitters;
  const gridMax = Math.max(GRID_MIN, free - (inRow.details ? DETAILS_MIN : 0));
  const shownGrid = clamp(grid, GRID_MIN, gridMax);
  const detailsMax = Math.max(DETAILS_MIN, Math.min(DETAILS_MAX, free - (inRow.grid ? shownGrid : 0)));
  return {
    grid: shownGrid,
    gridMax,
    details: clamp(details, DETAILS_MIN, detailsMax),
    detailsMax,
  };
}

/** The bottom strip's height as shown, and the most its splitter allows. */
export interface Strip {
  height: number;
  max: number;
}

/**
 * The bottom strip's height for the stored one, in a review screen `room`
 * pixels tall (null until measured): at least 120, and at most what leaves
 * the preview 200 above it. The stored height is never changed here.
 */
export function stripHeight(room: number | null, stored: number): Strip {
  const max = room === null ? Infinity : Math.max(STRIP_MIN, room - PREVIEW_MIN_HEIGHT - SPLITTER);
  return { height: clamp(stored, STRIP_MIN, max), max };
}
