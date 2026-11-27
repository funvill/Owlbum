import { describe, expect, it } from "vitest";
import type { CropRect } from "../../api/bindings/CropRect";
import { ASPECTS, HANDLES, MIN_CROP, aspectRatio, canSwap, fitCropToAspect, fractionRatio, resizeCrop } from "./crop";

const FRAME = { w: 3000, h: 2000 };
const MID: CropRect = { x: 0.25, y: 0.25, w: 0.5, h: 0.5 };
const close = (a: CropRect, b: CropRect) => {
  for (const k of ["x", "y", "w", "h"] as const) expect(a[k]).toBeCloseTo(b[k], 9);
};
const inside = (c: CropRect) => {
  expect(c.x).toBeGreaterThanOrEqual(-1e-12);
  expect(c.y).toBeGreaterThanOrEqual(-1e-12);
  expect(c.x + c.w).toBeLessThanOrEqual(1 + 1e-12);
  expect(c.y + c.h).toBeLessThanOrEqual(1 + 1e-12);
  expect(c.w).toBeGreaterThanOrEqual(MIN_CROP - 1e-12);
  expect(c.h).toBeGreaterThanOrEqual(MIN_CROP - 1e-12);
};
/** The crop's shape in pixels. */
const shape = (c: CropRect) => (c.w * FRAME.w) / (c.h * FRAME.h);

describe("aspect presets", () => {
  it("gives each preset's ratio, and the inverse when swapped", () => {
    expect(aspectRatio("free", false, FRAME)).toBeNull();
    expect(aspectRatio("original", false, FRAME)).toBe(1.5);
    expect(aspectRatio("original", true, FRAME)).toBeCloseTo(2 / 3);
    expect(aspectRatio("square", true, FRAME)).toBe(1);
    expect(aspectRatio("4:5", false, FRAME)).toBe(0.8);
    expect(aspectRatio("4:5", true, FRAME)).toBe(1.25);
    expect(aspectRatio("3:2", false, FRAME)).toBe(1.5);
    expect(aspectRatio("16:9", true, FRAME)).toBeCloseTo(9 / 16);
  });

  it("offers Swap for 4:5, 3:2, 16:9, and Original unless the photo is square", () => {
    expect(ASPECTS.filter((a) => canSwap(a, FRAME))).toEqual(["original", "4:5", "3:2", "16:9"]);
    expect(canSwap("original", { w: 100, h: 100 })).toBe(false);
  });

  it("fits the largest crop of each ratio about the old centre, inside the frame", () => {
    for (const aspect of ASPECTS) {
      for (const swapped of [false, true]) {
        const ratio = aspectRatio(aspect, swapped, FRAME);
        if (ratio === null) continue;
        const c = fitCropToAspect({ x: 0.9, y: 0.1, w: 0.1, h: 0.1 }, ratio, FRAME);
        inside(c);
        expect(shape(c)).toBeCloseTo(ratio, 9);
        expect(Math.max(c.w, c.h)).toBeCloseTo(1, 9);
      }
    }
    close(fitCropToAspect(MID, 1, FRAME), { x: 1 / 6, y: 0, w: 2 / 3, h: 1 });
  });
});

describe("resizeCrop, free", () => {
  it("moves the box inside the frame, keeping its size", () => {
    close(resizeCrop(MID, "move", 0.1, -0.1, null, FRAME), { x: 0.35, y: 0.15, w: 0.5, h: 0.5 });
    close(resizeCrop(MID, "move", 2, -2, null, FRAME), { x: 0.5, y: 0, w: 0.5, h: 0.5 });
  });

  it("drags each edge on its own, stopping at the frame and 0.05 from the other edge", () => {
    close(resizeCrop(MID, "e", 0.1, 0.3, null, FRAME), { x: 0.25, y: 0.25, w: 0.6, h: 0.5 });
    close(resizeCrop(MID, "w", -1, 0, null, FRAME), { x: 0, y: 0.25, w: 0.75, h: 0.5 });
    close(resizeCrop(MID, "n", 0, 1, null, FRAME), { x: 0.25, y: 0.7, w: 0.5, h: 0.05 });
    close(resizeCrop(MID, "se", 1, 1, null, FRAME), { x: 0.25, y: 0.25, w: 0.75, h: 0.75 });
    close(resizeCrop(MID, "nw", 0.6, 0.6, null, FRAME), { x: 0.7, y: 0.7, w: 0.05, h: 0.05 });
  });

  it("never leaves the frame or goes under the minimum, for every handle", () => {
    for (const handle of [...HANDLES, "move" as const]) {
      for (const [dx, dy] of [
        [-2, -2],
        [2, 2],
        [0.7, -0.7],
        [-0.7, 0.7],
      ]) {
        inside(resizeCrop(MID, handle, dx!, dy!, null, FRAME));
      }
    }
  });
});

describe("resizeCrop, fixed ratio", () => {
  const ratio = 1.5;
  const start = fitCropToAspect(MID, ratio, FRAME);
  /** Square in pixels: 0.2 of the width is 0.3 of the height. */
  const small = { x: 0.4, y: 0.4, w: 0.2, h: 0.2 / fractionRatio(1, FRAME) };

  it("keeps the opposite corner and the ratio when a corner is dragged", () => {
    const c = resizeCrop(small, "se", 0.1, 0.05, 1, FRAME);
    expect(c.x).toBeCloseTo(0.4);
    expect(c.y).toBeCloseTo(0.4);
    expect(shape(c)).toBeCloseTo(1, 9);
    const nw = resizeCrop(small, "nw", -0.1, -0.1, 1, FRAME);
    expect(nw.x + nw.w).toBeCloseTo(0.6);
    expect(nw.y + nw.h).toBeCloseTo(small.y + small.h);
    expect(shape(nw)).toBeCloseTo(1, 9);
  });

  it("keeps the opposite side and the centre when a side is dragged", () => {
    const c = resizeCrop(small, "e", -0.05, 0, 1, FRAME);
    expect(c.x).toBeCloseTo(0.4);
    expect(c.y + c.h / 2).toBeCloseTo(small.y + small.h / 2);
    expect(shape(c)).toBeCloseTo(1, 9);
    const n = resizeCrop(small, "n", 0, -0.05, 1, FRAME);
    expect(n.y + n.h).toBeCloseTo(small.y + small.h);
    expect(n.x + n.w / 2).toBeCloseTo(0.5);
  });

  it("never leaves the frame, loses the ratio or goes under the minimum", () => {
    for (const r of [0.8, 1, 1.5, 9 / 16]) {
      for (const handle of HANDLES) {
        for (const [dx, dy] of [
          [-2, -2],
          [2, 2],
          [0.7, -0.7],
          [-0.7, 0.7],
        ]) {
          const c = resizeCrop(fitCropToAspect(MID, r, FRAME), handle, dx!, dy!, r, FRAME);
          inside(c);
          expect(shape(c)).toBeCloseTo(r, 6);
        }
      }
    }
    inside(resizeCrop(start, "se", 1, 1, ratio, FRAME));
  });
});
