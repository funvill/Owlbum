import { describe, expect, it } from "vitest";
import { GROUP_GAP, HEAD, groupAt, groupOfRow, rowAt, rowOf, rowTiles, rowTop } from "./groups";
import { GAP, PAD, anchorAfterResize, followScroll, gridLayout, scrollToShow, viewIndex, visibleRange } from "./layout";

// 4 columns of 160: 4 × 160 + 3 × 8 = 664 wide inside the padding.
const WIDTH = 664 + 2 * PAD;

describe("the timeline's layout", () => {
  it("lays out a group of 9 at 4 columns as one heading and rows of 4, 4 and 1", () => {
    const l = gridLayout(WIDTH, 9, 160, [{ start: 0, count: 9 }]);
    expect(l.cols).toBe(4);
    expect(l.rows).toBe(4);
    expect([0, 1, 2, 3].map((r) => rowTop(l, r))).toEqual([0, HEAD, HEAD + l.stride, HEAD + 2 * l.stride]);
    expect([0, 1, 2, 3].map((r) => rowTiles(l, r))).toEqual([null, [0, 4], [4, 8], [8, 9]]);
    expect(l.total).toBe(HEAD + 3 * l.stride - GAP);
  });

  it("puts 8 px after each group and finds rows and tiles by binary search", () => {
    const l = gridLayout(WIDTH, 7, 160, [
      { start: 0, count: 5 },
      { start: 5, count: 2 },
    ]);
    // Group 0: heading, rows of 4 and 1; group 1: heading, a row of 2.
    expect(l.rows).toBe(5);
    const second = HEAD + 2 * l.stride + GROUP_GAP;
    expect(rowTop(l, 3)).toBe(second);
    expect(rowAt(l, second - 1)).toBe(2);
    expect(rowAt(l, second)).toBe(3);
    expect(rowOf(l, 4)).toBe(2);
    expect(rowOf(l, 5)).toBe(4);
    expect(groupOfRow(l, 4)).toBe(1);
    expect(groupAt(l, 6)).toBe(1);
    expect(groupAt(l, 0)).toBe(0);
  });

  it("draws only the rows in view, plus the overscan", () => {
    const groups = Array.from({ length: 1000 }, (_, g) => ({ start: g * 3, count: 3 }));
    const l = gridLayout(WIDTH, 3000, 160, groups);
    const block = HEAD + l.stride + GROUP_GAP;
    const range = visibleRange(l, PAD + 500 * block, 2 * block, 3000);
    // Rows 1000 (heading of group 500) to 1003 are in view; 4 more each way.
    expect(range.firstRow).toBe(996);
    expect(range.endRow).toBe(1004 + 1 + 4);
    expect(range.firstIndex).toBe(498 * 3);
    expect(range.lastIndex).toBe(503 * 3 + 2);
  });

  it("brings a photo into view below the sticky heading", () => {
    const l = gridLayout(WIDTH, 30, 160, [
      { start: 0, count: 20 },
      { start: 20, count: 10 },
    ]);
    // Tile 12 is in row 4 of the first group (rows 1 to 5).
    const top = PAD + rowTop(l, rowOf(l, 12));
    expect(scrollToShow(12, l, top, 400)).toBe(top - PAD - HEAD);
    expect(scrollToShow(12, l, top - PAD - HEAD, 400)).toBeNull();
  });

  it("keeps its place by rows, whatever their heights", () => {
    const l = gridLayout(WIDTH, 30, 160, [
      { start: 0, count: 20 },
      { start: 20, count: 10 },
    ]);
    expect(followScroll(l, 100, 0, 20)).toBe(100 + rowTop(l, rowOf(l, 20)) - rowTop(l, rowOf(l, 0)));
    const real = PAD + rowTop(l, rowOf(l, 21));
    expect(viewIndex(l, real, 300, 30, null)).toBe(20);
    const narrow = gridLayout(2 * 168 + 2 * PAD, 30, 160, [
      { start: 0, count: 20 },
      { start: 20, count: 10 },
    ]);
    expect(anchorAfterResize(l, narrow, real)).toBe(rowTop(narrow, rowOf(narrow, 20)));
  });

  it("is empty with no groups", () => {
    const l = gridLayout(WIDTH, 0, 160, []);
    expect(l.total).toBe(0);
    expect(visibleRange(l, 0, 400, 0).lastIndex).toBe(-1);
  });

  it("lays out 100,000 photos over ten years by day quickly", () => {
    const groups = Array.from({ length: 3650 }, (_, g) => ({ start: g * 27, count: g === 3649 ? 100_000 - g * 27 : 27 }));
    const t = performance.now();
    const l = gridLayout(WIDTH, 100_000, 160, groups);
    const took = performance.now() - t;
    expect(l.rows).toBeGreaterThan(3650);
    // The page's target is 50 ms; a test machine is given room.
    expect(took).toBeLessThan(250);
  });
});
