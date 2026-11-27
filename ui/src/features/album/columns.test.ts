import { describe, expect, it } from "vitest";
import { columnWidths } from "./columns";

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
