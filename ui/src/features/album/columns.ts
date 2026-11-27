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

export interface Columns {
  grid: number;
  gridMax: number;
  details: number;
  detailsMax: number;
}

const clamp = (x: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, x));

/**
 * The widths shown for the stored widths in a row `room` pixels wide (null
 * until it has been measured). As the row narrows the preview gives up room
 * first, down to 300; then the details column, down to 260; then the
 * thumbnail column, down to 200. The stored widths are never changed here.
 */
export function columnWidths(room: number | null, grid: number, details: number): Columns {
  const free = room === null ? Infinity : room - PREVIEW_MIN - SPLITTERS;
  const gridMax = Math.max(GRID_MIN, free - DETAILS_MIN);
  const shownGrid = clamp(grid, GRID_MIN, gridMax);
  const detailsMax = Math.max(DETAILS_MIN, Math.min(DETAILS_MAX, free - shownGrid));
  return {
    grid: shownGrid,
    gridMax,
    details: clamp(details, DETAILS_MIN, detailsMax),
    detailsMax,
  };
}
