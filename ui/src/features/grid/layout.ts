// The grid's layout maths (docs/04-browsing.md, "Layout maths", "Very tall
// content", "Keeping the current photo in view" and "Thumbnail size").
// Pure functions of the box's size, the result's count and the thumbnail
// size, so they are tested without a browser.
//
// Positions are "real" pixels of the whole content: the 12 px padding,
// then every row. Above MAX_SCROLL the scroll box is shorter than that and
// `realScroll` maps its scrollTop onto the real position.

/** The padding inside the scroll box (`--space-4`). */
export const PAD = 12;
/** The gap between tiles (`--space-3`). */
export const GAP = 8;
/** Rows of tiles rendered above and below the viewport. */
export const OVERSCAN_ROWS = 4;
/** The tallest box the grid makes; above it positions are scaled. */
export const MAX_SCROLL = 8_000_000;

export const THUMB_MIN = 96;
export const THUMB_MAX = 320;
export const THUMB_DEFAULT = 160;
export const THUMB_STEP = 8;
/** The longest side of the small thumbnail. */
const SMALL_THUMB = 512;

/** `min(320, max(96, round(x)))`; anything that is not a finite number is 160. */
export function clampThumbSize(x: unknown): number {
  if (typeof x !== "number" || !Number.isFinite(x)) return THUMB_DEFAULT;
  return Math.min(THUMB_MAX, Math.max(THUMB_MIN, Math.round(x)));
}

/** Which thumbnail a tile of `side` pixels loads: small unless it needs more than 512 pixels. */
export function thumbFor(side: number, devicePixelRatio: number): "s" | "m" {
  return side * devicePixelRatio > SMALL_THUMB ? "m" : "s";
}

export interface GridLayout {
  /** Content width inside the padding. */
  width: number;
  cols: number;
  /** A tile's side. */
  side: number;
  /** `side + GAP`. */
  stride: number;
  rows: number;
  /** Height of the rows, without the padding. */
  total: number;
  /** Height of the scroll box's content: the rows and the padding, at most MAX_SCROLL and the padding. */
  height: number;
}

export function gridLayout(clientWidth: number, count: number, tile: number): GridLayout {
  const width = Math.max(0, clientWidth - 2 * PAD);
  const cols = Math.max(1, Math.floor((width + GAP) / (tile + GAP)));
  const side = Math.max(1, (width - (cols - 1) * GAP) / cols);
  const stride = side + GAP;
  const rows = Math.ceil(count / cols);
  const total = rows > 0 ? rows * stride - GAP : 0;
  const height = Math.min(total, MAX_SCROLL) + 2 * PAD;
  return { width, cols, side, stride, rows, total, height };
}

/** Where tile `i` sits within the rows (add PAD for the box). */
export function tilePlace(i: number, l: GridLayout): { x: number; y: number } {
  return { x: (i % l.cols) * l.stride, y: Math.floor(i / l.cols) * l.stride };
}

/** Real pixels moved per pixel of scrollTop: 1 below MAX_SCROLL. */
export function scrollRatio(l: GridLayout, viewport: number): number {
  if (l.total <= MAX_SCROLL) return 1;
  const real = l.total + 2 * PAD - viewport;
  const box = l.height - viewport;
  return box > 0 ? real / box : 1;
}

/** The real scroll position for the box's scrollTop. */
export function realScroll(scrollTop: number, l: GridLayout, viewport: number): number {
  return scrollTop * scrollRatio(l, viewport);
}

/** The box's scrollTop for a real scroll position. */
export function toScrollTop(real: number, l: GridLayout, viewport: number): number {
  return real / scrollRatio(l, viewport);
}

export interface VisibleRange {
  firstRow: number;
  /** One past the last row rendered. */
  endRow: number;
  firstIndex: number;
  /** The last index rendered; −1 when nothing is. */
  lastIndex: number;
}

/** The rows in view at real scroll `real`, plus the overscan. */
export function visibleRange(l: GridLayout, real: number, viewport: number, count: number): VisibleRange {
  const clamp = (v: number) => Math.min(l.rows, Math.max(0, v));
  const top = Math.max(0, real - PAD);
  const firstRow = clamp(Math.floor(top / l.stride) - OVERSCAN_ROWS);
  const endRow = clamp(Math.ceil((top + viewport) / l.stride) + OVERSCAN_ROWS);
  return { firstRow, endRow, firstIndex: firstRow * l.cols, lastIndex: Math.min(count, endRow * l.cols) - 1 };
}

/**
 * The tile the grid's place is kept by while rows arrive (docs/04-browsing.md,
 * "When the result changes"): the current photo `at` when its row is in view,
 * else the first tile in view. Null at the very top with the current photo out
 * of view, so the grid stays at the top and the photos that sort first show as
 * they arrive.
 */
export function viewIndex(
  l: GridLayout,
  real: number,
  viewport: number,
  count: number,
  at: number | null,
): number | null {
  if (count === 0 || l.cols === 0) return null;
  if (at !== null && at < count) {
    const top = PAD + Math.floor(at / l.cols) * l.stride;
    if (top + l.side > real && top < real + viewport) return at;
  }
  if (real <= 0) return null;
  const index = Math.floor(Math.max(0, real - PAD) / l.stride) * l.cols;
  return index < count ? index : null;
}

/** The real scroll position that keeps tile `from`, now at index `to`, where it was on screen. */
export function followScroll(l: GridLayout, real: number, from: number, to: number): number {
  if (l.cols === 0) return real;
  return Math.max(0, real + (Math.floor(to / l.cols) - Math.floor(from / l.cols)) * l.stride);
}

/**
 * The real scroll position that just shows the row of tile `index`, or
 * null when it is already in view.
 */
export function scrollToShow(index: number, l: GridLayout, real: number, viewport: number): number | null {
  const top = PAD + Math.floor(index / l.cols) * l.stride;
  if (top - PAD < real) return top - PAD;
  if (top + l.side + PAD > real + viewport) return top + l.side + PAD - viewport;
  return null;
}

/**
 * The real scroll position after the layout changed from `before` to
 * `after`, so the photo at the top-left of the viewport stays in the top row.
 */
export function anchorAfterResize(before: GridLayout, after: GridLayout, real: number): number {
  const row = Math.floor(real / before.stride);
  const index = row * before.cols;
  return Math.floor(index / after.cols) * after.stride;
}
