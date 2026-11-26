import { describe, expect, it } from "vitest";
import {
  MIN_ZOOM,
  WHEEL_STEP,
  centreOn,
  clampPan,
  fitView,
  fittedSize,
  maxZoom,
  mediumSize,
  miniMapSize,
  needsFull,
  visibleRect,
  zoomAt,
  type Box,
  type View,
} from "./zoom";

const stage: Box = { width: 800, height: 600 };

describe("fittedSize", () => {
  it("is the largest size with the picture's shape that fits", () => {
    expect(fittedSize(stage, { width: 4000, height: 2000 })).toEqual({ width: 800, height: 400 });
    expect(fittedSize(stage, { width: 1000, height: 2000 })).toEqual({ width: 300, height: 600 });
    // Small pictures are enlarged to fit too.
    expect(fittedSize(stage, { width: 40, height: 30 })).toEqual({ width: 800, height: 600 });
  });

  it("is 0 × 0 when any input is 0 or less", () => {
    expect(fittedSize({ width: 0, height: 600 }, { width: 4, height: 3 })).toEqual({ width: 0, height: 0 });
    expect(fittedSize(stage, { width: 4, height: 0 })).toEqual({ width: 0, height: 0 });
    expect(fittedSize(stage, { width: -1, height: 3 })).toEqual({ width: 0, height: 0 });
  });
});

describe("fitView", () => {
  it("is zoom 1, centred", () => {
    expect(fitView(stage, { width: 800, height: 400 })).toEqual({ zoom: 1, x: 0, y: 100 });
    expect(fitView(stage, { width: 300, height: 600 })).toEqual({ zoom: 1, x: 250, y: 0 });
  });
});

describe("clampPan", () => {
  const fitted = { width: 800, height: 400 };

  it("centres an axis where the scaled picture is no bigger than the stage", () => {
    // Height at zoom 1.5 is 600: exactly the stage, so centred (y = 0).
    expect(clampPan({ zoom: 1.5, x: -100, y: 50 }, stage, fitted)).toEqual({ zoom: 1.5, x: -100, y: 0 });
  });

  it("keeps the picture covering the stage on an axis where it is bigger", () => {
    // At zoom 2: 1600 × 800, so x in [-800, 0] and y in [-200, 0].
    expect(clampPan({ zoom: 2, x: 10, y: -500 }, stage, fitted)).toEqual({ zoom: 2, x: 0, y: -200 });
    expect(clampPan({ zoom: 2, x: -900, y: 5 }, stage, fitted)).toEqual({ zoom: 2, x: -800, y: 0 });
    expect(clampPan({ zoom: 2, x: -300, y: -100 }, stage, fitted)).toEqual({ zoom: 2, x: -300, y: -100 });
  });
});

describe("zoomAt", () => {
  const fitted = { width: 800, height: 400 };
  const fit = fitView(stage, fitted);

  it("keeps the picture point under the pointer fixed", () => {
    const v = zoomAt(fit, 2, { x: 200, y: 300 }, stage, fitted, 8);
    expect(v.zoom).toBe(2);
    // The point under (200, 300) was picture point (200, 200) at zoom 1; at zoom 2 it is (400, 400).
    expect(v.x).toBe(200 - 400);
    expect(v.y).toBe(300 - 400);
  });

  it("clamps the pan afterwards", () => {
    const v = zoomAt(fit, 2, { x: 0, y: 100 }, stage, fitted, 8);
    expect(v).toEqual({ zoom: 2, x: 0, y: 0 });
  });

  it("uses the fit view when the zoom comes back to 1", () => {
    const zoomed: View = { zoom: 1.2, x: -50, y: -10 };
    expect(zoomAt(zoomed, 1 / WHEEL_STEP, { x: 10, y: 10 }, stage, fitted, 8)).toEqual(fit);
    expect(zoomAt(zoomed, 0.1, { x: 10, y: 10 }, stage, fitted, 8)).toEqual(fit);
  });

  it("stops at the largest zoom", () => {
    expect(zoomAt({ zoom: 7, x: -100, y: -100 }, 2, { x: 0, y: 0 }, stage, fitted, 8).zoom).toBe(8);
    expect(MIN_ZOOM).toBe(1);
  });
});

describe("maxZoom", () => {
  it("is 8, or where one picture pixel is 2 screen pixels when that is larger", () => {
    expect(maxZoom(800, 1600, 1)).toBe(8);
    // 2 × 6000 / (800 × 1) = 15
    expect(maxZoom(800, 6000, 1)).toBe(15);
    expect(maxZoom(800, 6000, 2)).toBe(8);
    expect(maxZoom(0, 6000, 1)).toBe(8);
  });
});

describe("mediumSize", () => {
  it("fits the original inside 1600 px, never enlarging", () => {
    expect(mediumSize({ width: 4000, height: 3000 })).toEqual({ width: 1600, height: 1200 });
    expect(mediumSize({ width: 1000, height: 750 })).toEqual({ width: 1000, height: 750 });
  });
});

describe("needsFull", () => {
  const original = { width: 4000, height: 3000 };

  it("asks once the picture is drawn larger than the medium picture has pixels", () => {
    expect(needsFull(800, 1, 1, original)).toBe(false);
    expect(needsFull(800, 2, 1, original)).toBe(false);
    expect(needsFull(800, 2.01, 1, original)).toBe(true);
    expect(needsFull(800, 1.5, 2, original)).toBe(true);
  });

  it("never asks for a photo whose original is no larger than the medium picture", () => {
    expect(needsFull(800, 8, 2, { width: 1600, height: 900 })).toBe(false);
    expect(needsFull(800, 8, 2, { width: 0, height: 0 })).toBe(false);
  });
});

describe("mini map", () => {
  const fitted = { width: 800, height: 400 };

  it("has a long side of 160 px and the picture's shape", () => {
    expect(miniMapSize(fitted)).toEqual({ width: 160, height: 80 });
    expect(miniMapSize({ width: 300, height: 600 })).toEqual({ width: 80, height: 160 });
  });

  it("outlines the visible part of the picture, as fractions", () => {
    // Zoom 2: 1600 × 800, showing x 400..1200 and y 200..800.
    expect(visibleRect({ zoom: 2, x: -400, y: -200 }, stage, fitted)).toEqual({
      left: 0.25,
      top: 0.25,
      width: 0.5,
      height: 0.75,
    });
  });

  it("centres the view on a point of the picture, then clamps", () => {
    expect(centreOn({ zoom: 2, x: 0, y: 0 }, { u: 0.5, v: 0.5 }, stage, fitted)).toEqual({ zoom: 2, x: -400, y: -100 });
    expect(centreOn({ zoom: 2, x: -400, y: 0 }, { u: 0, v: 1 }, stage, fitted)).toEqual({ zoom: 2, x: 0, y: -200 });
  });
});
