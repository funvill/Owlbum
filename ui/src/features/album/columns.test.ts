import { describe, expect, it } from "vitest";
import { columnWidths, stripHeight } from "./columns";

describe("columnWidths", () => {
  it("shows the stored widths while the row has room, and before it is measured", () => {
    expect(columnWidths(1440, 420, 340)).toMatchObject({
      grid: 420,
      details: 340,
    });
    expect(columnWidths(null, 900, 500)).toMatchObject({
      grid: 900,
      details: 500,
    });
  });

  it("shrinks the preview, then the details column, then the thumbnail column", () => {
    // 420 + 340 + 300 + 12 = 1072: below that the details column gives way first.
    expect(columnWidths(1072, 420, 340)).toMatchObject({
      grid: 420,
      details: 340,
    });
    expect(columnWidths(1000, 420, 340)).toMatchObject({
      grid: 420,
      details: 268,
    });
    expect(columnWidths(960, 420, 340)).toMatchObject({
      grid: 388,
      details: 260,
    });
    expect(columnWidths(772, 420, 340)).toMatchObject({
      grid: 200,
      details: 260,
    });
    expect(columnWidths(600, 420, 340)).toMatchObject({
      grid: 200,
      details: 260,
    });
  });

  it("keeps the details column within 260 to 560 and gives the limits for the splitters", () => {
    expect(columnWidths(2000, 420, 900)).toMatchObject({
      details: 560,
      detailsMax: 560,
      gridMax: 1428,
    });
    expect(columnWidths(2000, 100, 100)).toMatchObject({
      grid: 200,
      details: 260,
    });
    expect(columnWidths(1200, 600, 340)).toMatchObject({
      grid: 600,
      detailsMax: 288,
      gridMax: 628,
    });
  });
});

describe("columns that are hidden or at the bottom", () => {
  it("gives the room of a column not in the row to the others, with no splitter for it", () => {
    // No thumbnails in the row: the details column may take what the preview leaves.
    expect(columnWidths(1000, 420, 900, { grid: false, details: true })).toMatchObject({
      details: 560,
      detailsMax: 560,
    });
    expect(columnWidths(800, 420, 560, { grid: false, details: true })).toMatchObject({ details: 494 });
    // No details column: the thumbnails may take all but the preview and one splitter.
    expect(columnWidths(1000, 900, 340, { grid: true, details: false })).toMatchObject({
      grid: 694,
      gridMax: 694,
    });
  });
});

describe("stripHeight", () => {
  it("keeps the stored height while it fits, and before the screen is measured", () => {
    expect(stripHeight(800, 200)).toEqual({ height: 200, max: 594 });
    expect(stripHeight(null, 900)).toEqual({ height: 900, max: Infinity });
  });

  it("stays at least 120 and leaves the preview 200 above it", () => {
    expect(stripHeight(800, 50).height).toBe(120);
    expect(stripHeight(500, 400)).toEqual({ height: 294, max: 294 });
    expect(stripHeight(300, 400)).toEqual({ height: 120, max: 120 });
  });
});
