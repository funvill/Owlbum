import { describe, expect, it } from "vitest";
import {
  GAP,
  MAX_SCROLL,
  PAD,
  anchorAfterResize,
  clampThumbSize,
  followScroll,
  gridLayout,
  realScroll,
  scrollRatio,
  scrollToShow,
  thumbFor,
  tilePlace,
  toScrollTop,
  viewIndex,
  visibleRange,
} from "./layout";

describe("keeping the grid's place while rows arrive", () => {
  // 5 columns; each row is one stride tall.
  const l = gridLayout(5 * 168 + 2 * PAD, 1000, 160);

  it("keeps the place by the current photo when it is in view", () => {
    expect(l.cols).toBe(5);
    const real = 10 * l.stride;
    expect(viewIndex(l, real, 4 * l.stride, 1000, 57)).toBe(57);
  });

  it("else by the first tile in view", () => {
    const real = 10 * l.stride + PAD;
    expect(viewIndex(l, real, 4 * l.stride, 1000, 500)).toBe(50);
    expect(viewIndex(l, real, 4 * l.stride, 1000, null)).toBe(50);
  });

  it("keeps nothing at the very top with the current photo out of view", () => {
    expect(viewIndex(l, 0, 4 * l.stride, 1000, null)).toBeNull();
    expect(viewIndex(l, 0, 4 * l.stride, 1000, 2)).toBe(2);
    expect(viewIndex(l, 0, 4 * l.stride, 0, null)).toBeNull();
  });

  it("scrolls by the rows the tile moved", () => {
    expect(followScroll(l, 1000, 50, 62)).toBe(1000 + 2 * l.stride);
    expect(followScroll(l, 1000, 50, 52)).toBe(1000);
    expect(followScroll(l, 100, 50, 10)).toBe(0);
  });
});

describe("thumbnail size", () => {
  it("clamps to 96 to 320 and rounds", () => {
    expect(clampThumbSize(50)).toBe(96);
    expect(clampThumbSize(1000)).toBe(320);
    expect(clampThumbSize(161.4)).toBe(161);
    expect(clampThumbSize(200)).toBe(200);
  });

  it("makes anything that is not a finite number 160", () => {
    expect(clampThumbSize(Number.NaN)).toBe(160);
    expect(clampThumbSize(Infinity)).toBe(160);
    expect(clampThumbSize("200")).toBe(160);
    expect(clampThumbSize(null)).toBe(160);
  });

  it("loads the small thumbnail unless the tile needs more than 512 pixels", () => {
    expect(thumbFor(160, 1)).toBe("s");
    expect(thumbFor(256, 2)).toBe("s");
    expect(thumbFor(257, 2)).toBe("m");
    expect(thumbFor(320, 1.75)).toBe("m");
  });
});

describe("grid layout", () => {
  it("follows the layout maths of docs/04", () => {
    // width = 420 − 24 = 396; cols = floor((396 + 8) / (160 + 8)) = 2
    const l = gridLayout(420, 1000, 160);
    expect(l.width).toBe(396);
    expect(l.cols).toBe(2);
    expect(l.side).toBe((396 - GAP) / 2);
    expect(l.stride).toBe(l.side + GAP);
    expect(l.rows).toBe(500);
    expect(l.total).toBe(500 * l.stride - GAP);
  });

  it("stretches tiles to fill the row and never shrinks them below the size", () => {
    const l = gridLayout(1000, 10, 160);
    expect(l.cols).toBe(5);
    expect(l.side).toBeGreaterThanOrEqual(160);
    expect(l.cols * l.side + (l.cols - 1) * GAP).toBeCloseTo(l.width);
  });

  it("keeps one column when the box is narrower than one tile", () => {
    const l = gridLayout(100, 3, 160);
    expect(l.cols).toBe(1);
    expect(l.side).toBe(76);
    expect(gridLayout(0, 3, 160).side).toBe(1);
  });

  it("has no height for an empty result", () => {
    const l = gridLayout(420, 0, 160);
    expect(l.rows).toBe(0);
    expect(l.total).toBe(0);
  });

  it("places tile i at its column and row", () => {
    const l = gridLayout(420, 10, 160);
    expect(tilePlace(0, l)).toEqual({ x: 0, y: 0 });
    expect(tilePlace(3, l)).toEqual({ x: l.stride, y: l.stride });
  });
});

