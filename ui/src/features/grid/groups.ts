// The timeline's rows (docs/04-browsing.md, "Timeline view", "Layout"): the
// grid's tiles under one heading per group. Rows are no longer all one
// height, so each row's top is kept in `tops` and found by binary search.
// Pure functions, tested without a browser; `layout.ts` uses them when its
// layout has groups.

import { GAP, type GridLayout } from "./layout";

/** The height of a heading row, and of the sticky heading. */
export const HEAD = 32;
/** The space after each group. */
export const GROUP_GAP = 8;

/** Where a group's tiles are in the result: the fields of `Group` that lay it out. */
export interface GroupSpan {
  start: number;
  count: number;
}

/** The rows of a grouped layout. */
export interface GroupRows {
  /** Each row's top, within the rows (add PAD for the box). */
  tops: Float64Array;
  /** Each row's first tile, or −1 − g for the heading of group g. */
  firsts: Int32Array;
  /** The row of each group's heading. */
  headRows: Int32Array;
  /** Each group's first tile and tile count. */
  starts: Int32Array;
  counts: Int32Array;
}

/** The rows of `groups` at `cols` columns of `stride`; `total` is the height of the rows. */
export function groupRows(groups: readonly GroupSpan[], cols: number, stride: number): { rows: GroupRows; total: number } {
  let n = 0;
  for (const g of groups) n += 1 + Math.ceil(g.count / cols);
  const tops = new Float64Array(n);
  const firsts = new Int32Array(n);
  const headRows = new Int32Array(groups.length);
  const starts = new Int32Array(groups.length);
  const counts = new Int32Array(groups.length);
  let r = 0;
  let y = 0;
  groups.forEach((g, i) => {
    headRows[i] = r;
    starts[i] = g.start;
    counts[i] = g.count;
    tops[r] = y;
    firsts[r++] = -1 - i;
    y += HEAD;
    for (let t = 0; t < g.count; t += cols) {
      tops[r] = y;
      firsts[r++] = g.start + t;
      y += stride;
    }
    y += GROUP_GAP;
  });
  // The last row's gap and the last group's space are not content.
  const last = groups.at(-1);
  const total = last === undefined ? 0 : y - GROUP_GAP - (last.count > 0 ? GAP : 0);
  return { rows: { tops, firsts, headRows, starts, counts }, total: Math.max(0, total) };
}

function rowsOf(l: GridLayout): GroupRows {
  if (l.groups === null) throw new Error("The layout has no groups.");
  return l.groups;
}

/** The last index in `a` (ascending) whose value is at most `x`; 0 when none is. */
function lastAtMost(a: Float64Array | Int32Array, x: number): number {
  let lo = 0;
  let hi = a.length - 1;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (a[mid]! <= x) lo = mid;
    else hi = mid - 1;
  }
  return Math.max(0, lo);
}

/** Row `r`'s top within the rows. */
export function rowTop(l: GridLayout, r: number): number {
  if (l.groups === null) return r * l.stride;
  const { tops } = l.groups;
  return r < tops.length ? tops[r]! : l.total;
}

/** The row holding position `y` within the rows. */
export function rowAt(l: GridLayout, y: number): number {
  if (l.groups === null) return Math.floor(y / l.stride);
  return lastAtMost(l.groups.tops, y);
}

/** The group holding tile `index`. */
export function groupAt(l: GridLayout, index: number): number {
  return lastAtMost(rowsOf(l).starts, index);
}

/** The group row `r` belongs to. */
export function groupOfRow(l: GridLayout, r: number): number {
  return lastAtMost(rowsOf(l).headRows, r);
}

/** The row of tile `index`. */
export function rowOf(l: GridLayout, index: number): number {
  if (l.groups === null) return Math.floor(index / l.cols);
  const g = groupAt(l, index);
  const { headRows, starts } = l.groups;
  return headRows[g]! + 1 + Math.floor((index - starts[g]!) / l.cols);
}

/** The tiles of row `r`, `[first, end)`; null for a heading. */
export function rowTiles(l: GridLayout, r: number): [number, number] | null {
  const { firsts, starts, counts } = rowsOf(l);
  const first = firsts[r]!;
  if (first < 0) return null;
  const g = groupAt(l, first);
  return [first, Math.min(starts[g]! + counts[g]!, first + l.cols)];
}

/** The first tile in row `r` or after it; `count` when there is none. */
export function tileFrom(l: GridLayout, r: number, count: number): number {
  if (l.groups === null) return Math.min(count, r * l.cols);
  const { firsts, starts } = l.groups;
  if (r >= firsts.length) return count;
  const first = firsts[r]!;
  return first >= 0 ? first : starts[-1 - first]!;
}

/** One past the last tile in row `r` or before it. */
export function tileEnd(l: GridLayout, r: number, count: number): number {
  if (l.groups === null) return Math.min(count, (r + 1) * l.cols);
  const { firsts } = l.groups;
  if (r < 0) return 0;
  if (r >= firsts.length) return count;
  const tiles = rowTiles(l, r);
  if (tiles !== null) return tiles[1];
  return tileFrom(l, r, count);
}