describe("visible range", () => {
  it("renders the rows in view plus four rows of overscan each way", () => {
    const l = gridLayout(420, 1000, 160);
    const r = visibleRange(l, PAD + 50 * l.stride, 600, 1000);
    const inView = Math.ceil(600 / l.stride);
    expect(r.firstRow).toBe(50 - 4);
    expect(r.endRow).toBe(50 + inView + 4);
    expect(r.firstIndex).toBe(r.firstRow * 2);
    expect(r.lastIndex).toBe(r.endRow * 2 - 1);
  });

  it("stops at the ends of the result", () => {
    const l = gridLayout(420, 5, 160);
    const r = visibleRange(l, 0, 600, 5);
    expect(r.firstRow).toBe(0);
    expect(r.endRow).toBe(3);
    expect(r.lastIndex).toBe(4);
    expect(visibleRange(gridLayout(420, 0, 160), 0, 600, 0).lastIndex).toBe(-1);
  });
});

describe("very tall content", () => {
  it("does nothing below the limit", () => {
    const l = gridLayout(420, 1000, 160);
    expect(scrollRatio(l, 600)).toBe(1);
    expect(realScroll(1234, l, 600)).toBe(1234);
    expect(toScrollTop(1234, l, 600)).toBe(1234);
  });

  it("scales positions above 8,000,000 px, end to end", () => {
    // 100,000 photos in two columns: 50,000 rows of about 202 px.
    const l = gridLayout(420, 100_000, 160);
    expect(l.total).toBeGreaterThan(MAX_SCROLL);
    expect(l.height).toBe(MAX_SCROLL + 2 * PAD);
    const viewport = 600;
    const ratio = scrollRatio(l, viewport);
    expect(ratio).toBeGreaterThan(1);
    expect(realScroll(0, l, viewport)).toBe(0);
    const lastTop = l.height - viewport;
    expect(realScroll(lastTop, l, viewport)).toBeCloseTo(l.total + 2 * PAD - viewport);
    expect(toScrollTop(realScroll(4_000_000, l, viewport), l, viewport)).toBeCloseTo(4_000_000);
    // The last row is reachable and fully visible at the end.
    const r = visibleRange(l, realScroll(lastTop, l, viewport), viewport, 100_000);
    expect(r.lastIndex).toBe(99_999);
  });
});

describe("keeping the current photo in view", () => {
  const l = gridLayout(420, 1000, 160);

  it("does nothing when the row is in view", () => {
    expect(scrollToShow(2, l, 0, 600)).toBeNull();
  });

  it("scrolls up so the row's top is at the top", () => {
    expect(scrollToShow(20, l, 5000, 600)).toBe(10 * l.stride);
  });

  it("scrolls down just enough to show the row's bottom", () => {
    const top = PAD + 100 * l.stride;
    expect(scrollToShow(200, l, 0, 600)).toBe(top + l.side + PAD - 600);
  });
});

describe("scroll while resizing", () => {
  it("keeps the photo at the top-left of the viewport in the top row", () => {
    const before = gridLayout(1000, 1000, 160); // 5 columns
    const after = gridLayout(1000, 1000, 320); // 2 or 3 columns
    const scroll = 40 * before.stride + 5; // row 40: photo 200 at the top-left
    const next = anchorAfterResize(before, after, scroll);
    expect(next).toBe(Math.floor(200 / after.cols) * after.stride);
    expect(anchorAfterResize(before, after, 0)).toBe(0);
  });
});
